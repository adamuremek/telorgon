//! Deterministic CPU reference renderer and headless framebuffer backend.

mod renderer;

#[cfg(target_os = "linux")]
pub(crate) use renderer::SoftwareCompositeLayer;
pub use renderer::{
    SoftwareFrameContext, SoftwareReadback, SoftwareRenderer, SoftwareScene, SoftwareSurface,
    SoftwareTarget,
};
