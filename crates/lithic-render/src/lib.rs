extern crate self as lithic_render;

pub use lithic_core as core;

mod graph;
mod packet;
mod renderer;
mod software;

pub use graph::{RenderGraph, RenderNodeDescriptor, RenderResource, RenderStage};
pub use packet::{
    CornerRadii, RenderBlit, RenderDmabuf, RenderDmabufPlane, RenderFrame, RenderMaterial,
    RenderMaterialKind, RenderMaterialPass, RenderOp, RenderRect, RenderTargetId, RenderText,
};
pub use renderer::{LiveRenderFrame, RenderError, RenderResult, RenderedFrame, Renderer};
pub use software::{SoftwareRenderer, render_frame_software};
