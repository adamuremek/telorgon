use ash::vk;

use crate::renderer_vulkan::target::VulkanImageState;

pub(crate) fn color_subresource() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

pub(crate) fn fragment_sampled_state() -> VulkanImageState {
    VulkanImageState {
        layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        stage: vk::PipelineStageFlags2::FRAGMENT_SHADER,
        access: vk::AccessFlags2::SHADER_SAMPLED_READ,
    }
}

pub(crate) fn image_transition(
    image: vk::Image,
    initial: VulkanImageState,
    final_state: VulkanImageState,
) -> vk::ImageMemoryBarrier2<'static> {
    vk::ImageMemoryBarrier2::default()
        .src_stage_mask(initial.stage)
        .src_access_mask(initial.access)
        .dst_stage_mask(final_state.stage)
        .dst_access_mask(final_state.access)
        .old_layout(initial.layout)
        .new_layout(final_state.layout)
        .image(image)
        .subresource_range(color_subresource())
}

pub(crate) fn target_to_color(
    image: vk::Image,
    initial: VulkanImageState,
) -> vk::ImageMemoryBarrier2<'static> {
    image_transition(image, initial, VulkanImageState::COLOR_ATTACHMENT)
}

pub(crate) fn color_to_final(
    image: vk::Image,
    final_state: VulkanImageState,
) -> vk::ImageMemoryBarrier2<'static> {
    image_transition(image, VulkanImageState::COLOR_ATTACHMENT, final_state)
}
