use std::sync::Arc;
use std::time::Duration;

use crate::core::{RectI, SizeI};
use crate::render::{
    ReadbackFormat, ReadbackImage, ReadbackRequest, RenderError, RenderErrorKind, RenderResult,
};
use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::renderer_vulkan::buffer::AllocatedBuffer;
use crate::renderer_vulkan::error::internal;
use crate::renderer_vulkan::{SubmissionReceipt, VulkanFrameContext, VulkanTarget};

pub struct PendingVulkanReadback {
    device_id: u64,
    frame_id: u64,
    buffer: Arc<AllocatedBuffer>,
    region: RectI,
    row_bytes: u32,
}

pub struct VulkanReadback {
    receipt: SubmissionReceipt,
    pending: PendingVulkanReadback,
}

impl VulkanFrameContext<'_> {
    pub fn record_readback(
        &mut self,
        target: &VulkanTarget<'_>,
        request: &ReadbackRequest,
    ) -> RenderResult<PendingVulkanReadback> {
        if request.format != ReadbackFormat::Rgba8 {
            return Err(RenderError::new(
                RenderErrorKind::Unsupported,
                "Vulkan readback supports only RGBA8",
            ));
        }
        let region = request.region;
        if region.x < 0
            || region.y < 0
            || region.width <= 0
            || region.height <= 0
            || region.x.saturating_add(region.width) > target.extent.width as i32
            || region.y.saturating_add(region.height) > target.extent.height as i32
        {
            return Err(RenderError::new(
                RenderErrorKind::InvalidTarget,
                "Vulkan readback region is outside the target",
            ));
        }
        let row_bytes = (region.width as u32)
            .checked_mul(4)
            .ok_or_else(|| internal("Vulkan readback row-byte overflow"))?;
        let byte_len = (row_bytes as u64)
            .checked_mul(region.height as u64)
            .ok_or_else(|| internal("Vulkan readback size overflow"))?;
        let buffer = Arc::new(AllocatedBuffer::new(
            self.core.device.inner.clone(),
            byte_len,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
            "Telorgon readback staging",
        )?);
        let to_transfer = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .image(target.image)
            .subresource_range(color_subresource());
        unsafe {
            self.core.device.inner.raw.cmd_pipeline_barrier2(
                self.core.command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&[to_transfer]),
            );
            self.core.device.inner.raw.cmd_copy_image_to_buffer(
                self.core.command_buffer,
                target.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.raw(),
                &[vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D {
                        x: region.x,
                        y: region.y,
                        z: 0,
                    })
                    .image_extent(vk::Extent3D {
                        width: region.width as u32,
                        height: region.height as u32,
                        depth: 1,
                    })],
            );
            let host_barrier = vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)
                .buffer(buffer.raw())
                .offset(0)
                .size(byte_len);
            self.core.device.inner.raw.cmd_pipeline_barrier2(
                self.core.command_buffer,
                &vk::DependencyInfo::default().buffer_memory_barriers(&[host_barrier]),
            );
        }
        self.core.buffers.push(Arc::clone(&buffer));
        Ok(PendingVulkanReadback {
            device_id: self.core.device.inner.id,
            frame_id: self.core.frame_id,
            buffer,
            region,
            row_bytes,
        })
    }
}

impl PendingVulkanReadback {
    pub fn bind_to_submission(self, receipt: SubmissionReceipt) -> RenderResult<VulkanReadback> {
        let completion = receipt.completion();
        if completion.device_id() != self.device_id || completion.frame_id() != self.frame_id {
            return Err(RenderError::new(
                RenderErrorKind::HostContract,
                "readback and submission belong to different Vulkan frames",
            ));
        }
        Ok(VulkanReadback {
            receipt,
            pending: self,
        })
    }
}

impl VulkanReadback {
    pub fn wait(mut self, timeout: Duration) -> RenderResult<ReadbackImage> {
        self.receipt.wait(timeout)?;
        let byte_len = self.pending.row_bytes as usize * self.pending.region.height as usize;
        let pixels = self.pending.buffer.read(byte_len)?;
        Ok(ReadbackImage {
            extent: SizeI {
                width: self.pending.region.width,
                height: self.pending.region.height,
            },
            row_bytes: self.pending.row_bytes as usize,
            pixels,
        })
    }
}

fn color_subresource() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}
