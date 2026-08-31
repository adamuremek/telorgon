use std::marker::PhantomData;

use crate::core::SizeI;
use crate::render::{AlphaMode, ColorSpace, RenderResult, RenderTargetInfo};
use ash::vk;

use crate::renderer_vulkan::image::AllocatedImage;
use crate::renderer_vulkan::{VulkanDevice, error::unsupported};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct VulkanImageState {
    pub(crate) layout: vk::ImageLayout,
    pub(crate) stage: vk::PipelineStageFlags2,
    pub(crate) access: vk::AccessFlags2,
}

impl VulkanImageState {
    pub(crate) const UNDEFINED: Self = Self {
        layout: vk::ImageLayout::UNDEFINED,
        stage: vk::PipelineStageFlags2::NONE,
        access: vk::AccessFlags2::NONE,
    };

    pub(crate) const COLOR_ATTACHMENT: Self = Self {
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        access: vk::AccessFlags2::from_raw(
            vk::AccessFlags2::COLOR_ATTACHMENT_READ.as_raw()
                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw(),
        ),
    };
}

pub struct OffscreenVulkanTarget {
    image: AllocatedImage,
    info: RenderTargetInfo,
}

impl OffscreenVulkanTarget {
    pub fn new(device: &VulkanDevice, extent: SizeI) -> RenderResult<Self> {
        if extent.width <= 0 || extent.height <= 0 {
            return Err(unsupported(
                "offscreen Vulkan target extent must be nonzero",
            ));
        }
        let image = AllocatedImage::new_color_target(
            device.inner.clone(),
            vk::Extent2D {
                width: extent.width as u32,
                height: extent.height as u32,
            },
            vk::Format::R8G8B8A8_UNORM,
            "Telorgon offscreen target",
        )?;
        Ok(Self {
            image,
            info: RenderTargetInfo {
                color_space: ColorSpace::Linear,
                alpha_mode: AlphaMode::Premultiplied,
                ..RenderTargetInfo::full(extent)
            },
        })
    }

    pub fn target(&self) -> VulkanTarget<'_> {
        VulkanTarget {
            device_id: self.image_device_id(),
            image: self.image.raw(),
            view: self.image.view(),
            format: self.image.format,
            extent: self.image.extent,
            info: self.info,
            initial_state: VulkanImageState::UNDEFINED,
            final_state: VulkanImageState::COLOR_ATTACHMENT,
            initial_queue_family: vk::QUEUE_FAMILY_IGNORED,
            final_queue_family: vk::QUEUE_FAMILY_IGNORED,
            _borrow: PhantomData,
        }
    }

    fn image_device_id(&self) -> u64 {
        self.image.device_id()
    }
}

#[derive(Copy, Clone)]
pub struct VulkanTarget<'frame> {
    pub(crate) device_id: u64,
    pub(crate) image: vk::Image,
    pub(crate) view: vk::ImageView,
    pub(crate) format: vk::Format,
    pub(crate) extent: vk::Extent2D,
    pub(crate) info: RenderTargetInfo,
    pub(crate) initial_state: VulkanImageState,
    pub(crate) final_state: VulkanImageState,
    pub(crate) initial_queue_family: u32,
    pub(crate) final_queue_family: u32,
    pub(crate) _borrow: PhantomData<&'frame mut vk::Image>,
}

impl VulkanTarget<'_> {
    pub fn info(&self) -> RenderTargetInfo {
        self.info
    }
}
