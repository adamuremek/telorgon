use std::fmt;
use std::os::fd::{AsRawFd, OwnedFd};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::{PointI, SizeI};
use crate::presenter_vulkan_kms::ffi;
use crate::presenter_vulkan_kms::{AtomicProperty, GbmBuffer, KmsFramebufferId, KmsPropertyId};
use crate::presenter_vulkan_kms::{KmsConnectorId, KmsCrtcId, KmsObjectProperties, KmsPlaneId};

const DRM_CLIENT_CAP_UNIVERSAL_PLANES: u64 = 2;
const DRM_CLIENT_CAP_ATOMIC: u64 = 3;

pub struct KmsDevice {
    fd: OwnedFd,
    page_flip_events: Box<AtomicU64>,
}

impl KmsDevice {
    pub fn new(fd: OwnedFd) -> Result<Self, KmsError> {
        for capability in [DRM_CLIENT_CAP_UNIVERSAL_PLANES, DRM_CLIENT_CAP_ATOMIC] {
            let result = unsafe { ffi::drmSetClientCap(fd.as_raw_fd(), capability, 1) };
            if result != 0 {
                return Err(KmsError::native(
                    KmsErrorKind::Unsupported,
                    "DRM device does not support required atomic modesetting capabilities",
                    result,
                ));
            }
        }
        Ok(Self {
            fd,
            page_flip_events: Box::new(AtomicU64::new(0)),
        })
    }

    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }

    pub fn atomic_request(&self) -> Result<AtomicRequest<'_>, KmsError> {
        AtomicRequest::new(self)
    }

    /// Dispatches readable DRM events and records completed nonblocking page flips.
    pub fn dispatch_events(&self) -> Result<(), KmsError> {
        let mut context = ffi::drmEventContext {
            version: ffi::DRM_EVENT_CONTEXT_VERSION,
            vblank_handler: None,
            page_flip_handler: Some(page_flip_handler),
            page_flip_handler2: None,
            sequence_handler: None,
        };
        let result = unsafe { ffi::drmHandleEvent(self.fd.as_raw_fd(), &mut context) };
        if result != 0 {
            Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM event dispatch failed",
                result,
            ))
        } else {
            Ok(())
        }
    }

    /// Takes the number of page flips completed since the previous call.
    pub fn take_completed_page_flips(&self) -> u64 {
        self.page_flip_events.swap(0, Ordering::AcqRel)
    }

    pub fn cursor_size(&self) -> Result<SizeI, KmsError> {
        let mut width = 0_u64;
        let mut height = 0_u64;
        for (capability, value) in [
            (ffi::DRM_CAP_CURSOR_WIDTH, &mut width),
            (ffi::DRM_CAP_CURSOR_HEIGHT, &mut height),
        ] {
            let result = unsafe { ffi::drmGetCap(self.fd.as_raw_fd(), capability, value) };
            if result != 0 {
                return Err(KmsError::native(
                    KmsErrorKind::Unsupported,
                    "DRM cursor-size capability query failed",
                    result,
                ));
            }
        }
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Err(KmsError::new(
                KmsErrorKind::Unsupported,
                "DRM reported an unusable hardware-cursor extent",
            ));
        }
        Ok(SizeI {
            width: width as i32,
            height: height as i32,
        })
    }

    pub fn set_cursor(
        &self,
        crtc: KmsCrtcId,
        handle: u32,
        size: SizeI,
        hotspot: PointI,
    ) -> Result<(), KmsError> {
        if handle == 0 || size.width <= 0 || size.height <= 0 {
            return Err(KmsError::new(
                KmsErrorKind::InvalidState,
                "hardware cursor requires a buffer and positive extent",
            ));
        }
        let result = unsafe {
            ffi::drmModeSetCursor2(
                self.fd.as_raw_fd(),
                crtc.get(),
                handle,
                size.width as u32,
                size.height as u32,
                hotspot.x,
                hotspot.y,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(KmsError::native(
                KmsErrorKind::Unsupported,
                "DRM hardware-cursor image update failed",
                result,
            ))
        }
    }

    pub fn move_cursor(&self, crtc: KmsCrtcId, position: PointI) -> Result<(), KmsError> {
        let result = unsafe {
            ffi::drmModeMoveCursor(self.fd.as_raw_fd(), crtc.get(), position.x, position.y)
        };
        if result == 0 {
            Ok(())
        } else {
            Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM hardware-cursor move failed",
                result,
            ))
        }
    }

    pub fn hide_cursor(&self, crtc: KmsCrtcId) -> Result<(), KmsError> {
        let result =
            unsafe { ffi::drmModeSetCursor2(self.fd.as_raw_fd(), crtc.get(), 0, 0, 0, 0, 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM hardware cursor could not be hidden",
                result,
            ))
        }
    }

    fn page_flip_user_data(&self) -> *mut std::ffi::c_void {
        std::ptr::from_ref(self.page_flip_events.as_ref())
            .cast_mut()
            .cast()
    }

    pub fn add_framebuffer<'device>(
        &'device self,
        buffer: &GbmBuffer<'_, '_>,
    ) -> Result<KmsFramebuffer<'device>, KmsError> {
        let size = buffer.size();
        let format = buffer.format();
        let count = buffer.plane_count()?;
        let mut handles = [0_u32; 4];
        let mut pitches = [0_u32; 4];
        let mut offsets = [0_u32; 4];
        let modifiers = [format.modifier; 4];
        for index in 0..count {
            handles[index] =
                unsafe { ffi::gbm_bo_get_handle_for_plane(buffer.raw(), index as i32) };
            pitches[index] =
                unsafe { ffi::gbm_bo_get_stride_for_plane(buffer.raw(), index as i32) };
            offsets[index] = unsafe { ffi::gbm_bo_get_offset(buffer.raw(), index as i32) };
            if handles[index] == 0 || pitches[index] == 0 {
                return Err(KmsError::new(
                    KmsErrorKind::InvalidState,
                    "GBM returned invalid KMS plane metadata",
                ));
            }
        }
        let mut id = 0;
        let result = unsafe {
            ffi::drmModeAddFB2WithModifiers(
                self.fd.as_raw_fd(),
                size.width as u32,
                size.height as u32,
                format.fourcc,
                handles.as_ptr(),
                pitches.as_ptr(),
                offsets.as_ptr(),
                modifiers.as_ptr(),
                &mut id,
                ffi::DRM_MODE_FB_MODIFIERS,
            )
        };
        if result != 0 {
            return Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM could not create a framebuffer for the GBM buffer",
                result,
            ));
        }
        Ok(KmsFramebuffer {
            device: self,
            id: KmsFramebufferId::from_raw(id).expect("successful AddFB2 returns a nonzero id"),
        })
    }

    pub fn create_property_blob<'device>(
        &'device self,
        bytes: &[u8],
    ) -> Result<PropertyBlob<'device>, KmsError> {
        if bytes.is_empty() {
            return Err(KmsError::new(
                KmsErrorKind::InvalidState,
                "DRM property blobs must not be empty",
            ));
        }
        let mut id = 0;
        let result = unsafe {
            ffi::drmModeCreatePropertyBlob(
                self.fd.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                &mut id,
            )
        };
        if result != 0 || id == 0 {
            Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM property blob creation failed",
                result,
            ))
        } else {
            Ok(PropertyBlob { device: self, id })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn primary_modeset_request(
        &self,
        connector: KmsConnectorId,
        connector_properties: &KmsObjectProperties,
        crtc: KmsCrtcId,
        crtc_properties: &KmsObjectProperties,
        plane: KmsPlaneId,
        plane_properties: &KmsObjectProperties,
        mode_blob: u32,
        framebuffer: KmsFramebufferId,
        width: u32,
        height: u32,
    ) -> Result<AtomicRequest<'_>, KmsError> {
        if mode_blob == 0 || width == 0 || height == 0 {
            return Err(KmsError::new(
                KmsErrorKind::InvalidState,
                "atomic modeset requires a mode blob and positive extent",
            ));
        }
        let mut request = self.atomic_request()?;
        add_named(
            &mut request,
            connector.get(),
            connector_properties,
            "CRTC_ID",
            u64::from(crtc.get()),
        )?;
        add_named(
            &mut request,
            crtc.get(),
            crtc_properties,
            "MODE_ID",
            u64::from(mode_blob),
        )?;
        add_named(&mut request, crtc.get(), crtc_properties, "ACTIVE", 1)?;
        for (name, value) in [
            ("FB_ID", u64::from(framebuffer.get())),
            ("CRTC_ID", u64::from(crtc.get())),
            ("SRC_X", 0),
            ("SRC_Y", 0),
            ("SRC_W", u64::from(width) << 16),
            ("SRC_H", u64::from(height) << 16),
            ("CRTC_X", 0),
            ("CRTC_Y", 0),
            ("CRTC_W", u64::from(width)),
            ("CRTC_H", u64::from(height)),
        ] {
            add_named(&mut request, plane.get(), plane_properties, name, value)?;
        }
        Ok(request)
    }
}

fn add_named(
    request: &mut AtomicRequest<'_>,
    object: u32,
    properties: &KmsObjectProperties,
    name: &'static str,
    value: u64,
) -> Result<(), KmsError> {
    let property = properties.named(name).ok_or_else(|| {
        KmsError::new(
            KmsErrorKind::Unsupported,
            format!("DRM object has no required atomic property {name}"),
        )
    })?;
    request.add(object, property.id, value)
}

pub struct KmsFramebuffer<'device> {
    device: &'device KmsDevice,
    id: KmsFramebufferId,
}

impl KmsFramebuffer<'_> {
    pub const fn id(&self) -> KmsFramebufferId {
        self.id
    }
}

impl Drop for KmsFramebuffer<'_> {
    fn drop(&mut self) {
        let result = unsafe { ffi::drmModeRmFB(self.device.fd.as_raw_fd(), self.id.get()) };
        debug_assert_eq!(result, 0, "DRM framebuffer removal failed");
    }
}

pub struct PropertyBlob<'device> {
    device: &'device KmsDevice,
    id: u32,
}

impl PropertyBlob<'_> {
    pub const fn id(&self) -> u32 {
        self.id
    }
}

impl Drop for PropertyBlob<'_> {
    fn drop(&mut self) {
        let result =
            unsafe { ffi::drmModeDestroyPropertyBlob(self.device.fd.as_raw_fd(), self.id) };
        debug_assert_eq!(result, 0, "DRM property blob removal failed");
    }
}

pub struct AtomicRequest<'device> {
    device: &'device KmsDevice,
    raw: NonNull<ffi::drmModeAtomicReq>,
    properties: Vec<AtomicProperty>,
}

impl<'device> AtomicRequest<'device> {
    fn new(device: &'device KmsDevice) -> Result<Self, KmsError> {
        let raw = NonNull::new(unsafe { ffi::drmModeAtomicAlloc() }).ok_or_else(|| {
            KmsError::new(
                KmsErrorKind::Allocation,
                "DRM atomic request allocation failed",
            )
        })?;
        Ok(Self {
            device,
            raw,
            properties: Vec::new(),
        })
    }

    pub fn add(
        &mut self,
        object: u32,
        property: KmsPropertyId,
        value: u64,
    ) -> Result<(), KmsError> {
        if object == 0 {
            return Err(KmsError::new(
                KmsErrorKind::InvalidState,
                "DRM atomic property object must be nonzero",
            ));
        }
        let result = unsafe {
            ffi::drmModeAtomicAddProperty(self.raw.as_ptr(), object, property.get(), value)
        };
        if result < 0 {
            return Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM rejected an atomic property",
                result,
            ));
        }
        self.properties.push(AtomicProperty {
            object,
            property,
            value,
        });
        Ok(())
    }

    pub fn properties(&self) -> &[AtomicProperty] {
        &self.properties
    }

    pub fn test(&self, allow_modeset: bool) -> Result<(), KmsError> {
        self.commit_inner(
            ffi::DRM_MODE_ATOMIC_TEST_ONLY
                | if allow_modeset {
                    ffi::DRM_MODE_ATOMIC_ALLOW_MODESET
                } else {
                    0
                },
        )
    }

    pub fn commit(self, allow_modeset: bool, page_flip_event: bool) -> Result<(), KmsError> {
        let flags = (if allow_modeset {
            ffi::DRM_MODE_ATOMIC_ALLOW_MODESET
        } else {
            0
        }) | if page_flip_event {
            ffi::DRM_MODE_PAGE_FLIP_EVENT | ffi::DRM_MODE_ATOMIC_NONBLOCK
        } else {
            0
        };
        self.commit_inner(flags)
    }

    fn commit_inner(&self, flags: u32) -> Result<(), KmsError> {
        if self.properties.is_empty() {
            return Err(KmsError::new(
                KmsErrorKind::InvalidState,
                "DRM atomic request contains no properties",
            ));
        }
        let result = unsafe {
            ffi::drmModeAtomicCommit(
                self.device.fd.as_raw_fd(),
                self.raw.as_ptr(),
                flags,
                if flags & ffi::DRM_MODE_PAGE_FLIP_EVENT != 0 {
                    self.device.page_flip_user_data()
                } else {
                    std::ptr::null_mut()
                },
            )
        };
        if result != 0 {
            Err(KmsError::native(
                KmsErrorKind::Native,
                "DRM atomic commit failed",
                result,
            ))
        } else {
            Ok(())
        }
    }
}

unsafe extern "C" fn page_flip_handler(
    _fd: i32,
    _sequence: u32,
    _tv_sec: u32,
    _tv_usec: u32,
    user_data: *mut std::ffi::c_void,
) {
    let Some(events) = NonNull::new(user_data.cast::<AtomicU64>()) else {
        return;
    };
    unsafe { events.as_ref() }.fetch_add(1, Ordering::Release);
}

impl Drop for AtomicRequest<'_> {
    fn drop(&mut self) {
        unsafe { ffi::drmModeAtomicFree(self.raw.as_ptr()) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KmsErrorKind {
    Unsupported,
    Allocation,
    InvalidState,
    Native,
}

#[derive(Debug)]
pub struct KmsError {
    kind: KmsErrorKind,
    context: String,
    native_code: Option<i32>,
}

impl KmsError {
    pub fn new(kind: KmsErrorKind, context: impl Into<String>) -> Self {
        Self {
            kind,
            context: context.into(),
            native_code: None,
        }
    }

    pub fn native(kind: KmsErrorKind, context: impl Into<String>, native_code: i32) -> Self {
        Self {
            kind,
            context: context.into(),
            native_code: Some(native_code),
        }
    }

    pub const fn kind(&self) -> KmsErrorKind {
        self.kind
    }

    pub const fn native_code(&self) -> Option<i32> {
        self.native_code
    }
}

impl fmt::Display for KmsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.context)
    }
}

impl std::error::Error for KmsError {}
