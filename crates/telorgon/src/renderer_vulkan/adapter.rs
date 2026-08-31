use std::ffi::CStr;

use crate::render::RenderResult;
use ash::vk;

use crate::renderer_vulkan::VulkanInstance;
use crate::renderer_vulkan::error::vk_error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterReport {
    pub index: usize,
    pub name: String,
    pub device_type: vk::PhysicalDeviceType,
    pub api_version: u32,
    pub graphics_queue_families: Vec<u32>,
    pub score: u32,
    pub supported: bool,
    pub rejection_reasons: Vec<String>,
}

impl AdapterReport {
    pub(crate) fn enumerate(instance: &VulkanInstance) -> RenderResult<Vec<Self>> {
        let devices = unsafe { instance.inner.raw.enumerate_physical_devices() }
            .map_err(|result| vk_error("failed to enumerate Vulkan physical devices", result))?;
        Ok(devices
            .into_iter()
            .enumerate()
            .map(|(index, physical_device)| {
                let properties = unsafe {
                    instance
                        .inner
                        .raw
                        .get_physical_device_properties(physical_device)
                };
                let queues = unsafe {
                    instance
                        .inner
                        .raw
                        .get_physical_device_queue_family_properties(physical_device)
                };
                let graphics_queue_families = queues
                    .iter()
                    .enumerate()
                    .filter(|(_, family)| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                    .map(|(family, _)| family as u32)
                    .collect::<Vec<_>>();
                let mut rejection_reasons = Vec::new();
                if properties.api_version < vk::API_VERSION_1_3 {
                    rejection_reasons.push("physical device does not expose Vulkan 1.3".to_owned());
                }
                if graphics_queue_families.is_empty() {
                    rejection_reasons.push("physical device has no graphics queue".to_owned());
                }
                if properties.device_type == vk::PhysicalDeviceType::CPU {
                    rejection_reasons.push(
                        "CPU Vulkan adapters are excluded from the hardware renderer profile"
                            .to_owned(),
                    );
                }
                let device_bonus = match properties.device_type {
                    vk::PhysicalDeviceType::DISCRETE_GPU => 4_000,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 3_000,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2_000,
                    vk::PhysicalDeviceType::CPU => 0,
                    _ => 1_000,
                };
                let supported = rejection_reasons.is_empty();
                Self {
                    index,
                    name: unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned(),
                    device_type: properties.device_type,
                    api_version: properties.api_version,
                    graphics_queue_families,
                    score: device_bonus + properties.limits.max_image_dimension2_d.min(8_192),
                    supported,
                    rejection_reasons,
                }
            })
            .collect())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DeviceSelection {
    pub adapter_index: usize,
}

impl DeviceSelection {
    pub fn best(adapters: &[AdapterReport]) -> Option<Self> {
        adapters
            .iter()
            .filter(|adapter| adapter.supported)
            .max_by_key(|adapter| adapter.score)
            .map(|adapter| Self {
                adapter_index: adapter.index,
            })
    }
}
