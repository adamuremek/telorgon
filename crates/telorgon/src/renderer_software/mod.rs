//! Deterministic CPU reference renderer and headless framebuffer backend.

mod renderer;

pub use renderer::{
    SoftwareFrameContext, SoftwareReadback, SoftwareRenderer, SoftwareScene, SoftwareSurface,
    SoftwareTarget,
};
