use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
#[cfg(feature = "profiler")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::compositor_render::{shm_image_resource, transform_surface_image};
use crate::compositor_wayland::{
    ButtonState as WaylandButtonState, ClientLimits, CompositorAction, CursorImage,
    NativeCompositor, OutputDescription, OutputMode, OutputState, OutputTransform,
    PointerConstraintKind, PointerConstraintState, ResizeEdge, SeatCapabilities,
    SeatState as WaylandSeatState, SurfaceRole, ToplevelState, WaylandSurfaceId,
};
use crate::core::{MonotonicInstant, PointF, PointI, RectI, SizeF, SizeI};
use crate::platform_linux::{
    KeyDirection, LibInputContext, LinuxInputEventKind, LinuxSeat, SeatState, XkbKeyboard,
};
use crate::presenter_vulkan_kms::{
    AtomicRequest, ConnectorStatus, DRM_FORMAT_ARGB8888, DRM_FORMAT_MOD_LINEAR,
    DRM_FORMAT_XRGB8888, DRM_PLANE_TYPE_CURSOR, DRM_PLANE_TYPE_PRIMARY, FrameSlot, FrameSlotState,
    GbmBuffer, GbmDevice, KmsCrtcId, KmsDevice, KmsFramebuffer, KmsObjectProperties, KmsPlaneId,
    KmsPropertyObject, KmsTopology, ScanoutFormat,
};
use crate::render::{
    BatchKey, BlendMode, DrawItem, ImageAlphaMode, ImageColorEncoding, ImageId, ImageInstance,
    ImageResource, ImageResourceUpdate, PipelineKind, PrimitiveKind, RenderBackend, RenderRequest,
    RenderScene, RenderTargetInfo, TargetLoad, TargetStore,
};
use crate::renderer_software::{SoftwareRenderer, SoftwareScene, SoftwareSurface, SoftwareTarget};
use crate::renderer_vulkan::{
    DeviceSelection, SubmissionReceipt, VulkanConfig, VulkanDevice, VulkanDmaBufScanoutTarget,
    VulkanInstance, VulkanScene,
};
use crate::runtime::CompositionDriver;
use crate::scene::NodeId;
use crate::wayland_server::{Display, ProtocolCatalog, ProtocolSourcePaths};
use crate::{
    AssetBundle, AssetMediaCache, AssetRasterSize, PointerConfiguration, PointerGraphic,
    PointerIcon, PointerRequest, PointerResolution, PointerTheme, WindowAction, WindowChromeModel,
    WindowChromeSnapshot, WindowChromeState, WindowResizeEdge, resolve_pointer,
};

use crate::application_host::declaration::ShellActionHandler;
use crate::application_host::{
    AppError, AppResult, ComposedAppRuntime, LinuxDesktopConfig, ReadyDesktopEnvironment, Renderer,
    ShellWidgetAnchor, ShellWidgetExtent, WindowFrameFactory,
};

struct Layer {
    runtime: ComposedAppRuntime,
    renderer: SoftwareRenderer,
    scene: SoftwareScene,
    surface: SoftwareSurface,
}

struct VulkanScanout {
    device: VulkanDevice,
    scene: VulkanScene,
    source: RenderScene,
    targets: Vec<VulkanDmaBufScanoutTarget>,
    content_version: u64,
    completion_worker: VulkanCompletionWorker,
    target_versions: Vec<u64>,
    damage_history: VecDeque<(u64, Option<RectI>)>,
}

struct RenderedCursor {
    rgba: Vec<u8>,
    size: SizeI,
    hotspot: PointI,
    premultiplied: bool,
}

#[cfg(feature = "profiler")]
#[derive(Clone, Copy)]
enum PointerCursorPath {
    SoftwareDamage,
    Hidden,
    Deferred,
    Unchanged,
}

#[cfg(feature = "profiler")]
struct PointerBatchProbe {
    enabled: bool,
    dispatch_started: Option<Instant>,
    dispatch_duration_ns: u64,
    handler_started: Option<Instant>,
    events: u64,
    newest_event_us: Option<u64>,
}

#[cfg(feature = "profiler")]
impl PointerBatchProbe {
    fn begin() -> Self {
        let enabled = crate::profiler::input_recording_enabled(
            crate::profiler::InputRecordingSource::PointerMotion,
        );
        Self {
            enabled,
            dispatch_started: enabled.then(Instant::now),
            dispatch_duration_ns: 0,
            handler_started: None,
            events: 0,
            newest_event_us: None,
        }
    }

    fn dispatch_completed(&mut self) {
        if let Some(started) = self.dispatch_started {
            self.dispatch_duration_ns = duration_ns(started.elapsed());
            self.handler_started = Some(Instant::now());
        }
    }

    fn observe_motion(&mut self, event_time_us: u64) {
        if !self.enabled {
            return;
        }
        self.events = self.events.saturating_add(1);
        self.newest_event_us = Some(
            self.newest_event_us
                .map_or(event_time_us, |newest| newest.max(event_time_us)),
        );
    }

    fn newest_event_us(&self) -> Option<u64> {
        self.enabled.then_some(self.newest_event_us).flatten()
    }

    fn has_motion(&self) -> bool {
        self.enabled && self.events != 0
    }

    fn finish(&self, path: PointerCursorPath) {
        if !self.enabled || self.events == 0 {
            return;
        }
        crate::profiler::record_instant_value(
            "input.libinput.pointer_motion.pipeline.events_per_batch",
            self.events,
        );
        crate::profiler::record_instant_value(
            "input.libinput.pointer_motion.pipeline.dispatch_duration_ns",
            self.dispatch_duration_ns,
        );
        crate::profiler::record_instant_value(
            "input.libinput.pointer_motion.pipeline.batch_handler_duration_ns",
            self.handler_started
                .map_or(0, |started| duration_ns(started.elapsed())),
        );
        crate::profiler::record_instant(match path {
            PointerCursorPath::SoftwareDamage => {
                "input.libinput.pointer_motion.pipeline.path.software_damage"
            }
            PointerCursorPath::Hidden => "input.libinput.pointer_motion.pipeline.path.hidden",
            PointerCursorPath::Deferred => "input.libinput.pointer_motion.pipeline.path.deferred",
            PointerCursorPath::Unchanged => {
                "input.libinput.pointer_motion.pipeline.path.position_unchanged"
            }
        });
    }
}

#[cfg(feature = "profiler")]
fn duration_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(feature = "profiler")]
fn record_pointer_event_latency(label: &'static str, event_time_us: u64) {
    if !crate::profiler::input_recording_enabled(
        crate::profiler::InputRecordingSource::PointerMotion,
    ) {
        return;
    }
    if let Some(now_us) = crate::platform_linux::monotonic_time_microseconds() {
        crate::profiler::record_instant_value(
            label,
            now_us.saturating_sub(event_time_us).saturating_mul(1_000),
        );
    }
}

const HARDWARE_CURSOR_BUFFER_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CursorSnapshot {
    serial: u64,
    buffer: Option<usize>,
    position: PointI,
    hotspot: PointI,
    visible: bool,
}

#[derive(Debug, Default)]
struct CursorCommitTracker {
    desired: CursorSnapshot,
    applied_serial: u64,
    current_buffer: Option<usize>,
    in_flight: Option<CursorSnapshot>,
    software_fallback_requested: bool,
}

impl CursorCommitTracker {
    fn move_to(&mut self, position: PointI) {
        if self.desired.position != position {
            self.desired.position = position;
            if self.desired.visible {
                self.bump_serial();
            }
        }
    }

    fn show(&mut self, buffer: usize, hotspot: PointI) {
        if !self.desired.visible
            || self.desired.buffer != Some(buffer)
            || self.desired.hotspot != hotspot
        {
            self.desired.visible = true;
            self.desired.buffer = Some(buffer);
            self.desired.hotspot = hotspot;
            self.bump_serial();
        }
    }

    fn hide(&mut self) {
        if self.desired.visible {
            self.desired.visible = false;
            self.bump_serial();
        }
    }

    fn request_software_fallback(&mut self) {
        self.software_fallback_requested = true;
        self.hide();
    }

    fn desired_submission(&self) -> Option<CursorSnapshot> {
        (self.in_flight.is_none() && self.desired.serial != self.applied_serial)
            .then_some(self.desired)
    }

    fn mark_submitted(&mut self, snapshot: CursorSnapshot) -> AppResult<()> {
        if self.in_flight.is_some() || snapshot.serial != self.desired.serial {
            return Err(AppError::new(
                "atomic cursor submission did not match the desired cursor generation",
            ));
        }
        self.in_flight = Some(snapshot);
        Ok(())
    }

    fn mark_completed(&mut self, snapshot: CursorSnapshot) -> AppResult<()> {
        if self.in_flight != Some(snapshot) {
            return Err(AppError::new(
                "DRM completed an unexpected atomic cursor generation",
            ));
        }
        self.in_flight = None;
        self.applied_serial = snapshot.serial;
        self.current_buffer = snapshot.visible.then_some(snapshot.buffer).flatten();
        Ok(())
    }

    fn reusable_buffer(&self, count: usize) -> Option<usize> {
        let in_flight = self.in_flight.and_then(|snapshot| snapshot.buffer);
        self.desired
            .buffer
            .filter(|buffer| Some(*buffer) != self.current_buffer && Some(*buffer) != in_flight)
            .or_else(|| {
                (0..count).find(|buffer| {
                    Some(*buffer) != self.current_buffer && Some(*buffer) != in_flight
                })
            })
    }

    fn ready_to_retire(&self) -> bool {
        self.software_fallback_requested
            && self.current_buffer.is_none()
            && self.in_flight.is_none()
    }

    fn bump_serial(&mut self) {
        self.desired.serial = self.desired.serial.wrapping_add(1).max(1);
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingKmsCommit {
    primary_slot: Option<usize>,
    cursor: Option<CursorSnapshot>,
    #[cfg(feature = "profiler")]
    cursor_event_us: Option<u64>,
}

struct HardwareCursor<'gbm, 'kms, 'fd> {
    // Framebuffers must be removed before the GBM buffers that back them are destroyed.
    framebuffers: Vec<KmsFramebuffer<'kms>>,
    buffers: Vec<GbmBuffer<'gbm, 'fd>>,
    extent: SizeI,
    plane: KmsPlaneId,
    properties: KmsObjectProperties,
    has_hotspot_properties: bool,
    state: CursorCommitTracker,
    image_signature: Option<u64>,
}

impl<'gbm, 'kms, 'fd> HardwareCursor<'gbm, 'kms, 'fd> {
    fn new(
        gbm: &'gbm GbmDevice<'fd>,
        kms: &'kms KmsDevice,
        plane: KmsPlaneId,
        properties: KmsObjectProperties,
    ) -> AppResult<Self> {
        for name in [
            "FB_ID", "CRTC_ID", "SRC_X", "SRC_Y", "SRC_W", "SRC_H", "CRTC_X", "CRTC_Y", "CRTC_W",
            "CRTC_H",
        ] {
            if properties.named(name).is_none() {
                return Err(AppError::new(format!(
                    "DRM cursor plane has no required atomic property {name}"
                )));
            }
        }
        let has_hotspot_properties = kms.cursor_plane_hotspot_capable();
        if has_hotspot_properties
            && (properties.named("HOTSPOT_X").is_none() || properties.named("HOTSPOT_Y").is_none())
        {
            return Err(AppError::new(
                "DRM cursor-hotspot capability was enabled without hotspot properties",
            ));
        }
        let extent = kms.cursor_size().map_err(app_error)?;
        let mut buffers = Vec::with_capacity(HARDWARE_CURSOR_BUFFER_COUNT);
        for _ in 0..HARDWARE_CURSOR_BUFFER_COUNT {
            buffers.push(gbm.allocate_cursor(extent).map_err(app_error)?);
        }
        let framebuffers = buffers
            .iter()
            .map(|buffer| kms.add_framebuffer(buffer).map_err(app_error))
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Self {
            framebuffers,
            buffers,
            extent,
            plane,
            properties,
            has_hotspot_properties,
            state: CursorCommitTracker::default(),
            image_signature: None,
        })
    }

    fn set_image(&mut self, cursor: &RenderedCursor) -> AppResult<()> {
        if cursor.size.width <= 0
            || cursor.size.height <= 0
            || cursor.size.width > self.extent.width
            || cursor.size.height > self.extent.height
        {
            return Err(AppError::new(
                "cursor image has an invalid DRM hardware-cursor extent",
            ));
        }
        let source_length = (cursor.size.width as usize)
            .checked_mul(cursor.size.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| AppError::new("cursor image byte length overflow"))?;
        if cursor.rgba.len() != source_length {
            return Err(AppError::new(
                "cursor image pixels do not match its declared extent",
            ));
        }
        let signature = cursor_image_signature(cursor);
        if self.image_signature == Some(signature) {
            let buffer = self.state.desired.buffer.ok_or_else(|| {
                AppError::new("atomic cursor image signature has no staged buffer")
            })?;
            self.state.show(buffer, cursor.hotspot);
            return Ok(());
        }
        let mut pixels = vec![0_u8; self.extent.width as usize * self.extent.height as usize * 4];
        let source_stride = cursor.size.width.max(0) as usize * 4;
        let target_stride = self.extent.width as usize * 4;
        for row in 0..cursor.size.height.max(0) as usize {
            let source = &cursor.rgba[row * source_stride..(row + 1) * source_stride];
            let target = &mut pixels[row * target_stride..row * target_stride + source_stride];
            target.copy_from_slice(source);
            if !cursor.premultiplied {
                for pixel in target.chunks_exact_mut(4) {
                    let alpha = u16::from(pixel[3]);
                    for channel in &mut pixel[..3] {
                        *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                    }
                }
            }
        }
        let next = self
            .state
            .reusable_buffer(self.buffers.len())
            .ok_or_else(|| AppError::new("no retired hardware-cursor buffer is available"))?;
        self.buffers[next]
            .map_write()
            .map_err(app_error)?
            .write_rgba8(&pixels)
            .map_err(app_error)?;
        self.state.show(next, cursor.hotspot);
        self.image_signature = Some(signature);
        Ok(())
    }

    fn move_to(&mut self, position: PointF) {
        self.state.move_to(PointI {
            x: position.x.round() as i32,
            y: position.y.round() as i32,
        });
    }

    fn hide(&mut self) {
        self.state.hide();
    }

    fn append_desired(
        &self,
        request: &mut AtomicRequest<'_>,
        crtc: KmsCrtcId,
    ) -> AppResult<Option<CursorSnapshot>> {
        let Some(snapshot) = self.state.desired_submission() else {
            return Ok(None);
        };
        if snapshot.visible {
            let buffer = snapshot.buffer.ok_or_else(|| {
                AppError::new("visible atomic cursor generation has no framebuffer")
            })?;
            let framebuffer = self.framebuffers.get(buffer).ok_or_else(|| {
                AppError::new("atomic cursor generation names an invalid framebuffer")
            })?;
            let destination = RectI {
                x: snapshot.position.x.saturating_sub(snapshot.hotspot.x),
                y: snapshot.position.y.saturating_sub(snapshot.hotspot.y),
                width: self.extent.width,
                height: self.extent.height,
            };
            request
                .set_plane(
                    self.plane,
                    &self.properties,
                    crtc,
                    framebuffer.id(),
                    RectI {
                        x: 0,
                        y: 0,
                        width: self.extent.width,
                        height: self.extent.height,
                    },
                    destination,
                )
                .map_err(app_error)?;
            if self.has_hotspot_properties {
                request
                    .set_cursor_hotspot(self.plane, &self.properties, snapshot.hotspot)
                    .map_err(app_error)?;
            }
        } else {
            request
                .disable_plane(self.plane, &self.properties)
                .map_err(app_error)?;
        }
        Ok(Some(snapshot))
    }

    fn mark_submitted(&mut self, snapshot: CursorSnapshot) -> AppResult<()> {
        self.state.mark_submitted(snapshot)
    }

    fn mark_completed(&mut self, snapshot: CursorSnapshot) -> AppResult<()> {
        self.state.mark_completed(snapshot)
    }

    fn needs_commit(&self) -> bool {
        self.state.desired_submission().is_some()
    }

    fn request_software_fallback(&mut self) {
        self.state.request_software_fallback();
    }

    fn software_fallback_requested(&self) -> bool {
        self.state.software_fallback_requested
    }

    fn ready_to_retire(&self) -> bool {
        self.state.ready_to_retire()
    }
}

struct VulkanCompletion {
    slot_index: usize,
    result: Result<(), String>,
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

struct InputReadyState {
    ready: AtomicBool,
    #[cfg(feature = "profiler")]
    callback_time_us: AtomicU64,
}

impl InputReadyState {
    const fn new(ready: bool) -> Self {
        Self {
            ready: AtomicBool::new(ready),
            #[cfg(feature = "profiler")]
            callback_time_us: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
struct EventNotifier {
    fd: Arc<OwnedFd>,
}

impl EventNotifier {
    fn new(context: &str) -> AppResult<Self> {
        let raw = unsafe {
            crate::platform_linux::ffi::eventfd(
                0,
                crate::platform_linux::ffi::EFD_CLOEXEC | crate::platform_linux::ffi::EFD_NONBLOCK,
            )
        };
        if raw < 0 {
            Err(AppError::new(format!("failed to create {context} eventfd")))
        } else {
            Ok(Self {
                fd: Arc::new(unsafe { OwnedFd::from_raw_fd(raw) }),
            })
        }
    }

    fn event_fd(&self) -> i32 {
        self.fd.as_raw_fd()
    }

    fn notify(&self) {
        let value = 1_u64;
        let _ = unsafe {
            crate::platform_linux::ffi::write(
                self.fd.as_raw_fd(),
                std::ptr::from_ref(&value).cast(),
                std::mem::size_of::<u64>(),
            )
        };
    }

    fn drain(&self) {
        let mut value = 0_u64;
        loop {
            let read = unsafe {
                crate::platform_linux::ffi::read(
                    self.fd.as_raw_fd(),
                    std::ptr::from_mut(&mut value).cast(),
                    std::mem::size_of::<u64>(),
                )
            };
            if read != std::mem::size_of::<u64>() as isize {
                break;
            }
        }
    }
}

unsafe extern "C" fn mark_external_fd_ready(
    _fd: i32,
    _mask: u32,
    data: *mut std::ffi::c_void,
) -> i32 {
    let Some(ready) = std::ptr::NonNull::new(data.cast::<AtomicBool>()) else {
        return 0;
    };
    unsafe { ready.as_ref() }.store(true, Ordering::Release);
    0
}

unsafe extern "C" fn mark_input_fd_ready(_fd: i32, _mask: u32, data: *mut std::ffi::c_void) -> i32 {
    let Some(state) = std::ptr::NonNull::new(data.cast::<InputReadyState>()) else {
        return 0;
    };
    let state = unsafe { state.as_ref() };
    #[cfg(feature = "profiler")]
    if crate::profiler::pointer_move_events_enabled()
        && let Some(now_us) = crate::platform_linux::monotonic_time_microseconds()
    {
        state.callback_time_us.store(now_us, Ordering::Release);
    }
    state.ready.store(true, Ordering::Release);
    0
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

const VULKAN_STAGING_MIN_BYTES_PER_SLOT: u64 = 4 * 1024 * 1024;
// A target can still require a full catch-up upload after startup, damage-history loss, or broad
// scene changes. Reserve that worst case plus the one-image scene buffers and copy alignment.
const VULKAN_STAGING_HEADROOM_BYTES_PER_SLOT: u64 = 1024 * 1024;

fn vulkan_staging_budget_bytes(extent: SizeI, frame_slots: usize) -> AppResult<u64> {
    let frame_bytes = u64::try_from(extent.width)
        .ok()
        .and_then(|width| {
            u64::try_from(extent.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| AppError::new("Vulkan scanout extent overflows its upload budget"))?;
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

impl VulkanScanout {
    fn new(
        buffers: &[crate::presenter_vulkan_kms::GbmBuffer<'_, '_>],
        extent: SizeI,
    ) -> AppResult<Self> {
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
            let scene = device.create_scene().map_err(app_error)?;
            let mut source = RenderScene::default();
            source.extent = SizeF {
                width: extent.width as f32,
                height: extent.height as f32,
            };
            let node = NodeId::new(0, 1);
            let rectangle = crate::core::RectF {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
            };
            source.images.upsert(
                node,
                ImageInstance {
                    node,
                    image: ImageId(1),
                    tint: None,
                    rect: rectangle,
                    view_bounds: rectangle,
                    content_version: 1,
                    opacity: 1.0,
                    clip: crate::render::ClipId(0),
                    spatial: crate::render::SpatialId(0),
                },
            );
            source.set_draw_order(vec![DrawItem {
                kind: PrimitiveKind::Image,
                index: 0,
                batch: BatchKey {
                    pipeline: PipelineKind::Image,
                    resource: 1,
                    clip: crate::render::ClipId(0),
                    blend: BlendMode::Opaque,
                    target: 0,
                },
            }]);
            let target_count = targets.len();
            return Ok(Self {
                device,
                scene,
                source,
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

    fn render(
        &mut self,
        target_index: usize,
        extent: SizeI,
        rgba: &[u8],
        damage: Option<RectI>,
    ) -> AppResult<()> {
        self.content_version = self.content_version.wrapping_add(1).max(1);
        let damage = damage.and_then(|rect| intersect_rect(rect, full_rect(extent)));
        if self.content_version == 1 || damage.is_none() {
            self.source
                .set_image_resource(ImageResource {
                    image: ImageId(1),
                    content_version: self.content_version,
                    extent,
                    color_encoding: ImageColorEncoding::Srgb,
                    alpha_mode: ImageAlphaMode::Opaque,
                    pixels_rgba8: Arc::from(rgba),
                })
                .map_err(app_error)?;
        } else if let Some(rect) = damage {
            self.source
                .update_image_resource_region(ImageResourceUpdate {
                    image: ImageId(1),
                    content_version: self.content_version,
                    extent,
                    rect,
                    row_bytes: rect.width as usize * 4,
                    color_encoding: ImageColorEncoding::Srgb,
                    alpha_mode: ImageAlphaMode::Opaque,
                    pixels_rgba8: Arc::from(copy_rgba_region(rgba, extent, rect)),
                })
                .map_err(app_error)?;
        }
        self.damage_history
            .push_back((self.content_version, damage));
        while self.damage_history.len() > 64 {
            self.damage_history.pop_front();
        }
        let previous_target_version = *self
            .target_versions
            .get(target_index)
            .ok_or_else(|| AppError::new("Vulkan scanout target index is invalid"))?;
        let render_damage = accumulated_damage(
            previous_target_version,
            self.content_version,
            &self.damage_history,
            extent,
        );
        let delta = self
            .source
            .take_delta()
            .ok_or_else(|| AppError::new("Vulkan scanout scene produced no frame delta"))?;
        self.device
            .apply_scene_delta(&mut self.scene, &delta)
            .map_err(app_error)?;
        let target = self
            .targets
            .get_mut(target_index)
            .ok_or_else(|| AppError::new("Vulkan scanout target index is invalid"))?;
        let target = target.target();
        let mut frame = self.device.begin_owned_frame().map_err(app_error)?;
        {
            let mut context = frame.context_mut();
            self.device
                .render(
                    &mut self.scene,
                    &mut context,
                    &target,
                    &RenderRequest {
                        force: true,
                        load: if render_damage.is_some() {
                            TargetLoad::Preserve
                        } else {
                            TargetLoad::Clear(self.source.background)
                        },
                        store: TargetStore::Store,
                        region: render_damage,
                    },
                )
                .map_err(app_error)?;
        }
        let receipt = frame
            .finish()
            .and_then(|frame| frame.submit())
            .map_err(app_error)?;
        self.target_versions[target_index] = self.content_version;
        self.completion_worker.submit(target_index, receipt)
    }

    fn completion_event_fd(&self) -> i32 {
        self.completion_worker.event_fd()
    }

    fn drain_completions(&self) -> Vec<VulkanCompletion> {
        self.completion_worker.drain()
    }
}

impl Layer {
    fn new(driver: CompositionDriver, extent: SizeI, assets: AssetBundle) -> AppResult<Self> {
        let renderer = SoftwareRenderer;
        let scene = renderer.create_scene().map_err(app_error)?;
        let mut runtime = ComposedAppRuntime::from_composition_driver(driver, extent)?;
        let mut media = AssetMediaCache::new(assets).map_err(app_error)?;
        for resource in media.preload_render_resources().map_err(app_error)? {
            runtime.set_image_resource(resource)?;
        }
        Ok(Self {
            runtime,
            renderer,
            scene,
            surface: SoftwareSurface::default(),
        })
    }

    fn render(&mut self, extent: SizeI, now: u64, force: bool) -> AppResult<&[u8]> {
        if self.runtime.extent()
            != (SizeF {
                width: extent.width as f32,
                height: extent.height as f32,
            })
        {
            self.runtime.resize(extent)?;
        }
        self.runtime
            .prepare_frame(MonotonicInstant::from_nanos(now), force)?;
        while let Some(delta) = self.runtime.pop_scene_delta() {
            self.renderer
                .apply_scene_delta(&mut self.scene, &delta)
                .map_err(app_error)?;
        }
        let target = SoftwareTarget::new(RenderTargetInfo::full(extent));
        let clear = self.scene.background();
        let mut frame = self.surface.begin_frame();
        self.renderer
            .render(
                &mut self.scene,
                &mut frame,
                &target,
                &RenderRequest {
                    force,
                    load: TargetLoad::Clear(clear),
                    store: TargetStore::Store,
                    region: None,
                },
            )
            .map_err(app_error)?;
        Ok(self.surface.pixels_rgba8())
    }

    fn pointer_motion(&mut self, position: PointF, now: MonotonicInstant) -> bool {
        self.runtime
            .queue_input(crate::input::InputEvent::mouse_moved(position));
        self.runtime.flush_input(now).frame_needed_after
    }

    fn pointer_button(&mut self, pressed: bool, now: MonotonicInstant) -> bool {
        self.runtime
            .queue_input(crate::input::InputEvent::mouse_button(
                crate::input::PointerButton::PRIMARY,
                if pressed {
                    crate::input::ButtonState::Pressed
                } else {
                    crate::input::ButtonState::Released
                },
            ));
        self.runtime.flush_input(now).frame_needed_after
    }

    fn has_pending_runtime_turn(&self, now: MonotonicInstant) -> bool {
        self.runtime.has_pending_runtime_turn(now)
            || (self.runtime.needs_frame() && !self.runtime.animation_active())
    }

    fn next_deadline(&self) -> Option<MonotonicInstant> {
        self.runtime.next_deadline()
    }

    fn animation_active(&self) -> bool {
        self.runtime.animation_active()
    }
}

fn route_frame_pointer_motion(
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    position: PointF,
    now: MonotonicInstant,
) -> bool {
    let mut repaint = false;
    for (surface, frame) in frames {
        let local = windows.get(surface).map_or(
            PointF {
                x: -1_000_000.0,
                y: -1_000_000.0,
            },
            |window| PointF {
                x: position.x - window.position.x as f32,
                y: position.y - window.position.y as f32,
            },
        );
        repaint |= frame.layer.pointer_motion(local, now);
    }
    repaint
}

fn route_frame_pointer_button(
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    pressed: bool,
    now: MonotonicInstant,
) -> bool {
    frames.values_mut().fold(false, |repaint, frame| {
        repaint | frame.layer.pointer_button(pressed, now)
    })
}

fn refresh_window_frames(
    factory: Option<&WindowFrameFactory>,
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    wayland: &NativeCompositor<'_>,
    config: &LinuxDesktopConfig,
    assets: AssetBundle,
    fallback_icon: &crate::AppIconProfile,
    wake: &EventNotifier,
    now: u64,
) -> AppResult<()> {
    let Some(factory) = factory else {
        frames.clear();
        for window in windows.values_mut() {
            window.chrome_outer = None;
            window.chrome_content_offset = None;
            window.chrome = None;
        }
        return Ok(());
    };

    frames.retain(|surface, _| {
        windows.get(surface).is_some_and(|window| {
            window.role == SurfaceRole::XdgToplevel && window_is_decorated(window)
        })
    });

    let active = wayland
        .core()
        .seats
        .get(&1)
        .and_then(|seat| seat.keyboard_focus)
        .map(|focus| focus.surface);
    let surfaces = windows
        .iter()
        .filter(|(_, window)| {
            window.role == SurfaceRole::XdgToplevel && window_is_decorated(window)
        })
        .map(|(surface, _)| *surface)
        .collect::<Vec<_>>();
    let mut updates = Vec::with_capacity(surfaces.len());

    for surface in surfaces {
        let window = windows
            .get(&surface)
            .expect("frame candidates came from live windows");
        let metadata = wayland.toplevel_metadata(surface);
        let title = metadata.map_or_else(String::new, |metadata| {
            if metadata.title.is_empty() {
                metadata.application_id.clone()
            } else {
                metadata.title.clone()
            }
        });
        let state = if window.fullscreen {
            WindowChromeState::Fullscreen
        } else if window.maximized {
            WindowChromeState::Maximized
        } else {
            WindowChromeState::Normal
        };
        let protocol_icon = wayland.toplevel_icon(surface);
        let icon_name = protocol_icon
            .and_then(|icon| icon.name.clone())
            .or_else(|| fallback_icon.name().map(str::to_owned));
        let icon_image = protocol_icon.and_then(|icon| {
            icon.images
                .iter()
                .min_by_key(|image| {
                    let logical = image.image.descriptor.size.width / image.scale.max(1);
                    logical.abs_diff(32)
                })
                .map(|image| (icon.revision, image.clone()))
        });
        let icon_image_id = icon_image
            .as_ref()
            .map(|(revision, _)| toplevel_icon_image_id(surface, *revision));
        let mut model = WindowChromeModel::new(u64::from(surface.get()), title)
            .state(state)
            .active(active == Some(surface));
        if let Some(name) = icon_name {
            model = model.app_icon_name(name);
        }
        if let Some(image) = icon_image_id {
            model = model.app_icon_image(image);
        } else if let Some(icon) = fallback_icon.preferred(32) {
            model = model.app_icon(icon);
        }
        let fallback_outer = legacy_window_outer(window, config);
        let previous_outer = frames
            .get(&surface)
            .map_or(fallback_outer, |frame| frame.outer);
        let rebuild = frames
            .get(&surface)
            .is_none_or(|frame| frame.model != model);
        if rebuild {
            let mut driver = factory.compose(model.clone());
            driver.set_wake({
                let wake = wake.clone();
                move || wake.notify()
            });
            let mut layer = Layer::new(driver, previous_outer, assets)?;
            if let Some((revision, icon)) = &icon_image {
                let mut resource =
                    shm_image_resource(icon.buffer, (*revision).max(1), icon.image.clone())
                        .map_err(app_error)?;
                resource.image = icon_image_id.expect("image source produced an image id");
                resource.content_version = (*revision).max(1);
                layer.runtime.set_image_resource(resource)?;
            }
            frames.insert(
                surface,
                WindowFrameLayer {
                    model: model.clone(),
                    layer,
                    snapshot: None,
                    outer: previous_outer,
                    pixels: Vec::new(),
                },
            );
        }

        let frame = frames
            .get_mut(&surface)
            .expect("window frame was created above");
        frame.model = model;
        frame.layer.render(frame.outer, now, rebuild)?;
        let mut snapshot =
            WindowChromeSnapshot::derive(frame.layer.runtime.ui(), frame.layer.runtime.layout())
                .map_err(app_error)?;
        let content_width = snapshot.content.bounds.width.round().max(1.0) as i32;
        let content_height = snapshot.content.bounds.height.round().max(1.0) as i32;
        let corrected = SizeI {
            width: frame
                .outer
                .width
                .saturating_add(window.size.width.saturating_sub(content_width))
                .max(1),
            height: frame
                .outer
                .height
                .saturating_add(window.size.height.saturating_sub(content_height))
                .max(1),
        };
        if corrected != frame.outer {
            frame.outer = corrected;
            frame.layer.render(frame.outer, now, true)?;
            snapshot = WindowChromeSnapshot::derive(
                frame.layer.runtime.ui(),
                frame.layer.runtime.layout(),
            )
            .map_err(app_error)?;
        }
        frame.snapshot = Some(snapshot.clone());
        frame.pixels = frame.layer.render(frame.outer, now, false)?.to_vec();
        updates.push((
            surface,
            frame.outer,
            PointI {
                x: snapshot.content.bounds.x.round() as i32,
                y: snapshot.content.bounds.y.round() as i32,
            },
            snapshot,
        ));
    }

    for (surface, outer, content_offset, snapshot) in updates {
        if let Some(window) = windows.get_mut(&surface) {
            window.chrome_outer = Some(outer);
            window.chrome_content_offset = Some(content_offset);
            window.chrome = Some(snapshot);
        }
    }
    Ok(())
}

fn toplevel_icon_image_id(surface: WaylandSurfaceId, revision: u64) -> ImageId {
    let folded = revision as u32 ^ (revision >> 32) as u32;
    ImageId(0x6000_0000_u32 ^ surface.get().rotate_left(11) ^ folded.rotate_left(3))
}

struct WidgetLayer {
    anchor: ShellWidgetAnchor,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
    reserved_space: i32,
    layer: Layer,
}

struct WindowFrameLayer {
    model: WindowChromeModel,
    layer: Layer,
    snapshot: Option<WindowChromeSnapshot>,
    outer: SizeI,
    pixels: Vec<u8>,
}

fn desktop_runtime_schedule(
    background: &Layer,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    pointer: Option<&Layer>,
    icons: &[(String, Layer)],
    widgets: &[WidgetLayer],
    now: MonotonicInstant,
) -> (bool, Option<MonotonicInstant>, bool) {
    let layers = std::iter::once(background)
        .chain(frames.values().map(|frame| &frame.layer))
        .chain(pointer)
        .chain(icons.iter().map(|(_, layer)| layer))
        .chain(widgets.iter().map(|widget| &widget.layer));
    let mut immediate = false;
    let mut deadline = None::<MonotonicInstant>;
    let mut animation = false;
    for layer in layers {
        immediate |= layer.has_pending_runtime_turn(now);
        animation |= layer.animation_active();
        if let Some(candidate) = layer.next_deadline() {
            deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
        }
    }
    (immediate, deadline, animation)
}

struct ClientWindow {
    revision: u64,
    role: SurfaceRole,
    parent: Option<WaylandSurfaceId>,
    offset: PointI,
    server_decorated: bool,
    position: PointI,
    size: SizeI,
    requested_size: SizeI,
    restore_geometry: Option<(PointI, SizeI)>,
    maximized: bool,
    fullscreen: bool,
    minimized: bool,
    chrome_outer: Option<SizeI>,
    chrome_content_offset: Option<PointI>,
    chrome: Option<WindowChromeSnapshot>,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
enum WindowInteraction {
    Move {
        surface: WaylandSurfaceId,
        pointer_start: PointF,
        position_start: PointI,
    },
    Resize {
        surface: WaylandSurfaceId,
        edge: ResizeEdge,
        pointer_start: PointF,
        position_start: PointI,
        size_start: SizeI,
    },
}

impl WindowInteraction {
    fn begin_move(
        windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
        surface: WaylandSurfaceId,
        pointer_start: PointF,
    ) -> Option<Self> {
        let window = windows.get(&surface)?;
        Some(Self::Move {
            surface,
            pointer_start,
            position_start: window.position,
        })
    }

    fn begin_resize(
        windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
        surface: WaylandSurfaceId,
        edge: ResizeEdge,
        pointer_start: PointF,
    ) -> Option<Self> {
        let window = windows.get(&surface)?;
        Some(Self::Resize {
            surface,
            edge,
            pointer_start,
            position_start: window.position,
            size_start: window.requested_size,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecorationHit {
    Frame,
    Titlebar,
    Resize(ResizeEdge),
    Close,
    Maximize,
    Minimize,
    SystemMenu,
    ShellAction(crate::ShellActionId),
}

pub(crate) fn run(application: ReadyDesktopEnvironment) -> AppResult<()> {
    let _profiler = crate::application_host::profiler::ManagedProfiler::start(
        crate::application_host::profiler::ProfileTarget::DesktopEnvironment,
    )?;
    #[cfg(feature = "profiler")]
    let _profile_view = {
        let view = crate::profiler::ProfileViewId::PRIMARY;
        let _ = crate::profiler::register_view(view, "Desktop environment output");
        crate::profiler::enter_view(Some(view))
    };
    let (_name, compositor, widgets, renderer, config, assets, pointer_config, app_icon_profile) =
        application.into_parts()?;
    let pointer_theme = pointer_config.load_theme(assets).map_err(app_error)?;
    let mut pointer_media = AssetMediaCache::new(assets).map_err(app_error)?;
    let (mut background, window_frame, mut pointer, mut icons, shell_actions) =
        compositor.into_runtime_parts();

    let seat = LinuxSeat::open().map_err(app_error)?;
    seat.dispatch(0).map_err(app_error)?;
    let input = LibInputContext::new(&seat, &config.seat_name).map_err(app_error)?;
    let drm_path = config
        .drm_device
        .to_str()
        .ok_or_else(|| AppError::new("DRM device path is not UTF-8"))?;
    let drm_seat_device = seat.open_device(drm_path).map_err(app_error)?;
    let kms =
        KmsDevice::new(drm_seat_device.try_clone_fd().map_err(app_error)?).map_err(app_error)?;
    let topology = KmsTopology::query(&kms).map_err(app_error)?;
    let connector = topology
        .connectors
        .iter()
        .find(|connector| {
            connector.status == ConnectorStatus::Connected && !connector.modes.is_empty()
        })
        .ok_or_else(|| AppError::new("no connected KMS output with a display mode was found"))?;
    let mode_index = connector
        .modes
        .iter()
        .position(|mode| mode.preferred())
        .unwrap_or(0);
    let mode = connector.modes[mode_index];
    let extent = mode.size();
    let refresh_period =
        Duration::from_nanos(1_000_000_000_000_u64 / u64::from(mode.refresh_millihertz().max(1)));
    let (crtc_index, crtc_raw) = topology
        .crtcs
        .iter()
        .copied()
        .enumerate()
        .find(|(index, _)| {
            let mask = 1_u32.checked_shl(*index as u32).unwrap_or(0);
            connector.possible_crtcs_mask & mask != 0
                && topology.planes.iter().any(|plane| {
                    plane.possible_crtcs_mask & mask != 0
                        && plane.formats.contains(&DRM_FORMAT_XRGB8888)
                        && is_plane_type(&kms, plane.id.get(), DRM_PLANE_TYPE_PRIMARY)
                })
        })
        .ok_or_else(|| AppError::new("no KMS CRTC has an XRGB8888 primary-plane candidate"))?;
    let crtc = KmsCrtcId::from_raw(crtc_raw)
        .ok_or_else(|| AppError::new("KMS returned CRTC identity zero"))?;
    let plane = topology
        .planes
        .iter()
        .find(|plane| {
            plane.possible_crtcs_mask & (1_u32.checked_shl(crtc_index as u32).unwrap_or(0)) != 0
                && plane.formats.contains(&DRM_FORMAT_XRGB8888)
                && is_plane_type(&kms, plane.id.get(), DRM_PLANE_TYPE_PRIMARY)
        })
        .ok_or_else(|| AppError::new("no compatible KMS plane was found"))?;
    let connector_properties =
        KmsTopology::object_properties(&kms, connector.id.get(), KmsPropertyObject::Connector)
            .map_err(app_error)?;
    let crtc_properties = KmsTopology::object_properties(&kms, crtc.get(), KmsPropertyObject::Crtc)
        .map_err(app_error)?;
    let plane_properties =
        KmsTopology::object_properties(&kms, plane.id.get(), KmsPropertyObject::Plane)
            .map_err(app_error)?;
    let mode_blob = kms.create_mode_blob(&mode).map_err(app_error)?;
    let gbm = GbmDevice::new(kms.fd()).map_err(app_error)?;
    let scanout_format = ScanoutFormat {
        fourcc: DRM_FORMAT_XRGB8888,
        modifier: DRM_FORMAT_MOD_LINEAR,
    };
    let mut scanout_buffers = vec![
        gbm.allocate(extent, scanout_format, &[DRM_FORMAT_MOD_LINEAR])
            .map_err(app_error)?,
        gbm.allocate(extent, scanout_format, &[DRM_FORMAT_MOD_LINEAR])
            .map_err(app_error)?,
        gbm.allocate(extent, scanout_format, &[DRM_FORMAT_MOD_LINEAR])
            .map_err(app_error)?,
    ];
    let framebuffers = scanout_buffers
        .iter()
        .map(|buffer| kms.add_framebuffer(buffer).map_err(app_error))
        .collect::<AppResult<Vec<_>>>()?;
    let mut frame_slots = framebuffers
        .iter()
        .enumerate()
        .map(|(index, framebuffer)| FrameSlot::new(index, framebuffer.id()))
        .collect::<Vec<_>>();
    let mut vulkan_scanout = match renderer {
        Renderer::Vulkan => Some(VulkanScanout::new(&scanout_buffers, extent)?),
        Renderer::Auto => VulkanScanout::new(&scanout_buffers, extent).ok(),
        Renderer::Software => None,
    };
    let cursor_plane = topology.planes.iter().find(|candidate| {
        candidate.possible_crtcs_mask & (1_u32.checked_shl(crtc_index as u32).unwrap_or(0)) != 0
            && candidate.formats.contains(&DRM_FORMAT_ARGB8888)
            && is_plane_type(&kms, candidate.id.get(), DRM_PLANE_TYPE_CURSOR)
    });
    let mut hardware_cursor = cursor_plane.and_then(|cursor_plane| {
        KmsTopology::object_properties(&kms, cursor_plane.id.get(), KmsPropertyObject::Plane)
            .map_err(app_error)
            .and_then(|properties| HardwareCursor::new(&gbm, &kms, cursor_plane.id, properties))
            .ok()
    });
    #[cfg(feature = "profiler")]
    crate::profiler::record_instant(if hardware_cursor.is_some() {
        "presentation.cursor.hardware_available"
    } else {
        "presentation.cursor.software_fallback"
    });

    let display = Display::new().map_err(app_error)?;
    let catalog =
        ProtocolCatalog::load_desktop(&ProtocolSourcePaths::standard_linux()).map_err(app_error)?;
    let mut wayland =
        NativeCompositor::new(&display, catalog, ClientLimits::default()).map_err(app_error)?;
    wayland
        .add_output(&display, 1, output_state(connector, mode_index, &config)?)
        .map_err(app_error)?;
    wayland
        .add_seat(
            &display,
            1,
            WaylandSeatState::new(
                &config.seat_name,
                SeatCapabilities {
                    pointer: true,
                    keyboard: true,
                    touch: true,
                },
            ),
        )
        .map_err(app_error)?;
    let _socket = match config.socket_name.as_deref() {
        Some(name) => {
            display.add_socket(name).map_err(app_error)?;
            name.to_owned()
        }
        None => display.add_socket_auto().map_err(app_error)?,
    };

    // Libwayland already owns the compositor's poll loop. Register every external readiness
    // source with it so input, seat changes, DRM flips, and GPU completions wake the same owner
    // thread without a fixed Wayland-only sleep.
    let runtime_wake = EventNotifier::new("desktop runtime wake")?;
    background.set_wake({
        let wake = runtime_wake.clone();
        move || wake.notify()
    });
    if let Some(driver) = &mut pointer {
        driver.set_wake({
            let wake = runtime_wake.clone();
            move || wake.notify()
        });
    }
    for (_, driver) in &mut icons {
        driver.set_wake({
            let wake = runtime_wake.clone();
            move || wake.notify()
        });
    }

    let seat_ready = Box::new(AtomicBool::new(true));
    let input_ready = Box::new(InputReadyState::new(true));
    let kms_ready = Box::new(AtomicBool::new(false));
    let runtime_ready = Box::new(AtomicBool::new(false));
    let vulkan_completion_ready = vulkan_scanout
        .as_ref()
        .map(|_| Box::new(AtomicBool::new(false)));
    let external_mask = crate::wayland_server::ffi::WL_EVENT_READABLE
        | crate::wayland_server::ffi::WL_EVENT_HANGUP
        | crate::wayland_server::ffi::WL_EVENT_ERROR;
    let _seat_source = unsafe {
        display.event_loop().add_fd(
            seat.event_fd(),
            external_mask,
            Some(mark_external_fd_ready),
            std::ptr::from_ref(seat_ready.as_ref()).cast_mut().cast(),
        )
    }
    .map_err(app_error)?;
    let _input_source = unsafe {
        display.event_loop().add_fd(
            input.event_fd(),
            external_mask,
            Some(mark_input_fd_ready),
            std::ptr::from_ref(input_ready.as_ref()).cast_mut().cast(),
        )
    }
    .map_err(app_error)?;
    let _kms_source = unsafe {
        display.event_loop().add_fd(
            kms.fd().as_raw_fd(),
            external_mask,
            Some(mark_external_fd_ready),
            std::ptr::from_ref(kms_ready.as_ref()).cast_mut().cast(),
        )
    }
    .map_err(app_error)?;
    let _runtime_source = unsafe {
        display.event_loop().add_fd(
            runtime_wake.event_fd(),
            external_mask,
            Some(mark_external_fd_ready),
            std::ptr::from_ref(runtime_ready.as_ref()).cast_mut().cast(),
        )
    }
    .map_err(app_error)?;
    let _vulkan_completion_source = match (&vulkan_scanout, &vulkan_completion_ready) {
        (Some(vulkan), Some(ready)) => Some(
            unsafe {
                display.event_loop().add_fd(
                    vulkan.completion_event_fd(),
                    external_mask,
                    Some(mark_external_fd_ready),
                    std::ptr::from_ref(ready.as_ref()).cast_mut().cast(),
                )
            }
            .map_err(app_error)?,
        ),
        _ => None,
    };

    let keyboard = XkbKeyboard::from_names(None, None, None, None, None).map_err(app_error)?;
    let keymap = keyboard.keymap_file().map_err(app_error)?;
    wayland
        .keyboard_keymap(1, keymap.fd(), keymap.size())
        .map_err(app_error)?;

    let mut background = Layer::new(background, extent, assets)?;
    let mut frame_layers = BTreeMap::<WaylandSurfaceId, WindowFrameLayer>::new();
    let mut pointer = pointer
        .map(|driver| Layer::new(driver, config.pointer_extent, assets))
        .transpose()?;
    let mut icon_layers = icons
        .into_iter()
        .map(|(name, driver)| {
            Ok((
                name,
                Layer::new(
                    driver,
                    SizeI {
                        width: 24,
                        height: 24,
                    },
                    assets,
                )?,
            ))
        })
        .collect::<AppResult<Vec<_>>>()?;
    // Mount icon components immediately so semantic icon declarations are validated and retained.
    for (_, icon) in &mut icon_layers {
        let _ = icon.render(
            SizeI {
                width: 24,
                height: 24,
            },
            0,
            true,
        )?;
    }
    let mut widgets = widgets
        .into_iter()
        .map(|widget| {
            let (_, anchor, width, height, reserved_space, mut driver) =
                widget.into_runtime_parts();
            driver.set_wake({
                let wake = runtime_wake.clone();
                move || wake.notify()
            });
            let widget_extent = resolved_widget_extent(extent, width, height);
            Ok(WidgetLayer {
                anchor,
                width,
                height,
                reserved_space: reserved_space.round().max(0.0) as i32,
                layer: Layer::new(driver, widget_extent, assets)?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let work_area = shell_work_area(extent, &widgets);

    let start = Instant::now();
    let mut pointer_position = PointF {
        x: extent.width as f32 * 0.5,
        y: extent.height as f32 * 0.5,
    };
    if let Some(cursor) = &mut hardware_cursor {
        cursor.move_to(pointer_position);
    }
    let mut drag_position = pointer_position;
    let mut pointer_focus = None;
    let mut touch_targets = BTreeMap::<i32, WaylandSurfaceId>::new();
    let mut windows = BTreeMap::<WaylandSurfaceId, ClientWindow>::new();
    let mut stacking_order = Vec::<WaylandSurfaceId>::new();
    let mut session_locked = false;
    let mut pending_session_lock = None;
    let mut window_interaction = None;
    let mut next_window_offset = 0_i32;
    let mut next_frame_id = 1_u64;
    let mut ready_scanout = VecDeque::<usize>::new();
    let mut pending_kms_commit = None::<PendingKmsCommit>;
    let mut current_scanout = None::<usize>;
    let mut first_modeset = true;
    let mut repaint = true;
    let mut retained_base_output = Vec::<u8>::new();
    let mut retained_output = Vec::<u8>::new();
    let mut retained_output_valid = false;
    let mut previous_software_cursor = None::<RectI>;
    let mut software_content_version = 0_u64;
    let mut software_target_versions = vec![0_u64; frame_slots.len()];
    let mut software_damage_history = VecDeque::<(u64, Option<RectI>)>::new();
    #[cfg(feature = "profiler")]
    let mut pending_primary_pointer_event_us = None::<u64>;
    #[cfg(feature = "profiler")]
    let mut pending_deferred_cursor_event_us = None::<u64>;
    #[cfg(feature = "profiler")]
    let mut frame_pointer_event_us = vec![None::<u64>; frame_slots.len()];
    #[cfg(feature = "profiler")]
    let mut previous_pointer_batch_us = None::<u64>;
    let mut keyboard = keyboard;

    loop {
        let mut presentation_completed = false;
        let mut pointer_motion_seen = false;
        let mut cursor_position_dirty = false;
        let mut pointer_primary_dirty = false;
        #[cfg(feature = "profiler")]
        let mut latest_pointer_event_this_turn = None::<u64>;
        let mut other_work_seen = repaint;
        let cursor_image_at_turn_start = wayland
            .core()
            .seats
            .get(&1)
            .expect("seat registered")
            .cursor;
        let schedule_now = MonotonicInstant::from_nanos(
            start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        );
        let (runtime_immediate, runtime_deadline, runtime_animation) = desktop_runtime_schedule(
            &background,
            &frame_layers,
            pointer.as_ref(),
            &icon_layers,
            &widgets,
            schedule_now,
        );
        let scanout_available = frame_slots
            .iter()
            .any(|slot| slot.state == FrameSlotState::Available);
        let cursor_commit_ready = !first_modeset
            && hardware_cursor
                .as_ref()
                .is_some_and(HardwareCursor::needs_commit);
        let immediate_work = ((repaint || runtime_immediate) && scanout_available)
            || (pending_kms_commit.is_none() && (!ready_scanout.is_empty() || cursor_commit_ready))
            || runtime_ready.load(Ordering::Acquire)
            || seat_ready.load(Ordering::Acquire)
            || input_ready.ready.load(Ordering::Acquire)
            || kms_ready.load(Ordering::Acquire)
            || vulkan_completion_ready
                .as_ref()
                .is_some_and(|ready| ready.load(Ordering::Acquire));
        let wait = if immediate_work {
            Some(Duration::ZERO)
        } else if let Some(deadline) = runtime_deadline {
            Some(Duration::from_nanos(
                deadline.as_nanos().saturating_sub(schedule_now.as_nanos()),
            ))
        } else if runtime_animation && pending_kms_commit.is_none() {
            Some(refresh_period)
        } else {
            None
        };
        display.dispatch_and_flush(wait).map_err(app_error)?;

        if seat_ready.swap(false, Ordering::AcqRel) {
            seat.dispatch(0).map_err(app_error)?;
        }
        if seat.state() == SeatState::Enabled && input_ready.ready.swap(false, Ordering::AcqRel) {
            #[cfg(feature = "profiler")]
            let input_callback_time_us = input_ready.callback_time_us.swap(0, Ordering::AcqRel);
            #[cfg(feature = "profiler")]
            let mut pointer_probe = PointerBatchProbe::begin();
            #[cfg(feature = "profiler")]
            let input_dispatch_started_us = pointer_probe
                .enabled
                .then(crate::platform_linux::monotonic_time_microseconds)
                .flatten();
            input.dispatch().map_err(app_error)?;
            #[cfg(feature = "profiler")]
            pointer_probe.dispatch_completed();
            #[cfg(feature = "profiler")]
            let input_observed_us = crate::platform_linux::monotonic_time_microseconds();
            while let Some(event) = input.next_event() {
                let time_microseconds = event.time_microseconds;
                if matches!(
                    event.kind,
                    LinuxInputEventKind::PointerMotion { .. }
                        | LinuxInputEventKind::PointerAbsolute { .. }
                ) {
                    pointer_motion_seen = true;
                    #[cfg(feature = "profiler")]
                    pointer_probe.observe_motion(time_microseconds);
                } else {
                    other_work_seen = true;
                }
                #[cfg(feature = "profiler")]
                {
                    record_libinput_event(event.kind, time_microseconds, input_observed_us);
                }
                let time = time_microseconds / 1_000;
                let time = u32::try_from(time).unwrap_or(u32::MAX);
                match event.kind {
                    LinuxInputEventKind::PointerMotion {
                        delta,
                        unaccelerated,
                    } => {
                        let scene_follows_pointer = window_interaction.is_some()
                            || (wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none());
                        other_work_seen |= scene_follows_pointer;
                        let constraint =
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                None
                            } else {
                                wayland.pointer_constraint(1)
                            };
                        let locked = constraint.as_ref().is_some_and(|constraint| {
                            constraint.kind == PointerConstraintKind::Locked
                        });
                        if !locked {
                            let previous_position = pointer_position;
                            let proposed = PointF {
                                x: (pointer_position.x + delta.x)
                                    .clamp(0.0, extent.width as f32 - 1.0),
                                y: (pointer_position.y + delta.y)
                                    .clamp(0.0, extent.height as f32 - 1.0),
                            };
                            pointer_position = constraint.as_ref().map_or(proposed, |constraint| {
                                constrain_pointer(
                                    &windows,
                                    pointer_position,
                                    proposed,
                                    constraint,
                                    &config,
                                )
                            });
                            if let Some(interaction) = window_interaction {
                                apply_window_interaction(
                                    &mut wayland,
                                    &mut windows,
                                    interaction,
                                    pointer_position,
                                    extent,
                                    &config,
                                )?;
                            } else {
                                route_pointer_motion(
                                    &display,
                                    &mut wayland,
                                    &windows,
                                    &stacking_order,
                                    session_locked,
                                    &mut pointer_focus,
                                    pointer_position,
                                    time,
                                    &config,
                                )?;
                            }
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                drag_position = pointer_position;
                            }
                            if pointer_position.x != previous_position.x
                                || pointer_position.y != previous_position.y
                            {
                                repaint |= route_frame_pointer_motion(
                                    &mut frame_layers,
                                    &windows,
                                    pointer_position,
                                    MonotonicInstant::from_nanos(
                                        time_microseconds.saturating_mul(1_000),
                                    ),
                                );
                                if window_interaction.is_none() {
                                    set_decoration_pointer_cursor(
                                        &mut wayland,
                                        &frame_layers,
                                        &windows,
                                        &stacking_order,
                                        pointer_focus,
                                        pointer_position,
                                        &config,
                                        &icon_layers,
                                    );
                                }
                                cursor_position_dirty = true;
                                pointer_primary_dirty |= scene_follows_pointer;
                            }
                        }
                        if pointer_focus.is_some()
                            && window_interaction.is_none()
                            && (!wayland.drag_active(1) || wayland.drag_touch_slot(1).is_some())
                        {
                            let _ = wayland.relative_pointer_motion(
                                1,
                                time_microseconds,
                                delta,
                                unaccelerated,
                            );
                        }
                    }
                    LinuxInputEventKind::PointerAbsolute { normalized } => {
                        let scene_follows_pointer = window_interaction.is_some()
                            || (wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none());
                        other_work_seen |= scene_follows_pointer;
                        let proposed = PointF {
                            x: normalized.x.clamp(0.0, 1.0) * (extent.width - 1) as f32,
                            y: normalized.y.clamp(0.0, 1.0) * (extent.height - 1) as f32,
                        };
                        let constraint =
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                None
                            } else {
                                wayland.pointer_constraint(1)
                            };
                        if !constraint.as_ref().is_some_and(|constraint| {
                            constraint.kind == PointerConstraintKind::Locked
                        }) {
                            let previous_position = pointer_position;
                            pointer_position = constraint.as_ref().map_or(proposed, |constraint| {
                                constrain_pointer(
                                    &windows,
                                    pointer_position,
                                    proposed,
                                    constraint,
                                    &config,
                                )
                            });
                            if let Some(interaction) = window_interaction {
                                apply_window_interaction(
                                    &mut wayland,
                                    &mut windows,
                                    interaction,
                                    pointer_position,
                                    extent,
                                    &config,
                                )?;
                            } else {
                                route_pointer_motion(
                                    &display,
                                    &mut wayland,
                                    &windows,
                                    &stacking_order,
                                    session_locked,
                                    &mut pointer_focus,
                                    pointer_position,
                                    time,
                                    &config,
                                )?;
                            }
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                drag_position = pointer_position;
                            }
                            if pointer_position.x != previous_position.x
                                || pointer_position.y != previous_position.y
                            {
                                repaint |= route_frame_pointer_motion(
                                    &mut frame_layers,
                                    &windows,
                                    pointer_position,
                                    MonotonicInstant::from_nanos(
                                        time_microseconds.saturating_mul(1_000),
                                    ),
                                );
                                if window_interaction.is_none() {
                                    set_decoration_pointer_cursor(
                                        &mut wayland,
                                        &frame_layers,
                                        &windows,
                                        &stacking_order,
                                        pointer_focus,
                                        pointer_position,
                                        &config,
                                        &icon_layers,
                                    );
                                }
                                cursor_position_dirty = true;
                                pointer_primary_dirty |= scene_follows_pointer;
                            }
                        }
                    }
                    LinuxInputEventKind::PointerButton { button, pressed } => {
                        if button == 0x110 && !session_locked {
                            repaint |= route_frame_pointer_button(
                                &mut frame_layers,
                                pressed,
                                MonotonicInstant::from_nanos(
                                    time_microseconds.saturating_mul(1_000),
                                ),
                            );
                        }
                        if pressed
                            && button == 0x110
                            && !session_locked
                            && let Some((surface, hit)) = hit_test_decoration(
                                &windows,
                                &stacking_order,
                                pointer_position,
                                &config,
                                &icon_layers,
                            )
                        {
                            set_decoration_pointer_cursor(
                                &mut wayland,
                                &frame_layers,
                                &windows,
                                &stacking_order,
                                pointer_focus,
                                pointer_position,
                                &config,
                                &icon_layers,
                            );
                            focus_toplevel(&display, &mut wayland, &windows, Some(surface))?;
                            match hit {
                                DecorationHit::Titlebar => {
                                    window_interaction = WindowInteraction::begin_move(
                                        &windows,
                                        surface,
                                        pointer_position,
                                    );
                                }
                                DecorationHit::Resize(edge) => {
                                    window_interaction = WindowInteraction::begin_resize(
                                        &windows,
                                        surface,
                                        edge,
                                        pointer_position,
                                    );
                                }
                                DecorationHit::Close => {
                                    wayland.close_toplevel(surface).map_err(app_error)?;
                                }
                                DecorationHit::Maximize => {
                                    let maximized = windows
                                        .get(&surface)
                                        .is_some_and(|window| !window.maximized);
                                    set_window_maximized(
                                        &mut wayland,
                                        &mut windows,
                                        surface,
                                        maximized,
                                        work_area,
                                        &config,
                                    )?;
                                }
                                DecorationHit::Minimize => {
                                    if let Some(window) = windows.get_mut(&surface) {
                                        window.minimized = true;
                                        stacking_order.retain(|candidate| *candidate != surface);
                                    }
                                }
                                DecorationHit::ShellAction(action) => {
                                    invoke_shell_action(
                                        &shell_actions,
                                        action,
                                        surface,
                                        &frame_layers,
                                    );
                                }
                                DecorationHit::Frame | DecorationHit::SystemMenu => {}
                            }
                            repaint = true;
                            continue;
                        }
                        if pointer_focus.is_some() {
                            let serial = display.next_serial();
                            wayland
                                .pointer_button(
                                    1,
                                    time,
                                    button,
                                    if pressed {
                                        WaylandButtonState::Pressed
                                    } else {
                                        WaylandButtonState::Released
                                    },
                                    serial,
                                )
                                .map_err(app_error)?;
                            if pressed {
                                focus_toplevel(&display, &mut wayland, &windows, pointer_focus)?;
                            }
                        }
                        if !pressed
                            && wayland.drag_active(1)
                            && wayland.drag_touch_slot(1).is_none()
                            && wayland
                                .core()
                                .seats
                                .get(&1)
                                .is_some_and(|seat| seat.pressed_buttons().is_empty())
                        {
                            wayland.drop_drag(1).map_err(app_error)?;
                            update_pointer_focus(
                                &display,
                                &mut wayland,
                                &windows,
                                &stacking_order,
                                session_locked,
                                &mut pointer_focus,
                                pointer_position,
                                &config,
                            )?;
                        }
                        if !pressed && let Some(interaction) = window_interaction.take() {
                            finish_window_interaction(&mut wayland, &windows, interaction)?;
                            set_decoration_pointer_cursor(
                                &mut wayland,
                                &frame_layers,
                                &windows,
                                &stacking_order,
                                pointer_focus,
                                pointer_position,
                                &config,
                                &icon_layers,
                            );
                        }
                    }
                    LinuxInputEventKind::KeyboardKey { keycode, pressed } => {
                        let direction = if pressed {
                            KeyDirection::Down
                        } else {
                            KeyDirection::Up
                        };
                        keyboard.update_key(keycode, direction);
                        if pointer_focus.is_some() {
                            let serial = display.next_serial();
                            let _ = wayland.keyboard_key(
                                1,
                                time,
                                keycode,
                                if pressed {
                                    WaylandButtonState::Pressed
                                } else {
                                    WaylandButtonState::Released
                                },
                                serial,
                            );
                            let modifiers = keyboard.modifiers();
                            let _ = wayland.keyboard_modifiers(
                                1,
                                serial,
                                modifiers.depressed,
                                modifiers.latched,
                                modifiers.locked,
                                modifiers.group,
                            );
                        }
                    }
                    LinuxInputEventKind::PointerAxis {
                        horizontal,
                        vertical,
                        discrete_x,
                        discrete_y,
                    } => {
                        if pointer_focus.is_some() {
                            let _ = wayland.pointer_axis(
                                1, time, horizontal, vertical, discrete_x, discrete_y,
                            );
                        }
                    }
                    LinuxInputEventKind::TouchDown { slot, normalized } => {
                        if slot >= 0 {
                            let position = normalized_output_position(normalized, extent);
                            if let Some(surface) = hit_test_surface(
                                &windows,
                                &stacking_order,
                                position,
                                &config,
                                session_locked,
                            ) {
                                let local =
                                    surface_local_position(&windows, surface, position, &config);
                                wayland
                                    .touch_down(
                                        1,
                                        surface,
                                        time,
                                        slot,
                                        local,
                                        display.next_serial(),
                                    )
                                    .map_err(app_error)?;
                                touch_targets.insert(slot, surface);
                            }
                        }
                    }
                    LinuxInputEventKind::TouchMotion { slot, normalized } => {
                        let position = normalized_output_position(normalized, extent);
                        if wayland.drag_touch_slot(1) == Some(slot) {
                            let target = hit_test_surface(
                                &windows,
                                &stacking_order,
                                position,
                                &config,
                                session_locked,
                            );
                            let local = target.map_or(position, |surface| {
                                surface_local_position(&windows, surface, position, &config)
                            });
                            wayland
                                .drag_motion(1, target, time, local)
                                .map_err(app_error)?;
                            drag_position = position;
                            repaint = true;
                        } else if let Some(surface) = touch_targets.get(&slot).copied() {
                            let local =
                                surface_local_position(&windows, surface, position, &config);
                            wayland
                                .touch_motion(1, time, slot, local)
                                .map_err(app_error)?;
                        }
                    }
                    LinuxInputEventKind::TouchUp { slot } => {
                        if touch_targets.remove(&slot).is_some() {
                            wayland
                                .touch_up(1, time, slot, display.next_serial())
                                .map_err(app_error)?;
                        }
                        if wayland.drag_touch_slot(1) == Some(slot) {
                            wayland.drop_drag(1).map_err(app_error)?;
                            repaint = true;
                        }
                    }
                    LinuxInputEventKind::TouchCancel => {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                        if wayland.drag_touch_slot(1).is_some() {
                            wayland.cancel_drag(1).map_err(app_error)?;
                            repaint = true;
                        }
                    }
                    LinuxInputEventKind::DeviceAdded | LinuxInputEventKind::DeviceRemoved => {}
                }
            }
            #[cfg(feature = "profiler")]
            let mut cursor_path = PointerCursorPath::Unchanged;
            #[cfg(feature = "profiler")]
            {
                latest_pointer_event_this_turn = pointer_probe.newest_event_us();
            }
            if cursor_position_dirty {
                let cursor_hidden = wayland
                    .core()
                    .seats
                    .get(&1)
                    .is_some_and(|seat| matches!(seat.cursor, CursorImage::Hidden));
                if cursor_hidden {
                    if let Some(cursor) = &mut hardware_cursor {
                        cursor.hide();
                    }
                    #[cfg(feature = "profiler")]
                    {
                        cursor_path = PointerCursorPath::Hidden;
                    }
                } else if hardware_cursor
                    .as_ref()
                    .is_some_and(|cursor| !cursor.software_fallback_requested())
                {
                    hardware_cursor
                        .as_mut()
                        .expect("atomic hardware cursor checked")
                        .move_to(pointer_position);
                    #[cfg(feature = "profiler")]
                    {
                        cursor_path = PointerCursorPath::Deferred;
                        pending_deferred_cursor_event_us = latest_pointer_event_this_turn;
                    }
                } else {
                    pointer_primary_dirty = true;
                    #[cfg(feature = "profiler")]
                    {
                        cursor_path = PointerCursorPath::SoftwareDamage;
                    }
                }
            }
            repaint |= pointer_primary_dirty;
            #[cfg(feature = "profiler")]
            {
                if pointer_primary_dirty {
                    pending_primary_pointer_event_us = latest_pointer_event_this_turn;
                }
                pointer_probe.finish(cursor_path);
                if pointer_probe.has_motion()
                    && input_callback_time_us != 0
                    && let Some(dispatch_started_us) = input_dispatch_started_us
                {
                    crate::profiler::record_instant_value(
                        "input.libinput.pointer_motion.pipeline.fd_callback_to_dispatch_ns",
                        dispatch_started_us
                            .saturating_sub(input_callback_time_us)
                            .saturating_mul(1_000),
                    );
                }
                if pointer_probe.has_motion()
                    && let Some(now_us) = crate::platform_linux::monotonic_time_microseconds()
                {
                    if let Some(previous_us) = previous_pointer_batch_us.replace(now_us) {
                        crate::profiler::record_instant_value(
                            "input.libinput.pointer_motion.pipeline.batch_interval_ns",
                            now_us.saturating_sub(previous_us).saturating_mul(1_000),
                        );
                    }
                }
            }
        }

        if runtime_ready.swap(false, Ordering::AcqRel) {
            runtime_wake.drain();
        }
        let runtime_now = MonotonicInstant::from_nanos(
            start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        );
        let (runtime_turn_ready, _, _) = desktop_runtime_schedule(
            &background,
            &frame_layers,
            pointer.as_ref(),
            &icon_layers,
            &widgets,
            runtime_now,
        );
        repaint |= runtime_turn_ready;
        other_work_seen |= runtime_turn_ready;

        if kms_ready.swap(false, Ordering::AcqRel) {
            kms.dispatch_events().map_err(app_error)?;
            for _ in 0..kms.take_completed_page_flips() {
                let completed = pending_kms_commit.take().ok_or_else(|| {
                    AppError::new("DRM completed a page flip with no tracked atomic commit")
                })?;
                if let Some(completed_slot) = completed.primary_slot {
                    frame_slots[completed_slot]
                        .page_flip_completed()
                        .map_err(app_error)?;
                    if let Some(previous) = current_scanout.replace(completed_slot) {
                        frame_slots[previous]
                            .page_flip_replaced()
                            .map_err(app_error)?;
                    }
                    presentation_completed = true;
                    #[cfg(feature = "profiler")]
                    if let Some(event_time_us) = frame_pointer_event_us[completed_slot].take() {
                        record_pointer_event_latency(
                            "input.libinput.pointer_motion.pipeline.event_to_primary_scanout_ns",
                            event_time_us,
                        );
                    }
                }
                if let Some(cursor_snapshot) = completed.cursor {
                    hardware_cursor
                        .as_mut()
                        .ok_or_else(|| {
                            AppError::new(
                                "DRM completed an atomic cursor commit after cursor retirement",
                            )
                        })?
                        .mark_completed(cursor_snapshot)?;
                    #[cfg(feature = "profiler")]
                    if let Some(event_time_us) = completed.cursor_event_us {
                        record_pointer_event_latency(
                            "input.libinput.pointer_motion.pipeline.event_to_cursor_scanout_ns",
                            event_time_us,
                        );
                    }
                }
                #[cfg(feature = "profiler")]
                {
                    crate::profiler::record_instant("presentation.kms.page_flip_completed");
                }
                if hardware_cursor
                    .as_ref()
                    .is_some_and(HardwareCursor::ready_to_retire)
                {
                    hardware_cursor = None;
                    #[cfg(feature = "profiler")]
                    crate::profiler::record_instant("presentation.cursor.software_fallback");
                }
            }
        }

        if vulkan_completion_ready
            .as_ref()
            .is_some_and(|ready| ready.swap(false, Ordering::AcqRel))
            && let Some(vulkan) = &vulkan_scanout
        {
            for completion in vulkan.drain_completions() {
                completion.result.map_err(AppError::new)?;
                frame_slots[completion.slot_index]
                    .gpu_completed()
                    .map_err(app_error)?;
                ready_scanout.push_back(completion.slot_index);
                #[cfg(feature = "profiler")]
                crate::profiler::record_instant("vulkan.scanout.completion_ready");
            }
        }

        let actions = wayland.core_mut().drain_actions().collect::<Vec<_>>();
        if !actions.is_empty() {
            other_work_seen = true;
        }
        for action in actions {
            match action {
                CompositorAction::PublishSurface(surface) => {
                    let snapshot = wayland
                        .core()
                        .world
                        .surface(surface)
                        .map(|surface| surface.snapshot().clone());
                    let Some(snapshot) = snapshot else { continue };
                    let Some(role) = snapshot.role else {
                        continue;
                    };
                    if !matches!(
                        role,
                        SurfaceRole::XdgToplevel
                            | SurfaceRole::XdgPopup
                            | SurfaceRole::Subsurface
                            | SurfaceRole::Cursor
                            | SurfaceRole::DragIcon
                            | SurfaceRole::SessionLock
                    ) {
                        continue;
                    }
                    let Some(attachment) = snapshot.attachment else {
                        continue;
                    };
                    let shm = wayland
                        .read_shm_buffer(attachment.buffer)
                        .map_err(app_error)?;
                    let image = shm_image_resource(attachment.buffer, snapshot.revision, shm)
                        .map_err(app_error)?;
                    let image = transform_surface_image(
                        image,
                        snapshot.buffer_scale,
                        snapshot.buffer_transform,
                        wayland.viewport(surface),
                    )
                    .map_err(app_error)?;
                    let (parent, offset, position) = if role == SurfaceRole::Subsurface {
                        let parent = wayland.core().subsurfaces.parent(surface);
                        let offset = wayland
                            .core()
                            .subsurfaces
                            .position(surface)
                            .map_or(PointI::default(), |position| position.offset);
                        let position = parent.and_then(|parent| windows.get(&parent)).map_or(
                            offset,
                            |parent| PointI {
                                x: parent.position.x + offset.x,
                                y: parent.position.y + offset.y,
                            },
                        );
                        (parent, offset, position)
                    } else if role == SurfaceRole::XdgPopup {
                        let (parent, geometry) = wayland.popup_placement(surface).unwrap_or((
                            None,
                            RectI {
                                x: 0,
                                y: 0,
                                width: image.extent.width,
                                height: image.extent.height,
                            },
                        ));
                        let offset = PointI {
                            x: geometry.x,
                            y: geometry.y,
                        };
                        let position = parent.and_then(|parent| windows.get(&parent)).map_or(
                            offset,
                            |parent| PointI {
                                x: parent.position.x + offset.x,
                                y: parent.position.y + offset.y,
                            },
                        );
                        (parent, offset, position)
                    } else if matches!(
                        role,
                        SurfaceRole::Cursor | SurfaceRole::DragIcon | SurfaceRole::SessionLock
                    ) {
                        (None, PointI::default(), PointI::default())
                    } else {
                        let position = windows.get(&surface).map_or_else(
                            || {
                                let offset = next_window_offset;
                                next_window_offset = (next_window_offset + 28) % 280;
                                PointI {
                                    x: work_area.x + 48 + offset,
                                    y: work_area.y + 48 + offset,
                                }
                            },
                            |window| window.position,
                        );
                        (None, PointI::default(), position)
                    };
                    let is_new = !windows.contains_key(&surface);
                    let requested_size = retained_requested_size(
                        windows.get(&surface).map(|window| window.requested_size),
                        image.extent,
                    );
                    let (
                        restore_geometry,
                        maximized,
                        fullscreen,
                        minimized,
                        chrome_outer,
                        chrome_content_offset,
                        chrome,
                    ) = windows.get(&surface).map_or(
                        (None, false, false, false, None, None, None),
                        |window| {
                            (
                                window.restore_geometry,
                                window.maximized,
                                window.fullscreen,
                                window.minimized,
                                window.chrome_outer,
                                window.chrome_content_offset,
                                window.chrome.clone(),
                            )
                        },
                    );
                    windows.insert(
                        surface,
                        ClientWindow {
                            revision: snapshot.revision,
                            role,
                            parent,
                            offset,
                            server_decorated: role == SurfaceRole::XdgToplevel
                                && wayland.decoration_mode(surface)
                                    != Some(crate::compositor_wayland::DecorationMode::ClientSide),
                            position,
                            size: image.extent,
                            requested_size,
                            restore_geometry,
                            maximized,
                            fullscreen,
                            minimized,
                            chrome_outer,
                            chrome_content_offset,
                            chrome,
                            rgba: image.pixels_rgba8.to_vec(),
                        },
                    );
                    if is_new && !matches!(role, SurfaceRole::Cursor | SurfaceRole::DragIcon) {
                        stacking_order.push(surface);
                    }
                    if session_locked && role == SurfaceRole::SessionLock {
                        wayland
                            .set_keyboard_focus(1, Some(surface), display.next_serial())
                            .map_err(app_error)?;
                    }
                    wayland
                        .release_buffer(attachment.buffer)
                        .map_err(app_error)?;
                    repaint = true;
                }
                CompositorAction::WithdrawSurface(surface) => {
                    windows.remove(&surface);
                    frame_layers.remove(&surface);
                    stacking_order.retain(|candidate| *candidate != surface);
                    if matches!(
                        window_interaction,
                        Some(WindowInteraction::Move {
                            surface: candidate,
                            ..
                        }
                            | WindowInteraction::Resize {
                                surface: candidate,
                                ..
                            }) if candidate == surface
                    ) {
                        window_interaction = None;
                    }
                    if touch_targets.values().any(|target| *target == surface) {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                    }
                    if pointer_focus == Some(surface) {
                        pointer_focus = None;
                    }
                    repaint = true;
                }
                CompositorAction::ActivateSurface {
                    surface,
                    application_id: _,
                    source_surface: _,
                } => {
                    if windows.contains_key(&surface) {
                        if let Some(window) = windows.get_mut(&surface) {
                            window.minimized = false;
                        }
                        stacking_order.retain(|candidate| *candidate != surface);
                        stacking_order.push(surface);
                        focus_toplevel(&display, &mut wayland, &windows, Some(surface))?;
                        repaint = true;
                    }
                }
                CompositorAction::MoveToplevel(surface) => {
                    if windows.get(&surface).is_some_and(|window| {
                        !window.maximized && !window.fullscreen && !window.minimized
                    }) {
                        window_interaction =
                            WindowInteraction::begin_move(&windows, surface, pointer_position);
                    }
                }
                CompositorAction::ResizeToplevel { surface, edge } => {
                    if windows.get(&surface).is_some_and(|window| {
                        !window.maximized && !window.fullscreen && !window.minimized
                    }) {
                        window_interaction = WindowInteraction::begin_resize(
                            &windows,
                            surface,
                            edge,
                            pointer_position,
                        );
                    }
                }
                CompositorAction::MaximizeToplevel { surface, maximized } => {
                    set_window_maximized(
                        &mut wayland,
                        &mut windows,
                        surface,
                        maximized,
                        work_area,
                        &config,
                    )?;
                    repaint = true;
                }
                CompositorAction::FullscreenToplevel {
                    surface,
                    fullscreen,
                    output: _,
                } => {
                    set_window_fullscreen(
                        &mut wayland,
                        &mut windows,
                        surface,
                        fullscreen,
                        extent,
                        &config,
                    )?;
                    repaint = true;
                }
                CompositorAction::MinimizeToplevel(surface) => {
                    if let Some(window) = windows.get_mut(&surface) {
                        window.minimized = true;
                        stacking_order.retain(|candidate| *candidate != surface);
                        repaint = true;
                    }
                }
                CompositorAction::SessionLockRequested(lock) => {
                    if wayland.drag_active(1) {
                        wayland.cancel_drag(1).map_err(app_error)?;
                    }
                    session_locked = true;
                    pending_session_lock = Some(lock);
                    window_interaction = None;
                    pointer_focus = None;
                    wayland
                        .set_pointer_focus(1, None, pointer_position, display.next_serial())
                        .map_err(app_error)?;
                    wayland
                        .set_keyboard_focus(1, None, display.next_serial())
                        .map_err(app_error)?;
                    if !touch_targets.is_empty() {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                    }
                    repaint = true;
                }
                CompositorAction::SessionLockCancelled(lock) => {
                    if pending_session_lock == Some(lock) && !wayland.session_locked() {
                        pending_session_lock = None;
                        session_locked = false;
                        repaint = true;
                    }
                }
                CompositorAction::SessionUnlockRequested(lock) => {
                    if pending_session_lock == Some(lock) {
                        pending_session_lock = None;
                    }
                    session_locked = false;
                    repaint = true;
                }
                CompositorAction::StartDrag {
                    seat: _,
                    origin: _,
                    icon: _,
                }
                | CompositorAction::FinishDrag { icon: _ } => repaint = true,
                CompositorAction::RepaintOutput(_) => repaint = true,
                CompositorAction::ImportBuffer(_)
                | CompositorAction::ReleaseBuffer(_)
                | CompositorAction::DisconnectClient(_) => {}
            }
        }

        let cursor_image = wayland
            .core()
            .seats
            .get(&1)
            .expect("seat registered")
            .cursor;
        repaint |=
            cursor_transition_requires_presentation(cursor_image_at_turn_start, cursor_image);
        let pointer_motion_only = repaint && pointer_motion_seen && !other_work_seen;

        // One atomic commit per CRTC may be outstanding. Primary frames retain mailbox behavior,
        // while cursor-only motion reuses the current primary plane and commits only cursor state.
        let cursor_commit_ready = !first_modeset
            && hardware_cursor
                .as_ref()
                .is_some_and(HardwareCursor::needs_commit);
        if pending_kms_commit.is_none() && (!ready_scanout.is_empty() || cursor_commit_ready) {
            while ready_scanout.len() > 1 {
                let stale = ready_scanout.pop_front().expect("length checked");
                frame_slots[stale].discard_ready().map_err(app_error)?;
                #[cfg(feature = "profiler")]
                {
                    crate::profiler::record_instant("presentation.frame.mailbox_replaced");
                    frame_pointer_event_us[stale] = None;
                }
            }
            let primary_slot = ready_scanout.pop_back();
            let mut request = if let Some(slot_index) = primary_slot {
                kms.primary_modeset_request(
                    connector.id,
                    &connector_properties,
                    crtc,
                    &crtc_properties,
                    plane.id,
                    &plane_properties,
                    mode_blob.id(),
                    frame_slots[slot_index].framebuffer,
                    extent.width as u32,
                    extent.height as u32,
                )
                .map_err(app_error)?
            } else {
                let mut request = kms.atomic_request().map_err(app_error)?;
                // A plane-disable sets its CRTC_ID to zero. Retaining ACTIVE in cursor-only
                // requests keeps the affected CRTC explicit so PAGE_FLIP_EVENT has one owner.
                request
                    .include_active_crtc(crtc, &crtc_properties)
                    .map_err(app_error)?;
                request
            };
            let cursor_snapshot = hardware_cursor
                .as_ref()
                .map(|cursor| cursor.append_desired(&mut request, crtc))
                .transpose()?
                .flatten();
            let cursor_fallback_submission = cursor_snapshot.is_some()
                && hardware_cursor
                    .as_ref()
                    .is_some_and(HardwareCursor::software_fallback_requested);
            #[cfg(feature = "profiler")]
            let _kms_commit = crate::profiler::span!("presentation.kms.atomic_commit");
            let commit_result = if first_modeset {
                match request.test(true) {
                    Ok(()) => request.commit(true, false),
                    Err(error) => Err(error),
                }
            } else {
                request.commit(false, true)
            };
            match commit_result {
                Ok(()) if first_modeset => {
                    let slot_index = primary_slot
                        .expect("the initial modeset is scheduled only with a primary frame");
                    frame_slots[slot_index]
                        .page_flip_submitted()
                        .and_then(|_| frame_slots[slot_index].page_flip_completed())
                        .map_err(app_error)?;
                    current_scanout = Some(slot_index);
                    if let Some(snapshot) = cursor_snapshot {
                        let cursor = hardware_cursor
                            .as_mut()
                            .expect("submitted atomic cursor remains owned");
                        cursor.mark_submitted(snapshot)?;
                        cursor.mark_completed(snapshot)?;
                        #[cfg(feature = "profiler")]
                        if let Some(event_time_us) = pending_deferred_cursor_event_us.take() {
                            record_pointer_event_latency(
                                "input.libinput.pointer_motion.pipeline.freshest_event_to_cursor_submit_ns",
                                event_time_us,
                            );
                            record_pointer_event_latency(
                                "input.libinput.pointer_motion.pipeline.event_to_cursor_scanout_ns",
                                event_time_us,
                            );
                        }
                    }
                    first_modeset = false;
                    presentation_completed = true;
                    #[cfg(feature = "profiler")]
                    if let Some(event_time_us) = frame_pointer_event_us[slot_index].take() {
                        record_pointer_event_latency(
                            "input.libinput.pointer_motion.pipeline.event_to_primary_scanout_ns",
                            event_time_us,
                        );
                    }
                    if hardware_cursor
                        .as_ref()
                        .is_some_and(HardwareCursor::ready_to_retire)
                    {
                        hardware_cursor = None;
                    }
                }
                Ok(()) => {
                    if let Some(slot_index) = primary_slot {
                        frame_slots[slot_index]
                            .page_flip_submitted()
                            .map_err(app_error)?;
                    }
                    if let Some(snapshot) = cursor_snapshot {
                        hardware_cursor
                            .as_mut()
                            .expect("submitted atomic cursor remains owned")
                            .mark_submitted(snapshot)?;
                    }
                    #[cfg(feature = "profiler")]
                    let cursor_event_us = if cursor_snapshot.is_some() {
                        let event_time_us = pending_deferred_cursor_event_us.take();
                        if let Some(event_time_us) = event_time_us {
                            record_pointer_event_latency(
                                "input.libinput.pointer_motion.pipeline.freshest_event_to_cursor_submit_ns",
                                event_time_us,
                            );
                        }
                        event_time_us
                    } else {
                        None
                    };
                    pending_kms_commit = Some(PendingKmsCommit {
                        primary_slot,
                        cursor: cursor_snapshot,
                        #[cfg(feature = "profiler")]
                        cursor_event_us,
                    });
                    #[cfg(feature = "profiler")]
                    if cursor_snapshot.is_some() {
                        crate::profiler::record_instant(
                            "presentation.cursor.hardware_atomic_submit",
                        );
                    }
                }
                Err(error) if cursor_snapshot.is_some() && !cursor_fallback_submission => {
                    if let Some(slot_index) = primary_slot {
                        frame_slots[slot_index].discard_ready().map_err(app_error)?;
                        #[cfg(feature = "profiler")]
                        {
                            frame_pointer_event_us[slot_index] = None;
                        }
                    }
                    let cursor = hardware_cursor
                        .as_mut()
                        .expect("failed atomic cursor remains owned");
                    cursor.request_software_fallback();
                    if cursor.ready_to_retire() {
                        hardware_cursor = None;
                    }
                    repaint = true;
                    #[cfg(feature = "profiler")]
                    {
                        pending_primary_pointer_event_us = pending_deferred_cursor_event_us.take();
                        crate::profiler::record_instant(
                            "presentation.cursor.hardware_atomic_failed",
                        );
                    }
                    let _ = error;
                }
                Err(error) => return Err(app_error(error)),
            }
        }

        if presentation_completed {
            if let Some(lock) = pending_session_lock.take() {
                wayland
                    .session_lock_frame_presented(lock)
                    .map_err(app_error)?;
            }
            let time = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);
            for (surface, window) in &windows {
                if window.revision != 0
                    && !window.minimized
                    && (window.role == SurfaceRole::SessionLock) == session_locked
                {
                    wayland
                        .surface_presented(*surface, time)
                        .map_err(app_error)?;
                }
            }
            let animation_now = MonotonicInstant::from_nanos(
                start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            );
            let (_, _, animation_active) = desktop_runtime_schedule(
                &background,
                &frame_layers,
                pointer.as_ref(),
                &icon_layers,
                &widgets,
                animation_now,
            );
            repaint |= animation_active;
        }

        let available_scanout = frame_slots
            .iter()
            .position(|slot| slot.state == FrameSlotState::Available);
        if repaint && seat.state() == SeatState::Enabled && available_scanout.is_some() {
            #[cfg(feature = "profiler")]
            let _profile_suppression = (pointer_motion_only
                && !crate::profiler::pointer_move_events_enabled())
            .then(crate::profiler::suppress_current_thread);
            #[cfg(feature = "profiler")]
            let _profile_frame = crate::profiler::start_frame("frame.total");
            #[cfg(feature = "profiler")]
            crate::profiler::counter!(
                "frame.trigger.pointer_move_only",
                u8::from(pointer_motion_only)
            );
            #[cfg(feature = "profiler")]
            {
                crate::profiler::counter!(
                    "presentation.scanout.available_slots",
                    frame_slots
                        .iter()
                        .filter(|slot| slot.state == FrameSlotState::Available)
                        .count() as u64
                );
                crate::profiler::counter!(
                    "presentation.scanout.gpu_in_flight",
                    frame_slots
                        .iter()
                        .filter(|slot| slot.state == FrameSlotState::GpuSubmitted)
                        .count() as u64
                );
                crate::profiler::counter!(
                    "presentation.scanout.ready_frames",
                    ready_scanout.len() as u64
                );
                crate::profiler::counter!(
                    "presentation.scanout.flip_pending",
                    u8::from(pending_kms_commit.is_some())
                );
            }
            let now = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            let software_cursor_active = hardware_cursor
                .as_ref()
                .is_none_or(HardwareCursor::software_fallback_requested);
            let partial_cursor_update =
                pointer_motion_only && software_cursor_active && retained_output_valid;
            let mut frame_damage = None::<RectI>;
            if partial_cursor_update {
                let cursor_image = wayland
                    .core()
                    .seats
                    .get(&1)
                    .expect("seat registered")
                    .cursor;
                let rendered_cursor = render_cursor_image(
                    cursor_image,
                    &mut pointer,
                    &mut icon_layers,
                    &windows,
                    config.pointer_extent,
                    now,
                    &pointer_config,
                    pointer_theme.as_ref(),
                    &mut pointer_media,
                )?;
                let next_cursor = rendered_cursor
                    .as_ref()
                    .and_then(|cursor| cursor_rect(cursor, pointer_position, extent));
                frame_damage = match (previous_software_cursor, next_cursor) {
                    (Some(old), Some(new)) => Some(union_rect(old, new)),
                    (Some(old), None) => Some(old),
                    (None, Some(new)) => Some(new),
                    (None, None) => None,
                }
                .and_then(|damage| intersect_rect(damage, full_rect(extent)));
                if let Some(damage) = frame_damage {
                    copy_rgba_region_into(
                        &retained_base_output,
                        &mut retained_output,
                        extent,
                        damage,
                    );
                    if let Some(cursor) = &rendered_cursor {
                        composite_cursor_image(
                            &mut retained_output,
                            extent,
                            cursor,
                            pointer_position,
                        );
                    }
                    previous_software_cursor = next_cursor;
                } else {
                    repaint = false;
                    continue;
                }
            } else {
                if !session_locked {
                    refresh_window_frames(
                        window_frame.as_ref(),
                        &mut frame_layers,
                        &mut windows,
                        &wayland,
                        &config,
                        assets,
                        &app_icon_profile,
                        &runtime_wake,
                        now,
                    )?;
                }
                let mut output = if session_locked {
                    let mut pixels = vec![0_u8; extent.width as usize * extent.height as usize * 4];
                    for pixel in pixels.chunks_exact_mut(4) {
                        pixel[3] = 255;
                    }
                    pixels
                } else {
                    background.render(extent, now, first_modeset)?.to_vec()
                };
                let placements = windows
                    .iter()
                    .map(|(surface, window)| {
                        let position = window
                            .parent
                            .and_then(|parent| windows.get(&parent))
                            .map_or(window.position, |parent| PointI {
                                x: parent.position.x + window.offset.x,
                                y: parent.position.y + window.offset.y,
                            });
                        (*surface, position)
                    })
                    .collect::<BTreeMap<_, _>>();
                for surface in &stacking_order {
                    let Some(window) = windows.get(surface) else {
                        continue;
                    };
                    if matches!(window.role, SurfaceRole::Cursor | SurfaceRole::DragIcon) {
                        continue;
                    }
                    if window.minimized {
                        continue;
                    }
                    if (window.role == SurfaceRole::SessionLock) != session_locked {
                        continue;
                    }
                    let position = placements.get(surface).copied().unwrap_or(window.position);
                    let outer = window
                        .chrome_outer
                        .unwrap_or_else(|| legacy_window_outer(window, &config));
                    if window_is_decorated(window)
                        && let Some(frame) = frame_layers.get(surface)
                    {
                        composite_rgba(
                            &mut output,
                            extent,
                            &frame.pixels,
                            frame.outer,
                            position,
                            false,
                        );
                    }
                    if window_is_decorated(window)
                        && window.chrome.is_none()
                        && window.role == SurfaceRole::XdgToplevel
                    {
                        let icon_extent = SizeI {
                            width: config.titlebar_height.clamp(1, 24),
                            height: config.titlebar_height.clamp(1, 24),
                        };
                        for (index, name) in ["window.close", "window.maximize", "window.minimize"]
                            .into_iter()
                            .enumerate()
                        {
                            if let Some((_, icon)) = icon_layers
                                .iter_mut()
                                .find(|(candidate, _)| candidate == name)
                            {
                                let pixels = icon.render(icon_extent, now, false)?;
                                composite_rgba(
                                    &mut output,
                                    extent,
                                    pixels,
                                    icon_extent,
                                    PointI {
                                        x: position.x + outer.width
                                            - config.window_border
                                            - (index as i32 + 1) * (icon_extent.width + 4),
                                        y: position.y
                                            + config.window_border
                                            + (config.titlebar_height - icon_extent.height) / 2,
                                    },
                                    true,
                                );
                            }
                        }
                    }
                    composite_rgba(
                        &mut output,
                        extent,
                        &window.rgba,
                        window.size,
                        PointI {
                            x: position.x + window_content_offset(window, &config).x,
                            y: position.y + window_content_offset(window, &config).y,
                        },
                        true,
                    );
                }
                if !session_locked {
                    for widget in &mut widgets {
                        let widget_extent =
                            resolved_widget_extent(extent, widget.width, widget.height);
                        let position = widget_position(extent, widget_extent, widget.anchor);
                        let pixels = widget.layer.render(widget_extent, now, false)?;
                        composite_rgba(&mut output, extent, pixels, widget_extent, position, false);
                    }
                }
                if !session_locked
                    && let Some(icon) = wayland.drag_icon(1)
                    && let Some(icon) = windows.get(&icon)
                {
                    composite_rgba(
                        &mut output,
                        extent,
                        &icon.rgba,
                        icon.size,
                        PointI {
                            x: drag_position.x.round() as i32,
                            y: drag_position.y.round() as i32,
                        },
                        true,
                    );
                }
                retained_base_output.clone_from(&output);
                let cursor_image = wayland
                    .core()
                    .seats
                    .get(&1)
                    .expect("seat registered")
                    .cursor;
                let rendered_cursor = render_cursor_image(
                    cursor_image,
                    &mut pointer,
                    &mut icon_layers,
                    &windows,
                    config.pointer_extent,
                    now,
                    &pointer_config,
                    pointer_theme.as_ref(),
                    &mut pointer_media,
                )?;
                let mut cursor_on_hardware = false;
                if let Some(cursor) = &mut hardware_cursor {
                    cursor.move_to(pointer_position);
                    if !cursor.software_fallback_requested() {
                        let update = match &rendered_cursor {
                            Some(image) => cursor.set_image(image),
                            None => {
                                cursor.hide();
                                Ok(())
                            }
                        };
                        if update.is_ok() {
                            cursor_on_hardware = true;
                            #[cfg(feature = "profiler")]
                            crate::profiler::record_instant(
                                "presentation.cursor.hardware_image_staged",
                            );
                        } else {
                            cursor.request_software_fallback();
                            #[cfg(feature = "profiler")]
                            {
                                pending_primary_pointer_event_us = latest_pointer_event_this_turn
                                    .or_else(|| pending_deferred_cursor_event_us.take());
                                crate::profiler::record_instant(
                                    "presentation.cursor.hardware_image_failed",
                                );
                            }
                        }
                    }
                }
                if hardware_cursor
                    .as_ref()
                    .is_some_and(HardwareCursor::ready_to_retire)
                {
                    hardware_cursor = None;
                }
                if !cursor_on_hardware && let Some(cursor) = &rendered_cursor {
                    composite_cursor_image(&mut output, extent, cursor, pointer_position);
                }
                previous_software_cursor = if cursor_on_hardware {
                    None
                } else {
                    rendered_cursor
                        .as_ref()
                        .and_then(|cursor| cursor_rect(cursor, pointer_position, extent))
                };
                retained_output = output;
                retained_output_valid = true;
            }
            let scanout_index = available_scanout.expect("availability checked before rendering");
            let frame_id = next_frame_id;
            next_frame_id = next_frame_id.wrapping_add(1).max(1);
            frame_slots[scanout_index]
                .begin_render(frame_id)
                .map_err(app_error)?;
            if let Some(vulkan) = &mut vulkan_scanout {
                vulkan.render(scanout_index, extent, &retained_output, frame_damage)?;
                frame_slots[scanout_index]
                    .gpu_submitted()
                    .map_err(app_error)?;
            } else {
                software_content_version = software_content_version.wrapping_add(1).max(1);
                software_damage_history.push_back((software_content_version, frame_damage));
                while software_damage_history.len() > 64 {
                    software_damage_history.pop_front();
                }
                let scanout_damage = accumulated_damage(
                    software_target_versions[scanout_index],
                    software_content_version,
                    &software_damage_history,
                    extent,
                );
                let scanout_region = scanout_damage.unwrap_or_else(|| full_rect(extent));
                #[cfg(feature = "profiler")]
                {
                    crate::profiler::counter!(
                        "render.upload_bytes",
                        rect_area(scanout_region).saturating_mul(4)
                    );
                    crate::profiler::counter!("render.damage_area", rect_area(scanout_region));
                }
                scanout_buffers[scanout_index]
                    .map_write()
                    .map_err(app_error)?
                    .write_rgba8_region(&retained_output, scanout_region)
                    .map_err(app_error)?;
                software_target_versions[scanout_index] = software_content_version;
                frame_slots[scanout_index]
                    .gpu_submitted()
                    .and_then(|_| frame_slots[scanout_index].gpu_completed())
                    .map_err(app_error)?;
                ready_scanout.push_back(scanout_index);
            }
            #[cfg(feature = "profiler")]
            {
                frame_pointer_event_us[scanout_index] = pending_primary_pointer_event_us.take();
                if let Some(event_time_us) = frame_pointer_event_us[scanout_index] {
                    record_pointer_event_latency(
                        "input.libinput.pointer_motion.pipeline.event_to_primary_submit_ns",
                        event_time_us,
                    );
                }
            }
            repaint = false;
        }
    }
}

#[cfg(feature = "profiler")]
fn record_libinput_event(
    kind: LinuxInputEventKind,
    event_time_us: u64,
    observed_time_us: Option<u64>,
) {
    use crate::profiler::InputRecordingSource;

    let queue_age_ns = observed_time_us
        .unwrap_or(event_time_us)
        .saturating_sub(event_time_us)
        .saturating_mul(1_000);
    let (source, label, has_queue_age) = match kind {
        LinuxInputEventKind::PointerMotion { .. } => (
            InputRecordingSource::PointerMotion,
            "input.libinput.pointer_motion.relative.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::PointerAbsolute { .. } => (
            InputRecordingSource::PointerMotion,
            "input.libinput.pointer_motion.absolute.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::PointerButton { pressed: true, .. } => (
            InputRecordingSource::PointerButton,
            "input.libinput.pointer_button.pressed.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::PointerButton { pressed: false, .. } => (
            InputRecordingSource::PointerButton,
            "input.libinput.pointer_button.released.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::PointerAxis { .. } => (
            InputRecordingSource::Scroll,
            "input.libinput.scroll.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::KeyboardKey { pressed: true, .. } => (
            InputRecordingSource::Keyboard,
            "input.libinput.keyboard.pressed.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::KeyboardKey { pressed: false, .. } => (
            InputRecordingSource::Keyboard,
            "input.libinput.keyboard.released.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::TouchMotion { .. } => (
            InputRecordingSource::TouchMotion,
            "input.libinput.touch_motion.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::TouchDown { .. } => (
            InputRecordingSource::TouchContact,
            "input.libinput.touch_contact.down.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::TouchUp { .. } => (
            InputRecordingSource::TouchContact,
            "input.libinput.touch_contact.up.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::TouchCancel => (
            InputRecordingSource::TouchContact,
            "input.libinput.touch_contact.cancel.queue_age_ns",
            true,
        ),
        LinuxInputEventKind::DeviceAdded => (
            InputRecordingSource::DeviceChange,
            "input.libinput.device_change.added",
            false,
        ),
        LinuxInputEventKind::DeviceRemoved => (
            InputRecordingSource::DeviceChange,
            "input.libinput.device_change.removed",
            false,
        ),
    };
    if !crate::profiler::input_recording_enabled(source) {
        return;
    }
    if has_queue_age {
        crate::profiler::record_instant_value(label, queue_age_ns);
    } else {
        crate::profiler::record_instant(label);
    }
}

fn output_state(
    connector: &crate::presenter_vulkan_kms::KmsConnector,
    current_mode: usize,
    config: &LinuxDesktopConfig,
) -> AppResult<OutputState> {
    let name = format!(
        "DRM-{}-{}",
        connector.connector_type, connector.connector_type_id
    );
    let modes = connector
        .modes
        .iter()
        .map(|mode| OutputMode {
            size: mode.size(),
            refresh_millihertz: mode.refresh_millihertz(),
            preferred: mode.preferred(),
        })
        .collect();
    OutputState::new(
        OutputDescription {
            name: name.clone(),
            description: format!("Telorgon output {name}"),
            make: "Unknown".to_owned(),
            model: name,
            physical_millimeters: connector.physical_millimeters,
            logical_position: PointI::default(),
            scale: config.output_scale,
            transform: OutputTransform::Normal,
            modes,
        },
        current_mode,
    )
    .map_err(app_error)
}

fn is_plane_type(kms: &KmsDevice, plane: u32, expected: u64) -> bool {
    KmsTopology::object_properties(kms, plane, KmsPropertyObject::Plane)
        .ok()
        .and_then(|properties| properties.named("type").map(|property| property.value))
        == Some(expected)
}

fn render_cursor_image(
    image: CursorImage,
    pointer: &mut Option<Layer>,
    icons: &mut [(String, Layer)],
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    extent: SizeI,
    now: u64,
    pointer_config: &PointerConfiguration,
    pointer_theme: Option<&PointerTheme>,
    pointer_media: &mut AssetMediaCache,
) -> AppResult<Option<RenderedCursor>> {
    let rendered = match image {
        CursorImage::TelorgonDefault => render_semantic_pointer(
            PointerIcon::Default,
            None,
            pointer,
            icons,
            extent,
            now,
            pointer_config,
            pointer_theme,
            pointer_media,
        )?,
        CursorImage::Shape(shape) => render_semantic_pointer(
            cursor_shape_pointer_icon(shape).unwrap_or(PointerIcon::Default),
            cursor_shape_icon_name(shape),
            pointer,
            icons,
            extent,
            now,
            pointer_config,
            pointer_theme,
            pointer_media,
        )?,
        CursorImage::ClientSurface {
            surface,
            hotspot_x,
            hotspot_y,
        } => match resolve_pointer(
            PointerRequest::ClientSurface,
            pointer_config.client_cursor_mode(),
            pointer_config.pointer_overrides(),
            pointer_theme,
        ) {
            PointerResolution::ClientSurface => {
                windows.get(&surface).map(|cursor| RenderedCursor {
                    rgba: cursor.rgba.clone(),
                    size: cursor.size,
                    hotspot: PointI {
                        x: hotspot_x,
                        y: hotspot_y,
                    },
                    premultiplied: true,
                })
            }
            PointerResolution::Graphic(graphic) => {
                Some(render_asset_pointer(graphic, extent, now, pointer_media)?)
            }
            PointerResolution::System(icon) => {
                render_composed_pointer(icon, None, pointer, icons, extent, now)?
            }
            PointerResolution::Hidden => None,
        },
        CursorImage::Hidden => None,
    };
    Ok(rendered)
}

#[allow(clippy::too_many_arguments)]
fn render_semantic_pointer(
    icon: PointerIcon,
    composed_icon_name: Option<&str>,
    pointer: &mut Option<Layer>,
    icons: &mut [(String, Layer)],
    extent: SizeI,
    now: u64,
    pointer_config: &PointerConfiguration,
    pointer_theme: Option<&PointerTheme>,
    pointer_media: &mut AssetMediaCache,
) -> AppResult<Option<RenderedCursor>> {
    match resolve_pointer(
        PointerRequest::Semantic(icon),
        pointer_config.client_cursor_mode(),
        pointer_config.pointer_overrides(),
        pointer_theme,
    ) {
        PointerResolution::Graphic(graphic) => Ok(Some(render_asset_pointer(
            graphic,
            extent,
            now,
            pointer_media,
        )?)),
        PointerResolution::System(icon) => {
            let rendered =
                render_composed_pointer(icon, composed_icon_name, pointer, icons, extent, now)?;
            if rendered.is_some() {
                return Ok(rendered);
            }
            let Some(fallback) = semantic_pointer_fallback(icon) else {
                return Ok(None);
            };
            render_semantic_pointer(
                fallback,
                cursor_shape_icon_name(pointer_icon_cursor_shape(fallback)),
                pointer,
                icons,
                extent,
                now,
                pointer_config,
                pointer_theme,
                pointer_media,
            )
        }
        PointerResolution::Hidden => Ok(None),
        PointerResolution::ClientSurface => {
            unreachable!("semantic requests cannot resolve to a client surface")
        }
    }
}

fn semantic_pointer_fallback(icon: PointerIcon) -> Option<PointerIcon> {
    match icon {
        PointerIcon::EResize
        | PointerIcon::NResize
        | PointerIcon::NeResize
        | PointerIcon::NwResize
        | PointerIcon::SResize
        | PointerIcon::SeResize
        | PointerIcon::SwResize
        | PointerIcon::WResize
        | PointerIcon::EwResize
        | PointerIcon::NsResize
        | PointerIcon::NeswResize
        | PointerIcon::NwseResize
        | PointerIcon::ColResize
        | PointerIcon::RowResize => Some(PointerIcon::AllResize),
        PointerIcon::Default => None,
        _ => Some(PointerIcon::Default),
    }
}

fn render_asset_pointer(
    graphic: &PointerGraphic,
    fallback_extent: SizeI,
    now_nanoseconds: u64,
    media: &mut AssetMediaCache,
) -> AppResult<RenderedCursor> {
    let frame = if graphic.frames().len() == 1 {
        graphic.frames()[0]
    } else {
        let cycle_ms = graphic
            .frames()
            .iter()
            .filter_map(|frame| frame.duration_ms)
            .map(|duration| u64::from(duration.get()))
            .sum::<u64>();
        let mut elapsed_ms = (now_nanoseconds / 1_000_000) % cycle_ms.max(1);
        *graphic
            .frames()
            .iter()
            .find(|frame| {
                let duration = u64::from(
                    frame
                        .duration_ms
                        .expect("animated frame was validated")
                        .get(),
                );
                if elapsed_ms < duration {
                    true
                } else {
                    elapsed_ms -= duration;
                    false
                }
            })
            .unwrap_or_else(|| {
                graphic
                    .frames()
                    .last()
                    .expect("pointer graphic has a frame")
            })
    };
    let requested = if let Some(size) = graphic.physical_size() {
        AssetRasterSize::new(u32::from(size), u32::from(size))
    } else {
        AssetRasterSize::new(
            fallback_extent.width.max(1) as u32,
            fallback_extent.height.max(1) as u32,
        )
    }
    .map_err(app_error)?;
    let decoded = match graphic.tint_color() {
        Some(tint) => media.tinted_cursor(frame.asset, Some(requested), tint),
        None => media.cursor(frame.asset, Some(requested)),
    }
    .map_err(app_error)?;
    let hotspot = graphic.pointer_hotspot();
    if i32::from(hotspot.x) >= decoded.extent.width || i32::from(hotspot.y) >= decoded.extent.height
    {
        return Err(AppError::new(
            "pointer hotspot is outside the decoded cursor image",
        ));
    }
    Ok(RenderedCursor {
        rgba: decoded.pixels_rgba8.to_vec(),
        size: decoded.extent,
        hotspot: PointI {
            x: i32::from(hotspot.x),
            y: i32::from(hotspot.y),
        },
        premultiplied: decoded.alpha_mode == ImageAlphaMode::Premultiplied,
    })
}

fn render_composed_pointer(
    _icon: PointerIcon,
    composed_icon_name: Option<&str>,
    pointer: &mut Option<Layer>,
    icons: &mut [(String, Layer)],
    extent: SizeI,
    now: u64,
) -> AppResult<Option<RenderedCursor>> {
    if let Some((_, icon)) = composed_icon_name
        .and_then(|name| icons.iter_mut().find(|(candidate, _)| candidate == name))
    {
        return Ok(Some(RenderedCursor {
            rgba: icon.render(extent, now, false)?.to_vec(),
            size: extent,
            hotspot: PointI::default(),
            premultiplied: true,
        }));
    }
    pointer
        .as_mut()
        .map(|pointer| {
            pointer
                .render(extent, now, false)
                .map(|pixels| RenderedCursor {
                    rgba: pixels.to_vec(),
                    size: extent,
                    hotspot: PointI::default(),
                    premultiplied: false,
                })
        })
        .transpose()
}

fn composite_cursor_image(
    target: &mut [u8],
    target_size: SizeI,
    cursor: &RenderedCursor,
    pointer_position: PointF,
) {
    composite_rgba(
        target,
        target_size,
        &cursor.rgba,
        cursor.size,
        PointI {
            x: pointer_position.x.round() as i32 - cursor.hotspot.x,
            y: pointer_position.y.round() as i32 - cursor.hotspot.y,
        },
        cursor.premultiplied,
    );
}

fn cursor_image_signature(cursor: &RenderedCursor) -> u64 {
    let mut hasher = DefaultHasher::new();
    cursor.size.width.hash(&mut hasher);
    cursor.size.height.hash(&mut hasher);
    cursor.hotspot.x.hash(&mut hasher);
    cursor.hotspot.y.hash(&mut hasher);
    cursor.premultiplied.hash(&mut hasher);
    cursor.rgba.hash(&mut hasher);
    hasher.finish()
}

fn cursor_rect(cursor: &RenderedCursor, position: PointF, output: SizeI) -> Option<RectI> {
    intersect_rect(
        RectI {
            x: position.x.round() as i32 - cursor.hotspot.x,
            y: position.y.round() as i32 - cursor.hotspot.y,
            width: cursor.size.width,
            height: cursor.size.height,
        },
        full_rect(output),
    )
}

fn update_pointer_focus(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    session_locked: bool,
    current: &mut Option<WaylandSurfaceId>,
    position: PointF,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let next = hit_test_surface(windows, stacking_order, position, config, session_locked);
    if next != *current {
        let local = next.map_or(position, |surface| {
            surface_local_position(windows, surface, position, config)
        });
        wayland
            .set_pointer_focus(1, next, local, display.next_serial())
            .map_err(app_error)?;
        *current = next;
    }
    Ok(())
}

fn route_pointer_motion(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    session_locked: bool,
    current: &mut Option<WaylandSurfaceId>,
    position: PointF,
    time: u32,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
        let target = hit_test_surface(windows, stacking_order, position, config, session_locked);
        let local = target.map_or(position, |surface| {
            surface_local_position(windows, surface, position, config)
        });
        wayland
            .drag_motion(1, target, time, local)
            .map_err(app_error)?;
    } else {
        update_pointer_focus(
            display,
            wayland,
            windows,
            stacking_order,
            session_locked,
            current,
            position,
            config,
        )?;
        if let Some(surface) = *current {
            let local = surface_local_position(windows, surface, position, config);
            let _ = wayland.pointer_motion(1, time, local);
        }
    }
    Ok(())
}

fn focus_toplevel(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: Option<WaylandSurfaceId>,
) -> AppResult<()> {
    let previous = wayland
        .core()
        .seats
        .get(&1)
        .and_then(|seat| seat.keyboard_focus)
        .map(|focus| focus.surface);
    if previous == surface {
        return Ok(());
    }
    if let Some(previous) = previous
        && wayland
            .core()
            .world
            .surface(previous)
            .is_some_and(|surface| surface.snapshot().role == Some(SurfaceRole::XdgToplevel))
    {
        wayland
            .configure_toplevel(
                previous,
                windows.get(&previous).map(|window| window.requested_size),
                windows
                    .get(&previous)
                    .map_or_else(ToplevelState::default, |window| {
                        window_toplevel_states(window, false, false)
                    }),
            )
            .map_err(app_error)?;
    }
    wayland
        .set_keyboard_focus(1, surface, display.next_serial())
        .map_err(app_error)?;
    if let Some(surface) = surface
        && wayland
            .core()
            .world
            .surface(surface)
            .is_some_and(|surface| surface.snapshot().role == Some(SurfaceRole::XdgToplevel))
    {
        wayland
            .configure_toplevel(
                surface,
                windows.get(&surface).map(|window| window.requested_size),
                windows.get(&surface).map_or(
                    ToplevelState {
                        activated: true,
                        ..ToplevelState::default()
                    },
                    |window| window_toplevel_states(window, true, false),
                ),
            )
            .map_err(app_error)?;
    }
    Ok(())
}

fn apply_window_interaction(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    interaction: WindowInteraction,
    pointer_position: PointF,
    output: SizeI,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    match interaction {
        WindowInteraction::Move {
            surface,
            pointer_start,
            position_start,
        } => {
            let Some(window) = windows.get_mut(&surface) else {
                return Ok(());
            };
            let delta = rounded_pointer_delta(pointer_start, pointer_position);
            window.position.x = position_start
                .x
                .saturating_add(delta.x)
                .clamp(32_i32.saturating_sub(window.size.width), output.width - 32);
            window.position.y = position_start
                .y
                .saturating_add(delta.y)
                .clamp(0, output.height - config.titlebar_height.max(1));
        }
        WindowInteraction::Resize {
            surface,
            edge,
            pointer_start,
            position_start,
            size_start,
        } => {
            let delta = rounded_pointer_delta(pointer_start, pointer_position);
            let (position, size) =
                resize_drag_geometry(position_start, size_start, edge, delta, output);
            let Some(window) = windows.get_mut(&surface) else {
                return Ok(());
            };
            window.position = position;
            if size != window.requested_size {
                window.requested_size = size;
                wayland
                    .configure_toplevel(
                        surface,
                        Some(size),
                        window_toplevel_states(window, true, true),
                    )
                    .map_err(app_error)?;
            }
        }
    }
    Ok(())
}

fn retained_requested_size(previous: Option<SizeI>, committed: SizeI) -> SizeI {
    previous.unwrap_or(committed)
}

fn rounded_pointer_delta(start: PointF, current: PointF) -> PointI {
    PointI {
        x: (current.x - start.x).round() as i32,
        y: (current.y - start.y).round() as i32,
    }
}

fn resize_drag_geometry(
    position_start: PointI,
    size_start: SizeI,
    edge: ResizeEdge,
    delta: PointI,
    output: SizeI,
) -> (PointI, SizeI) {
    const MINIMUM_WIDTH: i32 = 64;
    const MINIMUM_HEIGHT: i32 = 48;

    let mut position = position_start;
    let mut size = size_start;
    let maximum_width = output.width.max(MINIMUM_WIDTH);
    let maximum_height = output.height.max(MINIMUM_HEIGHT);
    if matches!(
        edge,
        ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
    ) {
        size.width = size_start
            .width
            .saturating_sub(delta.x)
            .clamp(MINIMUM_WIDTH, maximum_width);
        position.x = position_start
            .x
            .saturating_add(size_start.width.saturating_sub(size.width));
    }
    if matches!(
        edge,
        ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
    ) {
        size.width = size_start
            .width
            .saturating_add(delta.x)
            .clamp(MINIMUM_WIDTH, maximum_width);
    }
    if matches!(
        edge,
        ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
    ) {
        size.height = size_start
            .height
            .saturating_sub(delta.y)
            .clamp(MINIMUM_HEIGHT, maximum_height);
        position.y = position_start
            .y
            .saturating_add(size_start.height.saturating_sub(size.height));
    }
    if matches!(
        edge,
        ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
    ) {
        size.height = size_start
            .height
            .saturating_add(delta.y)
            .clamp(MINIMUM_HEIGHT, maximum_height);
    }
    (position, size)
}

fn finish_window_interaction(
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    interaction: WindowInteraction,
) -> AppResult<()> {
    let WindowInteraction::Resize { surface, .. } = interaction else {
        return Ok(());
    };
    if let Some(window) = windows.get(&surface) {
        wayland
            .configure_toplevel(
                surface,
                Some(window.requested_size),
                window_toplevel_states(window, true, false),
            )
            .map_err(app_error)?;
    }
    Ok(())
}

fn set_window_maximized(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    maximized: bool,
    work_area: RectI,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let Some(window) = windows.get_mut(&surface) else {
        return Ok(());
    };
    if maximized {
        if !window.maximized && !window.fullscreen {
            window.restore_geometry = Some((window.position, window.requested_size));
        }
        window.maximized = true;
        window.fullscreen = false;
        window.minimized = false;
        window.position = PointI {
            x: work_area.x,
            y: work_area.y,
        };
        window.requested_size = SizeI {
            width: (work_area.width - config.window_border * 2).max(1),
            height: (work_area.height - config.window_border * 2 - config.titlebar_height).max(1),
        };
    } else {
        window.maximized = false;
        if let Some((position, size)) = window.restore_geometry.take() {
            window.position = position;
            window.requested_size = size;
        }
    }
    wayland
        .configure_toplevel(
            surface,
            Some(window.requested_size),
            window_toplevel_states(window, true, false),
        )
        .map_err(app_error)?;
    Ok(())
}

fn set_window_fullscreen(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    fullscreen: bool,
    output: SizeI,
    _config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let Some(window) = windows.get_mut(&surface) else {
        return Ok(());
    };
    if fullscreen {
        if !window.maximized && !window.fullscreen {
            window.restore_geometry = Some((window.position, window.requested_size));
        }
        window.maximized = false;
        window.fullscreen = true;
        window.minimized = false;
        window.position = PointI::default();
        window.requested_size = output;
    } else {
        window.fullscreen = false;
        if let Some((position, size)) = window.restore_geometry.take() {
            window.position = position;
            window.requested_size = size;
        }
    }
    wayland
        .configure_toplevel(
            surface,
            Some(window.requested_size),
            window_toplevel_states(window, true, false),
        )
        .map_err(app_error)?;
    Ok(())
}

fn window_toplevel_states(window: &ClientWindow, activated: bool, resizing: bool) -> ToplevelState {
    ToplevelState {
        maximized: window.maximized,
        fullscreen: window.fullscreen,
        resizing,
        activated,
        ..ToplevelState::default()
    }
}

fn hit_test_decoration(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) -> Option<(WaylandSurfaceId, DecorationHit)> {
    for surface in stacking_order.iter().rev() {
        let Some(window) = windows.get(surface) else {
            continue;
        };
        if window.role != SurfaceRole::XdgToplevel
            || window.minimized
            || !window_is_decorated(window)
        {
            continue;
        }
        let outer = window
            .chrome_outer
            .unwrap_or_else(|| legacy_window_outer(window, config));
        let local = PointI {
            x: (position.x - window.position.x as f32).floor() as i32,
            y: (position.y - window.position.y as f32).floor() as i32,
        };
        if local.x < 0 || local.y < 0 || local.x >= outer.width || local.y >= outer.height {
            continue;
        }
        if let Some(chrome) = &window.chrome {
            let point = PointF {
                x: local.x as f32,
                y: local.y as f32,
            };
            let role = chrome.hit_test(point.x, point.y);
            if role.is_none() && chrome.content.bounds.contains(point) {
                continue;
            }
            let hit = match role {
                Some(crate::WindowChromeRole::DragRegion)
                | Some(crate::WindowChromeRole::Action(WindowAction::BeginMove)) => {
                    DecorationHit::Titlebar
                }
                Some(crate::WindowChromeRole::Action(WindowAction::BeginResize(edge))) => {
                    DecorationHit::Resize(wayland_resize_edge(edge))
                }
                Some(crate::WindowChromeRole::Action(WindowAction::Close)) => DecorationHit::Close,
                Some(crate::WindowChromeRole::Action(WindowAction::Minimize)) => {
                    DecorationHit::Minimize
                }
                Some(crate::WindowChromeRole::Action(WindowAction::ToggleMaximize)) => {
                    DecorationHit::Maximize
                }
                Some(crate::WindowChromeRole::Action(WindowAction::ShowSystemMenu)) => {
                    DecorationHit::SystemMenu
                }
                Some(crate::WindowChromeRole::ShellAction(action)) => {
                    DecorationHit::ShellAction(action)
                }
                Some(
                    crate::WindowChromeRole::Frame
                    | crate::WindowChromeRole::Content
                    | crate::WindowChromeRole::Title
                    | crate::WindowChromeRole::AppIcon,
                )
                | None => DecorationHit::Frame,
            };
            return Some((*surface, hit));
        }
        let border = config.window_border.max(1);
        let left = local.x < border;
        let right = local.x >= outer.width - border;
        let top = local.y < border;
        let bottom = local.y >= outer.height - border;
        let edge = match (left, right, top, bottom) {
            (true, _, true, _) => Some(ResizeEdge::TopLeft),
            (_, true, true, _) => Some(ResizeEdge::TopRight),
            (true, _, _, true) => Some(ResizeEdge::BottomLeft),
            (_, true, _, true) => Some(ResizeEdge::BottomRight),
            (true, _, _, _) => Some(ResizeEdge::Left),
            (_, true, _, _) => Some(ResizeEdge::Right),
            (_, _, true, _) => Some(ResizeEdge::Top),
            (_, _, _, true) => Some(ResizeEdge::Bottom),
            _ => None,
        };
        if let Some(edge) = edge {
            return Some((*surface, DecorationHit::Resize(edge)));
        }
        if local.y < border + config.titlebar_height {
            let icon_extent = config.titlebar_height.clamp(1, 24);
            for (index, (name, hit)) in [
                ("window.close", DecorationHit::Close),
                ("window.maximize", DecorationHit::Maximize),
                ("window.minimize", DecorationHit::Minimize),
            ]
            .into_iter()
            .enumerate()
            {
                if !icons.iter().any(|(candidate, _)| candidate == name) {
                    continue;
                }
                let x = outer.width - border - (index as i32 + 1) * (icon_extent + 4);
                let y = border + (config.titlebar_height - icon_extent) / 2;
                if local.x >= x
                    && local.x < x + icon_extent
                    && local.y >= y
                    && local.y < y + icon_extent
                {
                    return Some((*surface, hit));
                }
            }
            return Some((*surface, DecorationHit::Titlebar));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn set_decoration_pointer_cursor(
    wayland: &mut NativeCompositor<'_>,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    client_focus: Option<WaylandSurfaceId>,
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) {
    let next = decoration_pointer_request(frames, windows, stacking_order, position, config, icons)
        .map(pointer_request_cursor_image)
        .or_else(|| {
            client_focus
                .is_none()
                .then_some(CursorImage::TelorgonDefault)
        });
    let Some(next) = next else {
        return;
    };
    let Some(seat) = wayland.core_mut().seats.get_mut(&1) else {
        return;
    };
    if seat.cursor != next {
        seat.cursor = next;
    }
}

fn decoration_pointer_request(
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) -> Option<PointerRequest> {
    let (surface, hit) = hit_test_decoration(windows, stacking_order, position, config, icons)?;
    if let (Some(frame), Some(window)) = (frames.get(&surface), windows.get(&surface)) {
        let local = PointF {
            x: position.x - window.position.x as f32,
            y: position.y - window.position.y as f32,
        };
        if let Some(region) = window
            .chrome
            .as_ref()
            .and_then(|chrome| chrome.hit_test_region(local.x, local.y))
            && let Some(request) = frame
                .layer
                .runtime
                .ui()
                .pointer_requests
                .get(region.node)
                .copied()
        {
            return Some(request);
        }
        if let Some(request) = frame
            .layer
            .runtime
            .ui()
            .pointer_requests
            .iter()
            .filter_map(|(node, request)| {
                frame
                    .layer
                    .runtime
                    .layout()
                    .computed(node)
                    .and_then(|computed| {
                        (computed.visible_rect.contains(local)
                            && computed.border_rect.contains(local))
                        .then_some((*request, computed.border_rect.area()))
                    })
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(request, _)| request)
        {
            return Some(request);
        }
    }
    Some(match hit {
        DecorationHit::Titlebar => PointerRequest::Semantic(PointerIcon::Move),
        DecorationHit::Resize(edge) => PointerRequest::Semantic(resize_edge_pointer_icon(edge)),
        DecorationHit::Close
        | DecorationHit::Maximize
        | DecorationHit::Minimize
        | DecorationHit::SystemMenu
        | DecorationHit::ShellAction(_) => PointerRequest::Semantic(PointerIcon::Pointer),
        DecorationHit::Frame => PointerRequest::Semantic(PointerIcon::Default),
    })
}

fn pointer_request_cursor_image(request: PointerRequest) -> CursorImage {
    match request {
        PointerRequest::Hidden => CursorImage::Hidden,
        PointerRequest::ClientSurface => CursorImage::TelorgonDefault,
        PointerRequest::Semantic(icon) => CursorImage::Shape(pointer_icon_cursor_shape(icon)),
    }
}

fn cursor_transition_requires_presentation(previous: CursorImage, current: CursorImage) -> bool {
    previous != current
}

fn resize_edge_pointer_icon(edge: ResizeEdge) -> PointerIcon {
    match edge {
        ResizeEdge::None => PointerIcon::Default,
        ResizeEdge::Top => PointerIcon::NResize,
        ResizeEdge::TopRight => PointerIcon::NeResize,
        ResizeEdge::Right => PointerIcon::EResize,
        ResizeEdge::BottomRight => PointerIcon::SeResize,
        ResizeEdge::Bottom => PointerIcon::SResize,
        ResizeEdge::BottomLeft => PointerIcon::SwResize,
        ResizeEdge::Left => PointerIcon::WResize,
        ResizeEdge::TopLeft => PointerIcon::NwResize,
    }
}

fn invoke_shell_action(
    handlers: &[ShellActionHandler],
    action: crate::ShellActionId,
    surface: WaylandSurfaceId,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
) {
    let Some(handler) = handlers.iter().find(|handler| handler.id() == action) else {
        return;
    };
    let Some(frame) = frames.get(&surface) else {
        return;
    };
    handler.invoke(frame.model.clone());
}

fn hit_test_surface(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    session_locked: bool,
) -> Option<WaylandSurfaceId> {
    stacking_order
        .iter()
        .rev()
        .filter_map(|surface| windows.get(surface).map(|window| (*surface, window)))
        .find(|(_, window)| {
            let origin = window_content_origin(window, config);
            window.role != SurfaceRole::Cursor
                && !window.minimized
                && (window.role == SurfaceRole::SessionLock) == session_locked
                && position.x >= origin.x as f32
                && position.y >= origin.y as f32
                && position.x < (origin.x + window.size.width) as f32
                && position.y < (origin.y + window.size.height) as f32
        })
        .map(|(surface, _)| surface)
}

fn normalized_output_position(normalized: PointF, extent: SizeI) -> PointF {
    PointF {
        x: normalized.x.clamp(0.0, 1.0) * (extent.width - 1) as f32,
        y: normalized.y.clamp(0.0, 1.0) * (extent.height - 1) as f32,
    }
}

fn cursor_shape_icon_name(shape: u32) -> Option<&'static str> {
    Some(match shape {
        1 => "cursor.default",
        2 => "cursor.context-menu",
        3 => "cursor.help",
        4 => "cursor.pointer",
        5 => "cursor.progress",
        6 => "cursor.wait",
        7 => "cursor.cell",
        8 => "cursor.crosshair",
        9 => "cursor.text",
        10 => "cursor.vertical-text",
        11 => "cursor.alias",
        12 => "cursor.copy",
        13 => "cursor.move",
        14 => "cursor.no-drop",
        15 => "cursor.not-allowed",
        16 => "cursor.grab",
        17 => "cursor.grabbing",
        18 => "cursor.e-resize",
        19 => "cursor.n-resize",
        20 => "cursor.ne-resize",
        21 => "cursor.nw-resize",
        22 => "cursor.s-resize",
        23 => "cursor.se-resize",
        24 => "cursor.sw-resize",
        25 => "cursor.w-resize",
        26 => "cursor.ew-resize",
        27 => "cursor.ns-resize",
        28 => "cursor.nesw-resize",
        29 => "cursor.nwse-resize",
        30 => "cursor.col-resize",
        31 => "cursor.row-resize",
        32 => "cursor.all-scroll",
        33 => "cursor.zoom-in",
        34 => "cursor.zoom-out",
        35 => "cursor.dnd-ask",
        36 => "cursor.all-resize",
        _ => return None,
    })
}

fn cursor_shape_pointer_icon(shape: u32) -> Option<PointerIcon> {
    Some(match shape {
        1 => PointerIcon::Default,
        2 => PointerIcon::ContextMenu,
        3 => PointerIcon::Help,
        4 => PointerIcon::Pointer,
        5 => PointerIcon::Progress,
        6 => PointerIcon::Wait,
        7 => PointerIcon::Cell,
        8 => PointerIcon::Crosshair,
        9 => PointerIcon::Text,
        10 => PointerIcon::VerticalText,
        11 => PointerIcon::Alias,
        12 => PointerIcon::Copy,
        13 => PointerIcon::Move,
        14 => PointerIcon::NoDrop,
        15 => PointerIcon::NotAllowed,
        16 => PointerIcon::Grab,
        17 => PointerIcon::Grabbing,
        18 => PointerIcon::EResize,
        19 => PointerIcon::NResize,
        20 => PointerIcon::NeResize,
        21 => PointerIcon::NwResize,
        22 => PointerIcon::SResize,
        23 => PointerIcon::SeResize,
        24 => PointerIcon::SwResize,
        25 => PointerIcon::WResize,
        26 => PointerIcon::EwResize,
        27 => PointerIcon::NsResize,
        28 => PointerIcon::NeswResize,
        29 => PointerIcon::NwseResize,
        30 => PointerIcon::ColResize,
        31 => PointerIcon::RowResize,
        32 => PointerIcon::AllScroll,
        33 => PointerIcon::ZoomIn,
        34 => PointerIcon::ZoomOut,
        35 => PointerIcon::DndAsk,
        36 => PointerIcon::AllResize,
        _ => return None,
    })
}

fn pointer_icon_cursor_shape(icon: PointerIcon) -> u32 {
    match icon {
        PointerIcon::Default => 1,
        PointerIcon::ContextMenu => 2,
        PointerIcon::Help => 3,
        PointerIcon::Pointer => 4,
        PointerIcon::Progress => 5,
        PointerIcon::Wait => 6,
        PointerIcon::Cell => 7,
        PointerIcon::Crosshair => 8,
        PointerIcon::Text => 9,
        PointerIcon::VerticalText => 10,
        PointerIcon::Alias => 11,
        PointerIcon::Copy => 12,
        PointerIcon::Move => 13,
        PointerIcon::NoDrop => 14,
        PointerIcon::NotAllowed => 15,
        PointerIcon::Grab => 16,
        PointerIcon::Grabbing => 17,
        PointerIcon::EResize => 18,
        PointerIcon::NResize => 19,
        PointerIcon::NeResize => 20,
        PointerIcon::NwResize => 21,
        PointerIcon::SResize => 22,
        PointerIcon::SeResize => 23,
        PointerIcon::SwResize => 24,
        PointerIcon::WResize => 25,
        PointerIcon::EwResize => 26,
        PointerIcon::NsResize => 27,
        PointerIcon::NeswResize => 28,
        PointerIcon::NwseResize => 29,
        PointerIcon::ColResize => 30,
        PointerIcon::RowResize => 31,
        PointerIcon::AllScroll => 32,
        PointerIcon::ZoomIn => 33,
        PointerIcon::ZoomOut => 34,
        PointerIcon::DndAsk => 35,
        PointerIcon::AllResize => 36,
    }
}

fn window_content_origin(window: &ClientWindow, config: &LinuxDesktopConfig) -> PointI {
    let offset = window_content_offset(window, config);
    PointI {
        x: window.position.x + offset.x,
        y: window.position.y + offset.y,
    }
}

fn window_content_offset(window: &ClientWindow, config: &LinuxDesktopConfig) -> PointI {
    if !window_is_decorated(window) {
        PointI::default()
    } else {
        window.chrome_content_offset.unwrap_or(PointI {
            x: config.window_border,
            y: config.window_border + config.titlebar_height,
        })
    }
}

fn legacy_window_outer(window: &ClientWindow, config: &LinuxDesktopConfig) -> SizeI {
    SizeI {
        width: window.size.width + config.window_border * 2,
        height: window.size.height + config.window_border * 2 + config.titlebar_height,
    }
}

fn wayland_resize_edge(edge: WindowResizeEdge) -> ResizeEdge {
    match edge {
        WindowResizeEdge::Top => ResizeEdge::Top,
        WindowResizeEdge::TopRight => ResizeEdge::TopRight,
        WindowResizeEdge::Right => ResizeEdge::Right,
        WindowResizeEdge::BottomRight => ResizeEdge::BottomRight,
        WindowResizeEdge::Bottom => ResizeEdge::Bottom,
        WindowResizeEdge::BottomLeft => ResizeEdge::BottomLeft,
        WindowResizeEdge::Left => ResizeEdge::Left,
        WindowResizeEdge::TopLeft => ResizeEdge::TopLeft,
    }
}

fn window_is_decorated(window: &ClientWindow) -> bool {
    window.server_decorated && !window.fullscreen
}

fn surface_local_position(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    position: PointF,
    config: &LinuxDesktopConfig,
) -> PointF {
    let origin = windows.get(&surface).map_or(PointI::default(), |window| {
        window_content_origin(window, config)
    });
    PointF {
        x: position.x - origin.x as f32,
        y: position.y - origin.y as f32,
    }
}

fn constrain_pointer(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    current: PointF,
    proposed: PointF,
    constraint: &PointerConstraintState,
    config: &LinuxDesktopConfig,
) -> PointF {
    let Some(window) = windows.get(&constraint.surface) else {
        return current;
    };
    let origin = window_content_origin(window, config);
    let local = PointF {
        x: proposed.x - origin.x as f32,
        y: proposed.y - origin.y as f32,
    };
    let surface = RectI {
        x: 0,
        y: 0,
        width: window.size.width,
        height: window.size.height,
    };
    let regions = constraint.region.as_ref().map_or_else(
        || vec![surface],
        |region| {
            region
                .rectangles()
                .iter()
                .filter_map(|rectangle| intersect_rect(*rectangle, surface))
                .collect()
        },
    );
    let Some(nearest) = regions
        .iter()
        .map(|rectangle| {
            let maximum_x = (rectangle.x + rectangle.width) as f32 - 0.001;
            let maximum_y = (rectangle.y + rectangle.height) as f32 - 0.001;
            let point = PointF {
                x: local.x.clamp(rectangle.x as f32, maximum_x),
                y: local.y.clamp(rectangle.y as f32, maximum_y),
            };
            let distance_x = local.x - point.x;
            let distance_y = local.y - point.y;
            (distance_x * distance_x + distance_y * distance_y, point)
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, point)| point)
    else {
        return current;
    };
    PointF {
        x: nearest.x + origin.x as f32,
        y: nearest.y + origin.y as f32,
    }
}

fn intersect_rect(left: RectI, right: RectI) -> Option<RectI> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = (left.x + left.width).min(right.x + right.width);
    let bottom_edge = (left.y + left.height).min(right.y + right.height);
    (right_edge > x && bottom_edge > y).then_some(RectI {
        x,
        y,
        width: right_edge - x,
        height: bottom_edge - y,
    })
}

fn full_rect(size: SizeI) -> RectI {
    RectI {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
    }
}

fn union_rect(left: RectI, right: RectI) -> RectI {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = left.right().max(right.right());
    let bottom = left.bottom().max(right.bottom());
    RectI {
        x,
        y,
        width: right_edge.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

#[cfg(feature = "profiler")]
fn rect_area(rect: RectI) -> u64 {
    u64::try_from(rect.width)
        .ok()
        .and_then(|width| {
            u64::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .unwrap_or(0)
}

/// Returns the damage that must be applied to bring a retained scanout target from
/// `previous_version` to `current_version`. `None` deliberately means a full redraw.
fn accumulated_damage(
    previous_version: u64,
    current_version: u64,
    history: &VecDeque<(u64, Option<RectI>)>,
    extent: SizeI,
) -> Option<RectI> {
    if previous_version == 0 || previous_version >= current_version {
        return None;
    }
    let oldest_version = history.front().map(|(version, _)| *version)?;
    if previous_version.saturating_add(1) < oldest_version {
        return None;
    }
    let mut combined = None::<RectI>;
    for (_, damage) in history
        .iter()
        .filter(|(version, _)| *version > previous_version && *version <= current_version)
    {
        let Some(damage) = damage else {
            return None;
        };
        combined = Some(combined.map_or(*damage, |old| union_rect(old, *damage)));
    }
    combined.and_then(|damage| intersect_rect(damage, full_rect(extent)))
}

fn copy_rgba_region(source: &[u8], size: SizeI, region: RectI) -> Vec<u8> {
    let source_stride = size.width.max(0) as usize * 4;
    let row_bytes = region.width.max(0) as usize * 4;
    let x = region.x.max(0) as usize * 4;
    let mut pixels = Vec::with_capacity(row_bytes * region.height.max(0) as usize);
    for row in region.y.max(0) as usize..(region.y + region.height).max(0) as usize {
        let start = row * source_stride + x;
        pixels.extend_from_slice(&source[start..start + row_bytes]);
    }
    pixels
}

fn copy_rgba_region_into(source: &[u8], target: &mut [u8], size: SizeI, region: RectI) {
    let stride = size.width.max(0) as usize * 4;
    let row_bytes = region.width.max(0) as usize * 4;
    let x = region.x.max(0) as usize * 4;
    for row in region.y.max(0) as usize..(region.y + region.height).max(0) as usize {
        let start = row * stride + x;
        target[start..start + row_bytes].copy_from_slice(&source[start..start + row_bytes]);
    }
}

fn shell_work_area(output: SizeI, widgets: &[WidgetLayer]) -> RectI {
    let mut area = RectI {
        x: 0,
        y: 0,
        width: output.width,
        height: output.height,
    };
    for widget in widgets {
        let reserved = widget.reserved_space.max(0);
        match widget.anchor {
            ShellWidgetAnchor::Top => {
                let reserved = reserved.min(area.height.saturating_sub(1));
                area.y = area.y.saturating_add(reserved);
                area.height = area.height.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Right | ShellWidgetAnchor::Bottom => {
                let extent = if widget.anchor == ShellWidgetAnchor::Right {
                    &mut area.width
                } else {
                    &mut area.height
                };
                let reserved = reserved.min(extent.saturating_sub(1));
                *extent = extent.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Left => {
                let reserved = reserved.min(area.width.saturating_sub(1));
                area.x = area.x.saturating_add(reserved);
                area.width = area.width.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Floating => {}
        }
    }
    area
}

fn resolved_widget_extent(
    output: SizeI,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
) -> SizeI {
    SizeI {
        width: match width {
            ShellWidgetExtent::Fill => output.width,
            ShellWidgetExtent::Pixels(value) => value.round().max(1.0) as i32,
        },
        height: match height {
            ShellWidgetExtent::Fill => output.height,
            ShellWidgetExtent::Pixels(value) => value.round().max(1.0) as i32,
        },
    }
}

fn widget_position(output: SizeI, widget: SizeI, anchor: ShellWidgetAnchor) -> PointI {
    match anchor {
        ShellWidgetAnchor::Top | ShellWidgetAnchor::Left => PointI::default(),
        ShellWidgetAnchor::Right => PointI {
            x: output.width - widget.width,
            y: 0,
        },
        ShellWidgetAnchor::Bottom => PointI {
            x: 0,
            y: output.height - widget.height,
        },
        ShellWidgetAnchor::Floating => PointI {
            x: (output.width - widget.width) / 2,
            y: (output.height - widget.height) / 2,
        },
    }
}

fn composite_rgba(
    target: &mut [u8],
    target_size: SizeI,
    source: &[u8],
    source_size: SizeI,
    position: PointI,
    premultiplied: bool,
) {
    if source.len() < source_size.width.max(0) as usize * source_size.height.max(0) as usize * 4 {
        return;
    }
    for source_y in 0..source_size.height {
        let target_y = position.y + source_y;
        if !(0..target_size.height).contains(&target_y) {
            continue;
        }
        for source_x in 0..source_size.width {
            let target_x = position.x + source_x;
            if !(0..target_size.width).contains(&target_x) {
                continue;
            }
            let source_index =
                (source_y as usize * source_size.width as usize + source_x as usize) * 4;
            let target_index =
                (target_y as usize * target_size.width as usize + target_x as usize) * 4;
            let alpha = f32::from(source[source_index + 3]) / 255.0;
            let inverse = 1.0 - alpha;
            for channel in 0..3 {
                let source = f32::from(source[source_index + channel]);
                let destination = f32::from(target[target_index + channel]);
                let value = if premultiplied {
                    source + destination * inverse
                } else {
                    source * alpha + destination * inverse
                };
                target[target_index + channel] = value.round().clamp(0.0, 255.0) as u8;
            }
            target[target_index + 3] = 255;
        }
    }
}

fn app_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_cursor_motion_coalesces_while_a_commit_is_in_flight() {
        let mut cursor = CursorCommitTracker::default();
        cursor.move_to(PointI { x: 10, y: 20 });
        assert_eq!(cursor.desired_submission(), None);

        cursor.show(0, PointI::default());
        let submitted = cursor
            .desired_submission()
            .expect("visible cursor is dirty");
        cursor.mark_submitted(submitted).unwrap();
        cursor.move_to(PointI { x: 30, y: 40 });
        assert_eq!(cursor.desired_submission(), None);

        cursor.mark_completed(submitted).unwrap();
        let coalesced = cursor
            .desired_submission()
            .expect("newest position follows the completed generation");
        assert_eq!(coalesced.position, PointI { x: 30, y: 40 });
        assert_eq!(coalesced.buffer, Some(0));
    }

    #[test]
    fn atomic_cursor_staging_never_reuses_current_or_in_flight_buffers() {
        let mut cursor = CursorCommitTracker::default();
        cursor.show(0, PointI::default());
        let first = cursor.desired_submission().unwrap();
        cursor.mark_submitted(first).unwrap();
        cursor.mark_completed(first).unwrap();

        cursor.show(1, PointI::default());
        let second = cursor.desired_submission().unwrap();
        cursor.mark_submitted(second).unwrap();

        assert_eq!(cursor.current_buffer, Some(0));
        assert_eq!(cursor.in_flight.and_then(|state| state.buffer), Some(1));
        assert_eq!(
            cursor.reusable_buffer(HARDWARE_CURSOR_BUFFER_COUNT),
            Some(2)
        );
    }

    #[test]
    fn hardware_cursor_fallback_waits_for_plane_disable_completion() {
        let mut cursor = CursorCommitTracker::default();
        cursor.show(0, PointI::default());
        let visible = cursor.desired_submission().unwrap();
        cursor.mark_submitted(visible).unwrap();
        cursor.mark_completed(visible).unwrap();

        cursor.request_software_fallback();
        assert!(!cursor.ready_to_retire());
        let hidden = cursor.desired_submission().expect("plane disable is dirty");
        assert!(!hidden.visible);
        cursor.mark_submitted(hidden).unwrap();
        cursor.mark_completed(hidden).unwrap();
        assert!(cursor.ready_to_retire());
    }

    #[test]
    fn vulkan_staging_budget_covers_one_full_hd_upload_per_slot() {
        let slots = 2;
        let budget = vulkan_staging_budget_bytes(
            SizeI {
                width: 1920,
                height: 1080,
            },
            slots,
        )
        .expect("Full HD staging budget should fit");
        let bytes_per_slot = budget / slots as u64;

        assert_eq!(
            bytes_per_slot,
            1920 * 1080 * 4 + VULKAN_STAGING_HEADROOM_BYTES_PER_SLOT
        );
        // This is the upload size reported by the original Raspberry Pi Full HD failure.
        assert!(bytes_per_slot >= 8_294_628);
    }

    #[test]
    fn vulkan_staging_budget_applies_the_minimum_to_every_slot() {
        let slots = 3;
        let budget = vulkan_staging_budget_bytes(
            SizeI {
                width: 640,
                height: 480,
            },
            slots,
        )
        .expect("small scanout staging budget should fit");

        assert_eq!(budget, VULKAN_STAGING_MIN_BYTES_PER_SLOT * slots as u64);
    }

    #[test]
    fn vulkan_staging_budget_rejects_total_slot_overflow() {
        assert!(
            vulkan_staging_budget_bytes(
                SizeI {
                    width: i32::MAX,
                    height: i32::MAX,
                },
                usize::MAX,
            )
            .is_err()
        );
    }

    #[test]
    fn accumulated_damage_unions_every_change_since_the_target_version() {
        let history = VecDeque::from([
            (1, None),
            (
                2,
                Some(RectI {
                    x: 10,
                    y: 20,
                    width: 30,
                    height: 40,
                }),
            ),
            (
                3,
                Some(RectI {
                    x: 35,
                    y: 45,
                    width: 20,
                    height: 10,
                }),
            ),
        ]);

        assert_eq!(
            accumulated_damage(
                1,
                3,
                &history,
                SizeI {
                    width: 100,
                    height: 100,
                },
            ),
            Some(RectI {
                x: 10,
                y: 20,
                width: 45,
                height: 35,
            })
        );
    }

    #[test]
    fn accumulated_damage_requires_full_redraw_after_a_full_change_or_history_gap() {
        let extent = SizeI {
            width: 100,
            height: 100,
        };
        let full_change = VecDeque::from([
            (
                2,
                Some(RectI {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 2,
                }),
            ),
            (3, None),
        ]);
        let history_gap = VecDeque::from([(
            4,
            Some(RectI {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }),
        )]);

        assert_eq!(accumulated_damage(1, 3, &full_change, extent), None);
        assert_eq!(accumulated_damage(1, 4, &history_gap, extent), None);
        assert_eq!(accumulated_damage(0, 4, &history_gap, extent), None);
    }

    #[test]
    fn every_wayland_cursor_shape_round_trips_through_pointer_icons() {
        for shape in 1..=36 {
            let icon = cursor_shape_pointer_icon(shape).expect("shape must be supported");
            assert_eq!(pointer_icon_cursor_shape(icon), shape);
            assert!(cursor_shape_icon_name(shape).is_some());
        }
    }

    #[test]
    fn cursor_image_transitions_require_presentation_without_scene_damage() {
        assert!(cursor_transition_requires_presentation(
            CursorImage::Hidden,
            CursorImage::Shape(pointer_icon_cursor_shape(PointerIcon::Move)),
        ));
        assert!(cursor_transition_requires_presentation(
            CursorImage::Shape(pointer_icon_cursor_shape(PointerIcon::Move)),
            CursorImage::TelorgonDefault,
        ));
        assert!(!cursor_transition_requires_presentation(
            CursorImage::TelorgonDefault,
            CursorImage::TelorgonDefault,
        ));
    }

    #[test]
    fn unavailable_directional_resize_cursors_fall_back_without_recursing() {
        for icon in [
            PointerIcon::EResize,
            PointerIcon::NResize,
            PointerIcon::NeResize,
            PointerIcon::NwResize,
            PointerIcon::SResize,
            PointerIcon::SeResize,
            PointerIcon::SwResize,
            PointerIcon::WResize,
            PointerIcon::EwResize,
            PointerIcon::NsResize,
            PointerIcon::NeswResize,
            PointerIcon::NwseResize,
            PointerIcon::ColResize,
            PointerIcon::RowResize,
        ] {
            assert_eq!(
                semantic_pointer_fallback(icon),
                Some(PointerIcon::AllResize)
            );
        }
        assert_eq!(
            semantic_pointer_fallback(PointerIcon::AllResize),
            Some(PointerIcon::Default)
        );
        assert_eq!(semantic_pointer_fallback(PointerIcon::Default), None);
    }

    #[test]
    fn client_commits_do_not_roll_back_a_newer_requested_resize() {
        let committed = SizeI {
            width: 640,
            height: 480,
        };
        let requested = SizeI {
            width: 720,
            height: 540,
        };

        assert_eq!(retained_requested_size(None, committed), committed);
        assert_eq!(
            retained_requested_size(Some(requested), committed),
            requested
        );
    }

    #[test]
    fn resize_drag_geometry_is_derived_from_the_press_time_baseline() {
        let position = PointI { x: 100, y: 80 };
        let size = SizeI {
            width: 400,
            height: 300,
        };
        let output = SizeI {
            width: 1920,
            height: 1080,
        };

        assert_eq!(
            resize_drag_geometry(
                position,
                size,
                ResizeEdge::Right,
                PointI { x: 50, y: 0 },
                output,
            ),
            (
                position,
                SizeI {
                    width: 450,
                    height: 300,
                },
            )
        );
        assert_eq!(
            resize_drag_geometry(
                position,
                size,
                ResizeEdge::Right,
                PointI { x: 20, y: 0 },
                output,
            )
            .1
            .width,
            420
        );
        assert_eq!(
            resize_drag_geometry(
                position,
                size,
                ResizeEdge::Bottom,
                PointI { x: 0, y: 70 },
                output,
            )
            .1
            .height,
            370
        );
    }

    #[test]
    fn left_and_top_resize_edges_keep_the_opposite_edges_anchored() {
        let position = PointI { x: 100, y: 80 };
        let size = SizeI {
            width: 400,
            height: 300,
        };
        let output = SizeI {
            width: 1920,
            height: 1080,
        };
        let (resized_position, resized_size) = resize_drag_geometry(
            position,
            size,
            ResizeEdge::TopLeft,
            PointI { x: 40, y: 30 },
            output,
        );

        assert_eq!(resized_position, PointI { x: 140, y: 110 });
        assert_eq!(
            resized_size,
            SizeI {
                width: 360,
                height: 270,
            }
        );
        assert_eq!(
            resized_position.x + resized_size.width,
            position.x + size.width
        );
        assert_eq!(
            resized_position.y + resized_size.height,
            position.y + size.height
        );

        let (minimum_position, minimum_size) = resize_drag_geometry(
            position,
            size,
            ResizeEdge::TopLeft,
            PointI {
                x: 10_000,
                y: 10_000,
            },
            output,
        );
        assert_eq!(
            minimum_size,
            SizeI {
                width: 64,
                height: 48
            }
        );
        assert_eq!(
            minimum_position.x + minimum_size.width,
            position.x + size.width
        );
        assert_eq!(
            minimum_position.y + minimum_size.height,
            position.y + size.height
        );
    }
}
