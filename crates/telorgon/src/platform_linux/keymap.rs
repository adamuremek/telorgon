use std::ffi::{CStr, CString, c_char, c_void};
use std::io::Write;
use std::os::fd::{FromRawFd, OwnedFd};
use std::ptr::NonNull;

use crate::platform_linux::ffi;
use crate::platform_linux::{LinuxPlatformError, LinuxPlatformErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum KeyDirection {
    Up = 0,
    Down = 1,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XkbModifiers {
    pub depressed: u32,
    pub latched: u32,
    pub locked: u32,
    pub group: u32,
}

pub struct KeymapFile {
    fd: OwnedFd,
    size: u32,
}

impl KeymapFile {
    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }

    pub const fn size(&self) -> u32 {
        self.size
    }
}

pub struct XkbKeyboard {
    context: NonNull<ffi::xkb_context>,
    keymap: NonNull<ffi::xkb_keymap>,
    state: NonNull<ffi::xkb_state>,
}

impl XkbKeyboard {
    pub fn from_names(
        rules: Option<&str>,
        model: Option<&str>,
        layout: Option<&str>,
        variant: Option<&str>,
        options: Option<&str>,
    ) -> Result<Self, LinuxPlatformError> {
        let strings = [rules, model, layout, variant, options]
            .map(|value| value.map(CString::new).transpose())
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                LinuxPlatformError::new(
                    LinuxPlatformErrorKind::Keymap,
                    "XKB name contains an interior NUL",
                )
            })?;
        let pointer = |index: usize| {
            strings[index]
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr())
        };
        let names = ffi::xkb_rule_names {
            rules: pointer(0),
            model: pointer(1),
            layout: pointer(2),
            variant: pointer(3),
            options: pointer(4),
        };
        let context = NonNull::new(unsafe { ffi::xkb_context_new(0) }).ok_or_else(|| {
            LinuxPlatformError::new(
                LinuxPlatformErrorKind::Allocation,
                "XKB context allocation failed",
            )
        })?;
        let Some(keymap) =
            NonNull::new(unsafe { ffi::xkb_keymap_new_from_names(context.as_ptr(), &names, 0) })
        else {
            unsafe { ffi::xkb_context_unref(context.as_ptr()) };
            return Err(LinuxPlatformError::new(
                LinuxPlatformErrorKind::Keymap,
                "XKB could not compile the requested keymap",
            ));
        };
        let Some(state) = NonNull::new(unsafe { ffi::xkb_state_new(keymap.as_ptr()) }) else {
            unsafe {
                ffi::xkb_keymap_unref(keymap.as_ptr());
                ffi::xkb_context_unref(context.as_ptr());
            }
            return Err(LinuxPlatformError::new(
                LinuxPlatformErrorKind::Allocation,
                "XKB state allocation failed",
            ));
        };
        Ok(Self {
            context,
            keymap,
            state,
        })
    }

    pub fn update_key(&mut self, evdev_keycode: u32, direction: KeyDirection) -> u32 {
        let xkb_keycode = evdev_keycode.saturating_add(8);
        unsafe { ffi::xkb_state_update_key(self.state.as_ptr(), xkb_keycode, direction as u32) }
    }

    pub fn symbol(&self, evdev_keycode: u32) -> u32 {
        unsafe {
            ffi::xkb_state_key_get_one_sym(self.state.as_ptr(), evdev_keycode.saturating_add(8))
        }
    }

    pub fn utf8(&self, evdev_keycode: u32) -> Result<String, LinuxPlatformError> {
        let keycode = evdev_keycode.saturating_add(8);
        let required = unsafe {
            ffi::xkb_state_key_get_utf8(self.state.as_ptr(), keycode, std::ptr::null_mut(), 0)
        };
        if !(0..=4096).contains(&required) {
            return Err(LinuxPlatformError::new(
                LinuxPlatformErrorKind::Keymap,
                "XKB returned an invalid UTF-8 length",
            ));
        }
        let mut bytes = vec![0_u8; required as usize + 1];
        let written = unsafe {
            ffi::xkb_state_key_get_utf8(
                self.state.as_ptr(),
                keycode,
                bytes.as_mut_ptr().cast::<c_char>(),
                bytes.len(),
            )
        };
        if written < 0 {
            return Err(LinuxPlatformError::new(
                LinuxPlatformErrorKind::Keymap,
                "XKB could not encode the key text",
            ));
        }
        CStr::from_bytes_until_nul(&bytes)
            .map_err(|_| {
                LinuxPlatformError::new(
                    LinuxPlatformErrorKind::Keymap,
                    "XKB text was not terminated",
                )
            })?
            .to_str()
            .map(str::to_owned)
            .map_err(|_| {
                LinuxPlatformError::new(
                    LinuxPlatformErrorKind::Keymap,
                    "XKB returned invalid UTF-8",
                )
            })
    }

    pub fn keymap_string(&self) -> Result<String, LinuxPlatformError> {
        let raw = NonNull::new(unsafe { ffi::xkb_keymap_get_as_string(self.keymap.as_ptr(), 1) })
            .ok_or_else(|| {
            LinuxPlatformError::new(
                LinuxPlatformErrorKind::Keymap,
                "XKB could not serialize the keymap",
            )
        })?;
        let value = unsafe { CStr::from_ptr(raw.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        unsafe { ffi::free(raw.as_ptr().cast::<c_void>()) };
        Ok(value)
    }

    pub fn keymap_file(&self) -> Result<KeymapFile, LinuxPlatformError> {
        let mut bytes = self.keymap_string()?.into_bytes();
        if !bytes.ends_with(&[0]) {
            bytes.push(0);
        }
        let size = u32::try_from(bytes.len()).map_err(|_| {
            LinuxPlatformError::new(
                LinuxPlatformErrorKind::Keymap,
                "serialized XKB keymap is too large",
            )
        })?;
        let name = c"telorgon-xkb-keymap";
        let raw = unsafe { ffi::memfd_create(name.as_ptr(), 1) };
        if raw < 0 {
            return Err(LinuxPlatformError::native(
                LinuxPlatformErrorKind::Keymap,
                "memfd_create failed for the XKB keymap",
                raw,
            ));
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let truncate = unsafe { ffi::ftruncate(raw, i64::from(size)) };
        if truncate != 0 {
            return Err(LinuxPlatformError::native(
                LinuxPlatformErrorKind::Keymap,
                "could not size the XKB keymap file",
                truncate,
            ));
        }
        std::fs::File::from(fd.try_clone().map_err(|error| {
            LinuxPlatformError::new(LinuxPlatformErrorKind::Keymap, error.to_string())
        })?)
        .write_all(&bytes)
        .map_err(|error| {
            LinuxPlatformError::new(LinuxPlatformErrorKind::Keymap, error.to_string())
        })?;
        Ok(KeymapFile { fd, size })
    }

    pub fn modifiers(&self) -> XkbModifiers {
        const DEPRESSED: u32 = 1 << 0;
        const LATCHED: u32 = 1 << 1;
        const LOCKED: u32 = 1 << 2;
        const EFFECTIVE_LAYOUT: u32 = 1 << 7;
        XkbModifiers {
            depressed: unsafe { ffi::xkb_state_serialize_mods(self.state.as_ptr(), DEPRESSED) },
            latched: unsafe { ffi::xkb_state_serialize_mods(self.state.as_ptr(), LATCHED) },
            locked: unsafe { ffi::xkb_state_serialize_mods(self.state.as_ptr(), LOCKED) },
            group: unsafe {
                ffi::xkb_state_serialize_layout(self.state.as_ptr(), EFFECTIVE_LAYOUT)
            },
        }
    }
}

impl Drop for XkbKeyboard {
    fn drop(&mut self) {
        unsafe {
            ffi::xkb_state_unref(self.state.as_ptr());
            ffi::xkb_keymap_unref(self.keymap.as_ptr());
            ffi::xkb_context_unref(self.context.as_ptr());
        }
    }
}
