mod software;
mod vulkan;

use crate::application_host::{AppResult, Renderer};
use crate::core::{RectI, SizeI};
use crate::presenter_vulkan_kms::GbmBuffer;

use super::scene::DesktopFrame;
use software::SoftwareDesktopRenderer;
use vulkan::VulkanDesktopRenderer;

pub(super) use vulkan::VulkanCompletion;
pub(super) use vulkan::{DmaBufPublication, DmaBufQueueResult, DmaBufRelease, DmaBufRetirement};
#[cfg(test)]
pub(super) use vulkan::{
    VULKAN_STAGING_HEADROOM_BYTES_PER_SLOT, VULKAN_STAGING_MIN_BYTES_PER_SLOT,
    vulkan_staging_budget_bytes,
};

pub(super) enum DesktopRenderResult {
    Vulkan {
        releases: Vec<DmaBufRelease>,
        discarded: Vec<DmaBufRetirement>,
    },
    Software {
        damage: RectI,
    },
}

/// The only point where the Linux desktop chooses a rendering implementation.
///
/// After construction, frames stay entirely inside the selected backend. The enum does not
/// expose either backend's scene or target types to the Wayland/KMS owner loop.
pub(super) enum DesktopRenderer {
    Vulkan(VulkanDesktopRenderer),
    Software(SoftwareDesktopRenderer),
}

impl DesktopRenderer {
    pub(super) fn new(
        renderer: Renderer,
        buffers: &[GbmBuffer<'_, '_>],
        extent: SizeI,
    ) -> AppResult<Self> {
        match renderer {
            Renderer::Vulkan => VulkanDesktopRenderer::new(buffers, extent).map(Self::Vulkan),
            Renderer::Auto => Ok(match VulkanDesktopRenderer::new(buffers, extent) {
                Ok(renderer) => Self::Vulkan(renderer),
                Err(_) => Self::Software(SoftwareDesktopRenderer::new(buffers.len())),
            }),
            Renderer::Software => Ok(Self::Software(SoftwareDesktopRenderer::new(buffers.len()))),
        }
    }

    pub(super) fn is_vulkan(&self) -> bool {
        matches!(self, Self::Vulkan(_))
    }

    pub(super) fn dma_buf_formats(&self) -> Vec<crate::compositor_wayland::DmaBufFormat> {
        match self {
            Self::Vulkan(renderer) => renderer.dma_buf_formats(),
            Self::Software(_) => Vec::new(),
        }
    }

    pub(super) fn queue_dma_buf(
        &mut self,
        publication: DmaBufPublication,
    ) -> AppResult<DmaBufQueueResult> {
        match self {
            Self::Vulkan(renderer) => renderer.queue_dma_buf(publication),
            Self::Software(_) => Err(crate::application_host::AppError::new(
                "DMA-BUF publication reached the software desktop renderer",
            )),
        }
    }

    pub(super) fn cancel_dma_buf_surface(
        &mut self,
        surface: crate::compositor_wayland::WaylandSurfaceId,
    ) -> Option<DmaBufRetirement> {
        match self {
            Self::Vulkan(renderer) => renderer.cancel_dma_buf_surface(surface),
            Self::Software(_) => None,
        }
    }

    pub(super) fn completion_event_fd(&self) -> Option<i32> {
        match self {
            Self::Vulkan(renderer) => Some(renderer.completion_event_fd()),
            Self::Software(_) => None,
        }
    }

    pub(super) fn drain_completions(&self) -> Vec<VulkanCompletion> {
        match self {
            Self::Vulkan(renderer) => renderer.drain_completions(),
            Self::Software(_) => Vec::new(),
        }
    }

    pub(super) fn render(
        &mut self,
        target_index: usize,
        frame: DesktopFrame,
    ) -> AppResult<DesktopRenderResult> {
        match self {
            Self::Vulkan(renderer) => {
                let result = renderer.render(target_index, frame)?;
                Ok(DesktopRenderResult::Vulkan {
                    releases: result.releases,
                    discarded: result.discarded,
                })
            }
            Self::Software(renderer) => renderer
                .render(target_index, frame)
                .map(|damage| DesktopRenderResult::Software { damage }),
        }
    }

    pub(super) fn software_pixels(&self) -> Option<&[u8]> {
        match self {
            Self::Vulkan(_) => None,
            Self::Software(renderer) => Some(renderer.pixels()),
        }
    }

    pub(super) fn mark_software_copied(&mut self, target_index: usize) {
        if let Self::Software(renderer) = self {
            renderer.mark_copied(target_index);
        }
    }
}
