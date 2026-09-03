use std::collections::{BTreeMap, VecDeque};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::core::{ColorRgba8, RectI, SizeI};
use crate::presenter_vulkan_kms::GbmBuffer;
use crate::render::{RenderBackend, RenderRequest, TargetLoad, TargetStore};
use crate::renderer_vulkan::{
    DeviceSelection, SubmissionReceipt, VulkanCompositePlacement, VulkanCompositeScene,
    VulkanConfig, VulkanDevice, VulkanDmaBufScanoutTarget, VulkanInstance, VulkanScene,
};

use super::super::geometry::{accumulated_damage, full_rect, intersect_rect};
use super::super::scene::{DesktopFrame, DesktopSceneKey};
use crate::application_host::{AppError, AppResult};

pub(in crate::application_host::desktop_wayland) struct VulkanCompletion {
    pub(in crate::application_host::desktop_wayland) slot_index: usize,
    pub(in crate::application_host::desktop_wayland) result: Result<(), String>,
}

struct VulkanCompletionRequest {
    slot_index: usize,
    receipt: SubmissionReceipt,
}

struct VulkanCompletionWorker {
    requests: Option<mpsc::Sender<VulkanCompletionRequest>>,
    completions: mpsc::Receiver<VulkanCompletion>,
    wake: OwnedFd,
    thread: Option<thread::JoinHandle<()>>,
}

impl VulkanCompletionWorker {
    fn new() -> AppResult<Self> {
        let raw = unsafe {
            crate::platform_linux::ffi::eventfd(
                0,
                crate::platform_linux::ffi::EFD_CLOEXEC | crate::platform_linux::ffi::EFD_NONBLOCK,
            )
        };
        if raw < 0 {
            return Err(AppError::new("failed to create Vulkan completion eventfd"));
        }
        let wake = unsafe { OwnedFd::from_raw_fd(raw) };
        let thread_wake = wake.try_clone().map_err(|error| {
            AppError::new(format!("failed to clone completion eventfd: {error}"))
        })?;
        let (request_tx, request_rx) = mpsc::channel::<VulkanCompletionRequest>();
        let (completion_tx, completion_rx) = mpsc::channel::<VulkanCompletion>();
        let thread = thread::Builder::new()
            .name("telorgon-vulkan-completion".to_owned())
            .spawn(move || {
                while let Ok(mut request) = request_rx.recv() {
                    #[cfg(feature = "profiler")]
                    let _wait = crate::profiler::span!("vulkan.scanout.completion_wait.worker");
                    let result = request
                        .receipt
                        .wait(Duration::from_secs(2))
                        .map_err(|error| error.to_string());
                    if completion_tx
                        .send(VulkanCompletion {
                            slot_index: request.slot_index,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    let value = 1_u64;
                    let _ = unsafe {
                        crate::platform_linux::ffi::write(
                            thread_wake.as_raw_fd(),
                            std::ptr::from_ref(&value).cast(),
                            std::mem::size_of::<u64>(),
                        )
                    };
                }
            })
            .map_err(|error| {
                AppError::new(format!("failed to start Vulkan completion worker: {error}"))
            })?;
        Ok(Self {
            requests: Some(request_tx),
            completions: completion_rx,
            wake,
            thread: Some(thread),
        })
    }

    fn event_fd(&self) -> i32 {
        self.wake.as_raw_fd()
    }

    fn submit(&self, slot_index: usize, receipt: SubmissionReceipt) -> AppResult<()> {
        self.requests
            .as_ref()
            .ok_or_else(|| AppError::new("Vulkan completion worker is stopped"))?
            .send(VulkanCompletionRequest {
                slot_index,
                receipt,
            })
            .map_err(|_| AppError::new("Vulkan completion worker stopped unexpectedly"))
    }

    fn drain(&self) -> Vec<VulkanCompletion> {
        let mut value = 0_u64;
        loop {
            let read = unsafe {
                crate::platform_linux::ffi::read(
                    self.wake.as_raw_fd(),
                    std::ptr::from_mut(&mut value).cast(),
                    std::mem::size_of::<u64>(),
                )
            };
            if read != std::mem::size_of::<u64>() as isize {
                break;
            }
        }
        self.completions.try_iter().collect()
    }
}

impl Drop for VulkanCompletionWorker {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(in crate::application_host::desktop_wayland) const VULKAN_STAGING_MIN_BYTES_PER_SLOT: u64 =
    16 * 1024 * 1024;
pub(in crate::application_host::desktop_wayland) const VULKAN_STAGING_HEADROOM_BYTES_PER_SLOT: u64 =
    16 * 1024 * 1024;

pub(in crate::application_host::desktop_wayland) fn vulkan_staging_budget_bytes(
    extent: SizeI,
    frame_slots: usize,
) -> AppResult<u64> {
    let frame_bytes = u64::try_from(extent.width)
        .ok()
        .and_then(|width| {
            u64::try_from(extent.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| AppError::new("Vulkan scanout extent overflows its upload budget"))?;
    // A direct compositor may receive one full client image in addition to changed shell
    // resources. The budget is per reusable frame slot and performs no per-frame allocation.
    let bytes_per_slot = frame_bytes
        .checked_add(VULKAN_STAGING_HEADROOM_BYTES_PER_SLOT)
        .ok_or_else(|| AppError::new("Vulkan scanout staging headroom overflows its budget"))?
        .max(VULKAN_STAGING_MIN_BYTES_PER_SLOT);
    let frame_slots = u64::try_from(frame_slots.max(1))
        .map_err(|_| AppError::new("Vulkan frame-slot count overflows its staging budget"))?;
    bytes_per_slot
        .checked_mul(frame_slots)
        .ok_or_else(|| AppError::new("Vulkan frame slots overflow their staging budget"))
}

pub(in crate::application_host::desktop_wayland) struct VulkanDesktopRenderer {
    device: VulkanDevice,
    scenes: BTreeMap<DesktopSceneKey, VulkanScene>,
    targets: Vec<VulkanDmaBufScanoutTarget>,
    content_version: u64,
    completion_worker: VulkanCompletionWorker,
    target_versions: Vec<u64>,
    damage_history: VecDeque<(u64, Option<RectI>)>,
}

impl VulkanDesktopRenderer {
    pub(super) fn new(buffers: &[GbmBuffer<'_, '_>], extent: SizeI) -> AppResult<Self> {
        let frames_in_flight = buffers.len().max(2);
        let staging_budget_bytes = vulkan_staging_budget_bytes(extent, frames_in_flight)?;
        let config = VulkanConfig {
            enable_validation: false,
            frames_in_flight,
            staging_budget_bytes,
            ..VulkanConfig::default()
        };
        let instance = VulkanInstance::load(&config, &[]).map_err(app_error)?;
        let mut adapters = instance.adapters().map_err(app_error)?;
        adapters.sort_by_key(|adapter| std::cmp::Reverse(adapter.score));
        let mut failures = Vec::new();
        for adapter in adapters.into_iter().filter(|adapter| adapter.supported) {
            let selection = DeviceSelection {
                adapter_index: adapter.index,
            };
            let device =
                match VulkanDevice::create_owned(instance.clone(), &config, &selection, None) {
                    Ok(device) => device,
                    Err(error) => {
                        failures.push(format!("{}: {error}", adapter.name));
                        continue;
                    }
                };
            let targets = buffers
                .iter()
                .map(|buffer| {
                    let format = buffer.format();
                    let mut planes = buffer.export_planes().map_err(app_error)?;
                    if planes.len() != 1 {
                        return Err(AppError::new(
                            "Vulkan scanout currently requires one GBM DMA-BUF plane",
                        ));
                    }
                    let plane = planes.pop().expect("one plane checked");
                    unsafe {
                        VulkanDmaBufScanoutTarget::import(
                            &device,
                            plane.fd,
                            format.fourcc,
                            format.modifier,
                            buffer.size(),
                            u64::from(plane.offset),
                            plane.stride,
                        )
                    }
                    .map_err(app_error)
                })
                .collect::<AppResult<Vec<_>>>();
            let targets = match targets {
                Ok(targets) => targets,
                Err(error) => {
                    failures.push(format!("{}: {error}", adapter.name));
                    continue;
                }
            };
            let target_count = targets.len();
            return Ok(Self {
                device,
                scenes: BTreeMap::new(),
                targets,
                content_version: 0,
                completion_worker: VulkanCompletionWorker::new()?,
                target_versions: vec![0; target_count],
                damage_history: VecDeque::new(),
            });
        }
        Err(AppError::new(if failures.is_empty() {
            "no supported Vulkan adapter was found".to_owned()
        } else {
            format!(
                "no Vulkan adapter could import the KMS scanout buffers: {}",
                failures.join("; ")
            )
        }))
    }

    pub(super) fn render(&mut self, target_index: usize, frame: DesktopFrame) -> AppResult<()> {
        self.content_version = self.content_version.wrapping_add(1).max(1);
        let damage = frame
            .damage
            .and_then(|rect| intersect_rect(rect, full_rect(frame.extent)));
        self.damage_history
            .push_back((self.content_version, damage));
        while self.damage_history.len() > 64 {
            self.damage_history.pop_front();
        }
        for update in &frame.updates {
            let scene = match self.scenes.entry(update.key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(self.device.create_scene().map_err(app_error)?)
                }
            };
            for delta in &update.deltas {
                self.device
                    .apply_scene_delta(scene, delta)
                    .map_err(app_error)?;
            }
        }
        self.scenes.retain(|key, _| frame.live_scenes.contains(key));
        let previous_target_version = *self
            .target_versions
            .get(target_index)
            .ok_or_else(|| AppError::new("Vulkan scanout target index is invalid"))?;
        let render_damage = accumulated_damage(
            previous_target_version,
            self.content_version,
            &self.damage_history,
            frame.extent,
        );

        let scene_indices = self
            .scenes
            .keys()
            .copied()
            .enumerate()
            .map(|(index, key)| (key, index))
            .collect::<BTreeMap<_, _>>();
        let placements = frame
            .placements
            .iter()
            .map(|placement| {
                Ok(VulkanCompositePlacement {
                    scene_index: *scene_indices.get(&placement.scene).ok_or_else(|| {
                        AppError::new(format!(
                            "Vulkan desktop scene {:?} has no retained content",
                            placement.scene
                        ))
                    })?,
                    target: placement.target,
                    clip: placement.clip,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let mut scenes = self
            .scenes
            .values_mut()
            .map(|scene| VulkanCompositeScene { scene })
            .collect::<Vec<_>>();
        let receipt = {
            let target = self
                .targets
                .get_mut(target_index)
                .ok_or_else(|| AppError::new("Vulkan scanout target index is invalid"))?
                .target();
            let mut recording = self.device.begin_owned_frame().map_err(app_error)?;
            {
                let mut context = recording.context_mut();
                self.device
                    .render_composite(
                        &mut scenes,
                        &placements,
                        &mut context,
                        &target,
                        &RenderRequest {
                            force: true,
                            load: if render_damage.is_some() {
                                TargetLoad::Preserve
                            } else {
                                TargetLoad::Clear(ColorRgba8 {
                                    r: 0,
                                    g: 0,
                                    b: 0,
                                    a: 255,
                                })
                            },
                            store: TargetStore::Store,
                            region: render_damage,
                        },
                    )
                    .map_err(app_error)?;
            }
            recording
                .finish()
                .and_then(|frame| frame.submit())
                .map_err(app_error)?
        };
        self.targets[target_index].mark_initialized();
        self.target_versions[target_index] = self.content_version;
        self.completion_worker.submit(target_index, receipt)
    }

    pub(super) fn completion_event_fd(&self) -> i32 {
        self.completion_worker.event_fd()
    }

    pub(super) fn drain_completions(&self) -> Vec<VulkanCompletion> {
        self.completion_worker.drain()
    }
}

fn app_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(error.to_string())
}
