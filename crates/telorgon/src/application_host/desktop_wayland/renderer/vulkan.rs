use std::collections::{BTreeMap, VecDeque};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::compositor_render::{DmaBufImporter, dma_buf_image_id};
use crate::compositor_wayland::{
    BufferTransform, DmaBufFormat, DmaBufImage, ViewportSource, ViewportState, WaylandBufferId,
    WaylandSurfaceId,
};
use crate::core::{Affine2D, ColorRgba8, RectF, RectI, SizeF, SizeI};
use crate::presenter_vulkan_kms::GbmBuffer;
use crate::render::{
    BatchKey, BlendMode, ClipId, DrawItem, ImageAlphaMode, ImageId, ImageInstance,
    ImagePixelFormat, PipelineKind, PrimitiveKind, RenderBackend, RenderRequest, RenderScene,
    RenderSpatialNode, SpatialId, TargetLoad, TargetStore,
};
use crate::renderer_vulkan::{
    DeviceSelection, SubmissionReceipt, VulkanCompositePlacement, VulkanCompositeScene,
    VulkanConfig, VulkanDevice, VulkanDmaBufScanoutTarget, VulkanInstance,
    VulkanMaterializationTarget, VulkanScene,
};
use crate::scene::NodeId;

use super::super::geometry::{accumulated_damage, full_rect, intersect_rect};
use super::super::scene::{DesktopFrame, DesktopSceneKey};
use crate::application_host::{AppError, AppResult};

pub(in crate::application_host::desktop_wayland) struct VulkanCompletion {
    pub(in crate::application_host::desktop_wayland) slot_index: usize,
    pub(in crate::application_host::desktop_wayland) result: Result<(), String>,
    pub(in crate::application_host::desktop_wayland) dma_bufs: Vec<DmaBufRetirement>,
}

struct VulkanCompletionRequest {
    slot_index: usize,
    receipt: SubmissionReceipt,
    dma_bufs: Vec<DmaBufRetirement>,
}

pub(in crate::application_host::desktop_wayland) struct DmaBufPublication {
    pub surface: WaylandSurfaceId,
    pub revision: u64,
    pub buffer: WaylandBufferId,
    pub image: DmaBufImage,
    pub acquire: Option<OwnedFd>,
    pub buffer_scale: i32,
    pub buffer_transform: BufferTransform,
    pub viewport: Option<ViewportState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::application_host::desktop_wayland) struct DmaBufRetirement {
    pub surface: WaylandSurfaceId,
    pub revision: u64,
    pub buffer: WaylandBufferId,
}

pub(in crate::application_host::desktop_wayland) struct DmaBufQueueResult {
    pub extent: SizeI,
    pub pixel_format: ImagePixelFormat,
    pub alpha_mode: ImageAlphaMode,
    pub image: ImageId,
    pub replaced: Option<DmaBufRetirement>,
}

pub(in crate::application_host::desktop_wayland) struct DmaBufRelease {
    pub retirement: DmaBufRetirement,
    pub fence: OwnedFd,
}

pub(in crate::application_host::desktop_wayland) struct VulkanRenderResult {
    pub releases: Vec<DmaBufRelease>,
    pub discarded: Vec<DmaBufRetirement>,
}

struct PendingDmaBufPublication {
    publication: DmaBufPublication,
    content_version: u64,
    extent: SizeI,
    transform: Affine2D,
    alpha_mode: ImageAlphaMode,
}

struct DmaBufMaterialization {
    source: VulkanScene,
    target: VulkanMaterializationTarget,
    retirement: DmaBufRetirement,
    content_version: u64,
    lease_generation: u64,
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
                            dma_bufs: request.dma_bufs,
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

    fn submit(
        &self,
        slot_index: usize,
        receipt: SubmissionReceipt,
        dma_bufs: Vec<DmaBufRetirement>,
    ) -> AppResult<()> {
        self.requests
            .as_ref()
            .ok_or_else(|| AppError::new("Vulkan completion worker is stopped"))?
            .send(VulkanCompletionRequest {
                slot_index,
                receipt,
                dma_bufs,
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
    dma_buf_importer: Option<DmaBufImporter>,
    pending_dma_bufs: BTreeMap<DesktopSceneKey, PendingDmaBufPublication>,
    next_dma_buf_content_version: u64,
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
            let dma_buf_importer = DmaBufImporter::new(&device).ok();
            return Ok(Self {
                device,
                scenes: BTreeMap::new(),
                targets,
                content_version: 0,
                completion_worker: VulkanCompletionWorker::new()?,
                target_versions: vec![0; target_count],
                damage_history: VecDeque::new(),
                dma_buf_importer,
                pending_dma_bufs: BTreeMap::new(),
                next_dma_buf_content_version: 1,
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

    pub(super) fn dma_buf_formats(&self) -> Vec<DmaBufFormat> {
        self.dma_buf_importer
            .as_ref()
            .map_or_else(Vec::new, DmaBufImporter::advertised_formats)
    }

    pub(super) fn queue_dma_buf(
        &mut self,
        publication: DmaBufPublication,
    ) -> AppResult<DmaBufQueueResult> {
        let importer = self
            .dma_buf_importer
            .as_ref()
            .ok_or_else(|| AppError::new("Vulkan DMA-BUF import is unavailable"))?;
        let (_, alpha_mode) = importer
            .image_metadata(&publication.image)
            .map_err(app_error)?;
        let (extent, transform) = dma_buf_surface_mapping(
            publication.image.descriptor.size,
            publication.buffer_scale,
            publication.buffer_transform,
            publication.viewport,
            publication.image.descriptor.flags.y_invert,
        )?;
        let content_version = self.next_dma_buf_content_version;
        self.next_dma_buf_content_version = self
            .next_dma_buf_content_version
            .checked_add(1)
            .ok_or_else(|| AppError::new("DMA-BUF content version exhausted"))?;
        let scene = DesktopSceneKey::Surface(publication.surface.get());
        let replaced = self
            .pending_dma_bufs
            .insert(
                scene,
                PendingDmaBufPublication {
                    publication,
                    content_version,
                    extent,
                    transform,
                    alpha_mode,
                },
            )
            .map(|pending| DmaBufRetirement {
                surface: pending.publication.surface,
                revision: pending.publication.revision,
                buffer: pending.publication.buffer,
            });
        Ok(DmaBufQueueResult {
            extent,
            pixel_format: ImagePixelFormat::Rgba8,
            alpha_mode,
            image: dma_buf_image_id(),
            replaced,
        })
    }

    pub(super) fn cancel_dma_buf_surface(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Option<DmaBufRetirement> {
        self.pending_dma_bufs
            .remove(&DesktopSceneKey::Surface(surface.get()))
            .map(|pending| DmaBufRetirement {
                surface,
                revision: pending.publication.revision,
                buffer: pending.publication.buffer,
            })
    }

    pub(super) fn render(
        &mut self,
        target_index: usize,
        frame: DesktopFrame,
    ) -> AppResult<VulkanRenderResult> {
        self.content_version = self.content_version.wrapping_add(1).max(1);
        let damage = frame
            .damage
            .and_then(|rect| intersect_rect(rect, full_rect(frame.extent)));
        self.damage_history
            .push_back((self.content_version, damage));
        while self.damage_history.len() > 64 {
            self.damage_history.pop_front();
        }
        let (mut materializations, discarded) = self.prepare_dma_bufs(&frame.live_scenes)?;
        let materialized_scenes = materializations
            .iter()
            .map(|materialization| {
                DesktopSceneKey::Surface(materialization.retirement.surface.get())
            })
            .collect::<std::collections::BTreeSet<_>>();
        for update in &frame.updates {
            let scene = match self.scenes.entry(update.key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(self.device.create_scene().map_err(app_error)?)
                }
            };
            if matches!(update.key, DesktopSceneKey::Surface(_))
                && !materialized_scenes.contains(&update.key)
            {
                scene.remove_materialized_image(dma_buf_image_id());
            }
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
                    rounded_clips: placement.rounded_clips,
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
                for materialization in &mut materializations {
                    let target = materialization.target.target();
                    let placement = [VulkanCompositePlacement {
                        scene_index: 0,
                        target: full_rect(materialization.target.extent()),
                        clip: None,
                        rounded_clips: [None; 2],
                    }];
                    let mut source = [VulkanCompositeScene {
                        scene: &mut materialization.source,
                    }];
                    self.device
                        .render_composite(
                            &mut source,
                            &placement,
                            &mut context,
                            &target,
                            &RenderRequest {
                                force: true,
                                load: TargetLoad::Clear(ColorRgba8::rgba(0, 0, 0, 0)),
                                store: TargetStore::Store,
                                region: None,
                            },
                        )
                        .map_err(app_error)?;
                }
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
        let exported = receipt
            .export_dma_buf_release_sync_fds()
            .map_err(app_error)?;
        if exported.len() != materializations.len() {
            return Err(AppError::new(
                "Vulkan did not export one release fence per DMA-BUF materialization",
            ));
        }
        let mut releases = Vec::with_capacity(exported.len());
        for release in exported {
            let materialization = materializations
                .iter()
                .find(|materialization| {
                    materialization.content_version == release.content_version
                        && materialization.lease_generation == release.lease_generation
                })
                .ok_or_else(|| {
                    AppError::new("Vulkan returned an unknown DMA-BUF release generation")
                })?;
            releases.push(DmaBufRelease {
                retirement: materialization.retirement,
                fence: release.sync_fd,
            });
        }
        let dma_bufs = materializations
            .into_iter()
            .map(|materialization| materialization.retirement)
            .collect();
        self.completion_worker
            .submit(target_index, receipt, dma_bufs)?;
        Ok(VulkanRenderResult {
            releases,
            discarded,
        })
    }

    fn prepare_dma_bufs(
        &mut self,
        live_scenes: &std::collections::BTreeSet<DesktopSceneKey>,
    ) -> AppResult<(Vec<DmaBufMaterialization>, Vec<DmaBufRetirement>)> {
        let pending = std::mem::take(&mut self.pending_dma_bufs);
        let mut materializations = Vec::new();
        let mut discarded = Vec::new();
        for (scene_key, pending) in pending {
            let retirement = DmaBufRetirement {
                surface: pending.publication.surface,
                revision: pending.publication.revision,
                buffer: pending.publication.buffer,
            };
            if !live_scenes.contains(&scene_key) {
                discarded.push(retirement);
                continue;
            }
            let target = VulkanMaterializationTarget::new(&self.device, pending.extent)
                .map_err(app_error)?;
            let mut source = self.device.create_scene().map_err(app_error)?;
            let physical_extent = pending.publication.image.descriptor.size;
            let lease_generation = self
                .dma_buf_importer
                .as_mut()
                .ok_or_else(|| AppError::new("Vulkan DMA-BUF import is unavailable"))?
                .import_and_bind(
                    &self.device,
                    &mut source,
                    pending.publication.buffer,
                    pending.content_version,
                    pending.publication.image,
                    pending.publication.acquire,
                    vec![full_rect(physical_extent)],
                )
                .map_err(app_error)?;
            self.device
                .apply_scene_delta(
                    &mut source,
                    &dma_buf_source_delta(
                        physical_extent,
                        pending.extent,
                        pending.transform,
                        pending.content_version,
                        pending.alpha_mode,
                    ),
                )
                .map_err(app_error)?;
            let scene = match self.scenes.entry(scene_key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(self.device.create_scene().map_err(app_error)?)
                }
            };
            scene
                .bind_materialized_image(dma_buf_image_id(), &target, pending.alpha_mode)
                .map_err(app_error)?;
            materializations.push(DmaBufMaterialization {
                source,
                target,
                retirement,
                content_version: pending.content_version,
                lease_generation,
            });
        }
        Ok((materializations, discarded))
    }

    pub(super) fn completion_event_fd(&self) -> i32 {
        self.completion_worker.event_fd()
    }

    pub(super) fn drain_completions(&self) -> Vec<VulkanCompletion> {
        self.completion_worker.drain()
    }
}

fn dma_buf_surface_mapping(
    physical: SizeI,
    buffer_scale: i32,
    transform: BufferTransform,
    viewport: Option<ViewportState>,
    y_invert: bool,
) -> AppResult<(SizeI, Affine2D)> {
    if physical.width <= 0 || physical.height <= 0 || buffer_scale <= 0 {
        return Err(AppError::new("DMA-BUF surface geometry is invalid"));
    }
    let swap_axes = matches!(
        transform,
        BufferTransform::Rotate90
            | BufferTransform::Rotate270
            | BufferTransform::Flipped90
            | BufferTransform::Flipped270
    );
    let transformed = if swap_axes {
        SizeI {
            width: physical.height,
            height: physical.width,
        }
    } else {
        physical
    };
    if transformed.width % buffer_scale != 0 || transformed.height % buffer_scale != 0 {
        return Err(AppError::new(
            "DMA-BUF transformed extent is not divisible by its buffer scale",
        ));
    }
    let logical = SizeI {
        width: transformed.width / buffer_scale,
        height: transformed.height / buffer_scale,
    };
    let viewport = viewport.unwrap_or_default();
    let source = viewport.source.unwrap_or(ViewportSource {
        x: 0.0,
        y: 0.0,
        width: f64::from(logical.width),
        height: f64::from(logical.height),
    });
    if !source.x.is_finite()
        || !source.y.is_finite()
        || !source.width.is_finite()
        || !source.height.is_finite()
        || source.x < 0.0
        || source.y < 0.0
        || source.width <= 0.0
        || source.height <= 0.0
        || source.x + source.width > f64::from(logical.width)
        || source.y + source.height > f64::from(logical.height)
    {
        return Err(AppError::new(
            "DMA-BUF viewport source lies outside the logical surface",
        ));
    }
    let extent = viewport.destination.unwrap_or(SizeI {
        width: source.width as i32,
        height: source.height as i32,
    });
    if extent.width <= 0 || extent.height <= 0 {
        return Err(AppError::new("DMA-BUF viewport destination is invalid"));
    }

    let width = physical.width as f32;
    let height = physical.height as f32;
    let transformed = match transform {
        BufferTransform::Normal => Affine2D::IDENTITY,
        BufferTransform::Rotate90 => Affine2D {
            m11: 0.0,
            m12: 1.0,
            m21: -1.0,
            m22: 0.0,
            tx: height,
            ty: 0.0,
        },
        BufferTransform::Rotate180 => Affine2D {
            m11: -1.0,
            m12: 0.0,
            m21: 0.0,
            m22: -1.0,
            tx: width,
            ty: height,
        },
        BufferTransform::Rotate270 => Affine2D {
            m11: 0.0,
            m12: -1.0,
            m21: 1.0,
            m22: 0.0,
            tx: 0.0,
            ty: width,
        },
        BufferTransform::Flipped => Affine2D {
            m11: -1.0,
            m12: 0.0,
            m21: 0.0,
            m22: 1.0,
            tx: width,
            ty: 0.0,
        },
        BufferTransform::Flipped90 => Affine2D {
            m11: 0.0,
            m12: -1.0,
            m21: -1.0,
            m22: 0.0,
            tx: height,
            ty: width,
        },
        BufferTransform::Flipped180 => Affine2D {
            m11: 1.0,
            m12: 0.0,
            m21: 0.0,
            m22: -1.0,
            tx: 0.0,
            ty: height,
        },
        BufferTransform::Flipped270 => Affine2D {
            m11: 0.0,
            m12: 1.0,
            m21: 1.0,
            m22: 0.0,
            tx: 0.0,
            ty: 0.0,
        },
    };
    let origin = if y_invert {
        Affine2D {
            m11: 1.0,
            m12: 0.0,
            m21: 0.0,
            m22: -1.0,
            tx: 0.0,
            ty: height,
        }
    } else {
        Affine2D::IDENTITY
    };
    let transformed = transformed.then(origin);
    let scale = 1.0 / buffer_scale as f32;
    let logical_scale = Affine2D {
        m11: scale,
        m12: 0.0,
        m21: 0.0,
        m22: scale,
        tx: 0.0,
        ty: 0.0,
    };
    let viewport_scale_x = extent.width as f32 / source.width as f32;
    let viewport_scale_y = extent.height as f32 / source.height as f32;
    let viewport_transform = Affine2D {
        m11: viewport_scale_x,
        m12: 0.0,
        m21: 0.0,
        m22: viewport_scale_y,
        tx: -(source.x as f32) * viewport_scale_x,
        ty: -(source.y as f32) * viewport_scale_y,
    };
    Ok((
        extent,
        viewport_transform.then(logical_scale.then(transformed)),
    ))
}

fn dma_buf_source_delta(
    physical: SizeI,
    extent: SizeI,
    transform: Affine2D,
    content_version: u64,
    alpha_mode: ImageAlphaMode,
) -> crate::render::RenderSceneDelta {
    let mut source = RenderScene::default();
    source.background = ColorRgba8::rgba(0, 0, 0, 0);
    source.extent = SizeF {
        width: extent.width as f32,
        height: extent.height as f32,
    };
    source.damage.full = true;
    source.spatial_nodes.upsert(
        NodeId::new(2, 1),
        RenderSpatialNode {
            id: SpatialId(1),
            transform,
        },
    );
    source.images.upsert(
        NodeId::new(1, 1),
        ImageInstance {
            node: NodeId::new(1, 1),
            image: dma_buf_image_id(),
            tint: None,
            rect: RectF {
                x: 0.0,
                y: 0.0,
                width: physical.width as f32,
                height: physical.height as f32,
            },
            view_bounds: RectF {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
            },
            content_version,
            opacity: 1.0,
            clip: ClipId(0),
            spatial: SpatialId(1),
        },
    );
    source.set_draw_order(vec![DrawItem {
        kind: PrimitiveKind::Image,
        index: 0,
        batch: BatchKey {
            pipeline: PipelineKind::Image,
            resource: dma_buf_image_id().0,
            clip: ClipId(0),
            blend: if alpha_mode == ImageAlphaMode::Opaque {
                BlendMode::Opaque
            } else {
                BlendMode::Alpha
            },
            target: 0,
        },
    }]);
    source
        .take_delta()
        .expect("new DMA-BUF source scene always produces a delta")
}

fn app_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rect_close(actual: RectF, expected: RectF) {
        for (actual, expected) in [
            (actual.x, expected.x),
            (actual.y, expected.y),
            (actual.width, expected.width),
            (actual.height, expected.height),
        ] {
            assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
        }
    }

    #[test]
    fn every_wayland_buffer_transform_maps_into_its_logical_extent() {
        let physical = SizeI {
            width: 120,
            height: 80,
        };
        for transform in [
            BufferTransform::Normal,
            BufferTransform::Rotate90,
            BufferTransform::Rotate180,
            BufferTransform::Rotate270,
            BufferTransform::Flipped,
            BufferTransform::Flipped90,
            BufferTransform::Flipped180,
            BufferTransform::Flipped270,
        ] {
            let (extent, mapping) =
                dma_buf_surface_mapping(physical, 2, transform, None, false).unwrap();
            let expected = if matches!(
                transform,
                BufferTransform::Rotate90
                    | BufferTransform::Rotate270
                    | BufferTransform::Flipped90
                    | BufferTransform::Flipped270
            ) {
                SizeI {
                    width: 40,
                    height: 60,
                }
            } else {
                SizeI {
                    width: 60,
                    height: 40,
                }
            };
            assert_eq!(extent, expected);
            assert_rect_close(
                mapping.transform_rect(RectF {
                    x: 0.0,
                    y: 0.0,
                    width: physical.width as f32,
                    height: physical.height as f32,
                }),
                RectF {
                    x: 0.0,
                    y: 0.0,
                    width: expected.width as f32,
                    height: expected.height as f32,
                },
            );
        }
    }

    #[test]
    fn dma_buf_viewport_crop_maps_exactly_to_its_destination() {
        let (extent, mapping) = dma_buf_surface_mapping(
            SizeI {
                width: 200,
                height: 100,
            },
            1,
            BufferTransform::Normal,
            Some(ViewportState {
                source: Some(ViewportSource {
                    x: 50.0,
                    y: 20.0,
                    width: 100.0,
                    height: 50.0,
                }),
                destination: Some(SizeI {
                    width: 400,
                    height: 200,
                }),
            }),
            false,
        )
        .unwrap();

        assert_eq!(
            extent,
            SizeI {
                width: 400,
                height: 200
            }
        );
        assert_rect_close(
            mapping.transform_rect(RectF {
                x: 50.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
            }),
            RectF {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 200.0,
            },
        );
    }

    #[test]
    fn dma_buf_y_invert_is_normalized_before_surface_transform() {
        let physical = SizeI {
            width: 120,
            height: 80,
        };
        let (_, normal) =
            dma_buf_surface_mapping(physical, 2, BufferTransform::Normal, None, false).unwrap();
        let (_, inverted) =
            dma_buf_surface_mapping(physical, 2, BufferTransform::Normal, None, true).unwrap();
        let point = crate::core::PointF { x: 10.0, y: 6.0 };

        assert_eq!(
            normal.transform_point(point),
            crate::core::PointF { x: 5.0, y: 3.0 }
        );
        assert_eq!(
            inverted.transform_point(point),
            crate::core::PointF { x: 5.0, y: 37.0 }
        );
    }
}
