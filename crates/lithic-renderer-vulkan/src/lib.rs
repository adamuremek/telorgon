use std::collections::BTreeMap;
use std::ffi::CString;
use std::io::Cursor;
use std::os::fd::{AsFd, IntoRawFd};

use ash::{Entry, vk};

use lithic_core::{ColorRgba8, RectI, SizeI};
use lithic_material::execute_material_op;
use lithic_render::{
    RenderBlit, RenderDmabuf, RenderError, RenderFrame, RenderGraph, RenderOp, RenderRect,
    RenderResult, RenderTargetId, RenderText, RenderedFrame, Renderer,
};
use lithic_text::{AtlasGlyph, FontTextRenderer, GlyphAtlasView, TextLayoutRequest, TextStyle};

const QUAD_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/renderer_builtins/quad.vert.spv"));
const SOLID_FRAG_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/renderer_builtins/solid.frag.spv"
));
const GLYPH_VERT_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/renderer_builtins/glyph.vert.spv"
));
const GLYPH_FRAG_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/renderer_builtins/glyph.frag.spv"
));
const TEXTURE_FRAG_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/renderer_builtins/texture.frag.spv"
));
const TEXTURE_VERT_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/renderer_builtins/texture.vert.spv"
));
const COLOR_ATTACHMENT_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const BGRA_COLOR_ATTACHMENT_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;
const DRM_FORMAT_MOD_LINEAR: u64 = 0;
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

pub struct VulkanRenderer {
    _entry: Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    external_memory_fd: ash::khr::external_memory_fd::Device,
    external_semaphore_fd: ash::khr::external_semaphore_fd::Device,
    queue: vk::Queue,
    queue_family_index: u32,
    command_pool: vk::CommandPool,
    render_pass: vk::RenderPass,
    bgra_render_pass: vk::RenderPass,
    glyph_descriptor_set_layout: vk::DescriptorSetLayout,
    glyph_descriptor_pool: vk::DescriptorPool,
    texture_descriptor_set_layout: vk::DescriptorSetLayout,
    texture_descriptor_pool: vk::DescriptorPool,
    quad_pipeline_layout: vk::PipelineLayout,
    quad_pipeline: vk::Pipeline,
    bgra_quad_pipeline_layout: vk::PipelineLayout,
    bgra_quad_pipeline: vk::Pipeline,
    glyph_pipeline_layout: vk::PipelineLayout,
    glyph_pipeline: vk::Pipeline,
    bgra_glyph_pipeline_layout: vk::PipelineLayout,
    bgra_glyph_pipeline: vk::Pipeline,
    texture_pipeline_layout: vk::PipelineLayout,
    texture_pipeline: vk::Pipeline,
    bgra_texture_pipeline_layout: vk::PipelineLayout,
    bgra_texture_pipeline: vk::Pipeline,
    output_targets: BTreeMap<RenderTargetId, OutputTarget>,
    surface_textures: BTreeMap<u64, GpuTexture>,
    text_renderer: FontTextRenderer,
    glyph_atlas: Option<GpuGlyphAtlas>,
    pending_external_submission: Option<PendingExternalSubmission>,
    stats: VulkanRendererStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VulkanRendererStats {
    pub external_render_submissions: u64,
    pub dmabuf_imports: u64,
    pub texture_cache_hits: u64,
    pub texture_uploads: u64,
    pub cpu_fallback_segments: u64,
}

struct OutputTarget {
    extent: SizeI,
    image: vk::Image,
    image_memory: vk::DeviceMemory,
    image_view: vk::ImageView,
    framebuffer: vk::Framebuffer,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    byte_len: vk::DeviceSize,
}

struct ExternalImageTarget {
    extent: SizeI,
    image: vk::Image,
    framebuffer: vk::Framebuffer,
    format: vk::Format,
    render_pass: vk::RenderPass,
}

struct GpuGlyphAtlas {
    width_px: i32,
    height_px: i32,
    version: u64,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
}

struct GpuTexture {
    width_px: i32,
    height_px: i32,
    content_version: u64,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    sampler: vk::Sampler,
    layout: vk::ImageLayout,
    source_fingerprint: u64,
}

struct StagingBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

struct GpuBlitUpload {
    op_index: usize,
    staging: StagingBuffer,
    copies: Vec<vk::BufferCopy>,
}

struct PendingExternalSubmission {
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    glyph_descriptor_set: Option<vk::DescriptorSet>,
    texture_descriptor_sets: Vec<vk::DescriptorSet>,
    wait_semaphores: Vec<vk::Semaphore>,
}

#[derive(Clone)]
enum GraphicsDrawOp {
    Rect(RenderRect),
    Glyph(AtlasGlyph),
    Texture(RenderBlit),
}

#[derive(Copy, Clone)]
struct GraphicsPipelines {
    quad_layout: vk::PipelineLayout,
    quad: vk::Pipeline,
    glyph_layout: vk::PipelineLayout,
    glyph: vk::Pipeline,
    texture_layout: vk::PipelineLayout,
    texture: vk::Pipeline,
}

impl VulkanRenderer {
    pub fn new() -> RenderResult<Self> {
        let entry = unsafe { Entry::load() }.map_err(|error| {
            RenderError::new(format!(
                "failed to load Vulkan loader with ash::Entry::load: {error}"
            ))
        })?;
        let instance = create_instance(&entry)?;
        let (physical_device, queue_family_index) = select_physical_device(&instance)?;
        let (device, queue) =
            create_device_and_queue(&instance, physical_device, queue_family_index)?;
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(&instance, &device);
        let external_semaphore_fd =
            ash::khr::external_semaphore_fd::Device::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_family_index)?;
        let render_pass = create_color_render_pass(&device)?;
        let bgra_render_pass = create_color_render_pass_with_format(&device, BGRA_COLOR_ATTACHMENT_FORMAT)?;
        let glyph_descriptor_set_layout = create_glyph_descriptor_set_layout(&device)?;
        let glyph_descriptor_pool = create_glyph_descriptor_pool(&device)?;
        let texture_descriptor_set_layout = create_glyph_descriptor_set_layout(&device)?;
        let texture_descriptor_pool = create_glyph_descriptor_pool(&device)?;
        let (quad_pipeline_layout, quad_pipeline) =
            create_solid_quad_pipeline(&device, render_pass)?;
        let (bgra_quad_pipeline_layout, bgra_quad_pipeline) =
            create_solid_quad_pipeline(&device, bgra_render_pass)?;
        let (glyph_pipeline_layout, glyph_pipeline) =
            create_glyph_pipeline(&device, render_pass, glyph_descriptor_set_layout)?;
        let (bgra_glyph_pipeline_layout, bgra_glyph_pipeline) =
            create_glyph_pipeline(&device, bgra_render_pass, glyph_descriptor_set_layout)?;
        let (texture_pipeline_layout, texture_pipeline) =
            create_texture_pipeline(&device, render_pass, texture_descriptor_set_layout)?;
        let (bgra_texture_pipeline_layout, bgra_texture_pipeline) =
            create_texture_pipeline(&device, bgra_render_pass, texture_descriptor_set_layout)?;

        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            external_memory_fd,
            external_semaphore_fd,
            queue,
            queue_family_index,
            command_pool,
            render_pass,
            bgra_render_pass,
            glyph_descriptor_set_layout,
            glyph_descriptor_pool,
            texture_descriptor_set_layout,
            texture_descriptor_pool,
            quad_pipeline_layout,
            quad_pipeline,
            bgra_quad_pipeline_layout,
            bgra_quad_pipeline,
            glyph_pipeline_layout,
            glyph_pipeline,
            bgra_glyph_pipeline_layout,
            bgra_glyph_pipeline,
            texture_pipeline_layout,
            texture_pipeline,
            bgra_texture_pipeline_layout,
            bgra_texture_pipeline,
            output_targets: BTreeMap::new(),
            surface_textures: BTreeMap::new(),
            text_renderer: FontTextRenderer::new().map_err(text_error)?,
            glyph_atlas: None,
            pending_external_submission: None,
            stats: VulkanRendererStats::default(),
        })
    }

    pub fn take_stats(&mut self) -> VulkanRendererStats {
        std::mem::take(&mut self.stats)
    }

    pub fn wait_for_pending_external_submission(&mut self) -> RenderResult<()> {
        self.cleanup_pending_external_submission()
    }

    pub fn register_target(
        &mut self,
        target_id: RenderTargetId,
        extent: SizeI,
    ) -> RenderResult<()> {
        if let Some(old_target) = self.output_targets.remove(&target_id) {
            unsafe { self.destroy_output_target(old_target) };
        }

        let target = self.create_output_target(extent)?;
        self.output_targets.insert(target_id, target);
        Ok(())
    }

    pub fn registered_extent(&self, target_id: RenderTargetId) -> Option<SizeI> {
        self.output_targets
            .get(&target_id)
            .map(|target| target.extent)
    }

    pub fn entry(&self) -> &Entry {
        &self._entry
    }

    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    pub fn queue(&self) -> vk::Queue {
        self.queue
    }

    pub fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    pub fn supported_dmabuf_formats(&self) -> Vec<(u32, u64)> {
        let discovered: Vec<_> = dmabuf_format_candidates()
            .iter()
            .copied()
            .flat_map(|(drm_format, vk_format)| {
                self.supported_dmabuf_modifiers(vk_format)
                    .into_iter()
                    .map(move |modifier| (drm_format, modifier))
            })
            .collect();

        if discovered.is_empty() {
            return fallback_linear_dmabuf_formats();
        }
        discovered
    }

    fn supported_dmabuf_modifiers(&self, format: vk::Format) -> Vec<u64> {
        let mut modifier_list = vk::DrmFormatModifierPropertiesListEXT::default();
        let mut properties = vk::FormatProperties2::default().push_next(&mut modifier_list);
        unsafe {
            self.instance.get_physical_device_format_properties2(
                self.physical_device,
                format,
                &mut properties,
            );
        }

        if modifier_list.drm_format_modifier_count == 0 {
            return Vec::new();
        }

        let mut modifiers = vec![
            vk::DrmFormatModifierPropertiesEXT::default();
            modifier_list.drm_format_modifier_count as usize
        ];
        let mut modifier_list = vk::DrmFormatModifierPropertiesListEXT::default()
            .drm_format_modifier_properties(&mut modifiers);
        let mut properties = vk::FormatProperties2::default().push_next(&mut modifier_list);
        unsafe {
            self.instance.get_physical_device_format_properties2(
                self.physical_device,
                format,
                &mut properties,
            );
        }

        modifiers
            .iter()
            .filter(|modifier| {
                modifier.drm_format_modifier_plane_count == 1
                && modifier
                    .drm_format_modifier_tiling_features
                    .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
                    && self.supports_dmabuf_import(format, modifier.drm_format_modifier)
            })
            .map(|modifier| modifier.drm_format_modifier)
            .collect()
    }

    fn supports_dmabuf_import(&self, format: vk::Format, modifier: u64) -> bool {
        let mut modifier_info = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
            .drm_format_modifier(modifier)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let mut external_info = vk::PhysicalDeviceExternalImageFormatInfo::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let format_info = vk::PhysicalDeviceImageFormatInfo2::default()
            .push_next(&mut modifier_info)
            .push_next(&mut external_info)
            .format(format)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED);
        let mut external_properties = vk::ExternalImageFormatProperties::default();
        let mut image_properties =
            vk::ImageFormatProperties2::default().push_next(&mut external_properties);

        let result = unsafe {
            self.instance.get_physical_device_image_format_properties2(
                self.physical_device,
                &format_info,
                &mut image_properties,
            )
        };
        result.is_ok()
            && external_properties
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    }

    pub fn render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    pub fn render_to_image(
        &mut self,
        frame: &RenderFrame,
        image: vk::Image,
        extent: SizeI,
    ) -> RenderResult<()> {
        if frame.extent != extent {
            return Err(RenderError::new(format!(
                "external image extent {}x{} does not match render frame {}x{}",
                extent.width, extent.height, frame.extent.width, frame.extent.height
            )));
        }
        if !graphics_frame_eligible(frame) {
            return Err(RenderError::new(
                "external image rendering currently supports rect/text/blit frames only",
            ));
        }

        let draw_ops = self.prepare_graphics_draw_ops(frame)?;
        if draw_ops
            .iter()
            .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
        {
            self.sync_gpu_glyph_atlas()?;
        }

        let image_view = self.create_external_image_view(image, COLOR_ATTACHMENT_FORMAT)?;
        let render_pass = self.render_pass_for_format(COLOR_ATTACHMENT_FORMAT)?;
        let framebuffer = self.create_external_framebuffer(image_view, extent, render_pass)?;
        let target = ExternalImageTarget {
            extent,
            image,
            framebuffer,
            format: COLOR_ATTACHMENT_FORMAT,
            render_pass,
        };

        let result = self.submit_external_graphics_frame(&target, frame, &draw_ops);
        unsafe {
            self.device.destroy_framebuffer(framebuffer, None);
            self.device.destroy_image_view(image_view, None);
        }
        result
    }

    pub fn render_to_image_signal(
        &mut self,
        frame: &RenderFrame,
        image: vk::Image,
        extent: SizeI,
        signal_semaphore: vk::Semaphore,
    ) -> RenderResult<()> {
        self.render_to_image_signal_with_format(
            frame,
            image,
            extent,
            COLOR_ATTACHMENT_FORMAT,
            signal_semaphore,
        )
    }

    pub fn render_to_image_signal_with_format(
        &mut self,
        frame: &RenderFrame,
        image: vk::Image,
        extent: SizeI,
        format: vk::Format,
        signal_semaphore: vk::Semaphore,
    ) -> RenderResult<()> {
        self.stats.external_render_submissions += 1;
        self.cleanup_pending_external_submission()?;
        if frame.extent != extent {
            return Err(RenderError::new(format!(
                "external image extent {}x{} does not match render frame {}x{}",
                extent.width, extent.height, frame.extent.width, frame.extent.height
            )));
        }
        if !graphics_frame_eligible(frame) {
            return Err(RenderError::new(
                "external image rendering currently supports rect/text/blit frames only",
            ));
        }

        let draw_ops = self.prepare_graphics_draw_ops(frame)?;
        if draw_ops
            .iter()
            .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
        {
            self.sync_gpu_glyph_atlas()?;
        }

        let render_pass = self.render_pass_for_format(format)?;
        let image_view = self.create_external_image_view(image, format)?;
        let framebuffer = self.create_external_framebuffer(image_view, extent, render_pass)?;
        let target = ExternalImageTarget {
            extent,
            image,
            framebuffer,
            format,
            render_pass,
        };
        let result = self.submit_external_graphics_frame_signal(
            &target,
            frame,
            &draw_ops,
            signal_semaphore,
        );
        unsafe {
            self.device.destroy_framebuffer(framebuffer, None);
            self.device.destroy_image_view(image_view, None);
        }
        result
    }

    fn cleanup_pending_external_submission(&mut self) -> RenderResult<()> {
        let Some(pending) = self.pending_external_submission.take() else {
            return Ok(());
        };
        unsafe {
            self.device
                .wait_for_fences(std::slice::from_ref(&pending.fence), true, u64::MAX)
                .map_err(vk_error("wait_for_fences(external_render)"))?;
            self.device.destroy_fence(pending.fence, None);
            self.device.free_command_buffers(
                self.command_pool,
                std::slice::from_ref(&pending.command_buffer),
            );
            if let Some(descriptor_set) = pending.glyph_descriptor_set {
                let _ = self.device.free_descriptor_sets(
                    self.glyph_descriptor_pool,
                    std::slice::from_ref(&descriptor_set),
                );
            }
            if !pending.texture_descriptor_sets.is_empty() {
                let _ = self.device.free_descriptor_sets(
                    self.texture_descriptor_pool,
                    &pending.texture_descriptor_sets,
                );
            }
            for semaphore in pending.wait_semaphores {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        Ok(())
    }

    fn render_pass_for_format(&self, format: vk::Format) -> RenderResult<vk::RenderPass> {
        match format {
            COLOR_ATTACHMENT_FORMAT => Ok(self.render_pass),
            BGRA_COLOR_ATTACHMENT_FORMAT => Ok(self.bgra_render_pass),
            _ => Err(RenderError::new(format!(
                "unsupported external color attachment format {format:?}"
            ))),
        }
    }

    fn graphics_pipelines_for_format(&self, format: vk::Format) -> RenderResult<GraphicsPipelines> {
        match format {
            COLOR_ATTACHMENT_FORMAT => Ok(GraphicsPipelines {
                quad_layout: self.quad_pipeline_layout,
                quad: self.quad_pipeline,
                glyph_layout: self.glyph_pipeline_layout,
                glyph: self.glyph_pipeline,
                texture_layout: self.texture_pipeline_layout,
                texture: self.texture_pipeline,
            }),
            BGRA_COLOR_ATTACHMENT_FORMAT => Ok(GraphicsPipelines {
                quad_layout: self.bgra_quad_pipeline_layout,
                quad: self.bgra_quad_pipeline,
                glyph_layout: self.bgra_glyph_pipeline_layout,
                glyph: self.bgra_glyph_pipeline,
                texture_layout: self.bgra_texture_pipeline_layout,
                texture: self.bgra_texture_pipeline,
            }),
            _ => Err(RenderError::new(format!(
                "unsupported external graphics pipeline format {format:?}"
            ))),
        }
    }

    fn create_external_image_view(
        &self,
        image: vk::Image,
        format: vk::Format,
    ) -> RenderResult<vk::ImageView> {
        let image_view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        unsafe {
            self.device
                .create_image_view(&image_view_info, None)
                .map_err(vk_error("create_image_view(external)"))
        }
    }

    fn create_external_framebuffer(
        &self,
        image_view: vk::ImageView,
        extent: SizeI,
        render_pass: vk::RenderPass,
    ) -> RenderResult<vk::Framebuffer> {
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(std::slice::from_ref(&image_view))
            .width(extent.width as u32)
            .height(extent.height as u32)
            .layers(1);
        unsafe {
            self.device
                .create_framebuffer(&framebuffer_info, None)
                .map_err(vk_error("create_framebuffer(external)"))
        }
    }

    fn create_output_target(&self, extent: SizeI) -> RenderResult<OutputTarget> {
        if extent.width <= 0 || extent.height <= 0 {
            return Err(RenderError::new(format!(
                "output extent must be positive, got {}x{}",
                extent.width, extent.height
            )));
        }

        let byte_len = extent.width as u64 * extent.height as u64 * 4;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(COLOR_ATTACHMENT_FORMAT)
            .extent(vk::Extent3D {
                width: extent.width as u32,
                height: extent.height as u32,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe {
            self.device
                .create_image(&image_info, None)
                .map_err(vk_error("create_image"))?
        };
        let image_memory_requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let image_memory_type_index = self.find_memory_type_index(
            image_memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let image_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(image_memory_requirements.size)
            .memory_type_index(image_memory_type_index);
        let image_memory = unsafe {
            self.device
                .allocate_memory(&image_allocate_info, None)
                .map_err(vk_error("allocate_memory(image)"))?
        };
        unsafe {
            self.device
                .bind_image_memory(image, image_memory, 0)
                .map_err(vk_error("bind_image_memory"))?;
        }

        let image_view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(COLOR_ATTACHMENT_FORMAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe {
            self.device
                .create_image_view(&image_view_info, None)
                .map_err(vk_error("create_image_view(output)"))?
        };

        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(self.render_pass)
            .attachments(std::slice::from_ref(&image_view))
            .width(extent.width as u32)
            .height(extent.height as u32)
            .layers(1);
        let framebuffer = unsafe {
            self.device
                .create_framebuffer(&framebuffer_info, None)
                .map_err(vk_error("create_framebuffer"))?
        };

        let buffer_info = vk::BufferCreateInfo::default()
            .size(byte_len)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            self.device
                .create_buffer(&buffer_info, None)
                .map_err(vk_error("create_buffer"))?
        };

        let memory_requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = self.find_memory_type_index(
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(vk_error("allocate_memory"))?
        };

        unsafe {
            self.device
                .bind_buffer_memory(buffer, memory, 0)
                .map_err(vk_error("bind_buffer_memory"))?;
        }

        Ok(OutputTarget {
            extent,
            image,
            image_memory,
            image_view,
            framebuffer,
            buffer,
            memory,
            byte_len,
        })
    }

    unsafe fn destroy_output_target(&self, target: OutputTarget) {
        unsafe {
            self.device.destroy_framebuffer(target.framebuffer, None);
            self.device.destroy_image_view(target.image_view, None);
            self.device.destroy_image(target.image, None);
            self.device.free_memory(target.image_memory, None);
            self.device.destroy_buffer(target.buffer, None);
            self.device.free_memory(target.memory, None);
        }
    }

    unsafe fn destroy_glyph_atlas(&self, atlas: GpuGlyphAtlas) {
        unsafe {
            self.device.destroy_sampler(atlas.sampler, None);
            self.device.destroy_image_view(atlas.view, None);
            self.device.destroy_image(atlas.image, None);
            self.device.free_memory(atlas.memory, None);
        }
    }

    unsafe fn destroy_gpu_texture(&self, texture: GpuTexture) {
        unsafe {
            self.device.destroy_sampler(texture.sampler, None);
            self.device.destroy_image_view(texture.view, None);
            self.device.destroy_image(texture.image, None);
            self.device.free_memory(texture.memory, None);
        }
    }

    unsafe fn destroy_staging_buffer(&self, buffer: StagingBuffer) {
        unsafe {
            self.device.destroy_buffer(buffer.buffer, None);
            self.device.free_memory(buffer.memory, None);
        }
    }

    fn create_staging_buffer(&self, bytes: &[u8]) -> RenderResult<StagingBuffer> {
        let byte_len = bytes.len().max(1) as vk::DeviceSize;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(byte_len)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            self.device
                .create_buffer(&buffer_info, None)
                .map_err(vk_error("create_buffer"))?
        };
        let memory_requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = self.find_memory_type_index(
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(vk_error("allocate_memory"))?
        };

        unsafe {
            self.device
                .bind_buffer_memory(buffer, memory, 0)
                .map_err(vk_error("bind_buffer_memory"))?;
            let mapped = self
                .device
                .map_memory(memory, 0, byte_len, vk::MemoryMapFlags::empty())
                .map_err(vk_error("map_memory"))?;
            let dst = std::slice::from_raw_parts_mut(mapped.cast::<u8>(), byte_len as usize);
            dst[..bytes.len()].copy_from_slice(bytes);
            if bytes.is_empty() {
                dst[0] = 0;
            }
            self.device.unmap_memory(memory);
        }

        Ok(StagingBuffer { buffer, memory })
    }

    fn create_gpu_glyph_atlas(&self, width_px: i32, height_px: i32) -> RenderResult<GpuGlyphAtlas> {
        if width_px <= 0 || height_px <= 0 {
            return Err(RenderError::new(format!(
                "glyph atlas extent must be positive, got {width_px}x{height_px}"
            )));
        }

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
            .extent(vk::Extent3D {
                width: width_px as u32,
                height: height_px as u32,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            self.device
                .create_image(&image_info, None)
                .map_err(vk_error("create_image"))?
        };
        let memory_requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let memory_type_index = self.find_memory_type_index(
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(vk_error("allocate_memory"))?
        };
        unsafe {
            self.device
                .bind_image_memory(image, memory, 0)
                .map_err(vk_error("bind_image_memory"))?;
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe {
            self.device
                .create_image_view(&view_info, None)
                .map_err(vk_error("create_image_view"))?
        };

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(0.0);
        let sampler = unsafe {
            self.device
                .create_sampler(&sampler_info, None)
                .map_err(vk_error("create_sampler"))?
        };

        Ok(GpuGlyphAtlas {
            width_px,
            height_px,
            version: 0,
            image,
            memory,
            view,
            sampler,
        })
    }

    fn sync_gpu_glyph_atlas(&mut self) -> RenderResult<()> {
        let snapshot = {
            let atlas = self.text_renderer.atlas();
            let needs_upload = self.glyph_atlas.as_ref().is_none_or(|gpu_atlas| {
                gpu_atlas.version != atlas.version
                    || gpu_atlas.width_px != atlas.width_px
                    || gpu_atlas.height_px != atlas.height_px
            });
            if !needs_upload {
                return Ok(());
            }

            (
                atlas.width_px,
                atlas.height_px,
                atlas.version,
                atlas.pixels_a8.to_vec(),
            )
        };

        self.upload_gpu_glyph_atlas(snapshot.0, snapshot.1, snapshot.2, &snapshot.3)
    }

    fn upload_gpu_glyph_atlas(
        &mut self,
        width_px: i32,
        height_px: i32,
        version: u64,
        pixels_a8: &[u8],
    ) -> RenderResult<()> {
        let mut old_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        let needs_recreate = self
            .glyph_atlas
            .as_ref()
            .is_none_or(|atlas| atlas.width_px != width_px || atlas.height_px != height_px);
        if needs_recreate {
            if let Some(old_atlas) = self.glyph_atlas.take() {
                unsafe { self.destroy_glyph_atlas(old_atlas) };
            }
            self.glyph_atlas = Some(self.create_gpu_glyph_atlas(width_px, height_px)?);
            old_layout = vk::ImageLayout::UNDEFINED;
        }

        let atlas = self
            .glyph_atlas
            .as_ref()
            .ok_or_else(|| RenderError::new("glyph atlas was not created"))?;
        let staging = self.create_staging_buffer(pixels_a8)?;
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;

        let record_result = self.record_glyph_atlas_upload(
            command_buffer,
            atlas.image,
            staging.buffer,
            width_px,
            height_px,
            old_layout,
        );
        let submit_result = match record_result {
            Ok(()) => self.submit_and_wait(command_buffer),
            Err(error) => Err(error),
        };

        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
            self.destroy_staging_buffer(staging);
        }

        submit_result?;

        if let Some(atlas) = &mut self.glyph_atlas {
            atlas.version = version;
        }
        Ok(())
    }

    fn record_glyph_atlas_upload(
        &self,
        command_buffer: vk::CommandBuffer,
        image: vk::Image,
        staging_buffer: vk::Buffer,
        width_px: i32,
        height_px: i32,
        old_layout: vk::ImageLayout,
    ) -> RenderResult<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer"))?;
        }

        let (src_stage, src_access) = if old_layout == vk::ImageLayout::UNDEFINED {
            (
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::AccessFlags::empty(),
            )
        } else {
            (
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ,
            )
        };

        let transfer_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(src_access)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(glyph_atlas_subresource_range());
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                src_stage,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&transfer_barrier),
            );
        }

        let copy_region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: width_px as u32,
                height: height_px as u32,
                depth: 1,
            });
        unsafe {
            self.device.cmd_copy_buffer_to_image(
                command_buffer,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&copy_region),
            );
        }

        let shader_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(glyph_atlas_subresource_range());
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&shader_barrier),
            );
            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer"))?;
        }

        Ok(())
    }

    fn sync_gpu_texture(&mut self, blit: &RenderBlit) -> RenderResult<()> {
        let width_px = blit.src_width.max(1);
        let height_px = (blit.pixels_rgba8.len() as i32 / (width_px * 4)).max(1);
        let needs_upload = self
            .surface_textures
            .get(&blit.texture_key)
            .is_none_or(|texture| {
                texture.content_version != blit.content_version
                    || texture.width_px != width_px
                    || texture.height_px != height_px
                    || texture.source_fingerprint != blit_texture_fingerprint(blit)
            });
        if !needs_upload {
            self.stats.texture_cache_hits += 1;
            return Ok(());
        }

        self.stats.texture_uploads += 1;
        let texture = if blit.dmabuf.is_some() {
            self.create_dmabuf_gpu_texture(blit)?
        } else {
            self.create_gpu_texture(blit)?
        };
        if let Some(old_texture) = self.surface_textures.insert(blit.texture_key, texture) {
            unsafe {
                self.destroy_gpu_texture(old_texture);
            }
        }
        Ok(())
    }

    fn create_dmabuf_gpu_texture(&mut self, blit: &RenderBlit) -> RenderResult<GpuTexture> {
        self.stats.dmabuf_imports += 1;
        let dmabuf = blit
            .dmabuf
            .as_ref()
            .ok_or_else(|| RenderError::new("dmabuf blit did not include dmabuf metadata"))?;
        if dmabuf.planes.len() != 1 {
            return Err(RenderError::new(
                "Vulkan dmabuf import currently supports one-plane buffers only",
            ));
        }
        let plane = &dmabuf.planes[0];
        let import_format = dmabuf_vk_format(dmabuf.format)?;
        let drm_modifier = if dmabuf.modifier == DRM_FORMAT_MOD_INVALID {
            DRM_FORMAT_MOD_LINEAR
        } else {
            dmabuf.modifier
        };
        if !self.supports_dmabuf_import(import_format.format, drm_modifier) {
            return Err(RenderError::new(format!(
                "Vulkan dmabuf import does not support format 0x{:08x} modifier 0x{:016x}",
                dmabuf.format, drm_modifier
            )));
        }
        let plane_layout = vk::SubresourceLayout::default()
            .offset(plane.offset as vk::DeviceSize)
            .size((dmabuf.height as vk::DeviceSize).saturating_mul(plane.stride as vk::DeviceSize))
            .row_pitch(plane.stride as vk::DeviceSize)
            .array_pitch(0)
            .depth_pitch(0);
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(drm_modifier)
            .plane_layouts(std::slice::from_ref(&plane_layout));
        let image_info = vk::ImageCreateInfo::default()
            .push_next(&mut external_info)
            .push_next(&mut modifier_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(import_format.format)
            .extent(vk::Extent3D {
                width: dmabuf.width as u32,
                height: dmabuf.height as u32,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::PREINITIALIZED);
        let image = unsafe {
            self.device
                .create_image(&image_info, None)
                .map_err(vk_error("create_image(dmabuf_texture)"))?
        };

        let import_fd = plane
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|error| RenderError::new(format!("clone dmabuf fd failed: {error}")))?
            .into_raw_fd();
        let mut memory_fd_properties = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            self.external_memory_fd
                .get_memory_fd_properties(
                    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                    import_fd,
                    &mut memory_fd_properties,
                )
                .map_err(vk_error("get_memory_fd_properties(dmabuf)"))?;
        }
        let memory_requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let memory_type_index = self.find_memory_type_index(
            memory_requirements.memory_type_bits & memory_fd_properties.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(import_fd);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .push_next(&mut import_info)
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(vk_error("allocate_memory(dmabuf_texture)"))?
        };
        unsafe {
            self.device
                .bind_image_memory(image, memory, 0)
                .map_err(vk_error("bind_image_memory(dmabuf_texture)"))?;
        }
        self.transition_dmabuf_texture_image(image)?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(import_format.format)
            .components(import_format.components)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe {
            self.device
                .create_image_view(&view_info, None)
                .map_err(vk_error("create_image_view(dmabuf_texture)"))?
        };
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(0.0);
        let sampler = unsafe {
            self.device
                .create_sampler(&sampler_info, None)
                .map_err(vk_error("create_sampler(dmabuf_texture)"))?
        };

        Ok(GpuTexture {
            width_px: dmabuf.width,
            height_px: dmabuf.height,
            content_version: blit.content_version,
            image,
            memory,
            view,
            sampler,
            layout: vk::ImageLayout::GENERAL,
            source_fingerprint: blit_texture_fingerprint(blit),
        })
    }

    fn transition_dmabuf_texture_image(&self, image: vk::Image) -> RenderResult<()> {
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer(dmabuf_transition)"))?;
            let barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::PREINITIALIZED)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_access_mask(vk::AccessFlags::HOST_WRITE | vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::HOST | vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );
            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer(dmabuf_transition)"))?;
        }
        let submit_result = self.submit_and_wait(command_buffer);
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
        }
        submit_result
    }

    fn create_gpu_texture(&self, blit: &RenderBlit) -> RenderResult<GpuTexture> {
        let width_px = blit.src_width.max(1);
        let height_px = (blit.pixels_rgba8.len() as i32 / (width_px * 4)).max(1);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(COLOR_ATTACHMENT_FORMAT)
            .extent(vk::Extent3D {
                width: width_px as u32,
                height: height_px as u32,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe {
            self.device
                .create_image(&image_info, None)
                .map_err(vk_error("create_image(texture)"))?
        };
        let memory_requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let memory_type_index = self.find_memory_type_index(
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(vk_error("allocate_memory(texture)"))?
        };
        unsafe {
            self.device
                .bind_image_memory(image, memory, 0)
                .map_err(vk_error("bind_image_memory(texture)"))?;
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(COLOR_ATTACHMENT_FORMAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe {
            self.device
                .create_image_view(&view_info, None)
                .map_err(vk_error("create_image_view(texture)"))?
        };
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(0.0);
        let sampler = unsafe {
            self.device
                .create_sampler(&sampler_info, None)
                .map_err(vk_error("create_sampler(texture)"))?
        };

        let staging = match self.create_staging_buffer(&blit.pixels_rgba8) {
            Ok(staging) => staging,
            Err(error) => {
                unsafe {
                    self.device.destroy_sampler(sampler, None);
                    self.device.destroy_image_view(view, None);
                    self.device.destroy_image(image, None);
                    self.device.free_memory(memory, None);
                }
                return Err(error);
            }
        };
        let upload_result = self.upload_texture_image(image, width_px, height_px, staging.buffer);
        unsafe {
            self.destroy_staging_buffer(staging);
        }
        if let Err(error) = upload_result {
            unsafe {
                self.device.destroy_sampler(sampler, None);
                self.device.destroy_image_view(view, None);
                self.device.destroy_image(image, None);
                self.device.free_memory(memory, None);
            }
            return Err(error);
        }

        Ok(GpuTexture {
            width_px,
            height_px,
            content_version: blit.content_version,
            image,
            memory,
            view,
            sampler,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            source_fingerprint: blit_texture_fingerprint(blit),
        })
    }

    fn upload_texture_image(
        &self,
        image: vk::Image,
        width_px: i32,
        height_px: i32,
        staging_buffer: vk::Buffer,
    ) -> RenderResult<()> {
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let record_result = self.record_texture_upload(command_buffer, image, width_px, height_px, staging_buffer);
        let submit_result = match record_result {
            Ok(()) => self.submit_and_wait(command_buffer),
            Err(error) => Err(error),
        };
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
        }
        submit_result
    }

    fn record_texture_upload(
        &self,
        command_buffer: vk::CommandBuffer,
        image: vk::Image,
        width_px: i32,
        height_px: i32,
        staging_buffer: vk::Buffer,
    ) -> RenderResult<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer(texture_upload)"))?;

            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&to_transfer),
            );

            let copy_region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width: width_px as u32,
                    height: height_px as u32,
                    depth: 1,
                });
            self.device.cmd_copy_buffer_to_image(
                command_buffer,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&copy_region),
            );

            let to_shader_read = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&to_shader_read),
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer(texture_upload)"))?;
        }
        Ok(())
    }


    fn submit_and_wait(&self, command_buffer: vk::CommandBuffer) -> RenderResult<()> {
        self.submit_and_wait_with_semaphores(command_buffer, &[])
    }

    fn submit_and_wait_with_semaphores(
        &self,
        command_buffer: vk::CommandBuffer,
        wait_semaphores: &[vk::Semaphore],
    ) -> RenderResult<()> {
        let wait_stages = vec![vk::PipelineStageFlags::FRAGMENT_SHADER; wait_semaphores.len()];
        let submit_info =
            vk::SubmitInfo::default()
                .wait_semaphores(wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(std::slice::from_ref(&command_buffer));
        unsafe {
            self.device
                .queue_submit(
                    self.queue,
                    std::slice::from_ref(&submit_info),
                    vk::Fence::null(),
                )
                .map_err(vk_error("queue_submit"))?;
            self.device
                .queue_wait_idle(self.queue)
                .map_err(vk_error("queue_wait_idle"))?;
        }
        Ok(())
    }

    fn dmabuf_wait_semaphores(&self, draw_ops: &[GraphicsDrawOp]) -> RenderResult<Vec<vk::Semaphore>> {
        let mut semaphores = Vec::new();
        for op in draw_ops {
            let GraphicsDrawOp::Texture(blit) = op else {
                continue;
            };
            let Some(dmabuf) = blit.dmabuf.as_ref() else {
                continue;
            };
            let Some(acquire_fence) = dmabuf.acquire_fence.as_ref() else {
                continue;
            };
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            let semaphore = unsafe {
                self.device
                    .create_semaphore(&semaphore_info, None)
                    .map_err(vk_error("create_semaphore(dmabuf_acquire)"))?
            };
            let import_fd = acquire_fence
                .as_fd()
                .try_clone_to_owned()
                .map_err(|error| RenderError::new(format!("clone acquire fence fd failed: {error}")))?
                .into_raw_fd();
            let import_info = vk::ImportSemaphoreFdInfoKHR::default()
                .semaphore(semaphore)
                .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
                .fd(import_fd);
            unsafe {
                self.external_semaphore_fd
                    .import_semaphore_fd(&import_info)
                    .map_err(vk_error("import_semaphore_fd(dmabuf_acquire)"))?;
            }
            semaphores.push(semaphore);
        }
        Ok(semaphores)
    }

    #[cfg(test)]
    fn gpu_glyph_atlas_version(&self) -> Option<u64> {
        self.glyph_atlas.as_ref().map(|atlas| atlas.version)
    }

    fn find_memory_type_index(
        &self,
        memory_type_bits: u32,
        required: vk::MemoryPropertyFlags,
    ) -> RenderResult<u32> {
        let memory_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };

        for index in 0..memory_properties.memory_type_count {
            let supported = (memory_type_bits & (1u32 << index)) != 0;
            if !supported {
                continue;
            }

            let flags = memory_properties.memory_types[index as usize].property_flags;
            if flags.contains(required) {
                return Ok(index);
            }
        }

        Err(RenderError::new(format!(
            "could not find Vulkan memory type matching {required:?}"
        )))
    }

    fn prepare_graphics_draw_ops(
        &mut self,
        frame: &RenderFrame,
    ) -> RenderResult<Vec<GraphicsDrawOp>> {
        let mut draw_ops = Vec::new();
        for op in &frame.ops {
            match op {
                RenderOp::Rect(rect) => draw_ops.push(GraphicsDrawOp::Rect(*rect)),
                RenderOp::Text(text) => {
                    let prepared = self
                        .text_renderer
                        .prepare_text(TextLayoutRequest {
                            rect: text.rect,
                            text: &text.text,
                            style: TextStyle::new(text.color, text.font_size_px),
                        })
                        .map_err(text_error)?;
                    draw_ops.extend(prepared.glyphs.into_iter().map(GraphicsDrawOp::Glyph));
                }
                RenderOp::Blit(blit) => {
                    self.sync_gpu_texture(blit)?;
                    draw_ops.push(GraphicsDrawOp::Texture(blit.clone()));
                }
                RenderOp::Material(_) => {}
            }
        }

        Ok(draw_ops)
    }

    fn submit_graphics_frame(
        &self,
        target: &OutputTarget,
        frame: &RenderFrame,
        draw_ops: &[GraphicsDrawOp],
    ) -> RenderResult<()> {
        let glyph_descriptor_set = if draw_ops
            .iter()
            .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
        {
            Some(self.allocate_glyph_descriptor_set()?)
        } else {
            None
        };
        let texture_descriptor_sets = self.allocate_texture_descriptor_sets(draw_ops)?;
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let record_result = self.record_graphics_frame(
            command_buffer,
            target,
            frame,
            draw_ops,
            glyph_descriptor_set,
            &texture_descriptor_sets,
        );
        let submit_result = match record_result {
            Ok(()) => self.submit_and_wait(command_buffer),
            Err(error) => Err(error),
        };

        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
            if let Some(descriptor_set) = glyph_descriptor_set {
                let _ = self.device.free_descriptor_sets(
                    self.glyph_descriptor_pool,
                    std::slice::from_ref(&descriptor_set),
                );
            }
            if !texture_descriptor_sets.is_empty() {
                let descriptor_sets: Vec<_> = texture_descriptor_sets
                    .iter()
                    .map(|(_, descriptor_set)| *descriptor_set)
                    .collect();
                let _ = self
                    .device
                    .free_descriptor_sets(self.texture_descriptor_pool, &descriptor_sets);
            }
        }

        submit_result
    }

    fn submit_external_graphics_frame(
        &self,
        target: &ExternalImageTarget,
        frame: &RenderFrame,
        draw_ops: &[GraphicsDrawOp],
    ) -> RenderResult<()> {
        let glyph_descriptor_set = if draw_ops
            .iter()
            .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
        {
            Some(self.allocate_glyph_descriptor_set()?)
        } else {
            None
        };
        let texture_descriptor_sets = self.allocate_texture_descriptor_sets(draw_ops)?;
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let record_result = self.record_external_graphics_frame(
            command_buffer,
            target,
            frame,
            draw_ops,
            glyph_descriptor_set,
            &texture_descriptor_sets,
        );
        let submit_result = match record_result {
            Ok(()) => self.submit_and_wait(command_buffer),
            Err(error) => Err(error),
        };

        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
            if let Some(descriptor_set) = glyph_descriptor_set {
                let _ = self.device.free_descriptor_sets(
                    self.glyph_descriptor_pool,
                    std::slice::from_ref(&descriptor_set),
                );
            }
            if !texture_descriptor_sets.is_empty() {
                let descriptor_sets: Vec<_> = texture_descriptor_sets
                    .iter()
                    .map(|(_, descriptor_set)| *descriptor_set)
                    .collect();
                let _ = self
                    .device
                    .free_descriptor_sets(self.texture_descriptor_pool, &descriptor_sets);
            }
        }

        submit_result
    }

    fn submit_external_graphics_frame_signal(
        &mut self,
        target: &ExternalImageTarget,
        frame: &RenderFrame,
        draw_ops: &[GraphicsDrawOp],
        signal_semaphore: vk::Semaphore,
    ) -> RenderResult<()> {
        let glyph_descriptor_set = if draw_ops
            .iter()
            .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
        {
            Some(self.allocate_glyph_descriptor_set()?)
        } else {
            None
        };
        let texture_descriptor_sets = self.allocate_texture_descriptor_sets(draw_ops)?;
        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let record_result = self.record_external_graphics_frame(
            command_buffer,
            target,
            frame,
            draw_ops,
            glyph_descriptor_set,
            &texture_descriptor_sets,
        );
        if let Err(error) = record_result {
            unsafe {
                self.device
                    .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
                if let Some(descriptor_set) = glyph_descriptor_set {
                    let _ = self.device.free_descriptor_sets(
                        self.glyph_descriptor_pool,
                        std::slice::from_ref(&descriptor_set),
                    );
                }
                let descriptor_sets: Vec<_> = texture_descriptor_sets
                    .iter()
                    .map(|(_, descriptor_set)| *descriptor_set)
                    .collect();
                if !descriptor_sets.is_empty() {
                    let _ = self
                        .device
                        .free_descriptor_sets(self.texture_descriptor_pool, &descriptor_sets);
                }
            }
            return Err(error);
        }

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            self.device
                .create_fence(&fence_info, None)
                .map_err(vk_error("create_fence(external_render)"))?
        };
        let wait_semaphores = self.dmabuf_wait_semaphores(draw_ops)?;
        let signal_semaphores = [signal_semaphore];
        let command_buffers = [command_buffer];
        let wait_stages = vec![vk::PipelineStageFlags::FRAGMENT_SHADER; wait_semaphores.len()];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit_info), fence)
                .map_err(vk_error("queue_submit(external_render)"))?;
        }

        self.pending_external_submission = Some(PendingExternalSubmission {
            command_buffer,
            fence,
            glyph_descriptor_set,
            texture_descriptor_sets: texture_descriptor_sets
                .iter()
                .map(|(_, descriptor_set)| *descriptor_set)
                .collect(),
            wait_semaphores,
        });
        Ok(())
    }

    fn allocate_glyph_descriptor_set(&self) -> RenderResult<vk::DescriptorSet> {
        let atlas = self
            .glyph_atlas
            .as_ref()
            .ok_or_else(|| RenderError::new("glyph atlas was not uploaded before text draw"))?;
        let layouts = [self.glyph_descriptor_set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.glyph_descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_set = unsafe {
            self.device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(vk_error("allocate_descriptor_sets"))?
                .into_iter()
                .next()
                .ok_or_else(|| RenderError::new("Vulkan did not allocate a glyph descriptor set"))?
        };

        let image_info = vk::DescriptorImageInfo::default()
            .sampler(atlas.sampler)
            .image_view(atlas.view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));
        unsafe {
            self.device
                .update_descriptor_sets(std::slice::from_ref(&descriptor_write), &[]);
        }

        Ok(descriptor_set)
    }

    fn allocate_texture_descriptor_sets(
        &self,
        draw_ops: &[GraphicsDrawOp],
    ) -> RenderResult<Vec<(u64, vk::DescriptorSet)>> {
        let texture_keys: Vec<_> = draw_ops
            .iter()
            .filter_map(|op| match op {
                GraphicsDrawOp::Texture(blit) => Some(blit.texture_key),
                _ => None,
            })
            .collect();
        if texture_keys.is_empty() {
            return Ok(Vec::new());
        }

        let layouts = vec![self.texture_descriptor_set_layout; texture_keys.len()];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.texture_descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {
            self.device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(vk_error("allocate_descriptor_sets(texture)"))?
        };

        let mut result = Vec::with_capacity(texture_keys.len());
        for (texture_key, descriptor_set) in texture_keys.into_iter().zip(descriptor_sets) {
            let texture = self.surface_textures.get(&texture_key).ok_or_else(|| {
                RenderError::new(format!(
                    "surface texture {texture_key} was not uploaded before draw"
                ))
            })?;
            let image_info = vk::DescriptorImageInfo::default()
                .sampler(texture.sampler)
                .image_view(texture.view)
                .image_layout(texture.layout);
            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&image_info));
            unsafe {
                self.device
                    .update_descriptor_sets(std::slice::from_ref(&descriptor_write), &[]);
            }
            result.push((texture_key, descriptor_set));
        }

        Ok(result)
    }

    fn record_graphics_frame(
        &self,
        command_buffer: vk::CommandBuffer,
        target: &OutputTarget,
        frame: &RenderFrame,
        draw_ops: &[GraphicsDrawOp],
        glyph_descriptor_set: Option<vk::DescriptorSet>,
        texture_descriptor_sets: &[(u64, vk::DescriptorSet)],
    ) -> RenderResult<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer"))?;
        }

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: color_to_float32(frame.background),
            },
        }];
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: target.extent.width as u32,
                height: target.extent.height as u32,
            },
        };
        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(target.framebuffer)
            .render_area(render_area)
            .clear_values(&clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );
            self.device.cmd_set_viewport(
                command_buffer,
                0,
                std::slice::from_ref(&vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: target.extent.width as f32,
                    height: target.extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
            );
            self.device
                .cmd_set_scissor(command_buffer, 0, std::slice::from_ref(&render_area));

            let mut bound_pipeline = vk::Pipeline::null();
            for draw_op in draw_ops {
                match draw_op {
                    GraphicsDrawOp::Rect(rect) => {
                        if rect.rect.width <= 0
                            || rect.rect.height <= 0
                            || clipped_rect(rect.rect, target.extent).is_none()
                        {
                            continue;
                        }

                        if bound_pipeline != self.quad_pipeline {
                            self.device.cmd_bind_pipeline(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.quad_pipeline,
                            );
                            bound_pipeline = self.quad_pipeline;
                        }
                        let push_constants = quad_push_constants(rect, target.extent, false);
                        self.device.cmd_push_constants(
                            command_buffer,
                            self.quad_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            f32_slice_as_u8_slice(&push_constants),
                        );
                        self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                    }
                    GraphicsDrawOp::Glyph(glyph) => {
                        if glyph.width_px <= 0 || glyph.height_px <= 0 {
                            continue;
                        }
                        let Some(descriptor_set) = glyph_descriptor_set else {
                            continue;
                        };

                        if bound_pipeline != self.glyph_pipeline {
                            self.device.cmd_bind_pipeline(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.glyph_pipeline,
                            );
                            self.device.cmd_bind_descriptor_sets(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.glyph_pipeline_layout,
                                0,
                                std::slice::from_ref(&descriptor_set),
                                &[],
                            );
                            bound_pipeline = self.glyph_pipeline;
                        }
                        let Some(gpu_atlas) = self.glyph_atlas.as_ref() else {
                            continue;
                        };
                        let push_constants = glyph_push_constants(
                            glyph,
                            target.extent,
                            gpu_atlas.width_px,
                            gpu_atlas.height_px,
                            false,
                        );
                        self.device.cmd_push_constants(
                            command_buffer,
                            self.glyph_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            f32_slice_as_u8_slice(&push_constants),
                        );
                        self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                    }
                    GraphicsDrawOp::Texture(blit) => {
                        let Some(descriptor_set) = texture_descriptor_sets
                            .iter()
                            .find_map(|(texture_key, descriptor_set)| {
                                (*texture_key == blit.texture_key).then_some(*descriptor_set)
                            })
                        else {
                            continue;
                        };

                        if bound_pipeline != self.texture_pipeline {
                            self.device.cmd_bind_pipeline(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.texture_pipeline,
                            );
                            bound_pipeline = self.texture_pipeline;
                        }
                        self.device.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.texture_pipeline_layout,
                            0,
                            std::slice::from_ref(&descriptor_set),
                            &[],
                        );
                        let push_constants = texture_push_constants(blit, target.extent, false);
                        self.device.cmd_push_constants(
                            command_buffer,
                            self.texture_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            f32_slice_as_u8_slice(&push_constants),
                        );
                        self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                    }
                }
            }

            self.device.cmd_end_render_pass(command_buffer);

            let copy_region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: target.extent.width as u32,
                    height: target.extent.height as u32,
                    depth: 1,
                });
            self.device.cmd_copy_image_to_buffer(
                command_buffer,
                target.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                target.buffer,
                std::slice::from_ref(&copy_region),
            );

            let memory_barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(target.buffer)
                .offset(0)
                .size(target.byte_len);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&memory_barrier),
                &[],
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer"))?;
        }

        Ok(())
    }

    fn record_external_graphics_frame(
        &self,
        command_buffer: vk::CommandBuffer,
        target: &ExternalImageTarget,
        frame: &RenderFrame,
        draw_ops: &[GraphicsDrawOp],
        glyph_descriptor_set: Option<vk::DescriptorSet>,
        texture_descriptor_sets: &[(u64, vk::DescriptorSet)],
    ) -> RenderResult<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer(external)"))?;
        }

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: color_to_float32(frame.background),
            },
        }];
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: target.extent.width as u32,
                height: target.extent.height as u32,
            },
        };
        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(target.render_pass)
            .framebuffer(target.framebuffer)
            .render_area(render_area)
            .clear_values(&clear_values);
        let pipelines = self.graphics_pipelines_for_format(target.format)?;

        unsafe {
            self.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );
            self.device.cmd_set_viewport(
                command_buffer,
                0,
                std::slice::from_ref(&vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: target.extent.width as f32,
                    height: target.extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
            );
            self.device
                .cmd_set_scissor(command_buffer, 0, std::slice::from_ref(&render_area));

            self.record_graphics_draw_ops(
                command_buffer,
                pipelines,
                target.extent,
                draw_ops,
                glyph_descriptor_set,
                texture_descriptor_sets,
            );

            self.device.cmd_end_render_pass(command_buffer);

            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let to_present = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(target.image)
                .subresource_range(subresource_range);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&to_present),
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer(external)"))?;
        }

        Ok(())
    }

    fn record_graphics_draw_ops(
        &self,
        command_buffer: vk::CommandBuffer,
        pipelines: GraphicsPipelines,
        extent: SizeI,
        draw_ops: &[GraphicsDrawOp],
        glyph_descriptor_set: Option<vk::DescriptorSet>,
        texture_descriptor_sets: &[(u64, vk::DescriptorSet)],
    ) {
        unsafe {
            let mut bound_pipeline = vk::Pipeline::null();
            for draw_op in draw_ops {
                match draw_op {
                GraphicsDrawOp::Rect(rect) => {
                    if rect.rect.width <= 0
                        || rect.rect.height <= 0
                        || clipped_rect(rect.rect, extent).is_none()
                    {
                        continue;
                    }

                    if bound_pipeline != pipelines.quad {
                        self.device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipelines.quad,
                        );
                        bound_pipeline = pipelines.quad;
                    }
                    let push_constants = quad_push_constants(rect, extent, true);
                    self.device.cmd_push_constants(
                        command_buffer,
                        pipelines.quad_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        f32_slice_as_u8_slice(&push_constants),
                    );
                    self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
                GraphicsDrawOp::Glyph(glyph) => {
                    if glyph.width_px <= 0 || glyph.height_px <= 0 {
                        continue;
                    }
                    let Some(descriptor_set) = glyph_descriptor_set else {
                        continue;
                    };

                    if bound_pipeline != pipelines.glyph {
                        self.device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipelines.glyph,
                        );
                        self.device.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipelines.glyph_layout,
                            0,
                            std::slice::from_ref(&descriptor_set),
                            &[],
                        );
                        bound_pipeline = pipelines.glyph;
                    }
                    let Some(gpu_atlas) = self.glyph_atlas.as_ref() else {
                        continue;
                    };
                    let push_constants = glyph_push_constants(
                        glyph,
                        extent,
                        gpu_atlas.width_px,
                        gpu_atlas.height_px,
                        true,
                    );
                    self.device.cmd_push_constants(
                        command_buffer,
                        pipelines.glyph_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        f32_slice_as_u8_slice(&push_constants),
                    );
                    self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
                GraphicsDrawOp::Texture(blit) => {
                    let Some(descriptor_set) =
                        texture_descriptor_sets
                            .iter()
                            .find_map(|(texture_key, descriptor_set)| {
                                (*texture_key == blit.texture_key).then_some(*descriptor_set)
                            })
                    else {
                        continue;
                    };

                    if bound_pipeline != pipelines.texture {
                        self.device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipelines.texture,
                        );
                        bound_pipeline = pipelines.texture;
                    }
                    self.device.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipelines.texture_layout,
                        0,
                        std::slice::from_ref(&descriptor_set),
                        &[],
                    );
                    let push_constants = texture_push_constants(blit, extent, true);
                    self.device.cmd_push_constants(
                        command_buffer,
                        pipelines.texture_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        f32_slice_as_u8_slice(&push_constants),
                    );
                    self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
                }
            }
        }
    }

    fn submit_gpu_transfer_segment(
        &self,
        target: &OutputTarget,
        background: Option<ColorRgba8>,
        ops: &[RenderOp],
    ) -> RenderResult<()> {
        let mut blit_uploads = Vec::new();
        for (op_index, op) in ops.iter().enumerate() {
            if let RenderOp::Blit(blit) = op {
                let copies = blit_copy_regions(blit, target.extent);
                if copies.is_empty() {
                    continue;
                }

                match self.create_staging_buffer(&blit.pixels_rgba8) {
                    Ok(staging) => blit_uploads.push(GpuBlitUpload {
                        op_index,
                        staging,
                        copies,
                    }),
                    Err(error) => {
                        unsafe {
                            for upload in blit_uploads {
                                self.destroy_staging_buffer(upload.staging);
                            }
                        }
                        return Err(error);
                    }
                }
            }
        }

        let command_buffer = allocate_command_buffer(&self.device, self.command_pool)?;
        let record_result = self.record_gpu_transfer_segment(
            command_buffer,
            target,
            background,
            ops,
            &blit_uploads,
        );
        let submit_result = match record_result {
            Ok(()) => self.submit_and_wait(command_buffer),
            Err(error) => Err(error),
        };

        unsafe {
            self.device
                .free_command_buffers(self.command_pool, std::slice::from_ref(&command_buffer));
            for upload in blit_uploads {
                self.destroy_staging_buffer(upload.staging);
            }
        }

        submit_result
    }

    fn record_gpu_transfer_segment(
        &self,
        command_buffer: vk::CommandBuffer,
        target: &OutputTarget,
        background: Option<ColorRgba8>,
        ops: &[RenderOp],
        blit_uploads: &[GpuBlitUpload],
    ) -> RenderResult<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(vk_error("begin_command_buffer"))?;

            if let Some(background) = background {
                self.device.cmd_fill_buffer(
                    command_buffer,
                    target.buffer,
                    0,
                    target.byte_len,
                    background.to_ne_u32(),
                );
            }

            let mut has_transfer_write = background.is_some();
            for (op_index, op) in ops.iter().enumerate() {
                if has_transfer_write {
                    self.record_transfer_order_barrier(command_buffer, target);
                }

                match op {
                    RenderOp::Rect(draw_rect) => {
                        self.record_fill_rect(command_buffer, target, draw_rect);
                        has_transfer_write = true;
                    }
                    RenderOp::Blit(_) => {
                        if let Some(upload) = blit_uploads
                            .iter()
                            .find(|upload| upload.op_index == op_index)
                        {
                            self.device.cmd_copy_buffer(
                                command_buffer,
                                upload.staging.buffer,
                                target.buffer,
                                &upload.copies,
                            );
                            has_transfer_write = true;
                        }
                    }
                    RenderOp::Text(_) | RenderOp::Material(_) => {}
                }
            }

            let memory_barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ | vk::AccessFlags::HOST_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(target.buffer)
                .offset(0)
                .size(target.byte_len);

            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&memory_barrier),
                &[],
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(vk_error("end_command_buffer"))?;
        }

        Ok(())
    }

    unsafe fn record_transfer_order_barrier(
        &self,
        command_buffer: vk::CommandBuffer,
        target: &OutputTarget,
    ) {
        let memory_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(target.buffer)
            .offset(0)
            .size(target.byte_len);

        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&memory_barrier),
                &[],
            );
        }
    }

    unsafe fn record_fill_rect(
        &self,
        command_buffer: vk::CommandBuffer,
        target: &OutputTarget,
        rect: &RenderRect,
    ) {
        if let Some(clipped) = clipped_rect(rect.rect, target.extent) {
            let fill_width_bytes = (clipped.width * 4) as vk::DeviceSize;
            let fill_value = rect.color.to_ne_u32();
            for row in clipped.y..clipped.y + clipped.height {
                let offset = ((row * target.extent.width + clipped.x) * 4) as vk::DeviceSize;
                unsafe {
                    self.device.cmd_fill_buffer(
                        command_buffer,
                        target.buffer,
                        offset,
                        fill_width_bytes,
                        fill_value,
                    );
                }
            }
        }
    }

    fn apply_cpu_ops_to_target(
        &mut self,
        target: &OutputTarget,
        ops: &[RenderOp],
    ) -> RenderResult<()> {
        self.stats.cpu_fallback_segments += 1;
        unsafe {
            let mapped = self
                .device
                .map_memory(
                    target.memory,
                    0,
                    target.byte_len,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(vk_error("map_memory"))?;
            let mapped_range = vk::MappedMemoryRange::default()
                .memory(target.memory)
                .offset(0)
                .size(target.byte_len);
            let result = self
                .device
                .invalidate_mapped_memory_ranges(std::slice::from_ref(&mapped_range))
                .map_err(vk_error("invalidate_mapped_memory_ranges"))
                .and_then(|()| {
                    let frame_rgba8 = std::slice::from_raw_parts_mut(
                        mapped.cast::<u8>(),
                        target.byte_len as usize,
                    );
                    apply_render_ops(frame_rgba8, target.extent, ops, &mut self.text_renderer)
                });
            self.device.unmap_memory(target.memory);
            result
        }
    }

    fn read_target_pixels(&self, target: &OutputTarget) -> RenderResult<Vec<u8>> {
        unsafe {
            let mapped = self
                .device
                .map_memory(
                    target.memory,
                    0,
                    target.byte_len,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(vk_error("map_memory"))?;
            let mapped_range = vk::MappedMemoryRange::default()
                .memory(target.memory)
                .offset(0)
                .size(target.byte_len);
            let result = self
                .device
                .invalidate_mapped_memory_ranges(std::slice::from_ref(&mapped_range))
                .map_err(vk_error("invalidate_mapped_memory_ranges"))
                .map(|()| {
                    let bytes =
                        std::slice::from_raw_parts(mapped.cast::<u8>(), target.byte_len as usize);
                    bytes.to_vec()
                });
            self.device.unmap_memory(target.memory);
            result
        }
    }
}

impl Renderer for VulkanRenderer {
    fn register_target(&mut self, target_id: RenderTargetId, extent: SizeI) -> RenderResult<()> {
        VulkanRenderer::register_target(self, target_id, extent)
    }

    fn registered_extent(&self, target_id: RenderTargetId) -> Option<SizeI> {
        VulkanRenderer::registered_extent(self, target_id)
    }

    fn render(&mut self, frame: &RenderFrame, _graph: &RenderGraph) -> RenderResult<RenderedFrame> {
        let output_id = frame.output_id;
        let target_extent = self
            .output_targets
            .get(&output_id)
            .ok_or_else(|| RenderError::new(format!("output {output_id} is not registered")))?
            .extent;
        if frame.extent != target_extent {
            return Err(RenderError::new(format!(
                "render frame extent {}x{} does not match registered output extent {}x{} for {}",
                frame.extent.width,
                frame.extent.height,
                target_extent.width,
                target_extent.height,
                output_id
            )));
        }

        let output_target = self.output_targets.get(&output_id).ok_or_else(|| {
            RenderError::new(format!("output {output_id} disappeared during render"))
        })? as *const OutputTarget;

        if graphics_frame_eligible(frame) {
            let draw_ops = self.prepare_graphics_draw_ops(frame)?;
            if draw_ops
                .iter()
                .any(|op| matches!(op, GraphicsDrawOp::Glyph(_)))
            {
                self.sync_gpu_glyph_atlas()?;
            }
            let output_target = unsafe { &*output_target };
            self.submit_graphics_frame(output_target, frame, &draw_ops)?;
            let pixels_rgba8 = self.read_target_pixels(output_target)?;
            return Ok(RenderedFrame {
                output_id,
                extent: output_target.extent,
                pixels_rgba8,
            });
        }

        let output_target = unsafe { &*output_target };
        let mut cursor = 0;
        let mut background_pending = true;
        while cursor < frame.ops.len() {
            if gpu_transfer_eligible(&frame.ops[cursor], output_target.extent) {
                let start = cursor;
                while cursor < frame.ops.len()
                    && gpu_transfer_eligible(&frame.ops[cursor], output_target.extent)
                {
                    cursor += 1;
                }
                let background = if background_pending {
                    background_pending = false;
                    Some(frame.background)
                } else {
                    None
                };
                self.submit_gpu_transfer_segment(
                    output_target,
                    background,
                    &frame.ops[start..cursor],
                )?;
            } else {
                if background_pending {
                    self.submit_gpu_transfer_segment(output_target, Some(frame.background), &[])?;
                    background_pending = false;
                }

                let start = cursor;
                while cursor < frame.ops.len()
                    && !gpu_transfer_eligible(&frame.ops[cursor], output_target.extent)
                {
                    cursor += 1;
                }
                self.apply_cpu_ops_to_target(output_target, &frame.ops[start..cursor])?;
            }
        }

        if background_pending {
            self.submit_gpu_transfer_segment(output_target, Some(frame.background), &[])?;
        }

        if frame.ops.iter().any(|op| matches!(op, RenderOp::Text(_))) {
            self.sync_gpu_glyph_atlas()?;
        }

        let pixels_rgba8 = self.read_target_pixels(output_target)?;

        Ok(RenderedFrame {
            output_id,
            extent: output_target.extent,
            pixels_rgba8,
        })
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        let _ = self.cleanup_pending_external_submission();
        for (_, target) in std::mem::take(&mut self.output_targets) {
            unsafe { self.destroy_output_target(target) };
        }
        if let Some(atlas) = self.glyph_atlas.take() {
            unsafe {
                self.destroy_glyph_atlas(atlas);
            }
        }
        for (_, texture) in std::mem::take(&mut self.surface_textures) {
            unsafe {
                self.destroy_gpu_texture(texture);
            }
        }

        unsafe {
            self.device.destroy_pipeline(self.bgra_texture_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.bgra_texture_pipeline_layout, None);
            self.device.destroy_pipeline(self.texture_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.texture_pipeline_layout, None);
            self.device.destroy_pipeline(self.bgra_glyph_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.bgra_glyph_pipeline_layout, None);
            self.device.destroy_pipeline(self.glyph_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.glyph_pipeline_layout, None);
            self.device.destroy_pipeline(self.bgra_quad_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.bgra_quad_pipeline_layout, None);
            self.device.destroy_pipeline(self.quad_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.quad_pipeline_layout, None);
            self.device
                .destroy_descriptor_pool(self.texture_descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.texture_descriptor_set_layout, None);
            self.device
                .destroy_descriptor_pool(self.glyph_descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.glyph_descriptor_set_layout, None);
            self.device.destroy_render_pass(self.bgra_render_pass, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn create_instance(entry: &Entry) -> RenderResult<ash::Instance> {
    let app_name = CString::new("lithic-renderer-vulkan").expect("CString literal");
    let engine_name = CString::new("lithic").expect("CString literal");
    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_0);
    let extension_names = [
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::display::NAME.as_ptr(),
    ];
    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);

    unsafe {
        entry
            .create_instance(&create_info, None)
            .map_err(vk_error("create_instance"))
    }
}

fn select_physical_device(instance: &ash::Instance) -> RenderResult<(vk::PhysicalDevice, u32)> {
    let physical_devices = unsafe {
        instance
            .enumerate_physical_devices()
            .map_err(vk_error("enumerate_physical_devices"))?
    };

    for physical_device in physical_devices {
        let queue_family_properties =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        for (index, properties) in queue_family_properties.iter().enumerate() {
            if properties.queue_count == 0 {
                continue;
            }

            if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                return Ok((physical_device, index as u32));
            }
        }
    }

    Err(RenderError::new(
        "could not find a Vulkan physical device with a graphics-capable queue",
    ))
}

fn create_device_and_queue(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
) -> RenderResult<(ash::Device, vk::Queue)> {
    let queue_priorities = [1.0f32];
    let queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&queue_priorities);
    let device_extensions = [
        ash::khr::swapchain::NAME.as_ptr(),
        ash::khr::external_memory::NAME.as_ptr(),
        ash::khr::external_memory_fd::NAME.as_ptr(),
        ash::khr::external_semaphore::NAME.as_ptr(),
        ash::khr::external_semaphore_fd::NAME.as_ptr(),
        ash::ext::external_memory_dma_buf::NAME.as_ptr(),
        ash::ext::image_drm_format_modifier::NAME.as_ptr(),
    ];
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_create_info))
        .enabled_extension_names(&device_extensions);

    let device = unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .map_err(vk_error("create_device"))?
    };
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
    Ok((device, queue))
}

fn create_command_pool(
    device: &ash::Device,
    queue_family_index: u32,
) -> RenderResult<vk::CommandPool> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

    unsafe {
        device
            .create_command_pool(&create_info, None)
            .map_err(vk_error("create_command_pool"))
    }
}

fn create_color_render_pass(device: &ash::Device) -> RenderResult<vk::RenderPass> {
    create_color_render_pass_with_format(device, COLOR_ATTACHMENT_FORMAT)
}

fn create_color_render_pass_with_format(
    device: &ash::Device,
    format: vk::Format,
) -> RenderResult<vk::RenderPass> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
    let color_attachment_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(std::slice::from_ref(&color_attachment_ref));
    let dependencies = [
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            ),
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
    ];
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&color_attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(&dependencies);

    unsafe {
        device
            .create_render_pass(&create_info, None)
            .map_err(vk_error("create_render_pass"))
    }
}

fn create_glyph_descriptor_set_layout(
    device: &ash::Device,
) -> RenderResult<vk::DescriptorSetLayout> {
    let binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);
    let create_info =
        vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding));

    unsafe {
        device
            .create_descriptor_set_layout(&create_info, None)
            .map_err(vk_error("create_descriptor_set_layout"))
    }
}

fn create_glyph_descriptor_pool(device: &ash::Device) -> RenderResult<vk::DescriptorPool> {
    let pool_size = vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 64,
    };
    let create_info = vk::DescriptorPoolCreateInfo::default()
        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
        .max_sets(64)
        .pool_sizes(std::slice::from_ref(&pool_size));

    unsafe {
        device
            .create_descriptor_pool(&create_info, None)
            .map_err(vk_error("create_descriptor_pool"))
    }
}

fn create_solid_quad_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
) -> RenderResult<(vk::PipelineLayout, vk::Pipeline)> {
    let vertex_shader = create_shader_module(device, QUAD_VERT_SPV, "quad vertex shader")?;
    let fragment_shader =
        match create_shader_module(device, SOLID_FRAG_SPV, "solid fragment shader") {
            Ok(shader) => shader,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size((16 * std::mem::size_of::<f32>()) as u32);
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));
    let pipeline_layout =
        match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
            .map_err(vk_error("create_pipeline_layout"))
        {
            Ok(layout) => layout,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(fragment_shader, None);
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let entry_name = CString::new("main").expect("CString literal");
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .name(&entry_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .name(&entry_name),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .sample_shading_enable(false)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        );
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(
            vk::PipelineCache::null(),
            std::slice::from_ref(&pipeline_info),
            None,
        )
    };
    unsafe {
        device.destroy_shader_module(fragment_shader, None);
        device.destroy_shader_module(vertex_shader, None);
    }

    match pipeline_result {
        Ok(mut pipelines) => Ok((pipeline_layout, pipelines.remove(0))),
        Err((pipelines, error)) => {
            unsafe {
                for pipeline in pipelines {
                    device.destroy_pipeline(pipeline, None);
                }
                device.destroy_pipeline_layout(pipeline_layout, None);
            }
            Err(RenderError::new(format!(
                "Vulkan create_graphics_pipelines failed: {error:?}"
            )))
        }
    }
}

fn create_glyph_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    glyph_descriptor_set_layout: vk::DescriptorSetLayout,
) -> RenderResult<(vk::PipelineLayout, vk::Pipeline)> {
    let vertex_shader = create_shader_module(device, GLYPH_VERT_SPV, "glyph vertex shader")?;
    let fragment_shader =
        match create_shader_module(device, GLYPH_FRAG_SPV, "glyph fragment shader") {
            Ok(shader) => shader,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size((16 * std::mem::size_of::<f32>()) as u32);
    let set_layouts = [glyph_descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));
    let pipeline_layout =
        match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
            .map_err(vk_error("create_pipeline_layout(glyph)"))
        {
            Ok(layout) => layout,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(fragment_shader, None);
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let entry_name = CString::new("main").expect("CString literal");
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .name(&entry_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .name(&entry_name),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .sample_shading_enable(false)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        );
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(
            vk::PipelineCache::null(),
            std::slice::from_ref(&pipeline_info),
            None,
        )
    };
    unsafe {
        device.destroy_shader_module(fragment_shader, None);
        device.destroy_shader_module(vertex_shader, None);
    }

    match pipeline_result {
        Ok(mut pipelines) => Ok((pipeline_layout, pipelines.remove(0))),
        Err((pipelines, error)) => {
            unsafe {
                for pipeline in pipelines {
                    device.destroy_pipeline(pipeline, None);
                }
                device.destroy_pipeline_layout(pipeline_layout, None);
            }
            Err(RenderError::new(format!(
                "Vulkan create_graphics_pipelines(glyph) failed: {error:?}"
            )))
        }
    }
}

fn create_texture_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    texture_descriptor_set_layout: vk::DescriptorSetLayout,
) -> RenderResult<(vk::PipelineLayout, vk::Pipeline)> {
    let vertex_shader = create_shader_module(device, TEXTURE_VERT_SPV, "texture vertex shader")?;
    let fragment_shader =
        match create_shader_module(device, TEXTURE_FRAG_SPV, "texture fragment shader") {
            Ok(shader) => shader,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size((16 * std::mem::size_of::<f32>()) as u32);
    let set_layouts = [texture_descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));
    let pipeline_layout =
        match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
            .map_err(vk_error("create_pipeline_layout(texture)"))
        {
            Ok(layout) => layout,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(fragment_shader, None);
                    device.destroy_shader_module(vertex_shader, None);
                }
                return Err(error);
            }
        };

    let entry_name = CString::new("main").expect("CString literal");
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .name(&entry_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .name(&entry_name),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .sample_shading_enable(false)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        );
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(
            vk::PipelineCache::null(),
            std::slice::from_ref(&pipeline_info),
            None,
        )
    };
    unsafe {
        device.destroy_shader_module(fragment_shader, None);
        device.destroy_shader_module(vertex_shader, None);
    }

    match pipeline_result {
        Ok(mut pipelines) => Ok((pipeline_layout, pipelines.remove(0))),
        Err((pipelines, error)) => {
            unsafe {
                for pipeline in pipelines {
                    device.destroy_pipeline(pipeline, None);
                }
                device.destroy_pipeline_layout(pipeline_layout, None);
            }
            Err(RenderError::new(format!(
                "Vulkan create_graphics_pipelines(texture) failed: {error:?}"
            )))
        }
    }
}

fn create_shader_module(
    device: &ash::Device,
    bytes: &[u8],
    description: &'static str,
) -> RenderResult<vk::ShaderModule> {
    let code = ash::util::read_spv(&mut Cursor::new(bytes)).map_err(|error| {
        RenderError::new(format!(
            "failed to read {description} SPIR-V module: {error}"
        ))
    })?;
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
    unsafe {
        device
            .create_shader_module(&create_info, None)
            .map_err(vk_error("create_shader_module"))
    }
}

fn allocate_command_buffer(
    device: &ash::Device,
    command_pool: vk::CommandPool,
) -> RenderResult<vk::CommandBuffer> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffers = unsafe {
        device
            .allocate_command_buffers(&allocate_info)
            .map_err(vk_error("allocate_command_buffers"))?
    };
    command_buffers
        .into_iter()
        .next()
        .ok_or_else(|| RenderError::new("Vulkan did not allocate a primary command buffer"))
}

fn glyph_atlas_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn graphics_frame_eligible(frame: &RenderFrame) -> bool {
    frame.ops.iter().all(|op| match op {
        RenderOp::Rect(rect) => rect.color.a > 0,
        RenderOp::Text(_) => true,
        RenderOp::Blit(blit) => {
            corner_radii_are_zero(blit.corner_radii_px)
                && blit.width > 0
                && blit.height > 0
                && blit.src_width > 0
                && !blit.pixels_rgba8.is_empty()
        }
        RenderOp::Material(_) => false,
    })
}

fn color_to_float32(color: ColorRgba8) -> [f32; 4] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    ]
}

fn quad_push_constants(rect: &RenderRect, extent: SizeI, flip_y: bool) -> [f32; 16] {
    let corner_radii = rect.corner_radii_px.sanitize();
    [
        rect.rect.x as f32,
        viewport_y(rect.rect.y, rect.rect.height, extent, flip_y),
        rect.rect.width as f32,
        rect.rect.height as f32,
        rect.color.r as f32 / 255.0,
        rect.color.g as f32 / 255.0,
        rect.color.b as f32 / 255.0,
        rect.color.a as f32 / 255.0,
        extent.width as f32,
        extent.height as f32,
        0.0,
        0.0,
        corner_radii.top_left as f32,
        corner_radii.top_right as f32,
        corner_radii.bottom_right as f32,
        corner_radii.bottom_left as f32,
    ]
}

fn glyph_push_constants(
    glyph: &AtlasGlyph,
    extent: SizeI,
    atlas_width_px: i32,
    atlas_height_px: i32,
    flip_y: bool,
) -> [f32; 16] {
    let atlas_width = atlas_width_px.max(1) as f32;
    let atlas_height = atlas_height_px.max(1) as f32;
    [
        glyph.dst_x as f32,
        viewport_y(glyph.dst_y, glyph.height_px, extent, flip_y),
        glyph.width_px as f32,
        glyph.height_px as f32,
        glyph.atlas_x as f32 / atlas_width,
        texture_uv_y(glyph.atlas_y, glyph.height_px, atlas_height, flip_y),
        (glyph.atlas_x + glyph.width_px) as f32 / atlas_width,
        texture_uv_y(glyph.atlas_y + glyph.height_px, -glyph.height_px, atlas_height, flip_y),
        glyph.color.r as f32 / 255.0,
        glyph.color.g as f32 / 255.0,
        glyph.color.b as f32 / 255.0,
        glyph.color.a as f32 / 255.0,
        extent.width as f32,
        extent.height as f32,
        0.0,
        0.0,
    ]
}

fn texture_push_constants(blit: &RenderBlit, extent: SizeI, flip_y: bool) -> [f32; 16] {
    let src_width = blit.src_width.max(1) as f32;
    let pixel_src_height =
        (blit.pixels_rgba8.len() as i32 / (blit.src_width.max(1) * 4)).max(1);
    let src_height = pixel_src_height.max(blit.src_y + blit.height).max(1) as f32;
    [
        blit.dst_x as f32,
        viewport_y(blit.dst_y, blit.height, extent, flip_y),
        blit.width as f32,
        blit.height as f32,
        blit.src_x as f32 / src_width,
        texture_uv_y(blit.src_y, blit.height, src_height, flip_y),
        (blit.src_x + blit.width) as f32 / src_width,
        texture_uv_y(blit.src_y + blit.height, -blit.height, src_height, flip_y),
        1.0,
        1.0,
        1.0,
        1.0,
        extent.width as f32,
        extent.height as f32,
        0.0,
        0.0,
    ]
}

fn viewport_y(y: i32, height: i32, extent: SizeI, flip_y: bool) -> f32 {
    if flip_y {
        (extent.height - y - height) as f32
    } else {
        y as f32
    }
}

fn texture_uv_y(y: i32, height: i32, src_height: f32, flip_y: bool) -> f32 {
    if flip_y {
        (y + height) as f32 / src_height
    } else {
        y as f32 / src_height
    }
}

fn f32_slice_as_u8_slice(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

fn gpu_transfer_eligible(op: &RenderOp, extent: SizeI) -> bool {
    match op {
        RenderOp::Rect(rect) => {
            rect.color.a == 255
                && corner_radii_are_zero(rect.corner_radii_px)
                && clipped_rect(rect.rect, extent).is_some()
        }
        RenderOp::Blit(blit) => {
            corner_radii_are_zero(blit.corner_radii_px)
                && !blit_copy_regions(blit, extent).is_empty()
                && blit_visible_pixels_are_opaque(blit, extent)
        }
        RenderOp::Text(_) | RenderOp::Material(_) => false,
    }
}

fn corner_radii_are_zero(corner_radii: lithic_render::CornerRadii) -> bool {
    corner_radii.top_left == 0
        && corner_radii.top_right == 0
        && corner_radii.bottom_right == 0
        && corner_radii.bottom_left == 0
}

fn clipped_rect(rect: RectI, extent: SizeI) -> Option<RectI> {
    if rect.width <= 0 || rect.height <= 0 || extent.width <= 0 || extent.height <= 0 {
        return None;
    }

    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = rect.right().min(extent.width);
    let bottom = rect.bottom().min(extent.height);
    if right <= left || bottom <= top {
        return None;
    }

    Some(RectI {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

struct DmabufVkFormat {
    format: vk::Format,
    components: vk::ComponentMapping,
}

fn dmabuf_format_candidates() -> &'static [(u32, vk::Format)] {
    &[
        (0x3432_5241, vk::Format::B8G8R8A8_UNORM),
        (0x3432_5258, vk::Format::B8G8R8A8_UNORM),
        (0x3432_4241, vk::Format::R8G8B8A8_UNORM),
        (0x3432_4258, vk::Format::R8G8B8A8_UNORM),
        (0x3432_4142, vk::Format::R8G8B8A8_UNORM),
        (0x3432_5842, vk::Format::R8G8B8A8_UNORM),
        (0x3432_4152, vk::Format::B8G8R8A8_UNORM),
        (0x3432_5852, vk::Format::B8G8R8A8_UNORM),
    ]
}

fn fallback_linear_dmabuf_formats() -> Vec<(u32, u64)> {
    dmabuf_format_candidates()
        .iter()
        .map(|(format, _)| (*format, DRM_FORMAT_MOD_LINEAR))
        .collect()
}

fn dmabuf_vk_format(format: u32) -> RenderResult<DmabufVkFormat> {
    let identity = vk::ComponentMapping::default();
    let opaque_alpha = vk::ComponentMapping::default().a(vk::ComponentSwizzle::ONE);
    match format {
        // ABGR8888 / RGBA8888
        0x3432_4241 | 0x3432_4142 => Ok(DmabufVkFormat {
            format: vk::Format::R8G8B8A8_UNORM,
            components: identity,
        }),
        // XBGR8888 / RGBX8888
        0x3432_4258 | 0x3432_5842 => Ok(DmabufVkFormat {
            format: vk::Format::R8G8B8A8_UNORM,
            components: opaque_alpha,
        }),
        // ARGB8888 / BGRA8888
        0x3432_5241 | 0x3432_4152 => Ok(DmabufVkFormat {
            format: vk::Format::B8G8R8A8_UNORM,
            components: identity,
        }),
        // XRGB8888 / BGRX8888
        0x3432_5258 | 0x3432_5852 => Ok(DmabufVkFormat {
            format: vk::Format::B8G8R8A8_UNORM,
            components: opaque_alpha,
        }),
        _ => Err(RenderError::new(format!(
            "unsupported dmabuf DRM format 0x{format:08x}"
        ))),
    }
}

fn blit_texture_fingerprint(blit: &RenderBlit) -> u64 {
    let Some(dmabuf) = blit.dmabuf.as_ref() else {
        return 0;
    };
    dmabuf_fingerprint(dmabuf)
}

fn dmabuf_fingerprint(dmabuf: &RenderDmabuf) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for value in [
        dmabuf.width as u64,
        dmabuf.height as u64,
        dmabuf.format as u64,
        dmabuf.modifier,
        dmabuf.planes.len() as u64,
    ] {
        hash = hash.wrapping_mul(0x100_0000_01b3) ^ value;
    }
    for plane in dmabuf.planes.iter() {
        for value in [
            plane.plane_index as u64,
            plane.offset as u64,
            plane.stride as u64,
        ] {
            hash = hash.wrapping_mul(0x100_0000_01b3) ^ value;
        }
    }
    hash
}

fn blit_copy_regions(blit: &RenderBlit, extent: SizeI) -> Vec<vk::BufferCopy> {
    let mut regions = Vec::new();
    if blit.width <= 0
        || blit.height <= 0
        || blit.src_width <= 0
        || extent.width <= 0
        || extent.height <= 0
    {
        return regions;
    }

    for row in 0..blit.height {
        let dst_y = blit.dst_y + row;
        let src_y = blit.src_y + row;
        if dst_y < 0 || dst_y >= extent.height || src_y < 0 {
            continue;
        }

        let copy_start = 0.max(-blit.dst_x).max(-blit.src_x);
        let copy_end = blit
            .width
            .min(extent.width - blit.dst_x)
            .min(blit.src_width - blit.src_x);
        if copy_end <= copy_start {
            continue;
        }

        let dst_x = blit.dst_x + copy_start;
        let src_x = blit.src_x + copy_start;
        let src_offset = ((src_y * blit.src_width + src_x) * 4) as vk::DeviceSize;
        let dst_offset = ((dst_y * extent.width + dst_x) * 4) as vk::DeviceSize;
        let size = ((copy_end - copy_start) * 4) as vk::DeviceSize;
        if src_offset + size > blit.pixels_rgba8.len() as vk::DeviceSize {
            continue;
        }

        regions.push(
            vk::BufferCopy::default()
                .src_offset(src_offset)
                .dst_offset(dst_offset)
                .size(size),
        );
    }

    regions
}

fn blit_visible_pixels_are_opaque(blit: &RenderBlit, extent: SizeI) -> bool {
    let regions = blit_copy_regions(blit, extent);
    if regions.is_empty() {
        return false;
    }

    for region in regions {
        let first_pixel = region.src_offset as usize / 4;
        let pixel_count = region.size as usize / 4;
        for pixel_index in first_pixel..first_pixel + pixel_count {
            let alpha_offset = pixel_index * 4 + 3;
            if alpha_offset >= blit.pixels_rgba8.len() || blit.pixels_rgba8[alpha_offset] != 255 {
                return false;
            }
        }
    }

    true
}

fn apply_render_ops(
    frame_rgba8: &mut [u8],
    extent: SizeI,
    ops: &[RenderOp],
    text_renderer: &mut FontTextRenderer,
) -> RenderResult<()> {
    for op in ops {
        match op {
            RenderOp::Rect(draw_rect) => {
                fill_rect_cpu(frame_rgba8, extent.width, extent.height, draw_rect)
            }
            RenderOp::Blit(blit) => composite_blit(frame_rgba8, extent.width, extent.height, blit),
            RenderOp::Text(text) => composite_text(
                frame_rgba8,
                extent.width,
                extent.height,
                text_renderer,
                text,
            )?,
            RenderOp::Material(material) => execute_material_op(frame_rgba8, extent, material),
        }
    }

    Ok(())
}

fn fill_rect_cpu(frame_rgba8: &mut [u8], frame_width: i32, frame_height: i32, rect: &RenderRect) {
    for row in rect.rect.y..rect.rect.y + rect.rect.height {
        if row < 0 || row >= frame_height {
            continue;
        }

        for column in rect.rect.x..rect.rect.x + rect.rect.width {
            if column < 0 || column >= frame_width {
                continue;
            }

            let offset = ((row * frame_width + column) * 4) as usize;
            blend_premultiplied_rgba(
                &mut frame_rgba8[offset..offset + 4],
                &[rect.color.r, rect.color.g, rect.color.b, rect.color.a],
            );
        }
    }
}

fn composite_blit(frame_rgba8: &mut [u8], frame_width: i32, frame_height: i32, blit: &RenderBlit) {
    for row in 0..blit.height {
        let dst_y = blit.dst_y + row;
        let src_y = blit.src_y + row;
        if dst_y < 0 || dst_y >= frame_height || src_y < 0 {
            continue;
        }

        for column in 0..blit.width {
            let dst_x = blit.dst_x + column;
            let src_x = blit.src_x + column;
            if dst_x < 0 || dst_x >= frame_width || src_x < 0 {
                continue;
            }

            let dst_offset = ((dst_y * frame_width + dst_x) * 4) as usize;
            let src_offset = ((src_y * blit.src_width + src_x) * 4) as usize;
            if src_offset + 4 > blit.pixels_rgba8.len() || dst_offset + 4 > frame_rgba8.len() {
                continue;
            }

            blend_premultiplied_rgba(
                &mut frame_rgba8[dst_offset..dst_offset + 4],
                &blit.pixels_rgba8[src_offset..src_offset + 4],
            );
        }
    }
}

fn composite_text(
    frame_rgba8: &mut [u8],
    frame_width: i32,
    frame_height: i32,
    text_renderer: &mut FontTextRenderer,
    text: &RenderText,
) -> RenderResult<()> {
    let prepared = text_renderer
        .prepare_text(TextLayoutRequest {
            rect: text.rect,
            text: &text.text,
            style: TextStyle::new(text.color, text.font_size_px),
        })
        .map_err(text_error)?;
    let atlas = text_renderer.atlas();
    for glyph in prepared.glyphs {
        composite_atlas_glyph(frame_rgba8, frame_width, frame_height, atlas, glyph);
    }

    Ok(())
}

fn composite_atlas_glyph(
    frame_rgba8: &mut [u8],
    frame_width: i32,
    frame_height: i32,
    atlas: GlyphAtlasView<'_>,
    glyph: AtlasGlyph,
) {
    for row in 0..glyph.height_px {
        let dst_y = glyph.dst_y + row;
        let atlas_y = glyph.atlas_y + row;
        if dst_y < 0 || dst_y >= frame_height || atlas_y < 0 || atlas_y >= atlas.height_px {
            continue;
        }

        for column in 0..glyph.width_px {
            let dst_x = glyph.dst_x + column;
            let atlas_x = glyph.atlas_x + column;
            if dst_x < 0 || dst_x >= frame_width || atlas_x < 0 || atlas_x >= atlas.width_px {
                continue;
            }

            let dst_offset = ((dst_y * frame_width + dst_x) * 4) as usize;
            let atlas_offset = (atlas_y * atlas.width_px + atlas_x) as usize;
            if dst_offset + 4 > frame_rgba8.len() || atlas_offset >= atlas.pixels_a8.len() {
                continue;
            }

            let mask_alpha = atlas.pixels_a8[atlas_offset] as u32;
            if mask_alpha == 0 {
                continue;
            }

            let source_alpha = (mask_alpha * glyph.color.a as u32 / 255).min(255);
            let source = [
                (glyph.color.r as u32 * source_alpha / 255).min(255) as u8,
                (glyph.color.g as u32 * source_alpha / 255).min(255) as u8,
                (glyph.color.b as u32 * source_alpha / 255).min(255) as u8,
                source_alpha as u8,
            ];
            blend_premultiplied_rgba(&mut frame_rgba8[dst_offset..dst_offset + 4], &source);
        }
    }
}

fn blend_premultiplied_rgba(dst: &mut [u8], src: &[u8]) {
    let src_alpha = src[3] as u32;
    if src_alpha == 0 {
        return;
    }
    if src_alpha == 255 {
        dst.copy_from_slice(src);
        return;
    }

    let inverse_alpha = 255 - src_alpha;
    dst[0] = (src[0] as u32 + (dst[0] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[1] = (src[1] as u32 + (dst[1] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[2] = (src[2] as u32 + (dst[2] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[3] = (src_alpha + (dst[3] as u32 * inverse_alpha) / 255).min(255) as u8;
}

fn vk_error(context: &'static str) -> impl FnOnce(vk::Result) -> RenderError {
    move |error| RenderError::new(format!("Vulkan {context} failed: {error:?}"))
}

fn text_error(error: impl std::fmt::Display) -> RenderError {
    RenderError::new(format!("text rendering failed: {error}"))
}

#[cfg(test)]
mod tests {
    use lithic_core::{ColorRgba8, RectI, SizeI};
    use lithic_render::{
        CornerRadii, RenderBlit, RenderFrame, RenderGraph, RenderOp, RenderRect, RenderTargetId,
        RenderText, Renderer,
    };
    use lithic_text::FontTextRenderer;

    use super::{
        VulkanRenderer, apply_render_ops, graphics_frame_eligible, texture_uv_y, viewport_y,
    };

    #[test]
    fn presentation_viewport_y_flips_top_left_coordinates() {
        let extent = SizeI {
            width: 100,
            height: 80,
        };

        assert_eq!(viewport_y(6, 10, extent, false), 6.0);
        assert_eq!(viewport_y(6, 10, extent, true), 64.0);
    }

    #[test]
    fn presentation_texture_uv_y_flips_sample_orientation() {
        assert_eq!(texture_uv_y(6, 10, 80.0, false), 0.075);
        assert_eq!(texture_uv_y(16, -10, 80.0, false), 0.2);
        assert_eq!(texture_uv_y(6, 10, 80.0, true), 0.2);
        assert_eq!(texture_uv_y(16, -10, 80.0, true), 0.075);
    }

    #[test]
    fn blits_surface_pixels_over_existing_frame() {
        let mut frame = Vec::new();
        for _ in 0..(4 * 4) {
            frame.extend_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        }
        let blit = RenderBlit {
            texture_key: 1,
            dst_x: 1,
            dst_y: 1,
            width: 2,
            height: 2,
            src_x: 0,
            src_y: 0,
            src_width: 2,
            pixels_rgba8: vec![
                0xaa, 0xbb, 0xcc, 0xff, 0x11, 0x22, 0x33, 0xff, 0x44, 0x55, 0x66, 0xff, 0x77, 0x88,
                0x99, 0xff,
            ]
            .into(),
            dmabuf: None,
            content_version: 1,
            damage_rects: Vec::<RectI>::new().into(),
            corner_radii_px: CornerRadii::zero(),
        };

        let mut text_renderer = FontTextRenderer::new().unwrap();
        apply_render_ops(
            &mut frame,
            SizeI {
                width: 4,
                height: 4,
            },
            &[RenderOp::Blit(blit)],
            &mut text_renderer,
        )
        .unwrap();

        assert_eq!(
            pixel_at(&frame, 4, 1, 1),
            ColorRgba8::rgba(0xaa, 0xbb, 0xcc, 0xff)
        );
        assert_eq!(
            pixel_at(&frame, 4, 2, 2),
            ColorRgba8::rgba(0x77, 0x88, 0x99, 0xff)
        );
    }

    #[test]
    fn text_op_draws_glyph_pixels() {
        let mut frame = repeated_pixel(ColorRgba8::rgba(0x00, 0x00, 0x00, 0xff), 48 * 24);

        let mut text_renderer = FontTextRenderer::new().unwrap();
        apply_render_ops(
            &mut frame,
            SizeI {
                width: 48,
                height: 24,
            },
            &[RenderOp::Text(RenderText {
                rect: RectI {
                    x: 2,
                    y: 1,
                    width: 44,
                    height: 22,
                },
                text: "A".to_string(),
                color: ColorRgba8::rgba(0xaa, 0xbb, 0xcc, 0xff),
                font_size_px: 14,
            })],
            &mut text_renderer,
        )
        .unwrap();

        assert!(frame.chunks_exact(4).any(|pixel| pixel[0] > 0));
    }

    #[test]
    fn vulkan_renderer_can_render_offscreen_if_loader_is_available() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(3);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 32,
                    height: 20,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 32,
                height: 20,
            },
            background: ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Blit(RenderBlit {
                texture_key: 1,
                dst_x: 4,
                dst_y: 3,
                width: 8,
                height: 6,
                src_x: 0,
                src_y: 0,
                src_width: 8,
                pixels_rgba8: repeated_pixel(ColorRgba8::rgba(0xaa, 0xbb, 0xcc, 0xff), 8 * 6)
                    .into(),
                dmabuf: None,
                content_version: 1,
                damage_rects: Vec::<RectI>::new().into(),
                corner_radii_px: CornerRadii::zero(),
            })],
        };
        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 0, 0),
            ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff)
        );
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 5, 16),
            ColorRgba8::rgba(0xaa, 0xbb, 0xcc, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_draws_opaque_rects_to_output() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(5);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 16,
                    height: 12,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 16,
                height: 12,
            },
            background: ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Rect(RenderRect {
                rect: RectI {
                    x: 2,
                    y: 3,
                    width: 5,
                    height: 4,
                },
                color: ColorRgba8::rgba(0xd0, 0x11, 0x22, 0xff),
                corner_radii_px: CornerRadii::zero(),
            })],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 0, 0),
            ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff)
        );
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 4, 5),
            ColorRgba8::rgba(0xd0, 0x11, 0x22, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_draws_translucent_rects_with_graphics_pipeline() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(7);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 16,
                    height: 12,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 16,
                height: 12,
            },
            background: ColorRgba8::rgba(0x00, 0x00, 0x00, 0xff),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Rect(RenderRect {
                rect: RectI {
                    x: 2,
                    y: 2,
                    width: 12,
                    height: 8,
                },
                color: ColorRgba8::rgba(0x80, 0x20, 0x10, 0x80),
                corner_radii_px: CornerRadii::zero(),
            })],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 8, 6),
            ColorRgba8::rgba(0x80, 0x20, 0x10, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_draws_rounded_opaque_rects_with_graphics_pipeline() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(8);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 20,
                    height: 20,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 20,
                height: 20,
            },
            background: ColorRgba8::rgba(0x04, 0x08, 0x0c, 0xff),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Rect(RenderRect {
                rect: RectI {
                    x: 2,
                    y: 2,
                    width: 12,
                    height: 12,
                },
                color: ColorRgba8::rgba(0xe0, 0x30, 0x40, 0xff),
                corner_radii_px: CornerRadii::all(6),
            })],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 2, 2),
            ColorRgba8::rgba(0x04, 0x08, 0x0c, 0xff)
        );
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 8, 8),
            ColorRgba8::rgba(0xe0, 0x30, 0x40, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_draws_translucent_rounded_rects_with_graphics_pipeline() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(10);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 20,
                    height: 20,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 20,
                height: 20,
            },
            background: ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Rect(RenderRect {
                rect: RectI {
                    x: 2,
                    y: 2,
                    width: 12,
                    height: 12,
                },
                color: ColorRgba8::rgba(0x80, 0x10, 0x20, 0x80),
                corner_radii_px: CornerRadii::all(6),
            })],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 2, 2),
            ColorRgba8::rgba(0x10, 0x20, 0x30, 0xff)
        );
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 8, 8),
            ColorRgba8::rgba(0x88, 0x20, 0x38, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_preserves_order_across_gpu_and_cpu_segments() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(6);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 16,
                    height: 12,
                },
            )
            .unwrap();

        let frame = renderer
            .render(
                &RenderFrame {
                    output_id,
                    extent: SizeI {
                        width: 16,
                        height: 12,
                    },
                    background: ColorRgba8::rgba(0x00, 0x00, 0x00, 0xff),
                    damage_rects: Vec::<RectI>::new().into(),
                    ops: vec![
                        RenderOp::Rect(RenderRect {
                            rect: RectI {
                                x: 1,
                                y: 1,
                                width: 8,
                                height: 8,
                            },
                            color: ColorRgba8::rgba(0xaa, 0x00, 0x00, 0xff),
                            corner_radii_px: CornerRadii::zero(),
                        }),
                        RenderOp::Rect(RenderRect {
                            rect: RectI {
                                x: 4,
                                y: 4,
                                width: 4,
                                height: 4,
                            },
                            color: ColorRgba8::rgba(0x00, 0xbb, 0x00, 0x80),
                            corner_radii_px: CornerRadii::all(1),
                        }),
                        RenderOp::Rect(RenderRect {
                            rect: RectI {
                                x: 5,
                                y: 5,
                                width: 2,
                                height: 2,
                            },
                            color: ColorRgba8::rgba(0x11, 0x22, 0xee, 0xff),
                            corner_radii_px: CornerRadii::zero(),
                        }),
                    ],
                },
                &RenderGraph::default(),
            )
            .unwrap();

        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 3, 3),
            ColorRgba8::rgba(0xaa, 0x00, 0x00, 0xff)
        );
        assert_eq!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 5, 5),
            ColorRgba8::rgba(0x11, 0x22, 0xee, 0xff)
        );
        assert_ne!(
            pixel_at(&frame.pixels_rgba8, frame.extent.width, 4, 4),
            ColorRgba8::rgba(0xaa, 0x00, 0x00, 0xff)
        );
    }

    #[test]
    fn vulkan_renderer_uploads_text_atlas_when_text_op_renders() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(4);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 96,
                    height: 40,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 96,
                height: 40,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Text(RenderText {
                rect: RectI {
                    x: 4,
                    y: 4,
                    width: 88,
                    height: 28,
                },
                text: "Atlas".to_string(),
                color: ColorRgba8::rgba(0xee, 0xee, 0xee, 0xff),
                font_size_px: 16,
            })],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert!(frame.pixels_rgba8.chunks_exact(4).any(|pixel| pixel[0] > 0));
        assert_eq!(
            renderer.gpu_glyph_atlas_version(),
            Some(renderer.text_renderer.atlas().version)
        );
    }

    #[test]
    fn vulkan_renderer_draws_text_over_opaque_rects_with_graphics_pipeline() {
        let mut renderer = match VulkanRenderer::new() {
            Ok(renderer) => renderer,
            Err(_) => return,
        };

        let output_id = RenderTargetId::new(9);
        renderer
            .register_target(
                output_id,
                SizeI {
                    width: 96,
                    height: 40,
                },
            )
            .unwrap();

        let render_frame = RenderFrame {
            output_id,
            extent: SizeI {
                width: 96,
                height: 40,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![
                RenderOp::Rect(RenderRect {
                    rect: RectI {
                        x: 0,
                        y: 0,
                        width: 96,
                        height: 40,
                    },
                    color: ColorRgba8::rgba(0x08, 0x10, 0x18, 0xff),
                    corner_radii_px: CornerRadii::zero(),
                }),
                RenderOp::Text(RenderText {
                    rect: RectI {
                        x: 4,
                        y: 4,
                        width: 88,
                        height: 28,
                    },
                    text: "Lithic".to_string(),
                    color: ColorRgba8::rgba(0xf0, 0xf0, 0xf0, 0xff),
                    font_size_px: 16,
                }),
            ],
        };
        assert!(graphics_frame_eligible(&render_frame));

        let frame = renderer
            .render(&render_frame, &RenderGraph::default())
            .unwrap();

        assert!(
            frame
                .pixels_rgba8
                .chunks_exact(4)
                .any(|pixel| { pixel[0] > 0x08 || pixel[1] > 0x10 || pixel[2] > 0x18 })
        );
    }

    fn pixel_at(bytes: &[u8], width: i32, x: i32, y: i32) -> ColorRgba8 {
        let offset = ((y * width + x) * 4) as usize;
        ColorRgba8::rgba(
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        )
    }

    fn repeated_pixel(color: ColorRgba8, count: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(count * 4);
        for _ in 0..count {
            bytes.extend_from_slice(&[color.r, color.g, color.b, color.a]);
        }
        bytes
    }
}
