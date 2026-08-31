use crate::render::RenderResult;
use ash::vk;

use crate::renderer_vulkan::error::vk_error;

pub(crate) struct DescriptorLayouts {
    device: ash::Device,
    pub(crate) sets: [vk::DescriptorSetLayout; 4],
    pub(crate) pipeline: vk::PipelineLayout,
}

pub(crate) const PRIMITIVE_SET_COUNT: usize = 4;
pub(crate) const MAX_TEXTURE_SETS: usize = 128;

#[derive(Copy, Clone)]
pub(crate) struct FrameDescriptorSets {
    pub(crate) view: vk::DescriptorSet,
    pub(crate) scene: vk::DescriptorSet,
    pub(crate) primitives: [vk::DescriptorSet; PRIMITIVE_SET_COUNT],
    pub(crate) textures: [vk::DescriptorSet; MAX_TEXTURE_SETS],
}

impl DescriptorLayouts {
    pub(crate) fn new(device: &ash::Device) -> RenderResult<Self> {
        let mut created = Vec::with_capacity(4);
        let set0 = create_layout(
            device,
            &[vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)],
        )?;
        created.push(set0);
        let set1 = match create_layout(
            device,
            &[
                storage_binding(0, vk::ShaderStageFlags::VERTEX),
                storage_binding(1, vk::ShaderStageFlags::FRAGMENT),
                storage_binding(2, vk::ShaderStageFlags::VERTEX),
            ],
        ) {
            Ok(layout) => layout,
            Err(error) => return cleanup_layout_error(device, created, error),
        };
        created.push(set1);
        let set2 = match create_layout(
            device,
            &[
                storage_binding(
                    0,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                ),
                storage_binding(1, vk::ShaderStageFlags::FRAGMENT),
            ],
        ) {
            Ok(layout) => layout,
            Err(error) => return cleanup_layout_error(device, created, error),
        };
        created.push(set2);
        let set3 = match create_layout(
            device,
            &[
                sampled_binding(0, vk::ShaderStageFlags::FRAGMENT),
                sampled_binding(1, vk::ShaderStageFlags::FRAGMENT),
            ],
        ) {
            Ok(layout) => layout,
            Err(error) => return cleanup_layout_error(device, created, error),
        };
        created.push(set3);
        let sets = [set0, set1, set2, set3];
        let pipeline = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&sets),
                None,
            )
        }
        .map_err(|result| vk_error("failed to create Vulkan pipeline layout", result));
        let pipeline = match pipeline {
            Ok(pipeline) => pipeline,
            Err(error) => return cleanup_layout_error(device, created, error),
        };
        Ok(Self {
            device: device.clone(),
            sets,
            pipeline,
        })
    }
}

fn cleanup_layout_error<T>(
    device: &ash::Device,
    layouts: Vec<vk::DescriptorSetLayout>,
    error: crate::render::RenderError,
) -> RenderResult<T> {
    unsafe {
        for layout in layouts {
            device.destroy_descriptor_set_layout(layout, None);
        }
    }
    Err(error)
}

impl Drop for DescriptorLayouts {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline_layout(self.pipeline, None);
            for layout in self.sets {
                self.device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

fn create_layout(
    device: &ash::Device,
    bindings: &[vk::DescriptorSetLayoutBinding<'_>],
) -> RenderResult<vk::DescriptorSetLayout> {
    unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
            None,
        )
    }
    .map_err(|result| vk_error("failed to create Vulkan descriptor layout", result))
}

fn storage_binding(
    binding: u32,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(1)
        .stage_flags(stages)
}

fn sampled_binding(
    binding: u32,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(stages)
}

pub(crate) fn allocate_frame_sets(
    device: &ash::Device,
    layouts: &DescriptorLayouts,
) -> RenderResult<(vk::DescriptorPool, FrameDescriptorSets)> {
    let pool_sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 3 + (PRIMITIVE_SET_COUNT as u32 * 2),
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: MAX_TEXTURE_SETS as u32 * 2,
        },
    ];
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets((2 + PRIMITIVE_SET_COUNT + MAX_TEXTURE_SETS) as u32)
                .pool_sizes(&pool_sizes),
            None,
        )
    }
    .map_err(|result| vk_error("failed to create Vulkan frame descriptor pool", result))?;
    let sets = match unsafe {
        let mut set_layouts = Vec::with_capacity(2 + PRIMITIVE_SET_COUNT + MAX_TEXTURE_SETS);
        set_layouts.push(layouts.sets[0]);
        set_layouts.push(layouts.sets[1]);
        set_layouts.extend(std::iter::repeat_n(layouts.sets[2], PRIMITIVE_SET_COUNT));
        set_layouts.extend(std::iter::repeat_n(layouts.sets[3], MAX_TEXTURE_SETS));
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&set_layouts),
        )
    } {
        Ok(sets) => sets,
        Err(result) => {
            unsafe { device.destroy_descriptor_pool(pool, None) };
            return Err(vk_error(
                "failed to allocate Vulkan frame descriptor sets",
                result,
            ));
        }
    };
    if sets.len() != 2 + PRIMITIVE_SET_COUNT + MAX_TEXTURE_SETS {
        unsafe { device.destroy_descriptor_pool(pool, None) };
        return Err(crate::renderer_vulkan::error::internal(
            "Vulkan returned the wrong descriptor-set count",
        ));
    }
    let primitives = sets[2..2 + PRIMITIVE_SET_COUNT]
        .try_into()
        .expect("primitive descriptor-set count was checked");
    let textures = sets[2 + PRIMITIVE_SET_COUNT..]
        .try_into()
        .expect("texture descriptor-set count was checked");
    Ok((
        pool,
        FrameDescriptorSets {
            view: sets[0],
            scene: sets[1],
            primitives,
            textures,
        },
    ))
}
