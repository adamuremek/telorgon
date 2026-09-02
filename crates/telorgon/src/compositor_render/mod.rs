//! Buffer import bridge from Telorgon's Wayland protocol core into Telorgon render resources.
//!
//! SHM conversion is explicit and bounded. Linux DMA-BUF content remains zero-copy and is imported
//! by Telorgon's Vulkan renderer with the exact device-advertised fourcc/modifier tuple.

use std::fmt;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{
    DmaBufImporter, imported_image_id, shm_image_metadata, shm_image_resource, shm_image_update,
    transform_surface_image,
};

pub const NATIVE_WAYLAND_RENDER_IMPORT_AVAILABLE: bool = cfg!(target_os = "linux");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositorRenderError {
    context: String,
}

impl CompositorRenderError {
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            context: context.into(),
        }
    }
}

impl fmt::Display for CompositorRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.context)
    }
}

impl std::error::Error for CompositorRenderError {}
