use std::fmt;
use std::os::fd::{AsRawFd, OwnedFd};
use std::ptr::NonNull;

use crate::presenter_vulkan_kms::ffi;
use crate::presenter_vulkan_kms::{AtomicProperty, GbmBuffer, KmsFramebufferId, KmsPropertyId};
use crate::presenter_vulkan_kms::{KmsConnectorId, KmsCrtcId, KmsObjectProperties, KmsPlaneId};

const DRM_CLIENT_CAP_UNIVERSAL_PLANES: u64 = 2;
const DRM_CLIENT_CAP_ATOMIC: u64 = 3;

pub struct KmsDevice {
    fd: OwnedFd,
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
        Ok(Self { fd })
    }

    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }

    pub fn atomic_request(&self) -> Result<AtomicRequest<'_>, KmsError> {
        AtomicRequest::new(self)
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
                std::ptr::null_mut(),
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
