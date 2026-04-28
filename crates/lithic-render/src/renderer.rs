use std::fmt;

use crate::core::SizeI;
use crate::{RenderFrame, RenderGraph, RenderTargetId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

pub type RenderResult<T> = Result<T, RenderError>;

/// CPU-readable RGBA output for tests, snapshots, and fallback render paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedFrame {
    pub output_id: RenderTargetId,
    pub extent: SizeI,
    pub pixels_rgba8: Vec<u8>,
}

/// GPU-presentable render packet resolved for an output without forcing CPU readback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveRenderFrame {
    pub output_id: RenderTargetId,
    pub extent: SizeI,
    pub frame: RenderFrame,
}

pub trait Renderer {
    fn register_target(&mut self, target_id: RenderTargetId, extent: SizeI) -> RenderResult<()>;
    fn registered_extent(&self, target_id: RenderTargetId) -> Option<SizeI>;
    fn render(&mut self, frame: &RenderFrame, graph: &RenderGraph) -> RenderResult<RenderedFrame>;
}
