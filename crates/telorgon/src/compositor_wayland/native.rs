use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::{c_long, c_void};
use std::fmt;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::FileExt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::core::{PointI, RectI};
use crate::wayland_server::ffi;
use crate::wayland_server::{
    ClientRef, Display, Global, IncomingRequest, NativeProtocol, ProtocolCatalog, ResourceRef,
};

use crate::compositor_wayland::synchronization::{
    take_surface_commits_through, take_surface_feedbacks_through,
};
use crate::compositor_wayland::{
    BufferAttachment, BufferDescriptor, BufferTransform, ClientId, ClientLimits, CompositorAction,
    CompositorCore, ObjectMetadata, ProtocolObjectId, ProtocolObjectKind, Region, ShmBuffer,
    ShmFormat, ShmPool, SurfaceRole, WaylandBufferId, WaylandSurfaceId, XdgConfigure,
    XdgToplevelState,
};

const IMPLEMENTED_GLOBALS: &[(&str, ResourceKind, u32)] = &[
    ("wl_compositor", ResourceKind::Compositor, 6),
    ("wl_shm", ResourceKind::Shm, 1),
    ("wl_subcompositor", ResourceKind::Subcompositor, 1),
    ("wl_data_device_manager", ResourceKind::DataDeviceManager, 3),
    ("xdg_wm_base", ResourceKind::XdgWmBase, 7),
    (
        "zxdg_decoration_manager_v1",
        ResourceKind::DecorationManager,
        1,
    ),
    (
        "wp_cursor_shape_manager_v1",
        ResourceKind::CursorShapeManager,
        1,
    ),
    (
        "xdg_toplevel_icon_manager_v1",
        ResourceKind::ToplevelIconManager,
        1,
    ),
    (
        "wp_fractional_scale_manager_v1",
        ResourceKind::FractionalScaleManager,
        1,
    ),
    ("wp_viewporter", ResourceKind::Viewporter, 1),
    ("wp_presentation", ResourceKind::Presentation, 2),
    ("xdg_activation_v1", ResourceKind::Activation, 1),
    (
        "ext_session_lock_manager_v1",
        ResourceKind::SessionLockManager,
        1,
    ),
    (
        "zwp_relative_pointer_manager_v1",
        ResourceKind::RelativePointerManager,
        1,
    ),
    (
        "zwp_idle_inhibit_manager_v1",
        ResourceKind::IdleInhibitManager,
        1,
    ),
    (
        "zwp_pointer_constraints_v1",
        ResourceKind::PointerConstraints,
        1,
    ),
];

#[derive(Clone, Copy, Debug)]
enum ResourceKind {
    Compositor,
    Surface(WaylandSurfaceId),
    Region(ProtocolObjectId),
    Shm,
    ShmPool(ProtocolObjectId),
    Buffer(WaylandBufferId),
    Subcompositor,
    Subsurface(WaylandSurfaceId),
    XdgWmBase,
    XdgPositioner(ProtocolObjectId),
    XdgSurface(WaylandSurfaceId),
    XdgToplevel(WaylandSurfaceId),
    XdgPopup(WaylandSurfaceId),
    Callback(WaylandSurfaceId),
    Output(u32),
    Seat(u32),
    Pointer(u32),
    Keyboard(u32),
    Touch(u32),
    DataDeviceManager,
    DataDevice(u32),
    DataSource(ProtocolObjectId),
    DataOffer(ProtocolObjectId),
    LinuxDmaBuf,
    LinuxBufferParams(ProtocolObjectId),
    DecorationManager,
    ToplevelDecoration(WaylandSurfaceId),
    CursorShapeManager,
    CursorShapeDevice(u32),
    ToplevelIconManager,
    ToplevelIcon(ProtocolObjectId),
    FractionalScaleManager,
    FractionalScale,
    Viewporter,
    Viewport(WaylandSurfaceId),
    Presentation,
    PresentationFeedback(WaylandSurfaceId),
    Activation,
    ActivationToken(ProtocolObjectId),
    SessionLockManager,
    SessionLock(ProtocolObjectId),
    SessionLockSurface(WaylandSurfaceId),
    RelativePointerManager,
    RelativePointer(u32),
    IdleInhibitManager,
    IdleInhibitor(ProtocolObjectId),
    PointerConstraints,
    LockedPointer(ProtocolObjectId),
    ConfinedPointer(ProtocolObjectId),
    ExplicitSynchronization,
    SurfaceSynchronization(WaylandSurfaceId),
    ExplicitBufferRelease(WaylandSurfaceId),
}

impl ResourceKind {
    fn object_kind(self) -> ProtocolObjectKind {
        match self {
            Self::Compositor => ProtocolObjectKind::Compositor,
            Self::Surface(_) => ProtocolObjectKind::Surface,
            Self::Region(_) => ProtocolObjectKind::Region,
            Self::Shm => ProtocolObjectKind::Shm,
            Self::ShmPool(_) => ProtocolObjectKind::ShmPool,
            Self::Buffer(_) => ProtocolObjectKind::Buffer,
            Self::Subcompositor => ProtocolObjectKind::Subcompositor,
            Self::Subsurface(_) => ProtocolObjectKind::Subsurface,
            Self::XdgWmBase => ProtocolObjectKind::XdgWmBase,
            Self::XdgPositioner(_) => ProtocolObjectKind::XdgPositioner,
            Self::XdgSurface(_) => ProtocolObjectKind::XdgSurface,
            Self::XdgToplevel(_) => ProtocolObjectKind::XdgToplevel,
            Self::XdgPopup(_) => ProtocolObjectKind::XdgPopup,
            Self::Callback(_) => ProtocolObjectKind::Callback,
            Self::Output(_) => ProtocolObjectKind::Output,
            Self::Seat(_) => ProtocolObjectKind::Seat,
            Self::Pointer(_) => ProtocolObjectKind::Pointer,
            Self::Keyboard(_) => ProtocolObjectKind::Keyboard,
            Self::Touch(_) => ProtocolObjectKind::Touch,
            Self::DataDeviceManager => ProtocolObjectKind::DataDeviceManager,
            Self::DataDevice(_) => ProtocolObjectKind::DataDevice,
            Self::DataSource(_) => ProtocolObjectKind::DataSource,
            Self::DataOffer(_) => ProtocolObjectKind::DataOffer,
            Self::LinuxDmaBuf => ProtocolObjectKind::LinuxDmaBuf,
            Self::LinuxBufferParams(_) => ProtocolObjectKind::LinuxBufferParams,
            Self::DecorationManager => ProtocolObjectKind::DecorationManager,
            Self::ToplevelDecoration(_) => ProtocolObjectKind::ToplevelDecoration,
            Self::CursorShapeManager => ProtocolObjectKind::CursorShapeManager,
            Self::CursorShapeDevice(_) => ProtocolObjectKind::CursorShapeDevice,
            Self::ToplevelIconManager => ProtocolObjectKind::ToplevelIconManager,
            Self::ToplevelIcon(_) => ProtocolObjectKind::ToplevelIcon,
            Self::FractionalScaleManager => ProtocolObjectKind::FractionalScaleManager,
            Self::FractionalScale => ProtocolObjectKind::FractionalScale,
            Self::Viewporter => ProtocolObjectKind::Viewporter,
            Self::Viewport(_) => ProtocolObjectKind::Viewport,
            Self::Presentation => ProtocolObjectKind::Presentation,
            Self::PresentationFeedback(_) => ProtocolObjectKind::PresentationFeedback,
            Self::Activation => ProtocolObjectKind::Activation,
            Self::ActivationToken(_) => ProtocolObjectKind::ActivationToken,
            Self::SessionLockManager => ProtocolObjectKind::SessionLockManager,
            Self::SessionLock(_) => ProtocolObjectKind::SessionLock,
            Self::SessionLockSurface(_) => ProtocolObjectKind::SessionLockSurface,
            Self::RelativePointerManager => ProtocolObjectKind::RelativePointerManager,
            Self::RelativePointer(_) => ProtocolObjectKind::RelativePointer,
            Self::IdleInhibitManager => ProtocolObjectKind::IdleInhibitManager,
            Self::IdleInhibitor(_) => ProtocolObjectKind::IdleInhibitor,
            Self::PointerConstraints => ProtocolObjectKind::PointerConstraints,
            Self::LockedPointer(_) => ProtocolObjectKind::LockedPointer,
            Self::ConfinedPointer(_) => ProtocolObjectKind::ConfinedPointer,
            Self::ExplicitSynchronization => ProtocolObjectKind::ExplicitSynchronization,
            Self::SurfaceSynchronization(_) => ProtocolObjectKind::SurfaceSynchronization,
            Self::ExplicitBufferRelease(_) => ProtocolObjectKind::LinuxBufferRelease,
        }
    }
}

struct ResourceContext {
    state: *mut NativeState,
    object: ProtocolObjectId,
    client: ClientId,
    interface: String,
    kind: ResourceKind,
}

struct BindContext {
    state: *mut NativeState,
    interface: &'static str,
    kind: ResourceKind,
}

struct NativeShmPool {
    owner: ClientId,
    fd: OwnedFd,
    pool: ShmPool,
}

#[derive(Debug)]
struct NativeDmaBufPlane {
    fd: OwnedFd,
    offset: u32,
    stride: u32,
    modifier: u64,
}

#[derive(Debug, Default)]
struct NativeDmaBufParams {
    planes: BTreeMap<u32, NativeDmaBufPlane>,
    used: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaBufFormat {
    pub fourcc: u32,
    pub modifier: u64,
}

#[derive(Debug)]
pub struct DmaBufImage {
    pub descriptor: crate::compositor_wayland::DmaBufDescriptor,
    pub planes: Vec<OwnedFd>,
}

impl DmaBufImage {
    /// Snapshots the producer write fences carried by this DMA-BUF for a Vulkan read submission.
    ///
    /// Linux-DMA-BUF uses implicit synchronization unless a protocol extension supplies an
    /// explicit acquire fence. Vulkan itself is explicit-only, so the kernel reservation fences
    /// must be exported as a sync file before importing the image into a Vulkan command stream.
    pub fn export_implicit_read_sync_file(&self) -> Result<OwnedFd, NativeCompositorError> {
        let plane = self
            .planes
            .first()
            .ok_or_else(|| NativeCompositorError::new("DMA-BUF image has no plane file"))?;
        let mut export = crate::platform_linux::ffi::dma_buf_export_sync_file {
            flags: crate::platform_linux::ffi::DMA_BUF_SYNC_READ,
            fd: -1,
        };
        let result = unsafe {
            crate::platform_linux::ffi::ioctl(
                plane.as_raw_fd(),
                crate::platform_linux::ffi::DMA_BUF_IOCTL_EXPORT_SYNC_FILE,
                std::ptr::from_mut(&mut export),
            )
        };
        if result != 0 {
            return Err(NativeCompositorError::new(format!(
                "failed to export the DMA-BUF implicit read fence: {}",
                std::io::Error::last_os_error()
            )));
        }
        if export.fd < 0 {
            return Err(NativeCompositorError::new(
                "DMA-BUF implicit read-fence export returned no sync file",
            ));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(export.fd) })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportSource {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewportState {
    pub source: Option<ViewportSource>,
    pub destination: Option<crate::core::SizeI>,
}

#[derive(Clone, Copy, Debug, Default)]
struct NativeViewport {
    current: ViewportState,
    pending_source: Option<Option<ViewportSource>>,
    pending_destination: Option<Option<crate::core::SizeI>>,
}

#[derive(Clone, Copy, Debug)]
struct NativeTouchPoint {
    client: ClientId,
    surface: WaylandSurfaceId,
    down_serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeDragGrab {
    Pointer,
    Touch(i32),
}

#[derive(Debug)]
struct NativeDragTarget {
    surface: WaylandSurfaceId,
    devices: Vec<ProtocolObjectId>,
    offers: Vec<ProtocolObjectId>,
}

#[derive(Debug)]
struct NativeDrag {
    seat: u32,
    source: Option<ProtocolObjectId>,
    origin: WaylandSurfaceId,
    icon: Option<WaylandSurfaceId>,
    grab: NativeDragGrab,
    target: Option<NativeDragTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerConstraintKind {
    Locked,
    Confined,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerConstraintState {
    pub kind: PointerConstraintKind,
    pub surface: WaylandSurfaceId,
    pub region: Option<Region>,
}

#[derive(Clone, Debug)]
struct NativePointerConstraint {
    seat: u32,
    surface: WaylandSurfaceId,
    kind: PointerConstraintKind,
    region: Option<Region>,
    cursor_hint: Option<crate::core::PointF>,
    persistent: bool,
    active: bool,
    finished: bool,
}

#[derive(Debug, Default)]
struct NativeActivationToken {
    serial: Option<(u32, u32)>,
    application_id: Option<String>,
    surface: Option<WaylandSurfaceId>,
    committed: bool,
}

#[derive(Clone, Debug)]
struct NativeActivationGrant {
    authorized: bool,
    application_id: Option<String>,
    source_surface: Option<WaylandSurfaceId>,
}

#[derive(Debug)]
struct NativeSessionLock {
    client: ClientId,
    locked_event_sent: bool,
    finished_event_sent: bool,
}

#[derive(Debug)]
struct NativeSessionLockSurface {
    lock: ProtocolObjectId,
    output: u32,
    pending_configures: VecDeque<(u32, crate::core::SizeI)>,
    last_acked: Option<(u32, crate::core::SizeI)>,
}

#[derive(Debug, Default)]
struct NativeToplevelIcon {
    name: Option<String>,
    buffers: BTreeMap<(i32, i32), WaylandBufferId>,
    immutable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToplevelIconImage {
    pub buffer: WaylandBufferId,
    pub scale: i32,
    pub image: ShmImage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToplevelIconSnapshot {
    pub revision: u64,
    pub name: Option<String>,
    pub images: Vec<ToplevelIconImage>,
}

#[derive(Clone, Debug)]
enum PendingToplevelIcon {
    Reset,
    Icon(ToplevelIconSnapshot),
}

impl NativeViewport {
    fn commit(&mut self) {
        if let Some(source) = self.pending_source.take() {
            self.current.source = source;
        }
        if let Some(destination) = self.pending_destination.take() {
            self.current.destination = destination;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct NativeXdgPositioner {
    size: Option<crate::core::SizeI>,
    anchor_rect: Option<RectI>,
    anchor: u32,
    gravity: u32,
    constraint_adjustment: u32,
    offset: PointI,
    reactive: bool,
    parent_size: Option<crate::core::SizeI>,
    parent_configure: Option<u32>,
}

impl NativeXdgPositioner {
    fn finish(self) -> Result<crate::compositor_wayland::XdgPositioner, NativeCompositorError> {
        crate::compositor_wayland::XdgPositioner {
            size: self
                .size
                .ok_or_else(|| NativeCompositorError::new("positioner size was not set"))?,
            anchor_rect: self.anchor_rect.ok_or_else(|| {
                NativeCompositorError::new("positioner anchor rectangle was not set")
            })?,
            anchor: self.anchor,
            gravity: self.gravity,
            constraint_adjustment: self.constraint_adjustment,
            offset: self.offset,
            reactive: self.reactive,
            parent_size: self.parent_size,
            parent_configure: self.parent_configure,
        }
        .validate()
        .map_err(error)
    }
}

struct NativeState {
    display: std::ptr::NonNull<ffi::wl_display>,
    protocol: NativeProtocol,
    core: CompositorCore,
    clients: BTreeMap<usize, ClientId>,
    resources: BTreeMap<ProtocolObjectId, usize>,
    regions: BTreeMap<ProtocolObjectId, Vec<RectI>>,
    shm_pools: BTreeMap<ProtocolObjectId, NativeShmPool>,
    buffer_files: BTreeMap<WaylandBufferId, OwnedFd>,
    dmabuf_files: BTreeMap<WaylandBufferId, Vec<OwnedFd>>,
    callbacks: BTreeMap<WaylandSurfaceId, Vec<ProtocolObjectId>>,
    committed_callbacks: BTreeMap<(WaylandSurfaceId, u64), Vec<ProtocolObjectId>>,
    pending_presentation_feedbacks: BTreeMap<WaylandSurfaceId, Vec<ProtocolObjectId>>,
    committed_presentation_feedbacks: BTreeMap<(WaylandSurfaceId, u64), Vec<ProtocolObjectId>>,
    xdg_resources: BTreeMap<WaylandSurfaceId, ProtocolObjectId>,
    toplevels: BTreeMap<WaylandSurfaceId, XdgToplevelState>,
    toplevel_icons: BTreeMap<ProtocolObjectId, NativeToplevelIcon>,
    pending_toplevel_icons: BTreeMap<WaylandSurfaceId, PendingToplevelIcon>,
    committed_toplevel_icons: BTreeMap<WaylandSurfaceId, ToplevelIconSnapshot>,
    positioners: BTreeMap<ProtocolObjectId, NativeXdgPositioner>,
    popups: BTreeMap<WaylandSurfaceId, crate::compositor_wayland::XdgPopupState>,
    viewports: BTreeMap<WaylandSurfaceId, NativeViewport>,
    dmabuf_formats: Vec<DmaBufFormat>,
    dmabuf_params: BTreeMap<ProtocolObjectId, NativeDmaBufParams>,
    keyboard_keymaps: BTreeMap<u32, (OwnedFd, u32)>,
    touch_points: BTreeMap<(u32, i32), NativeTouchPoint>,
    active_drag: Option<NativeDrag>,
    finished_drag_sources: BTreeSet<ProtocolObjectId>,
    idle_inhibitors: BTreeMap<ProtocolObjectId, WaylandSurfaceId>,
    pointer_constraints: BTreeMap<ProtocolObjectId, NativePointerConstraint>,
    activation_tokens: BTreeMap<ProtocolObjectId, NativeActivationToken>,
    activation_grants: BTreeMap<String, NativeActivationGrant>,
    activation_order: VecDeque<String>,
    session_locks: BTreeMap<ProtocolObjectId, NativeSessionLock>,
    session_lock_surfaces: BTreeMap<WaylandSurfaceId, NativeSessionLockSurface>,
    active_session_lock: Option<ProtocolObjectId>,
    secure_session_locked: bool,
    synchronized_surfaces: BTreeSet<WaylandSurfaceId>,
    pending_acquire_fences: BTreeMap<WaylandSurfaceId, OwnedFd>,
    pending_releases: BTreeMap<WaylandSurfaceId, ProtocolObjectId>,
    committed_acquire_fences: BTreeMap<(WaylandSurfaceId, u64), OwnedFd>,
    committed_releases: BTreeMap<(WaylandSurfaceId, u64), ProtocolObjectId>,
    initial_configures: BTreeSet<WaylandSurfaceId>,
    next_client: u32,
    next_object: u32,
    next_surface: u32,
    next_buffer: u32,
    presentation_sequence: u64,
    toplevel_icon_revision: u64,
}

pub struct NativeCompositor<'display> {
    state: Box<NativeState>,
    // libwayland retains pointers to these values, so each context needs an allocation whose
    // address is stable even when this vector grows.
    #[allow(clippy::vec_box)]
    bind_contexts: Vec<Box<BindContext>>,
    globals: Vec<Global<'display>>,
}

impl Drop for NativeCompositor<'_> {
    fn drop(&mut self) {
        // Every resource context retains a pointer into `state`. Disconnect clients while that
        // state is still alive so libwayland's resource-destroy callbacks cannot dereference it
        // after this owner has been dropped. `Display::drop` may repeat this on an empty list.
        unsafe { ffi::wl_display_destroy_clients(self.state.display.as_ptr()) };
        // Libwayland also retains each bind callback and its data pointer in the corresponding
        // global. Destroy the globals before releasing those callback contexts.
        self.globals.clear();
        self.bind_contexts.clear();
    }
}

impl<'display> NativeCompositor<'display> {
    pub fn new(
        display: &'display Display,
        catalog: ProtocolCatalog,
        limits: ClientLimits,
    ) -> Result<Self, NativeCompositorError> {
        let protocol =
            NativeProtocol::new(catalog.merged_schema().map_err(error)?).map_err(error)?;
        let mut state = Box::new(NativeState {
            display: display.native_handle(),
            protocol,
            core: CompositorCore::new(limits).map_err(error)?,
            clients: BTreeMap::new(),
            resources: BTreeMap::new(),
            regions: BTreeMap::new(),
            shm_pools: BTreeMap::new(),
            buffer_files: BTreeMap::new(),
            dmabuf_files: BTreeMap::new(),
            callbacks: BTreeMap::new(),
            committed_callbacks: BTreeMap::new(),
            pending_presentation_feedbacks: BTreeMap::new(),
            committed_presentation_feedbacks: BTreeMap::new(),
            xdg_resources: BTreeMap::new(),
            toplevels: BTreeMap::new(),
            toplevel_icons: BTreeMap::new(),
            pending_toplevel_icons: BTreeMap::new(),
            committed_toplevel_icons: BTreeMap::new(),
            positioners: BTreeMap::new(),
            popups: BTreeMap::new(),
            viewports: BTreeMap::new(),
            dmabuf_formats: Vec::new(),
            dmabuf_params: BTreeMap::new(),
            keyboard_keymaps: BTreeMap::new(),
            touch_points: BTreeMap::new(),
            active_drag: None,
            finished_drag_sources: BTreeSet::new(),
            idle_inhibitors: BTreeMap::new(),
            pointer_constraints: BTreeMap::new(),
            activation_tokens: BTreeMap::new(),
            activation_grants: BTreeMap::new(),
            activation_order: VecDeque::new(),
            session_locks: BTreeMap::new(),
            session_lock_surfaces: BTreeMap::new(),
            active_session_lock: None,
            secure_session_locked: false,
            synchronized_surfaces: BTreeSet::new(),
            pending_acquire_fences: BTreeMap::new(),
            pending_releases: BTreeMap::new(),
            committed_acquire_fences: BTreeMap::new(),
            committed_releases: BTreeMap::new(),
            initial_configures: BTreeSet::new(),
            next_client: 0,
            next_object: 0,
            next_surface: 0,
            next_buffer: 0,
            presentation_sequence: 0,
            toplevel_icon_revision: 0,
        });
        let state_pointer = (&mut *state) as *mut NativeState;
        let mut bind_contexts = Vec::new();
        let mut globals = Vec::new();
        for (interface_name, kind, maximum_version) in IMPLEMENTED_GLOBALS {
            let interface = state
                .protocol
                .interface(interface_name)
                .ok_or_else(|| NativeCompositorError::new(format!("missing {interface_name}")))?;
            let advertised = crate::wayland_server::protocol::interface(interface_name)
                .map(|profile| profile.advertised_version)
                .unwrap_or(1)
                .min(interface.version as u32)
                .min(*maximum_version);
            let mut bind = Box::new(BindContext {
                state: state_pointer,
                interface: interface_name,
                kind: *kind,
            });
            let data = (&mut *bind as *mut BindContext).cast::<c_void>();
            let global =
                unsafe { display.create_global(interface, advertised, data, Some(bind_global)) }
                    .map_err(error)?;
            bind_contexts.push(bind);
            globals.push(global);
        }
        Ok(Self {
            state,
            bind_contexts,
            globals,
        })
    }

    pub fn core(&self) -> &CompositorCore {
        &self.state.core
    }

    pub fn core_mut(&mut self) -> &mut CompositorCore {
        &mut self.state.core
    }

    pub fn advertised_globals(&self) -> usize {
        debug_assert_eq!(self.bind_contexts.len(), self.globals.len());
        self.globals.len()
    }

    pub fn add_output(
        &mut self,
        display: &'display Display,
        id: u32,
        output: crate::compositor_wayland::OutputState,
    ) -> Result<(), NativeCompositorError> {
        if id == 0 || self.state.core.outputs.contains_key(&id) {
            return Err(NativeCompositorError::new(
                "invalid or duplicate output identity",
            ));
        }
        self.state.core.outputs.insert(id, output);
        self.add_dynamic_global(display, "wl_output", ResourceKind::Output(id))
    }

    pub fn add_seat(
        &mut self,
        display: &'display Display,
        id: u32,
        seat: crate::compositor_wayland::SeatState,
    ) -> Result<(), NativeCompositorError> {
        if id == 0 || self.state.core.seats.contains_key(&id) {
            return Err(NativeCompositorError::new(
                "invalid or duplicate seat identity",
            ));
        }
        self.state.core.seats.insert(id, seat);
        self.add_dynamic_global(display, "wl_seat", ResourceKind::Seat(id))
    }

    pub fn add_linux_dmabuf(
        &mut self,
        display: &'display Display,
        mut formats: Vec<DmaBufFormat>,
    ) -> Result<(), NativeCompositorError> {
        formats.sort_unstable_by_key(|format| (format.fourcc, format.modifier));
        formats.dedup();
        if formats.is_empty() {
            return Err(NativeCompositorError::new(
                "DMA-BUF cannot be advertised without an importable format",
            ));
        }
        self.state.dmabuf_formats = formats;
        self.add_dynamic_global_version(
            display,
            "zwp_linux_dmabuf_v1",
            ResourceKind::LinuxDmaBuf,
            3,
        )
    }

    pub fn add_explicit_synchronization(
        &mut self,
        display: &'display Display,
    ) -> Result<(), NativeCompositorError> {
        self.add_dynamic_global_version(
            display,
            "zwp_linux_explicit_synchronization_v1",
            ResourceKind::ExplicitSynchronization,
            2,
        )
    }

    pub fn take_acquire_fence(
        &mut self,
        surface: WaylandSurfaceId,
        revision: u64,
    ) -> Option<OwnedFd> {
        self.state
            .committed_acquire_fences
            .remove(&(surface, revision))
    }

    pub fn finish_explicit_release(
        &mut self,
        surface: WaylandSurfaceId,
        revision: u64,
        fence: Option<OwnedFd>,
    ) -> Result<bool, NativeCompositorError> {
        let identity = self
            .state
            .finish_explicit_release(surface, revision, fence)?;
        if let Some(identity) = identity
            && let Some(resource) =
                unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
        {
            unsafe { resource.destroy() };
        }
        Ok(identity.is_some())
    }

    pub fn read_dma_buf(
        &self,
        buffer: WaylandBufferId,
    ) -> Result<DmaBufImage, NativeCompositorError> {
        let BufferDescriptor::DmaBuf(descriptor) = self
            .state
            .core
            .buffer(buffer)
            .ok_or_else(|| NativeCompositorError::new("unknown Wayland buffer"))?
        else {
            return Err(NativeCompositorError::new("buffer is not a DMA-BUF"));
        };
        let planes = self
            .state
            .dmabuf_files
            .get(&buffer)
            .ok_or_else(|| NativeCompositorError::new("DMA-BUF plane storage is absent"))?
            .iter()
            .map(OwnedFd::try_clone)
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        Ok(DmaBufImage {
            descriptor: descriptor.clone(),
            planes,
        })
    }

    fn add_dynamic_global(
        &mut self,
        display: &'display Display,
        interface_name: &'static str,
        kind: ResourceKind,
    ) -> Result<(), NativeCompositorError> {
        let version = self
            .state
            .protocol
            .interface(interface_name)
            .ok_or_else(|| NativeCompositorError::new(format!("missing {interface_name}")))?
            .version as u32;
        self.add_dynamic_global_version(display, interface_name, kind, version)
    }

    fn add_dynamic_global_version(
        &mut self,
        display: &'display Display,
        interface_name: &'static str,
        kind: ResourceKind,
        maximum_version: u32,
    ) -> Result<(), NativeCompositorError> {
        let state_pointer = (&mut *self.state) as *mut NativeState;
        let interface = self
            .state
            .protocol
            .interface(interface_name)
            .ok_or_else(|| NativeCompositorError::new(format!("missing {interface_name}")))?;
        let advertised = crate::wayland_server::protocol::interface(interface_name)
            .map(|profile| profile.advertised_version)
            .unwrap_or(1)
            .min(interface.version as u32)
            .min(maximum_version);
        let mut bind = Box::new(BindContext {
            state: state_pointer,
            interface: interface_name,
            kind,
        });
        let data = (&mut *bind as *mut BindContext).cast::<c_void>();
        let global =
            unsafe { display.create_global(interface, advertised, data, Some(bind_global)) }
                .map_err(error)?;
        self.bind_contexts.push(bind);
        self.globals.push(global);
        Ok(())
    }

    pub fn duplicate_shm_fd(
        &self,
        buffer: WaylandBufferId,
    ) -> Result<OwnedFd, NativeCompositorError> {
        self.state
            .buffer_files
            .get(&buffer)
            .ok_or_else(|| NativeCompositorError::new("buffer is not backed by shared memory"))?
            .try_clone()
            .map_err(error)
    }

    /// Captures immutable SHM metadata plus a duplicated FD for copying outside protocol state.
    pub fn shm_buffer_reader(
        &self,
        buffer: WaylandBufferId,
    ) -> Result<ShmBufferReader, NativeCompositorError> {
        let BufferDescriptor::Shm(descriptor) = self
            .state
            .core
            .buffer(buffer)
            .ok_or_else(|| NativeCompositorError::new("unknown Wayland buffer"))?
        else {
            return Err(NativeCompositorError::new("buffer is not shared memory"));
        };
        Ok(ShmBufferReader {
            descriptor: *descriptor,
            file: std::fs::File::from(self.duplicate_shm_fd(buffer)?),
        })
    }

    /// Copies one committed SHM buffer into host-owned bytes suitable for a Telorgon image resource.
    pub fn read_shm_buffer(
        &self,
        buffer: WaylandBufferId,
    ) -> Result<ShmImage, NativeCompositorError> {
        self.shm_buffer_reader(buffer)?.read_full()
    }

    /// Copies one buffer-local rectangle from a committed SHM buffer into tightly packed rows.
    pub fn read_shm_buffer_region(
        &self,
        buffer: WaylandBufferId,
        rect: RectI,
    ) -> Result<ShmImageRegion, NativeCompositorError> {
        self.shm_buffer_reader(buffer)?.read_region(rect)
    }

    pub fn set_pointer_focus(
        &mut self,
        seat: u32,
        surface: Option<WaylandSurfaceId>,
        position: crate::core::PointF,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .set_pointer_focus(seat, surface, position, serial)
    }

    pub fn pointer_motion(
        &mut self,
        seat: u32,
        time_milliseconds: u32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        self.state.pointer_motion(seat, time_milliseconds, position)
    }

    pub fn relative_pointer_motion(
        &self,
        seat: u32,
        time_microseconds: u64,
        delta: crate::core::PointF,
        unaccelerated: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .relative_pointer_motion(seat, time_microseconds, delta, unaccelerated)
    }

    pub fn pointer_button(
        &mut self,
        seat: u32,
        time_milliseconds: u32,
        button: u32,
        state: crate::compositor_wayland::ButtonState,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .pointer_button(seat, time_milliseconds, button, state, serial)
    }

    pub fn pointer_axis(
        &self,
        seat: u32,
        time_milliseconds: u32,
        horizontal: f64,
        vertical: f64,
        discrete_x: i32,
        discrete_y: i32,
    ) -> Result<(), NativeCompositorError> {
        self.state.pointer_axis(
            seat,
            time_milliseconds,
            horizontal,
            vertical,
            discrete_x,
            discrete_y,
        )
    }

    pub fn drag_active(&self, seat: u32) -> bool {
        self.state
            .active_drag
            .as_ref()
            .is_some_and(|drag| drag.seat == seat)
    }

    pub fn drag_icon(&self, seat: u32) -> Option<WaylandSurfaceId> {
        self.state
            .active_drag
            .as_ref()
            .filter(|drag| drag.seat == seat)
            .and_then(|drag| drag.icon)
    }

    pub fn drag_touch_slot(&self, seat: u32) -> Option<i32> {
        self.state
            .active_drag
            .as_ref()
            .filter(|drag| drag.seat == seat)
            .and_then(|drag| match drag.grab {
                NativeDragGrab::Pointer => None,
                NativeDragGrab::Touch(slot) => Some(slot),
            })
    }

    pub fn drag_motion(
        &mut self,
        seat: u32,
        target: Option<WaylandSurfaceId>,
        time_milliseconds: u32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .drag_motion(seat, target, time_milliseconds, position)
    }

    pub fn drop_drag(&mut self, seat: u32) -> Result<(), NativeCompositorError> {
        self.state.drop_drag(seat)
    }

    pub fn cancel_drag(&mut self, seat: u32) -> Result<(), NativeCompositorError> {
        self.state.cancel_drag(seat)
    }

    pub fn set_keyboard_focus(
        &mut self,
        seat: u32,
        surface: Option<WaylandSurfaceId>,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state.set_keyboard_focus(seat, surface, serial)
    }

    pub fn configure_toplevel(
        &mut self,
        surface: WaylandSurfaceId,
        size: Option<crate::core::SizeI>,
        states: crate::compositor_wayland::ToplevelState,
    ) -> Result<u32, NativeCompositorError> {
        self.state.send_toplevel_configure(surface, size, states)
    }

    pub fn close_toplevel(&self, surface: WaylandSurfaceId) -> Result<(), NativeCompositorError> {
        let resource = self
            .state
            .resource_for_kind(
                |kind| matches!(kind, ResourceKind::XdgToplevel(candidate) if candidate == surface),
            )?
            .ok_or_else(|| NativeCompositorError::new("xdg_toplevel resource is absent"))?;
        self.state
            .post_event(resource, "xdg_toplevel", "close", &mut [])
    }

    pub fn keyboard_keymap(
        &mut self,
        seat: u32,
        keymap: &OwnedFd,
        size: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state.keyboard_keymap(seat, keymap, size)
    }

    pub fn keyboard_key(
        &mut self,
        seat: u32,
        time_milliseconds: u32,
        key: u32,
        state: crate::compositor_wayland::ButtonState,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .keyboard_key(seat, time_milliseconds, key, state, serial)
    }

    pub fn keyboard_modifiers(
        &mut self,
        seat: u32,
        serial: u32,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .keyboard_modifiers(seat, serial, depressed, latched, locked, group)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn touch_down(
        &mut self,
        seat: u32,
        surface: WaylandSurfaceId,
        time_milliseconds: u32,
        touch_id: i32,
        position: crate::core::PointF,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .touch_down(seat, surface, time_milliseconds, touch_id, position, serial)
    }

    pub fn touch_motion(
        &self,
        seat: u32,
        time_milliseconds: u32,
        touch_id: i32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .touch_motion(seat, time_milliseconds, touch_id, position)
    }

    pub fn touch_up(
        &mut self,
        seat: u32,
        time_milliseconds: u32,
        touch_id: i32,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        self.state
            .touch_up(seat, time_milliseconds, touch_id, serial)
    }

    pub fn touch_cancel(&mut self, seat: u32) -> Result<(), NativeCompositorError> {
        self.state.touch_cancel(seat)
    }

    pub fn surface_presented(
        &mut self,
        surface: WaylandSurfaceId,
        through_revision: u64,
        time_milliseconds: u32,
    ) -> Result<(), NativeCompositorError> {
        let identities = self.state.surface_frame_completed(
            surface,
            through_revision,
            time_milliseconds,
            true,
        )?;
        for identity in identities {
            if let Some(resource) =
                unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
            {
                unsafe { resource.destroy() };
            }
        }
        Ok(())
    }

    /// Lets an occluded client draw again without claiming its hidden content was presented.
    /// Frame callbacks are pacing hints; presentation feedback stays pending until a displayed
    /// frame either consumes or supersedes it, so older in-flight frames can still report truthfully.
    pub(crate) fn surface_occluded_frame_ready(
        &mut self,
        surface: WaylandSurfaceId,
        through_revision: u64,
        time_milliseconds: u32,
    ) -> Result<(), NativeCompositorError> {
        let identities = self.state.surface_frame_completed(
            surface,
            through_revision,
            time_milliseconds,
            false,
        )?;
        for identity in identities {
            if let Some(resource) =
                unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
            {
                unsafe { resource.destroy() };
            }
        }
        Ok(())
    }

    pub fn release_buffer(&self, buffer: WaylandBufferId) -> Result<(), NativeCompositorError> {
        let Some(resource) = self.state.resource_for_kind(
            |kind| matches!(kind, ResourceKind::Buffer(candidate) if candidate == buffer),
        )?
        else {
            // A client may destroy wl_buffer after attach. The duplicated storage FD remains valid,
            // but there is no live protocol object to receive release once the copy completes.
            return Ok(());
        };
        self.state
            .post_event(resource, "wl_buffer", "release", &mut [])
    }

    pub fn popup_placement(
        &self,
        surface: WaylandSurfaceId,
    ) -> Option<(Option<WaylandSurfaceId>, RectI)> {
        self.state
            .popups
            .get(&surface)
            .map(|popup| (popup.parent, popup_geometry(popup.positioner)))
    }

    pub fn decoration_mode(
        &self,
        surface: WaylandSurfaceId,
    ) -> Option<crate::compositor_wayland::DecorationMode> {
        self.state
            .toplevels
            .get(&surface)
            .map(|toplevel| toplevel.decoration)
    }

    /// Returns client-authored metadata used to compose server-side window chrome.
    pub fn toplevel_metadata(
        &self,
        surface: WaylandSurfaceId,
    ) -> Option<&crate::compositor_wayland::XdgToplevelState> {
        self.state.toplevels.get(&surface)
    }

    /// Returns the icon snapshot applied by the latest `wl_surface.commit` for a toplevel.
    pub fn toplevel_icon(&self, surface: WaylandSurfaceId) -> Option<&ToplevelIconSnapshot> {
        self.state.committed_toplevel_icons.get(&surface)
    }

    pub fn viewport(&self, surface: WaylandSurfaceId) -> Option<ViewportState> {
        self.state
            .viewports
            .get(&surface)
            .map(|viewport| viewport.current)
    }

    pub fn idle_inhibited(&self) -> bool {
        !self.state.idle_inhibitors.is_empty()
    }

    pub fn pointer_constraint(&self, seat: u32) -> Option<PointerConstraintState> {
        self.state
            .pointer_constraints
            .values()
            .find(|constraint| constraint.seat == seat && constraint.active)
            .map(|constraint| PointerConstraintState {
                kind: constraint.kind,
                surface: constraint.surface,
                region: constraint.region.clone(),
            })
    }

    /// Completes a pending secure-lock transition after a blank/lock-only frame has reached every
    /// active KMS output.
    pub fn session_lock_frame_presented(
        &mut self,
        lock: ProtocolObjectId,
    ) -> Result<(), NativeCompositorError> {
        self.state.session_lock_frame_presented(lock)
    }

    pub fn session_locked(&self) -> bool {
        self.state.secure_session_locked
    }
}

#[derive(Debug)]
pub struct ShmBufferReader {
    descriptor: ShmBuffer,
    file: std::fs::File,
}

impl ShmBufferReader {
    pub fn read_full(self) -> Result<ShmImage, NativeCompositorError> {
        let height = usize::try_from(self.descriptor.size.height)
            .map_err(|_| NativeCompositorError::new("invalid SHM image height"))?;
        let length = (self.descriptor.stride as usize)
            .checked_mul(height)
            .ok_or_else(|| NativeCompositorError::new("SHM image length overflow"))?;
        let mut pixels = vec![0_u8; length];
        read_shm_exact(
            &self.file,
            self.descriptor.offset as u64,
            &mut pixels,
            "shared-memory buffer ended before its declared extent",
        )?;
        Ok(ShmImage {
            descriptor: self.descriptor,
            pixels,
        })
    }

    pub fn read_region(self, rect: RectI) -> Result<ShmImageRegion, NativeCompositorError> {
        let right = i64::from(rect.x) + i64::from(rect.width);
        let bottom = i64::from(rect.y) + i64::from(rect.height);
        if rect.x < 0
            || rect.y < 0
            || rect.width <= 0
            || rect.height <= 0
            || right > i64::from(self.descriptor.size.width)
            || bottom > i64::from(self.descriptor.size.height)
        {
            return Err(NativeCompositorError::new(
                "SHM read rectangle lies outside the buffer",
            ));
        }
        let bytes_per_pixel = self
            .descriptor
            .format
            .bytes_per_pixel()
            .ok_or_else(|| NativeCompositorError::new("unsupported SHM pixel format"))?
            as usize;
        let row_bytes = (rect.width as usize)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| NativeCompositorError::new("SHM region row size overflow"))?;
        let length = row_bytes
            .checked_mul(rect.height as usize)
            .ok_or_else(|| NativeCompositorError::new("SHM region size overflow"))?;
        let x_bytes = (rect.x as usize)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| NativeCompositorError::new("SHM x offset overflow"))?;
        let mut pixels = vec![0_u8; length];
        for row in 0..rect.height as usize {
            let file_offset = self
                .descriptor
                .offset
                .checked_add(
                    (rect.y as usize + row)
                        .checked_mul(self.descriptor.stride as usize)
                        .ok_or_else(|| NativeCompositorError::new("SHM row offset overflow"))?,
                )
                .and_then(|offset| offset.checked_add(x_bytes))
                .ok_or_else(|| NativeCompositorError::new("SHM region offset overflow"))?;
            read_shm_exact(
                &self.file,
                file_offset as u64,
                &mut pixels[row * row_bytes..(row + 1) * row_bytes],
                "shared-memory buffer ended before its damaged region",
            )?;
        }
        Ok(ShmImageRegion {
            descriptor: self.descriptor,
            rect,
            row_bytes,
            pixels,
        })
    }
}

fn read_shm_exact(
    file: &std::fs::File,
    offset: u64,
    target: &mut [u8],
    eof_context: &'static str,
) -> Result<(), NativeCompositorError> {
    let mut read = 0;
    while read < target.len() {
        let read_offset = offset
            .checked_add(read as u64)
            .ok_or_else(|| NativeCompositorError::new("SHM read offset overflow"))?;
        let count = file
            .read_at(&mut target[read..], read_offset)
            .map_err(error)?;
        if count == 0 {
            return Err(NativeCompositorError::new(eof_context));
        }
        read += count;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShmImage {
    pub descriptor: ShmBuffer,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShmImageRegion {
    pub descriptor: ShmBuffer,
    pub rect: RectI,
    pub row_bytes: usize,
    pub pixels: Vec<u8>,
}

unsafe extern "C" fn bind_global(
    client: *mut ffi::wl_client,
    data: *mut c_void,
    version: u32,
    id: u32,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let bind = unsafe { &mut *data.cast::<BindContext>() };
        let Some(client) = (unsafe { ClientRef::from_raw(client) }) else {
            return;
        };
        let state = unsafe { &mut *bind.state };
        if state
            .bind(client, bind.interface, bind.kind, version, id)
            .is_err()
        {
            client.post_no_memory();
        }
    }));
    if result.is_err() {
        // An unwind may never cross the C ABI. The client will be disconnected by libwayland when
        // its bind did not produce the requested object.
    }
}

unsafe extern "C" fn dispatch_resource(
    _implementation: *const c_void,
    target: *mut c_void,
    opcode: u32,
    _message: *const ffi::wl_message,
    arguments: *mut ffi::wl_argument,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(resource) = (unsafe { ResourceRef::from_raw(target.cast::<ffi::wl_resource>()) })
        else {
            return -1;
        };
        let context_pointer = resource.user_data().cast::<ResourceContext>();
        if context_pointer.is_null() {
            return -1;
        }
        let (state_pointer, kind, interface) = {
            let context = unsafe { &*context_pointer };
            (context.state, context.kind, context.interface.clone())
        };
        let outcome = {
            let state = unsafe { &mut *state_pointer };
            let Some(message) = state
                .protocol
                .interface_schema(&interface)
                .and_then(|schema| schema.request(opcode))
                .cloned()
            else {
                resource.post_error(0, "unknown request opcode");
                return -1;
            };
            if message.since > resource.version() {
                resource.post_error(0, "request is newer than the bound interface version");
                return -1;
            }
            let mut request = match unsafe { IncomingRequest::from_raw(&message, arguments) } {
                Ok(request) => request,
                Err(error) => {
                    resource.post_error(0, &error.to_string());
                    return -1;
                }
            };
            match state.dispatch(resource, context_pointer, kind, &mut request) {
                Ok(outcome) => outcome,
                Err(error) => {
                    resource.post_error(0, &error.to_string());
                    return -1;
                }
            }
        };
        for identity in outcome.destroy_others {
            if let Some(resource) =
                unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
            {
                unsafe { resource.destroy() };
            }
        }
        if outcome.destroy_self {
            unsafe { resource.destroy() };
        }
        0
    }));
    result.unwrap_or(-1)
}

unsafe extern "C" fn destroy_resource(resource: *mut ffi::wl_resource) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(resource) = (unsafe { ResourceRef::from_raw(resource) }) else {
            return;
        };
        let pointer = resource.user_data().cast::<ResourceContext>();
        if pointer.is_null() {
            return;
        }
        unsafe { resource.set_user_data(std::ptr::null_mut()) };
        let context = unsafe { Box::from_raw(pointer) };
        let state = unsafe { &mut *context.state };
        state.destroy_context(&context);
    }));
}

#[derive(Default)]
struct DispatchOutcome {
    destroy_self: bool,
    destroy_others: Vec<usize>,
}

impl NativeState {
    fn finish_explicit_release(
        &mut self,
        surface: WaylandSurfaceId,
        revision: u64,
        fence: Option<OwnedFd>,
    ) -> Result<Option<usize>, NativeCompositorError> {
        let Some(object) = self.committed_releases.remove(&(surface, revision)) else {
            return Ok(None);
        };
        let identity = self
            .resources
            .get(&object)
            .copied()
            .ok_or_else(|| NativeCompositorError::new("explicit release resource is absent"))?;
        let resource = unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
            .ok_or_else(|| NativeCompositorError::new("explicit release resource is stale"))?;
        if let Some(fence) = fence.as_ref() {
            self.post_event(
                resource,
                "zwp_linux_buffer_release_v1",
                "fenced_release",
                &mut [ffi::wl_argument {
                    h: fence.as_raw_fd(),
                }],
            )?;
        } else {
            self.post_event(
                resource,
                "zwp_linux_buffer_release_v1",
                "immediate_release",
                &mut [],
            )?;
        }
        Ok(Some(identity))
    }

    fn surface_frame_completed(
        &mut self,
        surface: WaylandSurfaceId,
        through_revision: u64,
        time_milliseconds: u32,
        presented: bool,
    ) -> Result<Vec<usize>, NativeCompositorError> {
        let current_revision = self
            .core
            .world
            .surface(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown wl_surface"))?
            .snapshot()
            .revision;
        if through_revision > current_revision {
            return Err(NativeCompositorError::new(
                "presented surface revision is newer than committed state",
            ));
        }
        let callback_commits =
            take_surface_commits_through(&mut self.committed_callbacks, surface, through_revision);
        let (presented_feedbacks, discarded_feedbacks) = take_surface_feedbacks_through(
            &mut self.committed_presentation_feedbacks,
            surface,
            through_revision,
            presented,
        );
        let callback_count = callback_commits
            .iter()
            .map(|(_, callbacks)| callbacks.len())
            .sum::<usize>();
        let feedback_count = presented_feedbacks.len() + discarded_feedbacks.len();
        let mut identities = Vec::with_capacity(callback_count + feedback_count);
        for object in callback_commits
            .into_iter()
            .flat_map(|(_, callbacks)| callbacks)
        {
            let Some(identity) = self.resources.get(&object).copied() else {
                continue;
            };
            let Some(resource) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            self.post_event(
                resource,
                "wl_callback",
                "done",
                &mut [ffi::wl_argument {
                    u: time_milliseconds,
                }],
            )?;
            identities.push(identity);
        }
        for object in discarded_feedbacks {
            let Some(identity) = self.resources.get(&object).copied() else {
                continue;
            };
            let Some(resource) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            self.post_event(resource, "wp_presentation_feedback", "discarded", &mut [])?;
            identities.push(identity);
        }
        if !presented_feedbacks.is_empty() {
            let timestamp = monotonic_timestamp()?;
            self.presentation_sequence = self.presentation_sequence.wrapping_add(1).max(1);
            let sequence = self.presentation_sequence;
            let refresh = self
                .core
                .outputs
                .values()
                .find(|output| output.enabled)
                .map(|output| output.current_mode().refresh_millihertz)
                .map(|millihertz| {
                    u32::try_from(1_000_000_000_000_u64 / u64::from(millihertz)).unwrap_or(u32::MAX)
                })
                .unwrap_or(0);
            for object in presented_feedbacks {
                let Some(identity) = self.resources.get(&object).copied() else {
                    continue;
                };
                let Some(resource) =
                    (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
                else {
                    continue;
                };
                self.post_event(
                    resource,
                    "wp_presentation_feedback",
                    "presented",
                    &mut [
                        ffi::wl_argument {
                            u: (timestamp.seconds >> 32) as u32,
                        },
                        ffi::wl_argument {
                            u: timestamp.seconds as u32,
                        },
                        ffi::wl_argument {
                            u: timestamp.nanoseconds,
                        },
                        ffi::wl_argument { u: refresh },
                        ffi::wl_argument {
                            u: (sequence >> 32) as u32,
                        },
                        ffi::wl_argument { u: sequence as u32 },
                        // The blocking KMS path does not yet expose hardware timestamp proof.
                        ffi::wl_argument { u: 0 },
                    ],
                )?;
                identities.push(identity);
            }
        }
        Ok(identities)
    }
    fn set_pointer_focus(
        &mut self,
        seat_id: u32,
        surface: Option<WaylandSurfaceId>,
        position: crate::core::PointF,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        let previous = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .pointer_focus;
        if let Some(previous) = previous {
            if let Some(surface_resource) = self
                .resource_for_kind(
                    |kind| matches!(kind, ResourceKind::Surface(candidate) if candidate == previous.surface),
                )?
                .map(|resource| resource.identity() as *mut ffi::wl_resource)
            {
                for resource in self.resources_for_client(
                    previous.client,
                    |kind| matches!(kind, ResourceKind::Pointer(candidate) if candidate == seat_id),
                )? {
                    self.post_event(
                        resource,
                        "wl_pointer",
                        "leave",
                        &mut [
                            ffi::wl_argument { u: serial },
                            ffi::wl_argument {
                                o: surface_resource,
                            },
                        ],
                    )?;
                }
            }
            self.clear_selection_for_client(seat_id, previous.client)?;
        }
        let focus = if let Some(surface) = surface {
            let client = self
                .core
                .world
                .surface_owner(surface)
                .ok_or_else(|| NativeCompositorError::new("unknown focus surface"))?;
            self.core
                .serials
                .issue(
                    serial,
                    client,
                    crate::compositor_wayland::SerialKind::PointerEnter,
                    Some(surface),
                )
                .map_err(error)?;
            let surface_resource = self.surface_resource(surface)?;
            for resource in self.resources_for_client(
                client,
                |kind| matches!(kind, ResourceKind::Pointer(candidate) if candidate == seat_id),
            )? {
                self.post_event(
                    resource,
                    "wl_pointer",
                    "enter",
                    &mut [
                        ffi::wl_argument { u: serial },
                        ffi::wl_argument {
                            o: surface_resource,
                        },
                        ffi::wl_argument {
                            f: fixed(position.x),
                        },
                        ffi::wl_argument {
                            f: fixed(position.y),
                        },
                    ],
                )?;
                self.pointer_frame(resource)?;
            }
            Some(crate::compositor_wayland::PointerFocus {
                client,
                surface,
                position,
                enter_serial: serial,
            })
        } else {
            None
        };
        let seat = self.core.seats.get_mut(&seat_id).expect("seat checked");
        seat.pointer_focus = focus;
        // A client cursor is scoped to the focus that authorized it. Do not retain it while the
        // new focus decides which cursor to install, or after the old surface has been destroyed.
        seat.cursor = crate::compositor_wayland::CursorImage::TelorgonDefault;
        self.update_pointer_constraints(seat_id, focus.map(|focus| focus.surface))?;
        Ok(())
    }

    fn pointer_motion(
        &mut self,
        seat_id: u32,
        time: u32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        let focus = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .pointer_focus
            .ok_or_else(|| NativeCompositorError::new("pointer has no focused surface"))?;
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::Pointer(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_pointer",
                "motion",
                &mut [
                    ffi::wl_argument { u: time },
                    ffi::wl_argument {
                        f: fixed(position.x),
                    },
                    ffi::wl_argument {
                        f: fixed(position.y),
                    },
                ],
            )?;
            self.pointer_frame(resource)?;
        }
        self.core
            .seats
            .get_mut(&seat_id)
            .expect("seat checked")
            .pointer_focus
            .as_mut()
            .expect("focus checked")
            .position = position;
        Ok(())
    }

    fn relative_pointer_motion(
        &self,
        seat_id: u32,
        time_microseconds: u64,
        delta: crate::core::PointF,
        unaccelerated: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        let focus = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .pointer_focus
            .ok_or_else(|| NativeCompositorError::new("pointer has no focused surface"))?;
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::RelativePointer(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "zwp_relative_pointer_v1",
                "relative_motion",
                &mut [
                    ffi::wl_argument {
                        u: (time_microseconds >> 32) as u32,
                    },
                    ffi::wl_argument {
                        u: time_microseconds as u32,
                    },
                    ffi::wl_argument { f: fixed(delta.x) },
                    ffi::wl_argument { f: fixed(delta.y) },
                    ffi::wl_argument {
                        f: fixed(unaccelerated.x),
                    },
                    ffi::wl_argument {
                        f: fixed(unaccelerated.y),
                    },
                ],
            )?;
        }
        Ok(())
    }

    fn pointer_button(
        &mut self,
        seat_id: u32,
        time: u32,
        button: u32,
        state: crate::compositor_wayland::ButtonState,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        let focus = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .pointer_focus
            .ok_or_else(|| NativeCompositorError::new("pointer has no focused surface"))?;
        self.core
            .serials
            .issue(
                serial,
                focus.client,
                crate::compositor_wayland::SerialKind::PointerButton,
                Some(focus.surface),
            )
            .map_err(error)?;
        self.core
            .seats
            .get_mut(&seat_id)
            .expect("seat checked")
            .set_button(button, state);
        let wire_state = u32::from(matches!(
            state,
            crate::compositor_wayland::ButtonState::Pressed
        ));
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::Pointer(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_pointer",
                "button",
                &mut [
                    ffi::wl_argument { u: serial },
                    ffi::wl_argument { u: time },
                    ffi::wl_argument { u: button },
                    ffi::wl_argument { u: wire_state },
                ],
            )?;
            self.pointer_frame(resource)?;
        }
        Ok(())
    }

    fn pointer_axis(
        &self,
        seat_id: u32,
        time: u32,
        horizontal: f64,
        vertical: f64,
        discrete_x: i32,
        discrete_y: i32,
    ) -> Result<(), NativeCompositorError> {
        let focus = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .pointer_focus
            .ok_or_else(|| NativeCompositorError::new("pointer has no focused surface"))?;
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::Pointer(candidate) if candidate == seat_id),
        )? {
            if resource.version() >= 5 {
                self.post_event(
                    resource,
                    "wl_pointer",
                    "axis_source",
                    &mut [ffi::wl_argument { u: 0 }],
                )?;
            }
            for (axis, value, discrete) in [
                (0_u32, vertical, discrete_y),
                (1_u32, horizontal, discrete_x),
            ] {
                if value == 0.0 && discrete == 0 {
                    continue;
                }
                self.post_event(
                    resource,
                    "wl_pointer",
                    "axis",
                    &mut [
                        ffi::wl_argument { u: time },
                        ffi::wl_argument { u: axis },
                        ffi::wl_argument {
                            f: fixed_f64(value),
                        },
                    ],
                )?;
                if resource.version() >= 8 {
                    self.post_event(
                        resource,
                        "wl_pointer",
                        "axis_value120",
                        &mut [
                            ffi::wl_argument { u: axis },
                            ffi::wl_argument {
                                i: discrete.saturating_mul(120),
                            },
                        ],
                    )?;
                } else if resource.version() >= 5 && discrete != 0 {
                    self.post_event(
                        resource,
                        "wl_pointer",
                        "axis_discrete",
                        &mut [
                            ffi::wl_argument { u: axis },
                            ffi::wl_argument { i: discrete },
                        ],
                    )?;
                }
            }
            self.pointer_frame(resource)?;
        }
        Ok(())
    }

    fn drag_motion(
        &mut self,
        seat_id: u32,
        target: Option<WaylandSurfaceId>,
        time: u32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        let (drag_seat, source, current) = self
            .active_drag
            .as_ref()
            .map(|drag| {
                (
                    drag.seat,
                    drag.source,
                    drag.target.as_ref().map(|target| target.surface),
                )
            })
            .ok_or_else(|| NativeCompositorError::new("no drag is active"))?;
        if drag_seat != seat_id {
            return Err(NativeCompositorError::new("drag belongs to another seat"));
        }
        if let Some(surface) = target
            && self.core.world.surface(surface).is_none()
        {
            return Err(NativeCompositorError::new("unknown drag target surface"));
        }

        if current != target {
            if let Some(previous) = self
                .active_drag
                .as_mut()
                .and_then(|drag| drag.target.take())
            {
                self.send_drag_leave(&previous, source)?;
            }
            let Some(surface) = target else {
                return Ok(());
            };
            self.enter_drag_target(seat_id, surface, position)?;
            return Ok(());
        }

        let device_objects = self
            .active_drag
            .as_ref()
            .and_then(|drag| drag.target.as_ref())
            .map(|target| target.devices.clone())
            .unwrap_or_default();
        for object in device_objects {
            let Some(device) = self.resource_for_object(object)? else {
                continue;
            };
            self.post_event(
                device,
                "wl_data_device",
                "motion",
                &mut [
                    ffi::wl_argument { u: time },
                    ffi::wl_argument {
                        f: fixed(position.x),
                    },
                    ffi::wl_argument {
                        f: fixed(position.y),
                    },
                ],
            )?;
        }
        Ok(())
    }

    fn enter_drag_target(
        &mut self,
        seat_id: u32,
        surface: WaylandSurfaceId,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        let client = self
            .core
            .world
            .surface_owner(surface)
            .ok_or_else(|| NativeCompositorError::new("drag target has no owner"))?;
        let source = self.active_drag.as_ref().and_then(|drag| drag.source);
        let devices = self
            .resources_for_client(
                client,
                |kind| matches!(kind, ResourceKind::DataDevice(candidate) if candidate == seat_id),
            )?
            .into_iter()
            .map(|resource| self.protocol_object_for_resource(resource))
            .collect::<Result<Vec<_>, _>>()?;
        if devices.is_empty() {
            return Ok(());
        }

        let serial = unsafe { ffi::wl_display_next_serial(self.display.as_ptr()) };
        self.core
            .serials
            .issue(
                serial,
                client,
                crate::compositor_wayland::SerialKind::DataDevice,
                Some(surface),
            )
            .map_err(error)?;
        let surface_resource = self.surface_resource(surface)?;
        let mut offers = Vec::new();
        for device_object in &devices {
            let Some(device_identity) = self.resources.get(device_object).copied() else {
                continue;
            };
            let offer = source
                .map(|source| self.create_drag_offer(device_identity, client, source))
                .transpose()?;
            let Some(device) =
                (unsafe { ResourceRef::from_raw(device_identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            let offer_resource = offer
                .map(|(object, identity)| {
                    offers.push(object);
                    identity as *mut ffi::wl_resource
                })
                .unwrap_or(std::ptr::null_mut());
            self.post_event(
                device,
                "wl_data_device",
                "enter",
                &mut [
                    ffi::wl_argument { u: serial },
                    ffi::wl_argument {
                        o: surface_resource,
                    },
                    ffi::wl_argument {
                        f: fixed(position.x),
                    },
                    ffi::wl_argument {
                        f: fixed(position.y),
                    },
                    ffi::wl_argument { o: offer_resource },
                ],
            )?;
        }
        if let Some(drag) = self.active_drag.as_mut() {
            drag.target = Some(NativeDragTarget {
                surface,
                devices,
                offers,
            });
        }
        Ok(())
    }

    fn send_drag_leave(
        &self,
        target: &NativeDragTarget,
        source: Option<ProtocolObjectId>,
    ) -> Result<(), NativeCompositorError> {
        for object in &target.devices {
            let Some(device) = self.resource_for_object(*object)? else {
                continue;
            };
            self.post_event(device, "wl_data_device", "leave", &mut [])?;
        }
        if let Some(source) = source
            && let Ok(source_resource) = self.data_source_resource(source)
        {
            self.post_event(
                source_resource,
                "wl_data_source",
                "target",
                &mut [ffi::wl_argument {
                    s: std::ptr::null(),
                }],
            )?;
        }
        Ok(())
    }

    fn drop_drag(&mut self, seat_id: u32) -> Result<(), NativeCompositorError> {
        let Some(drag) = self.active_drag.take() else {
            return Ok(());
        };
        if drag.seat != seat_id {
            self.active_drag = Some(drag);
            return Err(NativeCompositorError::new("drag belongs to another seat"));
        }

        let accepted = drag.source.is_none()
            || drag.target.as_ref().is_some_and(|target| {
                target.offers.iter().any(|offer| {
                    self.core.data_devices.offer(*offer).is_some_and(|offer| {
                        offer.accepted_mime_type.is_some()
                            && offer.selected_action != crate::compositor_wayland::DataAction::NONE
                    })
                })
            });
        if accepted {
            let mut has_version_3_offer = false;
            if let Some(target) = &drag.target {
                for object in &target.devices {
                    let Some(device) = self.resource_for_object(*object)? else {
                        continue;
                    };
                    self.post_event(device, "wl_data_device", "drop", &mut [])?;
                }
                for object in &target.offers {
                    if let Some(offer) = self.core.data_devices.offer_mut(*object)
                        && offer.accepted_mime_type.is_some()
                        && offer.selected_action != crate::compositor_wayland::DataAction::NONE
                    {
                        offer.dropped = true;
                        has_version_3_offer |= self
                            .resources
                            .get(object)
                            .copied()
                            .and_then(|identity| unsafe {
                                ResourceRef::from_raw(identity as *mut ffi::wl_resource)
                            })
                            .is_some_and(|resource| resource.version() >= 3);
                    }
                }
            }
            if let Some(source) = drag.source {
                let finish_legacy =
                    !has_version_3_offer && self.finished_drag_sources.insert(source);
                if let Ok(source_resource) = self.data_source_resource(source)
                    && source_resource.version() >= 3
                {
                    self.post_event(
                        source_resource,
                        "wl_data_source",
                        "dnd_drop_performed",
                        &mut [],
                    )?;
                    if finish_legacy {
                        self.post_event(
                            source_resource,
                            "wl_data_source",
                            "dnd_finished",
                            &mut [],
                        )?;
                    }
                }
            }
        } else {
            if let Some(target) = &drag.target {
                self.send_drag_leave(target, drag.source)?;
            }
            if let Some(source) = drag.source {
                self.cancel_data_source(source)?;
            }
        }
        self.finish_drag(drag.icon);
        Ok(())
    }

    fn cancel_drag(&mut self, seat_id: u32) -> Result<(), NativeCompositorError> {
        let Some(drag) = self.active_drag.take() else {
            return Ok(());
        };
        if drag.seat != seat_id {
            self.active_drag = Some(drag);
            return Err(NativeCompositorError::new("drag belongs to another seat"));
        }
        if let Some(target) = &drag.target {
            self.send_drag_leave(target, drag.source)?;
        }
        if let Some(source) = drag.source {
            self.cancel_data_source(source)?;
        }
        self.finish_drag(drag.icon);
        Ok(())
    }

    fn finish_drag(&mut self, icon: Option<WaylandSurfaceId>) {
        self.core.data_devices.finish_drag();
        self.core
            .queue_action(CompositorAction::FinishDrag { icon });
    }

    fn pointer_frame(&self, resource: ResourceRef<'_>) -> Result<(), NativeCompositorError> {
        if resource.version() >= 5 {
            self.post_event(resource, "wl_pointer", "frame", &mut [])?;
        }
        Ok(())
    }

    fn set_keyboard_focus(
        &mut self,
        seat_id: u32,
        surface: Option<WaylandSurfaceId>,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        let previous = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .keyboard_focus;
        if previous.map(|focus| focus.surface) == surface {
            return Ok(());
        }
        if let Some(previous) = previous {
            let surface_resource = self.surface_resource(previous.surface)?;
            for resource in self.resources_for_client(
                previous.client,
                |kind| matches!(kind, ResourceKind::Keyboard(candidate) if candidate == seat_id),
            )? {
                self.post_event(
                    resource,
                    "wl_keyboard",
                    "leave",
                    &mut [
                        ffi::wl_argument { u: serial },
                        ffi::wl_argument {
                            o: surface_resource,
                        },
                    ],
                )?;
            }
        }
        let focus = if let Some(surface) = surface {
            let client = self
                .core
                .world
                .surface_owner(surface)
                .ok_or_else(|| NativeCompositorError::new("unknown focus surface"))?;
            self.core
                .serials
                .issue(
                    serial,
                    client,
                    crate::compositor_wayland::SerialKind::KeyboardEnter,
                    Some(surface),
                )
                .map_err(error)?;
            let surface_resource = self.surface_resource(surface)?;
            let keys = self
                .core
                .seats
                .get(&seat_id)
                .expect("seat checked")
                .pressed_keys()
                .to_vec();
            let modifiers = self
                .core
                .seats
                .get(&seat_id)
                .expect("seat checked")
                .keyboard_modifiers();
            let mut keys = ffi::wl_array {
                size: std::mem::size_of_val(keys.as_slice()),
                alloc: std::mem::size_of_val(keys.as_slice()),
                data: keys.as_ptr().cast_mut().cast::<c_void>(),
            };
            for resource in self.resources_for_client(
                client,
                |kind| matches!(kind, ResourceKind::Keyboard(candidate) if candidate == seat_id),
            )? {
                self.post_event(
                    resource,
                    "wl_keyboard",
                    "enter",
                    &mut [
                        ffi::wl_argument { u: serial },
                        ffi::wl_argument {
                            o: surface_resource,
                        },
                        ffi::wl_argument { a: &mut keys },
                    ],
                )?;
                self.post_keyboard_modifiers(resource, serial, modifiers)?;
            }
            Some(crate::compositor_wayland::KeyboardFocus {
                client,
                surface,
                enter_serial: serial,
            })
        } else {
            None
        };
        self.core
            .seats
            .get_mut(&seat_id)
            .expect("seat checked")
            .keyboard_focus = focus;
        if let Some(focus) = focus {
            self.send_selection_to_client(seat_id, focus.client)?;
        }
        Ok(())
    }

    fn keyboard_keymap(
        &mut self,
        seat_id: u32,
        keymap: &OwnedFd,
        size: u32,
    ) -> Result<(), NativeCompositorError> {
        if size == 0 {
            return Err(NativeCompositorError::new("keyboard keymap is empty"));
        }
        self.keyboard_keymaps
            .insert(seat_id, (keymap.try_clone().map_err(error)?, size));
        for resource in self.resources_for_kind(
            |kind| matches!(kind, ResourceKind::Keyboard(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_keyboard",
                "keymap",
                &mut [
                    ffi::wl_argument { u: 1 },
                    ffi::wl_argument {
                        h: keymap.as_raw_fd(),
                    },
                    ffi::wl_argument { u: size },
                ],
            )?;
        }
        Ok(())
    }

    fn send_keyboard_initial(
        &self,
        resource: ResourceRef<'_>,
        seat_id: u32,
        client: ClientId,
    ) -> Result<(), NativeCompositorError> {
        if let Some((keymap, size)) = self.keyboard_keymaps.get(&seat_id) {
            self.post_event(
                resource,
                "wl_keyboard",
                "keymap",
                &mut [
                    ffi::wl_argument { u: 1 },
                    ffi::wl_argument {
                        h: keymap.as_raw_fd(),
                    },
                    ffi::wl_argument { u: *size },
                ],
            )?;
        }
        if resource.version() >= 4 {
            self.post_event(
                resource,
                "wl_keyboard",
                "repeat_info",
                &mut [ffi::wl_argument { i: 25 }, ffi::wl_argument { i: 600 }],
            )?;
        }
        if let Some(focus) = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .keyboard_focus
            .filter(|focus| focus.client == client)
        {
            let surface_resource = self.surface_resource(focus.surface)?;
            let seat = self.core.seats.get(&seat_id).expect("seat checked");
            let keys = seat.pressed_keys().to_vec();
            let modifiers = seat.keyboard_modifiers();
            let mut keys = ffi::wl_array {
                size: std::mem::size_of_val(keys.as_slice()),
                alloc: std::mem::size_of_val(keys.as_slice()),
                data: keys.as_ptr().cast_mut().cast::<c_void>(),
            };
            self.post_event(
                resource,
                "wl_keyboard",
                "enter",
                &mut [
                    ffi::wl_argument {
                        u: focus.enter_serial,
                    },
                    ffi::wl_argument {
                        o: surface_resource,
                    },
                    ffi::wl_argument { a: &mut keys },
                ],
            )?;
            self.post_keyboard_modifiers(resource, focus.enter_serial, modifiers)?;
        }
        Ok(())
    }

    fn keyboard_key(
        &mut self,
        seat_id: u32,
        time: u32,
        key: u32,
        state: crate::compositor_wayland::ButtonState,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        let focus = {
            let seat = self
                .core
                .seats
                .get_mut(&seat_id)
                .ok_or_else(|| NativeCompositorError::new("unknown seat"))?;
            seat.set_key(key, state);
            seat.keyboard_focus
        };
        let Some(focus) = focus else {
            return Ok(());
        };
        self.core
            .serials
            .issue(
                serial,
                focus.client,
                crate::compositor_wayland::SerialKind::KeyboardKey,
                Some(focus.surface),
            )
            .map_err(error)?;
        let wire_state = u32::from(matches!(
            state,
            crate::compositor_wayland::ButtonState::Pressed
        ));
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::Keyboard(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_keyboard",
                "key",
                &mut [
                    ffi::wl_argument { u: serial },
                    ffi::wl_argument { u: time },
                    ffi::wl_argument { u: key },
                    ffi::wl_argument { u: wire_state },
                ],
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn keyboard_modifiers(
        &mut self,
        seat_id: u32,
        serial: u32,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> Result<(), NativeCompositorError> {
        let focus = {
            let seat = self
                .core
                .seats
                .get_mut(&seat_id)
                .ok_or_else(|| NativeCompositorError::new("unknown seat"))?;
            seat.set_keyboard_modifiers(depressed, latched, locked, group);
            seat.keyboard_focus
        };
        let Some(focus) = focus else {
            return Ok(());
        };
        for resource in self.resources_for_client(
            focus.client,
            |kind| matches!(kind, ResourceKind::Keyboard(candidate) if candidate == seat_id),
        )? {
            self.post_keyboard_modifiers(resource, serial, (depressed, latched, locked, group))?;
        }
        Ok(())
    }

    fn post_keyboard_modifiers(
        &self,
        resource: ResourceRef<'_>,
        serial: u32,
        modifiers: (u32, u32, u32, u32),
    ) -> Result<(), NativeCompositorError> {
        self.post_event(
            resource,
            "wl_keyboard",
            "modifiers",
            &mut [
                ffi::wl_argument { u: serial },
                ffi::wl_argument { u: modifiers.0 },
                ffi::wl_argument { u: modifiers.1 },
                ffi::wl_argument { u: modifiers.2 },
                ffi::wl_argument { u: modifiers.3 },
            ],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn touch_down(
        &mut self,
        seat_id: u32,
        surface: WaylandSurfaceId,
        time_milliseconds: u32,
        touch_id: i32,
        position: crate::core::PointF,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        if touch_id < 0 || self.touch_points.contains_key(&(seat_id, touch_id)) {
            return Err(NativeCompositorError::new(
                "invalid or duplicate touch identity",
            ));
        }
        let seat = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?;
        if !seat.capabilities.touch {
            return Err(NativeCompositorError::new("seat has no touch capability"));
        }
        let client = self
            .core
            .world
            .surface_owner(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown touch surface"))?;
        self.core
            .serials
            .issue(
                serial,
                client,
                crate::compositor_wayland::SerialKind::TouchDown,
                Some(surface),
            )
            .map_err(error)?;
        let surface_resource = self.surface_resource(surface)?;
        for resource in self.resources_for_client(
            client,
            |kind| matches!(kind, ResourceKind::Touch(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_touch",
                "down",
                &mut [
                    ffi::wl_argument { u: serial },
                    ffi::wl_argument {
                        u: time_milliseconds,
                    },
                    ffi::wl_argument {
                        o: surface_resource,
                    },
                    ffi::wl_argument { i: touch_id },
                    ffi::wl_argument {
                        f: fixed(position.x),
                    },
                    ffi::wl_argument {
                        f: fixed(position.y),
                    },
                ],
            )?;
            self.post_event(resource, "wl_touch", "frame", &mut [])?;
        }
        self.touch_points.insert(
            (seat_id, touch_id),
            NativeTouchPoint {
                client,
                surface,
                down_serial: serial,
            },
        );
        Ok(())
    }

    fn touch_motion(
        &self,
        seat_id: u32,
        time_milliseconds: u32,
        touch_id: i32,
        position: crate::core::PointF,
    ) -> Result<(), NativeCompositorError> {
        let point = self
            .touch_points
            .get(&(seat_id, touch_id))
            .copied()
            .ok_or_else(|| NativeCompositorError::new("unknown touch identity"))?;
        for resource in self.resources_for_client(
            point.client,
            |kind| matches!(kind, ResourceKind::Touch(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_touch",
                "motion",
                &mut [
                    ffi::wl_argument {
                        u: time_milliseconds,
                    },
                    ffi::wl_argument { i: touch_id },
                    ffi::wl_argument {
                        f: fixed(position.x),
                    },
                    ffi::wl_argument {
                        f: fixed(position.y),
                    },
                ],
            )?;
            self.post_event(resource, "wl_touch", "frame", &mut [])?;
        }
        Ok(())
    }

    fn touch_up(
        &mut self,
        seat_id: u32,
        time_milliseconds: u32,
        touch_id: i32,
        serial: u32,
    ) -> Result<(), NativeCompositorError> {
        let point = self
            .touch_points
            .remove(&(seat_id, touch_id))
            .ok_or_else(|| NativeCompositorError::new("unknown touch identity"))?;
        self.core
            .serials
            .issue(
                serial,
                point.client,
                crate::compositor_wayland::SerialKind::DataDevice,
                Some(point.surface),
            )
            .map_err(error)?;
        for resource in self.resources_for_client(
            point.client,
            |kind| matches!(kind, ResourceKind::Touch(candidate) if candidate == seat_id),
        )? {
            self.post_event(
                resource,
                "wl_touch",
                "up",
                &mut [
                    ffi::wl_argument { u: serial },
                    ffi::wl_argument {
                        u: time_milliseconds,
                    },
                    ffi::wl_argument { i: touch_id },
                ],
            )?;
            self.post_event(resource, "wl_touch", "frame", &mut [])?;
        }
        Ok(())
    }

    fn touch_cancel(&mut self, seat_id: u32) -> Result<(), NativeCompositorError> {
        let clients = self
            .touch_points
            .iter()
            .filter_map(|((seat, _), point)| (*seat == seat_id).then_some(point.client))
            .collect::<BTreeSet<_>>();
        self.touch_points.retain(|(seat, _), _| *seat != seat_id);
        for client in clients {
            for resource in self.resources_for_client(
                client,
                |kind| matches!(kind, ResourceKind::Touch(candidate) if candidate == seat_id),
            )? {
                self.post_event(resource, "wl_touch", "cancel", &mut [])?;
            }
        }
        Ok(())
    }

    fn bind(
        &mut self,
        client: ClientRef<'_>,
        interface: &str,
        kind: ResourceKind,
        version: u32,
        id: u32,
    ) -> Result<(), NativeCompositorError> {
        let client_id = self.ensure_client(client)?;
        let resource =
            self.create_resource(client, client_id, interface, version, id, kind, true)?;
        if interface == "wl_shm" {
            for format in [0_u32, 1_u32] {
                self.post_event(
                    resource,
                    "wl_shm",
                    "format",
                    &mut [ffi::wl_argument { u: format }],
                )?;
            }
        }
        if interface == "wp_presentation" {
            self.post_event(
                resource,
                "wp_presentation",
                "clock_id",
                &mut [ffi::wl_argument { u: 1 }],
            )?;
        }
        if interface == "xdg_toplevel_icon_manager_v1" {
            for size in [16_i32, 24, 32, 48, 64] {
                self.post_event(
                    resource,
                    "xdg_toplevel_icon_manager_v1",
                    "icon_size",
                    &mut [ffi::wl_argument { i: size }],
                )?;
            }
            self.post_event(resource, "xdg_toplevel_icon_manager_v1", "done", &mut [])?;
        }
        if let ResourceKind::Output(output) = kind {
            self.send_output_description(resource, output)?;
        }
        if let ResourceKind::Seat(seat) = kind {
            self.send_seat_description(resource, seat)?;
        }
        if matches!(kind, ResourceKind::LinuxDmaBuf) {
            for format in &self.dmabuf_formats {
                self.post_event(
                    resource,
                    "zwp_linux_dmabuf_v1",
                    "modifier",
                    &mut [
                        ffi::wl_argument { u: format.fourcc },
                        ffi::wl_argument {
                            u: (format.modifier >> 32) as u32,
                        },
                        ffi::wl_argument {
                            u: format.modifier as u32,
                        },
                    ],
                )?;
            }
        }
        Ok(())
    }

    fn ensure_client(&mut self, client: ClientRef<'_>) -> Result<ClientId, NativeCompositorError> {
        if let Some(client) = self.clients.get(&client.identity()) {
            return Ok(*client);
        }
        self.next_client = next_nonzero(self.next_client)?;
        let id = ClientId::from_raw(self.next_client).expect("nonzero");
        self.core.connect_client(id).map_err(error)?;
        self.clients.insert(client.identity(), id);
        Ok(id)
    }

    fn send_output_description(
        &self,
        resource: ResourceRef<'_>,
        output_id: u32,
    ) -> Result<(), NativeCompositorError> {
        let output = self
            .core
            .outputs
            .get(&output_id)
            .ok_or_else(|| NativeCompositorError::new("unknown output"))?;
        let description = &output.description;
        let make = protocol_string(&description.make);
        let model = protocol_string(&description.model);
        let transform = output_transform_wire(description.transform);
        self.post_event(
            resource,
            "wl_output",
            "geometry",
            &mut [
                ffi::wl_argument {
                    i: description.logical_position.x,
                },
                ffi::wl_argument {
                    i: description.logical_position.y,
                },
                ffi::wl_argument {
                    i: description.physical_millimeters.width,
                },
                ffi::wl_argument {
                    i: description.physical_millimeters.height,
                },
                ffi::wl_argument { i: 0 },
                ffi::wl_argument { s: make.as_ptr() },
                ffi::wl_argument { s: model.as_ptr() },
                ffi::wl_argument { i: transform },
            ],
        )?;
        for (index, mode) in description.modes.iter().enumerate() {
            let mut flags = u32::from(index == output.current_mode);
            if mode.preferred {
                flags |= 2;
            }
            self.post_event(
                resource,
                "wl_output",
                "mode",
                &mut [
                    ffi::wl_argument { u: flags },
                    ffi::wl_argument { i: mode.size.width },
                    ffi::wl_argument {
                        i: mode.size.height,
                    },
                    ffi::wl_argument {
                        i: i32::try_from(mode.refresh_millihertz).unwrap_or(i32::MAX),
                    },
                ],
            )?;
        }
        if resource.version() >= 2 {
            self.post_event(
                resource,
                "wl_output",
                "scale",
                &mut [ffi::wl_argument {
                    i: description.scale,
                }],
            )?;
        }
        if resource.version() >= 4 {
            let name = protocol_string(&description.name);
            let detail = protocol_string(&description.description);
            self.post_event(
                resource,
                "wl_output",
                "name",
                &mut [ffi::wl_argument { s: name.as_ptr() }],
            )?;
            self.post_event(
                resource,
                "wl_output",
                "description",
                &mut [ffi::wl_argument { s: detail.as_ptr() }],
            )?;
        }
        if resource.version() >= 2 {
            self.post_event(resource, "wl_output", "done", &mut [])?;
        }
        Ok(())
    }

    fn send_seat_description(
        &self,
        resource: ResourceRef<'_>,
        seat_id: u32,
    ) -> Result<(), NativeCompositorError> {
        let seat = self
            .core
            .seats
            .get(&seat_id)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?;
        let capabilities = u32::from(seat.capabilities.pointer)
            | (u32::from(seat.capabilities.keyboard) << 1)
            | (u32::from(seat.capabilities.touch) << 2);
        if resource.version() >= 2 {
            let name = protocol_string(&seat.name);
            self.post_event(
                resource,
                "wl_seat",
                "name",
                &mut [ffi::wl_argument { s: name.as_ptr() }],
            )?;
        }
        self.post_event(
            resource,
            "wl_seat",
            "capabilities",
            &mut [ffi::wl_argument { u: capabilities }],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_resource<'client>(
        &mut self,
        client: ClientRef<'client>,
        client_id: ClientId,
        interface: &str,
        version: u32,
        wire_id: u32,
        kind: ResourceKind,
        register: bool,
    ) -> Result<ResourceRef<'client>, NativeCompositorError> {
        self.next_object = next_nonzero(self.next_object)?;
        let object = ProtocolObjectId::from_raw(self.next_object).expect("nonzero");
        let interface_descriptor = self
            .protocol
            .interface(interface)
            .ok_or_else(|| NativeCompositorError::new(format!("missing interface {interface}")))?;
        let version = version.min(interface_descriptor.version as u32);
        let resource = unsafe { client.create_resource(interface_descriptor, version, wire_id) }
            .map_err(error)?;
        if register {
            self.core
                .objects
                .insert(
                    object,
                    ObjectMetadata {
                        owner: client_id,
                        kind: kind.object_kind(),
                        version,
                    },
                )
                .map_err(error)?;
        }
        let context = Box::new(ResourceContext {
            state: self,
            object,
            client: client_id,
            interface: interface.to_owned(),
            kind,
        });
        let context = Box::into_raw(context);
        unsafe {
            resource.set_dispatcher(
                Some(dispatch_resource),
                std::ptr::null(),
                context.cast::<c_void>(),
                Some(destroy_resource),
            )
        };
        self.resources.insert(object, resource.identity());
        Ok(resource)
    }

    fn dispatch(
        &mut self,
        resource: ResourceRef<'_>,
        context: *const ResourceContext,
        kind: ResourceKind,
        request: &mut IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let context = unsafe { &*context };
        if let ResourceKind::SessionLock(object) = kind
            && request.message().destructor
        {
            return self.dispatch_session_lock(resource, context, object, request);
        }
        if request.message().destructor || request.message().name == "destroy" {
            return Ok(DispatchOutcome {
                destroy_self: true,
                ..DispatchOutcome::default()
            });
        }
        match kind {
            ResourceKind::Compositor => self.dispatch_compositor(resource, context, request),
            ResourceKind::Surface(surface) => {
                self.dispatch_surface(resource, context, surface, request)
            }
            ResourceKind::Region(object) => self.dispatch_region(object, request),
            ResourceKind::Shm => self.dispatch_shm(resource, context, request),
            ResourceKind::ShmPool(object) => {
                self.dispatch_shm_pool(resource, context, object, request)
            }
            ResourceKind::Buffer(_) | ResourceKind::Callback(_) => {
                Err(NativeCompositorError::new("resource has no requests"))
            }
            ResourceKind::Output(_) => Err(NativeCompositorError::new(
                "wl_output has no non-destructor requests",
            )),
            ResourceKind::Seat(seat) => self.dispatch_seat(resource, context, seat, request),
            ResourceKind::Pointer(seat) => self.dispatch_pointer(context, seat, request),
            ResourceKind::Keyboard(_) | ResourceKind::Touch(_) => Err(NativeCompositorError::new(
                "input resource has no non-destructor requests",
            )),
            ResourceKind::DataDeviceManager => {
                self.dispatch_data_device_manager(resource, context, request)
            }
            ResourceKind::DataDevice(seat) => self.dispatch_data_device(context, seat, request),
            ResourceKind::DataSource(source) => self.dispatch_data_source(source, request),
            ResourceKind::DataOffer(offer) => {
                self.dispatch_data_offer(resource, context, offer, request)
            }
            ResourceKind::LinuxDmaBuf => self.dispatch_linux_dmabuf(resource, context, request),
            ResourceKind::LinuxBufferParams(object) => {
                self.dispatch_linux_buffer_params(resource, context, object, request)
            }
            ResourceKind::DecorationManager => {
                self.dispatch_decoration_manager(resource, context, request)
            }
            ResourceKind::ToplevelDecoration(surface) => {
                self.dispatch_toplevel_decoration(resource, surface, request)
            }
            ResourceKind::CursorShapeManager => {
                self.dispatch_cursor_shape_manager(resource, context, request)
            }
            ResourceKind::CursorShapeDevice(seat) => {
                self.dispatch_cursor_shape_device(context, seat, request)
            }
            ResourceKind::ToplevelIconManager => {
                self.dispatch_toplevel_icon_manager(resource, context, request)
            }
            ResourceKind::ToplevelIcon(object) => {
                self.dispatch_toplevel_icon(resource, object, request)
            }
            ResourceKind::FractionalScaleManager => {
                self.dispatch_fractional_scale_manager(resource, context, request)
            }
            ResourceKind::FractionalScale => Err(NativeCompositorError::new(
                "fractional-scale object has no non-destructor requests",
            )),
            ResourceKind::Viewporter => self.dispatch_viewporter(resource, context, request),
            ResourceKind::Viewport(surface) => self.dispatch_viewport(surface, request),
            ResourceKind::Presentation => self.dispatch_presentation(resource, context, request),
            ResourceKind::PresentationFeedback(_) => Err(NativeCompositorError::new(
                "presentation-feedback object has no non-destructor requests",
            )),
            ResourceKind::Activation => self.dispatch_activation(resource, context, request),
            ResourceKind::ActivationToken(object) => {
                self.dispatch_activation_token(resource, context, object, request)
            }
            ResourceKind::SessionLockManager => {
                self.dispatch_session_lock_manager(resource, context, request)
            }
            ResourceKind::SessionLock(object) => {
                self.dispatch_session_lock(resource, context, object, request)
            }
            ResourceKind::SessionLockSurface(surface) => {
                self.dispatch_session_lock_surface(surface, request)
            }
            ResourceKind::RelativePointerManager => {
                self.dispatch_relative_pointer_manager(resource, context, request)
            }
            ResourceKind::RelativePointer(_) => Err(NativeCompositorError::new(
                "relative-pointer object has no non-destructor requests",
            )),
            ResourceKind::IdleInhibitManager => {
                self.dispatch_idle_inhibit_manager(resource, context, request)
            }
            ResourceKind::IdleInhibitor(_) => Err(NativeCompositorError::new(
                "idle-inhibitor object has no non-destructor requests",
            )),
            ResourceKind::PointerConstraints => {
                self.dispatch_pointer_constraints(resource, context, request)
            }
            ResourceKind::LockedPointer(object) => self.dispatch_locked_pointer(object, request),
            ResourceKind::ConfinedPointer(object) => {
                self.dispatch_confined_pointer(object, request)
            }
            ResourceKind::ExplicitSynchronization => {
                self.dispatch_explicit_synchronization(resource, context, request)
            }
            ResourceKind::SurfaceSynchronization(surface) => {
                self.dispatch_surface_synchronization(resource, context, surface, request)
            }
            ResourceKind::ExplicitBufferRelease(_) => Err(NativeCompositorError::new(
                "explicit buffer-release object has no requests",
            )),
            ResourceKind::Subcompositor => self.dispatch_subcompositor(resource, context, request),
            ResourceKind::Subsurface(surface) => self.dispatch_subsurface(surface, request),
            ResourceKind::XdgWmBase => self.dispatch_xdg_wm_base(resource, context, request),
            ResourceKind::XdgPositioner(object) => self.dispatch_xdg_positioner(object, request),
            ResourceKind::XdgSurface(surface) => {
                self.dispatch_xdg_surface(resource, context, surface, request)
            }
            ResourceKind::XdgToplevel(surface) => {
                self.dispatch_xdg_toplevel(context, surface, request)
            }
            ResourceKind::XdgPopup(surface) => self.dispatch_xdg_popup(context, surface, request),
        }
    }

    fn dispatch_seat(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        seat: u32,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let state = self
            .core
            .seats
            .get(&seat)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?;
        let (interface, kind, enabled) = match request.message().name.as_str() {
            "get_pointer" => (
                "wl_pointer",
                ResourceKind::Pointer(seat),
                state.capabilities.pointer,
            ),
            "get_keyboard" => (
                "wl_keyboard",
                ResourceKind::Keyboard(seat),
                state.capabilities.keyboard,
            ),
            "get_touch" => (
                "wl_touch",
                ResourceKind::Touch(seat),
                state.capabilities.touch,
            ),
            _ => return Err(unsupported_request(request)),
        };
        if !enabled {
            return Err(NativeCompositorError::new(
                "requested input capability is unavailable",
            ));
        }
        let input_resource = self.create_resource(
            resource.client(),
            context.client,
            interface,
            resource.version(),
            request.new_id(0).map_err(error)?,
            kind,
            true,
        )?;
        if matches!(kind, ResourceKind::Keyboard(_)) {
            self.send_keyboard_initial(input_resource, seat, context.client)?;
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_pointer(
        &mut self,
        context: &ResourceContext,
        seat: u32,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "set_cursor" {
            return Err(unsupported_request(request));
        }
        let serial = request.uint(0).map_err(error)?;
        self.core
            .serials
            .validate(
                context.client,
                serial,
                &[crate::compositor_wayland::SerialKind::PointerEnter],
                self.core
                    .seats
                    .get(&seat)
                    .and_then(|seat| seat.pointer_focus.map(|focus| focus.surface)),
            )
            .map_err(error)?;
        let cursor = request
            .object(1)
            .map_err(error)?
            .map(|resource| self.surface_from_resource(resource))
            .transpose()?;
        let cursor = match cursor {
            Some(surface) => {
                self.surface_mut(surface)?
                    .assign_role(SurfaceRole::Cursor)
                    .map_err(error)?;
                crate::compositor_wayland::CursorImage::ClientSurface {
                    surface,
                    hotspot_x: request.int(2).map_err(error)?,
                    hotspot_y: request.int(3).map_err(error)?,
                }
            }
            None => crate::compositor_wayland::CursorImage::Hidden,
        };
        self.core
            .seats
            .get_mut(&seat)
            .ok_or_else(|| NativeCompositorError::new("unknown seat"))?
            .cursor = cursor;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_data_device_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "create_data_source" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_data_source",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::DataSource(object),
                    true,
                )?;
                self.core
                    .data_devices
                    .create_source(crate::compositor_wayland::DataSource {
                        owner: context.client,
                        object,
                        mime_types: Vec::new(),
                        actions: crate::compositor_wayland::DataAction::NONE,
                        used: false,
                    })
                    .map_err(error)?;
            }
            "get_data_device" => {
                let seat_resource = request
                    .object(1)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing wl_seat"))?;
                let ResourceKind::Seat(seat) = self.resource_kind(seat_resource)? else {
                    return Err(NativeCompositorError::new(
                        "data-device target is not a wl_seat",
                    ));
                };
                let data_device = self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_data_device",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::DataDevice(seat),
                    true,
                )?;
                let focused = self
                    .core
                    .seats
                    .get(&seat)
                    .and_then(|seat| seat.keyboard_focus)
                    .is_some_and(|focus| focus.client == context.client);
                if focused {
                    self.send_selection_to_device(data_device, context.client)?;
                }
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_data_source(
        &mut self,
        source: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let source = self
            .core
            .data_devices
            .source_mut(source)
            .ok_or_else(|| NativeCompositorError::new("unknown wl_data_source"))?;
        match request.message().name.as_str() {
            "offer" => source
                .offer(
                    crate::compositor_wayland::MimeType::new(c_string(request, 0)?)
                        .map_err(error)?,
                )
                .map_err(error)?,
            "set_actions" => {
                let actions = crate::compositor_wayland::DataAction::from_protocol(
                    request.uint(0).map_err(error)?,
                )
                .ok_or_else(|| NativeCompositorError::new("invalid data-source actions"))?;
                if actions == crate::compositor_wayland::DataAction::NONE {
                    return Err(NativeCompositorError::new(
                        "data-source action set must not be empty",
                    ));
                }
                source.actions = actions;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_data_device(
        &mut self,
        context: &ResourceContext,
        seat: u32,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "set_selection" => {
                let focus = self
                    .core
                    .seats
                    .get(&seat)
                    .and_then(|seat| seat.keyboard_focus)
                    .ok_or_else(|| NativeCompositorError::new("seat has no keyboard focus"))?;
                if focus.client != context.client {
                    return Err(NativeCompositorError::new(
                        "only the keyboard-focused client may set the selection",
                    ));
                }
                let serial = request.uint(1).map_err(error)?;
                self.core
                    .serials
                    .consume(
                        context.client,
                        serial,
                        &[
                            crate::compositor_wayland::SerialKind::PointerButton,
                            crate::compositor_wayland::SerialKind::KeyboardKey,
                        ],
                        None,
                    )
                    .map_err(error)?;
                let source = request
                    .object(0)
                    .map_err(error)?
                    .map(|resource| self.data_source_from_resource(resource))
                    .transpose()?;
                let previous = self.core.data_devices.selection();
                self.core
                    .data_devices
                    .set_selection(context.client, source)
                    .map_err(error)?;
                if let Some(previous) = previous {
                    self.cancel_data_source(previous)?;
                }
                self.send_selection_to_client(seat, context.client)?;
            }
            "start_drag" => {
                let source = request
                    .object(0)
                    .map_err(error)?
                    .map(|resource| self.data_source_from_resource(resource))
                    .transpose()?;
                let origin = self.surface_from_resource(
                    request
                        .object(1)
                        .map_err(error)?
                        .ok_or_else(|| NativeCompositorError::new("missing drag origin"))?,
                )?;
                if self.core.world.surface_owner(origin) != Some(context.client) {
                    return Err(NativeCompositorError::new(
                        "drag origin belongs to another client",
                    ));
                }
                let serial = request.uint(3).map_err(error)?;
                let grab_serial = self
                    .core
                    .serials
                    .consume(
                        context.client,
                        serial,
                        &[
                            crate::compositor_wayland::SerialKind::PointerButton,
                            crate::compositor_wayland::SerialKind::TouchDown,
                        ],
                        Some(origin),
                    )
                    .map_err(error)?;
                let grab = match grab_serial.kind {
                    crate::compositor_wayland::SerialKind::PointerButton => NativeDragGrab::Pointer,
                    crate::compositor_wayland::SerialKind::TouchDown => {
                        let slot = self
                            .touch_points
                            .iter()
                            .find_map(|((candidate_seat, slot), point)| {
                                (*candidate_seat == seat
                                    && point.client == context.client
                                    && point.surface == origin
                                    && point.down_serial == serial)
                                    .then_some(*slot)
                            })
                            .ok_or_else(|| {
                                NativeCompositorError::new(
                                    "touch drag serial has no active touch point",
                                )
                            })?;
                        NativeDragGrab::Touch(slot)
                    }
                    _ => unreachable!("serial kind was constrained above"),
                };
                if let Some(source) = source {
                    let source_resource = self.data_source_resource(source)?;
                    let source_state = self
                        .core
                        .data_devices
                        .source(source)
                        .ok_or_else(|| NativeCompositorError::new("unknown drag source"))?;
                    if source_resource.version() >= 3
                        && source_state.actions == crate::compositor_wayland::DataAction::NONE
                    {
                        return Err(NativeCompositorError::new(
                            "version 3 drag source did not set its actions",
                        ));
                    }
                }
                let icon = request
                    .object(2)
                    .map_err(error)?
                    .map(|resource| self.surface_from_resource(resource))
                    .transpose()?;
                if let Some(icon) = icon {
                    if self.core.world.surface_owner(icon) != Some(context.client) {
                        return Err(NativeCompositorError::new(
                            "drag icon belongs to another client",
                        ));
                    }
                    self.surface_mut(icon)?
                        .assign_role(SurfaceRole::DragIcon)
                        .map_err(error)?;
                }
                self.core
                    .data_devices
                    .start_drag(context.client, source, origin)
                    .map_err(error)?;
                self.active_drag = Some(NativeDrag {
                    seat,
                    source,
                    origin,
                    icon,
                    grab,
                    target: None,
                });
                self.core
                    .queue_action(CompositorAction::StartDrag { seat, origin, icon });
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_data_offer(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        offer: ProtocolObjectId,
        request: &mut IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "accept" => {
                let is_drag = self
                    .core
                    .data_devices
                    .offer(offer)
                    .is_some_and(|offer| offer.drag);
                if !is_drag {
                    return Err(NativeCompositorError::new(
                        "selection offers cannot be accepted as drag offers",
                    ));
                }
                let target_surface = self
                    .active_drag
                    .as_ref()
                    .and_then(|drag| drag.target.as_ref())
                    .filter(|target| target.offers.contains(&offer))
                    .map(|target| target.surface)
                    .ok_or_else(|| NativeCompositorError::new("data offer is no longer active"))?;
                self.core
                    .serials
                    .validate(
                        context.client,
                        request.uint(0).map_err(error)?,
                        &[crate::compositor_wayland::SerialKind::DataDevice],
                        Some(target_surface),
                    )
                    .map_err(error)?;
                let accepted = request
                    .string(1)
                    .map_err(error)?
                    .map(|value| {
                        crate::compositor_wayland::MimeType::new(
                            value.to_string_lossy().into_owned(),
                        )
                        .map_err(error)
                    })
                    .transpose()?;
                if let Some(mime) = &accepted {
                    let source = self
                        .core
                        .data_devices
                        .offer(offer)
                        .and_then(|offer| self.core.data_devices.source(offer.source))
                        .ok_or_else(|| NativeCompositorError::new("unknown data offer source"))?;
                    if !source.mime_types.contains(mime) {
                        return Err(NativeCompositorError::new(
                            "accepted MIME type was not offered",
                        ));
                    }
                }
                let (source, drag) = {
                    let offer = self
                        .core
                        .data_devices
                        .offer_mut(offer)
                        .ok_or_else(|| NativeCompositorError::new("unknown wl_data_offer"))?;
                    offer.accepted_mime_type = accepted.clone();
                    if offer.drag && resource.version() < 3 {
                        offer.target_actions = crate::compositor_wayland::DataAction::COPY;
                        offer.selected_action = if accepted.is_some() {
                            crate::compositor_wayland::DataAction::COPY
                        } else {
                            crate::compositor_wayland::DataAction::NONE
                        };
                    }
                    (offer.source, offer.drag)
                };
                if drag {
                    let source_resource = self.data_source_resource(source)?;
                    let mime = accepted.as_ref().map(|mime| protocol_string(mime.as_str()));
                    self.post_event(
                        source_resource,
                        "wl_data_source",
                        "target",
                        &mut [ffi::wl_argument {
                            s: mime.as_ref().map_or(std::ptr::null(), |mime| mime.as_ptr()),
                        }],
                    )?;
                }
            }
            "receive" => {
                let mime = crate::compositor_wayland::MimeType::new(c_string(request, 0)?)
                    .map_err(error)?;
                let source = self
                    .core
                    .data_devices
                    .offer(offer)
                    .and_then(|offer| self.core.data_devices.source(offer.source))
                    .cloned()
                    .ok_or_else(|| NativeCompositorError::new("unknown data offer source"))?;
                if !source.mime_types.contains(&mime) {
                    return Err(NativeCompositorError::new(
                        "requested MIME type was not offered",
                    ));
                }
                let fd = request.take_fd(1).map_err(error)?;
                let source_resource = self.data_source_resource(source.object)?;
                let mime = protocol_string(mime.as_str());
                self.post_event(
                    source_resource,
                    "wl_data_source",
                    "send",
                    &mut [
                        ffi::wl_argument { s: mime.as_ptr() },
                        ffi::wl_argument { h: fd.as_raw_fd() },
                    ],
                )?;
            }
            "set_actions" => {
                if !self
                    .core
                    .data_devices
                    .offer(offer)
                    .is_some_and(|offer| offer.drag)
                {
                    return Err(NativeCompositorError::new(
                        "selection offers do not negotiate drag actions",
                    ));
                }
                let actions = crate::compositor_wayland::DataAction::from_protocol(
                    request.uint(0).map_err(error)?,
                )
                .ok_or_else(|| NativeCompositorError::new("invalid data-offer actions"))?;
                let preferred = crate::compositor_wayland::DataAction::from_protocol(
                    request.uint(1).map_err(error)?,
                )
                .filter(|action| {
                    [
                        crate::compositor_wayland::DataAction::NONE,
                        crate::compositor_wayland::DataAction::COPY,
                        crate::compositor_wayland::DataAction::MOVE,
                        crate::compositor_wayland::DataAction::ASK,
                    ]
                    .contains(action)
                })
                .ok_or_else(|| NativeCompositorError::new("invalid preferred data action"))?;
                if preferred != crate::compositor_wayland::DataAction::NONE
                    && !actions.contains(preferred)
                {
                    return Err(NativeCompositorError::new(
                        "preferred data action is not in the accepted set",
                    ));
                }
                let source = {
                    let offer = self
                        .core
                        .data_devices
                        .offer_mut(offer)
                        .ok_or_else(|| NativeCompositorError::new("unknown wl_data_offer"))?;
                    offer.target_actions = actions;
                    offer.preferred_action = preferred;
                    offer.source
                };
                let selected = self.core.data_devices.choose_action(offer).map_err(error)?;
                self.post_event(
                    resource,
                    "wl_data_offer",
                    "action",
                    &mut [ffi::wl_argument {
                        u: u32::from(selected.bits()),
                    }],
                )?;
                if let Ok(source_resource) = self.data_source_resource(source)
                    && source_resource.version() >= 3
                {
                    self.post_event(
                        source_resource,
                        "wl_data_source",
                        "action",
                        &mut [ffi::wl_argument {
                            u: u32::from(selected.bits()),
                        }],
                    )?;
                }
            }
            "finish" => {
                let source = {
                    let offer = self
                        .core
                        .data_devices
                        .offer_mut(offer)
                        .ok_or_else(|| NativeCompositorError::new("unknown wl_data_offer"))?;
                    if !offer.drag
                        || !offer.dropped
                        || offer.finished
                        || offer.selected_action == crate::compositor_wayland::DataAction::NONE
                    {
                        return Err(NativeCompositorError::new(
                            "data offer cannot be finished in its current state",
                        ));
                    }
                    offer.finished = true;
                    offer.source
                };
                let first_finish = self.finished_drag_sources.insert(source);
                if first_finish
                    && let Ok(source_resource) = self.data_source_resource(source)
                    && source_resource.version() >= 3
                {
                    self.post_event(source_resource, "wl_data_source", "dnd_finished", &mut [])?;
                }
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_linux_dmabuf(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "create_params" {
            return Err(unsupported_request(request));
        }
        let object = self.peek_next_object()?;
        self.create_resource(
            resource.client(),
            context.client,
            "zwp_linux_buffer_params_v1",
            resource.version(),
            request.new_id(0).map_err(error)?,
            ResourceKind::LinuxBufferParams(object),
            true,
        )?;
        self.dmabuf_params
            .insert(object, NativeDmaBufParams::default());
        Ok(DispatchOutcome::default())
    }

    fn dispatch_decoration_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_toplevel_decoration" {
            return Err(unsupported_request(request));
        }
        let toplevel_resource = request
            .object(1)
            .map_err(error)?
            .ok_or_else(|| NativeCompositorError::new("missing xdg_toplevel"))?;
        let ResourceKind::XdgToplevel(surface) = self.resource_kind(toplevel_resource)? else {
            return Err(NativeCompositorError::new(
                "decoration target is not an xdg_toplevel",
            ));
        };
        let decoration = self.create_resource(
            resource.client(),
            context.client,
            "zxdg_toplevel_decoration_v1",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::ToplevelDecoration(surface),
            true,
        )?;
        self.toplevels
            .get_mut(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_toplevel"))?
            .decoration = crate::compositor_wayland::DecorationMode::ServerSide;
        self.post_event(
            decoration,
            "zxdg_toplevel_decoration_v1",
            "configure",
            &mut [ffi::wl_argument { u: 2 }],
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_toplevel_decoration(
        &mut self,
        resource: ResourceRef<'_>,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let mode = match request.message().name.as_str() {
            "set_mode" => match request.uint(0).map_err(error)? {
                1 => crate::compositor_wayland::DecorationMode::ClientSide,
                2 => crate::compositor_wayland::DecorationMode::ServerSide,
                _ => return Err(NativeCompositorError::new("invalid decoration mode")),
            },
            "unset_mode" => crate::compositor_wayland::DecorationMode::ServerSide,
            _ => return Err(unsupported_request(request)),
        };
        // Telorgon policy owns the final choice. The default policy honors an explicit client-side
        // request and otherwise uses the configured Compose window-frame component.
        self.toplevels
            .get_mut(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_toplevel"))?
            .decoration = mode;
        self.post_event(
            resource,
            "zxdg_toplevel_decoration_v1",
            "configure",
            &mut [ffi::wl_argument {
                u: if mode == crate::compositor_wayland::DecorationMode::ServerSide {
                    2
                } else {
                    1
                },
            }],
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_cursor_shape_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_pointer" {
            return Err(unsupported_request(request));
        }
        let pointer = request
            .object(1)
            .map_err(error)?
            .ok_or_else(|| NativeCompositorError::new("missing wl_pointer"))?;
        let ResourceKind::Pointer(seat) = self.resource_kind(pointer)? else {
            return Err(NativeCompositorError::new(
                "cursor-shape target is not a wl_pointer",
            ));
        };
        self.create_resource(
            resource.client(),
            context.client,
            "wp_cursor_shape_device_v1",
            resource.version(),
            request.new_id(0).map_err(error)?,
            ResourceKind::CursorShapeDevice(seat),
            true,
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_cursor_shape_device(
        &mut self,
        context: &ResourceContext,
        seat: u32,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "set_shape" {
            return Err(unsupported_request(request));
        }
        let serial = request.uint(0).map_err(error)?;
        let shape = request.uint(1).map_err(error)?;
        if !(1..=36).contains(&shape) {
            return Err(NativeCompositorError::new("invalid cursor shape"));
        }
        let focus = self
            .core
            .seats
            .get(&seat)
            .and_then(|seat| seat.pointer_focus)
            .ok_or_else(|| NativeCompositorError::new("pointer has no focus"))?;
        self.core
            .serials
            .validate(
                context.client,
                serial,
                &[crate::compositor_wayland::SerialKind::PointerEnter],
                Some(focus.surface),
            )
            .map_err(error)?;
        self.core.seats.get_mut(&seat).expect("seat checked").cursor =
            crate::compositor_wayland::CursorImage::Shape(shape);
        Ok(DispatchOutcome::default())
    }

    fn dispatch_toplevel_icon_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "create_icon" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_toplevel_icon_v1",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::ToplevelIcon(object),
                    true,
                )?;
                self.toplevel_icons
                    .insert(object, NativeToplevelIcon::default());
            }
            "set_icon" => {
                let toplevel_resource = request
                    .object(0)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing xdg_toplevel"))?;
                let ResourceKind::XdgToplevel(surface) = self.resource_kind(toplevel_resource)?
                else {
                    return Err(NativeCompositorError::new(
                        "icon target is not an xdg_toplevel",
                    ));
                };
                if !self.toplevels.contains_key(&surface) {
                    return Err(NativeCompositorError::new("unknown xdg_toplevel"));
                }
                let icon = request
                    .object(1)
                    .map_err(error)?
                    .map(|resource| match self.resource_kind(resource)? {
                        ResourceKind::ToplevelIcon(object) => Ok(object),
                        _ => Err(NativeCompositorError::new(
                            "set_icon object is not an xdg_toplevel_icon_v1",
                        )),
                    })
                    .transpose()?;
                let Some(icon) = icon else {
                    self.pending_toplevel_icons
                        .insert(surface, PendingToplevelIcon::Reset);
                    return Ok(DispatchOutcome::default());
                };
                let (name, buffers) = {
                    let icon = self
                        .toplevel_icons
                        .get_mut(&icon)
                        .ok_or_else(|| NativeCompositorError::new("unknown toplevel icon"))?;
                    icon.immutable = true;
                    (icon.name.clone(), icon.buffers.clone())
                };
                if name.is_none() && buffers.is_empty() {
                    self.pending_toplevel_icons
                        .insert(surface, PendingToplevelIcon::Reset);
                    return Ok(DispatchOutcome::default());
                }
                let mut images = Vec::with_capacity(buffers.len());
                for ((_, scale), buffer) in buffers {
                    images.push(ToplevelIconImage {
                        buffer,
                        scale,
                        image: self.snapshot_shm_buffer(buffer)?,
                    });
                }
                self.toplevel_icon_revision = self.toplevel_icon_revision.wrapping_add(1).max(1);
                self.pending_toplevel_icons.insert(
                    surface,
                    PendingToplevelIcon::Icon(ToplevelIconSnapshot {
                        revision: self.toplevel_icon_revision,
                        name,
                        images,
                    }),
                );
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_toplevel_icon(
        &mut self,
        resource: ResourceRef<'_>,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let immutable = self
            .toplevel_icons
            .get(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown toplevel icon"))?
            .immutable;
        if immutable {
            resource.post_error(2, "the toplevel icon is immutable after assignment");
            return Ok(DispatchOutcome::default());
        }
        match request.message().name.as_str() {
            "set_name" => {
                let name = c_string(request, 0)?;
                if name.len() > 4_096 || name.contains('\0') {
                    return Err(NativeCompositorError::new("invalid toplevel icon name"));
                }
                self.toplevel_icons
                    .get_mut(&object)
                    .expect("icon was checked above")
                    .name = Some(name);
            }
            "add_buffer" => {
                let buffer_resource = request
                    .object(0)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing icon wl_buffer"))?;
                let ResourceKind::Buffer(buffer) = self.resource_kind(buffer_resource)? else {
                    resource.post_error(1, "icon object is not a wl_buffer");
                    return Ok(DispatchOutcome::default());
                };
                let scale = request.int(1).map_err(error)?;
                let descriptor = self.core.buffer(buffer);
                let Some(BufferDescriptor::Shm(descriptor)) = descriptor else {
                    resource.post_error(1, "icon buffer must use wl_shm");
                    return Ok(DispatchOutcome::default());
                };
                if scale <= 0 || descriptor.size.width != descriptor.size.height {
                    resource.post_error(1, "icon buffer must be square with a positive scale");
                    return Ok(DispatchOutcome::default());
                }
                self.toplevel_icons
                    .get_mut(&object)
                    .expect("icon was checked above")
                    .buffers
                    .insert((descriptor.size.width, scale), buffer);
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn snapshot_shm_buffer(
        &self,
        buffer: WaylandBufferId,
    ) -> Result<ShmImage, NativeCompositorError> {
        let BufferDescriptor::Shm(descriptor) = self
            .core
            .buffer(buffer)
            .ok_or_else(|| NativeCompositorError::new("unknown Wayland buffer"))?
        else {
            return Err(NativeCompositorError::new(
                "toplevel icon buffer is not shared memory",
            ));
        };
        let fd = self
            .buffer_files
            .get(&buffer)
            .ok_or_else(|| NativeCompositorError::new("icon SHM buffer has no backing file"))?
            .try_clone()
            .map_err(error)?;
        let file = std::fs::File::from(fd);
        let length = descriptor.stride as usize * descriptor.size.height as usize;
        let mut pixels = vec![0_u8; length];
        let mut read = 0;
        while read < pixels.len() {
            let count = file
                .read_at(&mut pixels[read..], descriptor.offset as u64 + read as u64)
                .map_err(error)?;
            if count == 0 {
                return Err(NativeCompositorError::new(
                    "icon shared-memory buffer ended before its declared extent",
                ));
            }
            read += count;
        }
        Ok(ShmImage {
            descriptor: *descriptor,
            pixels,
        })
    }

    fn dispatch_fractional_scale_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_fractional_scale" {
            return Err(unsupported_request(request));
        }
        let _surface = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
        )?;
        let scale = self.create_resource(
            resource.client(),
            context.client,
            "wp_fractional_scale_v1",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::FractionalScale,
            true,
        )?;
        let preferred = self
            .core
            .outputs
            .values()
            .find(|output| output.enabled)
            .map_or(120, |output| {
                u32::try_from(output.description.scale)
                    .unwrap_or(1)
                    .saturating_mul(120)
            });
        self.post_event(
            scale,
            "wp_fractional_scale_v1",
            "preferred_scale",
            &mut [ffi::wl_argument { u: preferred }],
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_viewporter(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_viewport" {
            return Err(unsupported_request(request));
        }
        let surface = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
        )?;
        if self.viewports.contains_key(&surface) {
            return Err(NativeCompositorError::new(
                "surface already has a viewport object",
            ));
        }
        self.create_resource(
            resource.client(),
            context.client,
            "wp_viewport",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::Viewport(surface),
            true,
        )?;
        self.viewports.insert(surface, NativeViewport::default());
        Ok(DispatchOutcome::default())
    }

    fn dispatch_viewport(
        &mut self,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let viewport = self
            .viewports
            .get_mut(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown wp_viewport"))?;
        match request.message().name.as_str() {
            "set_source" => {
                let values = [
                    request.fixed(0).map_err(error)?,
                    request.fixed(1).map_err(error)?,
                    request.fixed(2).map_err(error)?,
                    request.fixed(3).map_err(error)?,
                ];
                viewport.pending_source = if values == [-256; 4] {
                    Some(None)
                } else {
                    let [x, y, width, height] = values.map(|value| f64::from(value) / 256.0);
                    if x < 0.0 || y < 0.0 || width <= 0.0 || height <= 0.0 {
                        return Err(NativeCompositorError::new(
                            "viewport source rectangle is invalid",
                        ));
                    }
                    Some(Some(ViewportSource {
                        x,
                        y,
                        width,
                        height,
                    }))
                };
            }
            "set_destination" => {
                let width = request.int(0).map_err(error)?;
                let height = request.int(1).map_err(error)?;
                viewport.pending_destination = if width == -1 && height == -1 {
                    Some(None)
                } else if width > 0 && height > 0 && width <= 32_768 && height <= 32_768 {
                    Some(Some(crate::core::SizeI { width, height }))
                } else {
                    return Err(NativeCompositorError::new(
                        "viewport destination size is invalid",
                    ));
                };
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_presentation(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "feedback" {
            return Err(unsupported_request(request));
        }
        let surface = self.surface_from_resource(
            request
                .object(0)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
        )?;
        let object = self.peek_next_object()?;
        self.create_resource(
            resource.client(),
            context.client,
            "wp_presentation_feedback",
            resource.version(),
            request.new_id(1).map_err(error)?,
            ResourceKind::PresentationFeedback(surface),
            true,
        )?;
        self.pending_presentation_feedbacks
            .entry(surface)
            .or_default()
            .push(object);
        Ok(DispatchOutcome::default())
    }

    fn dispatch_activation(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "get_activation_token" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_activation_token_v1",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::ActivationToken(object),
                    true,
                )?;
                self.activation_tokens
                    .insert(object, NativeActivationToken::default());
            }
            "activate" => {
                let token = c_string(request, 0)?;
                let surface =
                    self.surface_from_resource(request.object(1).map_err(error)?.ok_or_else(
                        || NativeCompositorError::new("missing activation surface"),
                    )?)?;
                let Some(grant) = self.activation_grants.remove(&token) else {
                    return Ok(DispatchOutcome::default());
                };
                self.activation_order
                    .retain(|candidate| candidate != &token);
                if grant.authorized
                    && self.core.world.surface(surface).is_some_and(|surface| {
                        surface.snapshot().role == Some(SurfaceRole::XdgToplevel)
                    })
                {
                    self.core.queue_action(CompositorAction::ActivateSurface {
                        surface,
                        application_id: grant.application_id,
                        source_surface: grant.source_surface,
                    });
                }
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_activation_token(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if self
            .activation_tokens
            .get(&object)
            .is_some_and(|token| token.committed)
        {
            return Err(NativeCompositorError::new(
                "activation token was already committed",
            ));
        }
        match request.message().name.as_str() {
            "set_serial" => {
                let serial = request.uint(0).map_err(error)?;
                let seat_resource = request
                    .object(1)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing activation seat"))?;
                let ResourceKind::Seat(seat) = self.resource_kind(seat_resource)? else {
                    return Err(NativeCompositorError::new(
                        "activation serial object is not a wl_seat",
                    ));
                };
                self.activation_tokens
                    .get_mut(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown activation token"))?
                    .serial = Some((seat, serial));
            }
            "set_app_id" => {
                let application_id = c_string(request, 0)?;
                if application_id.len() > 4_096 {
                    return Err(NativeCompositorError::new(
                        "activation application id exceeds 4096 bytes",
                    ));
                }
                self.activation_tokens
                    .get_mut(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown activation token"))?
                    .application_id = Some(application_id);
            }
            "set_surface" => {
                let surface =
                    self.surface_from_resource(request.object(0).map_err(error)?.ok_or_else(
                        || NativeCompositorError::new("missing requesting surface"),
                    )?)?;
                self.activation_tokens
                    .get_mut(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown activation token"))?
                    .surface = Some(surface);
            }
            "commit" => {
                let (serial, application_id, source_surface) = {
                    let token = self
                        .activation_tokens
                        .get_mut(&object)
                        .ok_or_else(|| NativeCompositorError::new("unknown activation token"))?;
                    token.committed = true;
                    (token.serial, token.application_id.clone(), token.surface)
                };
                let authorized = serial.is_some_and(|(seat, serial)| {
                    self.core.seats.contains_key(&seat)
                        && self
                            .core
                            .serials
                            .consume(
                                context.client,
                                serial,
                                &[
                                    crate::compositor_wayland::SerialKind::PointerEnter,
                                    crate::compositor_wayland::SerialKind::PointerButton,
                                    crate::compositor_wayland::SerialKind::KeyboardEnter,
                                    crate::compositor_wayland::SerialKind::KeyboardKey,
                                    crate::compositor_wayland::SerialKind::TouchDown,
                                ],
                                source_surface,
                            )
                            .is_ok()
                });
                let handle = loop {
                    let candidate = activation_token_handle()?;
                    if !self.activation_grants.contains_key(&candidate) {
                        break candidate;
                    }
                };
                const MAX_ACTIVATION_GRANTS: usize = 1_024;
                while self.activation_order.len() >= MAX_ACTIVATION_GRANTS {
                    if let Some(expired) = self.activation_order.pop_front() {
                        self.activation_grants.remove(&expired);
                    }
                }
                self.activation_order.push_back(handle.clone());
                self.activation_grants.insert(
                    handle.clone(),
                    NativeActivationGrant {
                        authorized,
                        application_id,
                        source_surface,
                    },
                );
                let handle = protocol_string(&handle);
                self.post_event(
                    resource,
                    "xdg_activation_token_v1",
                    "done",
                    &mut [ffi::wl_argument { s: handle.as_ptr() }],
                )?;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_session_lock_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "lock" {
            return Err(unsupported_request(request));
        }
        let object = self.peek_next_object()?;
        let lock_resource = self.create_resource(
            resource.client(),
            context.client,
            "ext_session_lock_v1",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::SessionLock(object),
            true,
        )?;
        let denied = self.active_session_lock.is_some();
        self.session_locks.insert(
            object,
            NativeSessionLock {
                client: context.client,
                locked_event_sent: false,
                finished_event_sent: denied,
            },
        );
        if denied {
            self.post_event(lock_resource, "ext_session_lock_v1", "finished", &mut [])?;
        } else {
            self.active_session_lock = Some(object);
            self.core
                .queue_action(CompositorAction::SessionLockRequested(object));
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_session_lock(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let lock = self
            .session_locks
            .get(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown session lock"))?;
        if lock.client != context.client {
            return Err(NativeCompositorError::new(
                "session lock belongs to another client",
            ));
        }
        match request.message().name.as_str() {
            "destroy" => {
                if lock.locked_event_sent {
                    return Err(NativeCompositorError::new(
                        "locked session must use unlock_and_destroy",
                    ));
                }
                if self.active_session_lock == Some(object) {
                    self.active_session_lock = None;
                    self.core
                        .queue_action(CompositorAction::SessionLockCancelled(object));
                }
                Ok(DispatchOutcome {
                    destroy_self: true,
                    ..DispatchOutcome::default()
                })
            }
            "unlock_and_destroy" => {
                if !lock.locked_event_sent {
                    return Err(NativeCompositorError::new(
                        "session cannot unlock before the locked event",
                    ));
                }
                if self.active_session_lock == Some(object) {
                    self.active_session_lock = None;
                }
                self.secure_session_locked = false;
                self.core
                    .queue_action(CompositorAction::SessionUnlockRequested(object));
                Ok(DispatchOutcome {
                    destroy_self: true,
                    ..DispatchOutcome::default()
                })
            }
            "get_lock_surface" => {
                if lock.finished_event_sent {
                    return Err(NativeCompositorError::new(
                        "finished session lock cannot create surfaces",
                    ));
                }
                let surface = self.surface_from_resource(
                    request
                        .object(1)
                        .map_err(error)?
                        .ok_or_else(|| NativeCompositorError::new("missing lock surface"))?,
                )?;
                let output_resource = request
                    .object(2)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing lock output"))?;
                let ResourceKind::Output(output) = self.resource_kind(output_resource)? else {
                    return Err(NativeCompositorError::new(
                        "session-lock target is not a wl_output",
                    ));
                };
                if self
                    .session_lock_surfaces
                    .values()
                    .any(|candidate| candidate.lock == object && candidate.output == output)
                {
                    return Err(NativeCompositorError::new(
                        "session lock already has a surface for this output",
                    ));
                }
                let candidate = self
                    .core
                    .world
                    .surface(surface)
                    .ok_or_else(|| NativeCompositorError::new("unknown lock wl_surface"))?;
                if candidate.snapshot().role.is_some() {
                    return Err(NativeCompositorError::new(
                        "session-lock wl_surface already has a role",
                    ));
                }
                if candidate.snapshot().revision != 1
                    || candidate.snapshot().attachment.is_some()
                    || candidate.pending().attachment.is_some()
                {
                    return Err(NativeCompositorError::new(
                        "session-lock wl_surface was already constructed",
                    ));
                }
                self.surface_mut(surface)?
                    .assign_role(SurfaceRole::SessionLock)
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "ext_session_lock_surface_v1",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::SessionLockSurface(surface),
                    true,
                )?;
                self.session_lock_surfaces.insert(
                    surface,
                    NativeSessionLockSurface {
                        lock: object,
                        output,
                        pending_configures: VecDeque::new(),
                        last_acked: None,
                    },
                );
                self.send_session_lock_configure(surface)?;
                Ok(DispatchOutcome::default())
            }
            _ => Err(unsupported_request(request)),
        }
    }

    fn dispatch_session_lock_surface(
        &mut self,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "ack_configure" {
            return Err(unsupported_request(request));
        }
        let serial = request.uint(0).map_err(error)?;
        let lock_surface = self
            .session_lock_surfaces
            .get_mut(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown session-lock surface"))?;
        if lock_surface
            .last_acked
            .is_some_and(|(acked, _)| acked == serial)
        {
            return Err(NativeCompositorError::new(
                "session-lock configure serial was already acknowledged",
            ));
        }
        let index = lock_surface
            .pending_configures
            .iter()
            .position(|(candidate, _)| *candidate == serial)
            .ok_or_else(|| NativeCompositorError::new("invalid session-lock configure serial"))?;
        let acknowledged = lock_surface.pending_configures[index];
        lock_surface.pending_configures.drain(..=index);
        lock_surface.last_acked = Some(acknowledged);
        Ok(DispatchOutcome::default())
    }

    fn send_session_lock_configure(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Result<(), NativeCompositorError> {
        let output = self
            .session_lock_surfaces
            .get(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown session-lock surface"))?
            .output;
        let size = self.output_logical_size(output)?;
        let serial = unsafe { ffi::wl_display_next_serial(self.display.as_ptr()) };
        if serial == 0 {
            return Err(NativeCompositorError::new(
                "libwayland returned a zero lock configure serial",
            ));
        }
        let lock_surface = self
            .session_lock_surfaces
            .get_mut(&surface)
            .expect("checked above");
        while lock_surface.pending_configures.len() >= 64 {
            lock_surface.pending_configures.pop_front();
        }
        lock_surface.pending_configures.push_back((serial, size));
        let resource = self
            .resource_for_kind(
                |kind| matches!(kind, ResourceKind::SessionLockSurface(candidate) if candidate == surface),
            )?
            .ok_or_else(|| NativeCompositorError::new("lock-surface resource is absent"))?;
        self.post_event(
            resource,
            "ext_session_lock_surface_v1",
            "configure",
            &mut [
                ffi::wl_argument { u: serial },
                ffi::wl_argument {
                    u: size.width as u32,
                },
                ffi::wl_argument {
                    u: size.height as u32,
                },
            ],
        )
    }

    fn output_logical_size(
        &self,
        output: u32,
    ) -> Result<crate::core::SizeI, NativeCompositorError> {
        let output = self
            .core
            .outputs
            .get(&output)
            .ok_or_else(|| NativeCompositorError::new("unknown session-lock output"))?;
        let mode = output
            .description
            .modes
            .get(output.current_mode)
            .ok_or_else(|| NativeCompositorError::new("output has no current mode"))?;
        let transformed = match output.description.transform {
            crate::compositor_wayland::OutputTransform::Rotate90
            | crate::compositor_wayland::OutputTransform::Rotate270
            | crate::compositor_wayland::OutputTransform::Flipped90
            | crate::compositor_wayland::OutputTransform::Flipped270 => crate::core::SizeI {
                width: mode.size.height,
                height: mode.size.width,
            },
            _ => mode.size,
        };
        Ok(crate::core::SizeI {
            width: transformed.width / output.description.scale,
            height: transformed.height / output.description.scale,
        })
    }

    fn session_lock_frame_presented(
        &mut self,
        object: ProtocolObjectId,
    ) -> Result<(), NativeCompositorError> {
        if self.active_session_lock != Some(object) {
            return Err(NativeCompositorError::new(
                "presented session lock is not active",
            ));
        }
        let lock = self
            .session_locks
            .get(&object)
            .ok_or_else(|| NativeCompositorError::new("session lock object is absent"))?;
        if lock.finished_event_sent {
            return Err(NativeCompositorError::new("session lock was denied"));
        }
        if lock.locked_event_sent {
            return Ok(());
        }
        let resource = self
            .resource_for_kind(
                |kind| matches!(kind, ResourceKind::SessionLock(candidate) if candidate == object),
            )?
            .ok_or_else(|| NativeCompositorError::new("session lock resource is absent"))?;
        self.post_event(resource, "ext_session_lock_v1", "locked", &mut [])?;
        self.session_locks
            .get_mut(&object)
            .expect("checked above")
            .locked_event_sent = true;
        self.secure_session_locked = true;
        Ok(())
    }

    fn dispatch_relative_pointer_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_relative_pointer" {
            return Err(unsupported_request(request));
        }
        let pointer = request
            .object(1)
            .map_err(error)?
            .ok_or_else(|| NativeCompositorError::new("missing wl_pointer"))?;
        let ResourceKind::Pointer(seat) = self.resource_kind(pointer)? else {
            return Err(NativeCompositorError::new(
                "relative-pointer target is not a wl_pointer",
            ));
        };
        self.create_resource(
            resource.client(),
            context.client,
            "zwp_relative_pointer_v1",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::RelativePointer(seat),
            true,
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_idle_inhibit_manager(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "create_inhibitor" {
            return Err(unsupported_request(request));
        }
        let surface = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
        )?;
        let object = self.peek_next_object()?;
        self.create_resource(
            resource.client(),
            context.client,
            "zwp_idle_inhibitor_v1",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::IdleInhibitor(object),
            true,
        )?;
        self.idle_inhibitors.insert(object, surface);
        Ok(DispatchOutcome::default())
    }

    fn dispatch_pointer_constraints(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let kind = match request.message().name.as_str() {
            "lock_pointer" => PointerConstraintKind::Locked,
            "confine_pointer" => PointerConstraintKind::Confined,
            _ => return Err(unsupported_request(request)),
        };
        let surface = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing constraint surface"))?,
        )?;
        let pointer = request
            .object(2)
            .map_err(error)?
            .ok_or_else(|| NativeCompositorError::new("missing wl_pointer"))?;
        let ResourceKind::Pointer(seat) = self.resource_kind(pointer)? else {
            return Err(NativeCompositorError::new(
                "constraint target is not a wl_pointer",
            ));
        };
        if self.pointer_constraints.values().any(|constraint| {
            constraint.seat == seat && constraint.surface == surface && !constraint.finished
        }) {
            return Err(NativeCompositorError::new(
                "pointer already has a constraint for this surface",
            ));
        }
        let region = request
            .object(3)
            .map_err(error)?
            .map(|resource| self.region_from_resource(resource))
            .transpose()?;
        let persistent = match request.uint(4).map_err(error)? {
            1 => false,
            2 => true,
            _ => return Err(NativeCompositorError::new("invalid constraint lifetime")),
        };
        let object = self.peek_next_object()?;
        let (interface, resource_kind) = match kind {
            PointerConstraintKind::Locked => {
                ("zwp_locked_pointer_v1", ResourceKind::LockedPointer(object))
            }
            PointerConstraintKind::Confined => (
                "zwp_confined_pointer_v1",
                ResourceKind::ConfinedPointer(object),
            ),
        };
        self.create_resource(
            resource.client(),
            context.client,
            interface,
            1,
            request.new_id(0).map_err(error)?,
            resource_kind,
            true,
        )?;
        self.pointer_constraints.insert(
            object,
            NativePointerConstraint {
                seat,
                surface,
                kind,
                region,
                cursor_hint: None,
                persistent,
                active: false,
                finished: false,
            },
        );
        let focus = self
            .core
            .seats
            .get(&seat)
            .and_then(|seat| seat.pointer_focus)
            .map(|focus| focus.surface);
        self.update_pointer_constraints(seat, focus)?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_locked_pointer(
        &mut self,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let region = if request.message().name == "set_region" {
            Some(
                request
                    .object(0)
                    .map_err(error)?
                    .map(|resource| self.region_from_resource(resource))
                    .transpose()?,
            )
        } else {
            None
        };
        let constraint = self
            .pointer_constraints
            .get_mut(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown locked pointer"))?;
        match request.message().name.as_str() {
            "set_region" => constraint.region = region.expect("set above"),
            "set_cursor_position_hint" => {
                constraint.cursor_hint = Some(crate::core::PointF {
                    x: request.fixed(0).map_err(error)? as f32 / 256.0,
                    y: request.fixed(1).map_err(error)? as f32 / 256.0,
                });
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_confined_pointer(
        &mut self,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "set_region" {
            return Err(unsupported_request(request));
        }
        let region = request
            .object(0)
            .map_err(error)?
            .map(|resource| self.region_from_resource(resource))
            .transpose()?;
        self.pointer_constraints
            .get_mut(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown confined pointer"))?
            .region = region;
        Ok(DispatchOutcome::default())
    }

    fn update_pointer_constraints(
        &mut self,
        seat: u32,
        focus: Option<WaylandSurfaceId>,
    ) -> Result<(), NativeCompositorError> {
        let mut transitions = Vec::new();
        for (object, constraint) in &mut self.pointer_constraints {
            if constraint.seat != seat || constraint.finished {
                continue;
            }
            let activate = focus == Some(constraint.surface)
                && constraint
                    .region
                    .as_ref()
                    .is_none_or(|region| !region.rectangles().is_empty());
            if activate != constraint.active {
                constraint.active = activate;
                if !activate && !constraint.persistent {
                    constraint.finished = true;
                }
                transitions.push((*object, constraint.kind, activate));
            }
        }
        for (object, kind, active) in transitions {
            let resource = self
                .resource_for_kind(|candidate| match (candidate, kind) {
                    (ResourceKind::LockedPointer(candidate), PointerConstraintKind::Locked)
                    | (
                        ResourceKind::ConfinedPointer(candidate),
                        PointerConstraintKind::Confined,
                    ) => candidate == object,
                    _ => false,
                })?
                .ok_or_else(|| NativeCompositorError::new("pointer constraint is absent"))?;
            let (interface, event) = match (kind, active) {
                (PointerConstraintKind::Locked, true) => ("zwp_locked_pointer_v1", "locked"),
                (PointerConstraintKind::Locked, false) => ("zwp_locked_pointer_v1", "unlocked"),
                (PointerConstraintKind::Confined, true) => ("zwp_confined_pointer_v1", "confined"),
                (PointerConstraintKind::Confined, false) => {
                    ("zwp_confined_pointer_v1", "unconfined")
                }
            };
            self.post_event(resource, interface, event, &mut [])?;
        }
        Ok(())
    }

    fn dispatch_explicit_synchronization(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_synchronization" {
            return Err(unsupported_request(request));
        }
        let surface = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
        )?;
        if !self.synchronized_surfaces.insert(surface) {
            return Err(NativeCompositorError::new(
                "surface already has an explicit-synchronization object",
            ));
        }
        self.create_resource(
            resource.client(),
            context.client,
            "zwp_linux_surface_synchronization_v1",
            resource.version(),
            request.new_id(0).map_err(error)?,
            ResourceKind::SurfaceSynchronization(surface),
            true,
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_surface_synchronization(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        surface: WaylandSurfaceId,
        request: &mut IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "set_acquire_fence" => {
                if self.pending_acquire_fences.contains_key(&surface) {
                    return Err(NativeCompositorError::new(
                        "acquire fence was already set for the pending commit",
                    ));
                }
                self.pending_acquire_fences
                    .insert(surface, request.take_fd(0).map_err(error)?);
            }
            "get_release" => {
                if self.pending_releases.contains_key(&surface) {
                    return Err(NativeCompositorError::new(
                        "buffer release was already requested for the pending commit",
                    ));
                }
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "zwp_linux_buffer_release_v1",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::ExplicitBufferRelease(surface),
                    true,
                )?;
                self.pending_releases.insert(surface, object);
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_linux_buffer_params(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        object: ProtocolObjectId,
        request: &mut IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "add" => {
                let plane = NativeDmaBufPlane {
                    fd: request.take_fd(0).map_err(error)?,
                    offset: request.uint(2).map_err(error)?,
                    stride: request.uint(3).map_err(error)?,
                    modifier: (u64::from(request.uint(4).map_err(error)?) << 32)
                        | u64::from(request.uint(5).map_err(error)?),
                };
                let index = request.uint(1).map_err(error)?;
                let params = self
                    .dmabuf_params
                    .get_mut(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown DMA-BUF parameters"))?;
                if params.used || index >= 4 || plane.stride == 0 {
                    return Err(NativeCompositorError::new("invalid DMA-BUF plane"));
                }
                if params.planes.insert(index, plane).is_some() {
                    return Err(NativeCompositorError::new("duplicate DMA-BUF plane index"));
                }
            }
            "create" | "create_immed" => {
                let immediate = request.message().name == "create_immed";
                let base = usize::from(immediate);
                let wire_id = if immediate {
                    request.new_id(0).map_err(error)?
                } else {
                    0
                };
                let result = self.finish_dma_buf(
                    resource,
                    context,
                    object,
                    wire_id,
                    request.int(base).map_err(error)?,
                    request.int(base + 1).map_err(error)?,
                    request.uint(base + 2).map_err(error)?,
                    request.uint(base + 3).map_err(error)?,
                );
                match result {
                    Ok(buffer_resource) if !immediate => {
                        self.post_event(
                            resource,
                            "zwp_linux_buffer_params_v1",
                            "created",
                            &mut [ffi::wl_argument {
                                o: buffer_resource.identity() as *mut ffi::wl_resource,
                            }],
                        )?;
                    }
                    Ok(_) => {}
                    Err(_) if !immediate => {
                        self.post_event(resource, "zwp_linux_buffer_params_v1", "failed", &mut [])?;
                    }
                    Err(error) => return Err(error),
                }
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_dma_buf<'client>(
        &mut self,
        params_resource: ResourceRef<'client>,
        context: &ResourceContext,
        object: ProtocolObjectId,
        wire_id: u32,
        width: i32,
        height: i32,
        format: u32,
        flags: u32,
    ) -> Result<ResourceRef<'client>, NativeCompositorError> {
        let params = self
            .dmabuf_params
            .get_mut(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown DMA-BUF parameters"))?;
        if params.used {
            return Err(NativeCompositorError::new(
                "DMA-BUF parameters are single-use",
            ));
        }
        params.used = true;
        if width <= 0 || height <= 0 || flags & !0x7 != 0 || params.planes.is_empty() {
            return Err(NativeCompositorError::new("invalid DMA-BUF metadata"));
        }
        let modifier = params
            .planes
            .first_key_value()
            .map(|(_, plane)| plane.modifier)
            .expect("planes checked");
        if params.planes.iter().any(|(index, plane)| {
            *index as usize >= params.planes.len() || plane.modifier != modifier
        }) || !self.dmabuf_formats.contains(&DmaBufFormat {
            fourcc: format,
            modifier,
        }) {
            return Err(NativeCompositorError::new(
                "unsupported or non-contiguous DMA-BUF plane layout",
            ));
        }
        self.next_buffer = next_nonzero(self.next_buffer)?;
        let buffer = WaylandBufferId::from_raw(self.next_buffer).expect("nonzero");
        let planes = std::mem::take(&mut params.planes);
        let descriptor_planes = planes
            .iter()
            .map(|(index, plane)| crate::compositor_wayland::DmaBufPlane {
                index: *index as u8,
                fd_token: u64::try_from(plane.fd.as_raw_fd()).unwrap_or(1).max(1),
                offset: plane.offset,
                stride: plane.stride,
                modifier: plane.modifier,
            })
            .collect();
        let descriptor = crate::compositor_wayland::DmaBufDescriptor::new(
            crate::core::SizeI { width, height },
            format,
            crate::compositor_wayland::DmaBufFlags {
                y_invert: flags & 1 != 0,
                interlaced: flags & 2 != 0,
                bottom_field_first: flags & 4 != 0,
            },
            descriptor_planes,
        )
        .map_err(error)?;
        let files = planes.into_values().map(|plane| plane.fd).collect();
        self.core
            .register_buffer(context.client, buffer, BufferDescriptor::DmaBuf(descriptor))
            .map_err(error)?;
        let buffer_resource = self.create_resource(
            params_resource.client(),
            context.client,
            "wl_buffer",
            1,
            wire_id,
            ResourceKind::Buffer(buffer),
            true,
        )?;
        self.dmabuf_files.insert(buffer, files);
        Ok(buffer_resource)
    }

    fn dispatch_compositor(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "create_surface" => {
                self.next_surface = next_nonzero(self.next_surface)?;
                let surface = WaylandSurfaceId::from_raw(self.next_surface).expect("nonzero");
                self.core
                    .world
                    .create_surface(context.client, surface)
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_surface",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::Surface(surface),
                    true,
                )?;
            }
            "create_region" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_region",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::Region(object),
                    true,
                )?;
                self.regions.insert(object, Vec::new());
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_surface(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "attach" => {
                let buffer = request
                    .object(0)
                    .map_err(error)?
                    .map(|buffer| self.resource_kind(buffer))
                    .transpose()?
                    .map(|kind| match kind {
                        ResourceKind::Buffer(buffer) => Ok(buffer),
                        _ => Err(NativeCompositorError::new(
                            "wl_surface.attach object is not a buffer",
                        )),
                    })
                    .transpose()?;
                let offset = PointI {
                    x: request.int(1).map_err(error)?,
                    y: request.int(2).map_err(error)?,
                };
                if resource.version() >= 5 && offset != PointI::default() {
                    return Err(NativeCompositorError::new(
                        "wl_surface v5 attach offset must be zero",
                    ));
                }
                self.surface_mut(surface)?
                    .attach(buffer.map(|buffer| BufferAttachment { buffer, offset }));
            }
            "damage" | "damage_buffer" => {
                self.surface_mut(surface)?
                    .damage(RectI {
                        x: request.int(0).map_err(error)?,
                        y: request.int(1).map_err(error)?,
                        width: request.int(2).map_err(error)?,
                        height: request.int(3).map_err(error)?,
                    })
                    .map_err(error)?;
            }
            "frame" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_callback",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::Callback(surface),
                    true,
                )?;
                self.callbacks.entry(surface).or_default().push(object);
            }
            "set_opaque_region" | "set_input_region" => {
                let region = request
                    .object(0)
                    .map_err(error)?
                    .map(|region| self.region_from_resource(region))
                    .transpose()?;
                if request.message().name == "set_opaque_region" {
                    self.surface_mut(surface)?.set_opaque_region(region);
                } else {
                    self.surface_mut(surface)?.set_input_region(region);
                }
            }
            "set_buffer_transform" => {
                let transform = match request.int(0).map_err(error)? {
                    0 => BufferTransform::Normal,
                    1 => BufferTransform::Rotate90,
                    2 => BufferTransform::Rotate180,
                    3 => BufferTransform::Rotate270,
                    4 => BufferTransform::Flipped,
                    5 => BufferTransform::Flipped90,
                    6 => BufferTransform::Flipped180,
                    7 => BufferTransform::Flipped270,
                    _ => return Err(NativeCompositorError::new("invalid buffer transform")),
                };
                self.surface_mut(surface)?.set_buffer_transform(transform);
            }
            "set_buffer_scale" => self
                .surface_mut(surface)?
                .set_buffer_scale(request.int(0).map_err(error)?)
                .map_err(error)?,
            "offset" => {
                // wl_surface.offset is represented by the next attachment offset in the current
                // Telorgon surface profile. A pending attachment is required for it to take effect.
                let offset = PointI {
                    x: request.int(0).map_err(error)?,
                    y: request.int(1).map_err(error)?,
                };
                let pending = self.surface_mut(surface)?.pending().attachment.flatten();
                if let Some(mut attachment) = pending {
                    attachment.offset = offset;
                    self.surface_mut(surface)?.attach(Some(attachment));
                }
            }
            "commit" => return self.commit_surface(surface),
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_region(
        &mut self,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let rectangle = RectI {
            x: request.int(0).map_err(error)?,
            y: request.int(1).map_err(error)?,
            width: request.int(2).map_err(error)?,
            height: request.int(3).map_err(error)?,
        };
        if rectangle.width <= 0 || rectangle.height <= 0 {
            return Err(NativeCompositorError::new(
                "region rectangle must be positive",
            ));
        }
        let rectangles = self
            .regions
            .get_mut(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown region"))?;
        match request.message().name.as_str() {
            "add" => {
                if rectangles.len() >= Region::MAX_RECTANGLES {
                    return Err(NativeCompositorError::new(
                        "region rectangle limit exceeded",
                    ));
                }
                rectangles.push(rectangle);
            }
            "subtract" => {
                let mut difference = Vec::with_capacity(rectangles.len().saturating_mul(2));
                for current in rectangles.drain(..) {
                    difference.extend(subtract_rectangle(current, rectangle));
                    if difference.len() > Region::MAX_RECTANGLES {
                        return Err(NativeCompositorError::new(
                            "region rectangle limit exceeded by subtraction",
                        ));
                    }
                }
                *rectangles = difference;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_shm(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &mut IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "create_pool" {
            return Err(unsupported_request(request));
        }
        let object = self.peek_next_object()?;
        let pool = ShmPool::new(
            request.int(2).map_err(error)?,
            ClientLimits::default().maximum_buffer_bytes,
        )
        .map_err(error)?;
        let fd = request.take_fd(1).map_err(error)?;
        if fd_size(&fd).map_err(error)? < pool.size as u64 {
            return Err(NativeCompositorError::new(
                "shared-memory pool size exceeds the backing file",
            ));
        }
        self.create_resource(
            resource.client(),
            context.client,
            "wl_shm_pool",
            resource.version(),
            request.new_id(0).map_err(error)?,
            ResourceKind::ShmPool(object),
            true,
        )?;
        self.shm_pools.insert(
            object,
            NativeShmPool {
                owner: context.client,
                fd,
                pool,
            },
        );
        Ok(DispatchOutcome::default())
    }

    fn dispatch_shm_pool(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "resize" => {
                let new_size = request.int(0).map_err(error)?;
                let pool = self
                    .shm_pools
                    .get_mut(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown SHM pool"))?;
                if fd_size(&pool.fd).map_err(error)? < new_size.max(0) as u64 {
                    return Err(NativeCompositorError::new(
                        "resized shared-memory pool exceeds the backing file",
                    ));
                }
                pool.pool
                    .resize(new_size, ClientLimits::default().maximum_buffer_bytes)
                    .map_err(error)?;
            }
            "create_buffer" => {
                let pool = self
                    .shm_pools
                    .get(&object)
                    .ok_or_else(|| NativeCompositorError::new("unknown SHM pool"))?;
                if pool.owner != context.client {
                    return Err(NativeCompositorError::new("SHM pool ownership mismatch"));
                }
                let format = match request.uint(5).map_err(error)? {
                    0 => ShmFormat::Argb8888,
                    1 => ShmFormat::Xrgb8888,
                    value => ShmFormat::Other(value),
                };
                let descriptor = ShmBuffer::new(
                    pool.pool,
                    request.int(1).map_err(error)?,
                    request.int(2).map_err(error)?,
                    request.int(3).map_err(error)?,
                    request.int(4).map_err(error)?,
                    format,
                )
                .map_err(error)?;
                let fd = pool.fd.try_clone().map_err(error)?;
                self.next_buffer = next_nonzero(self.next_buffer)?;
                let buffer = WaylandBufferId::from_raw(self.next_buffer).expect("nonzero");
                self.core
                    .register_buffer(context.client, buffer, BufferDescriptor::Shm(descriptor))
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "wl_buffer",
                    1,
                    request.new_id(0).map_err(error)?,
                    ResourceKind::Buffer(buffer),
                    true,
                )?;
                self.buffer_files.insert(buffer, fd);
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_subcompositor(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if request.message().name != "get_subsurface" {
            return Err(unsupported_request(request));
        }
        let child = self.surface_from_resource(
            request
                .object(1)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing child surface"))?,
        )?;
        let parent = self.surface_from_resource(
            request
                .object(2)
                .map_err(error)?
                .ok_or_else(|| NativeCompositorError::new("missing parent surface"))?,
        )?;
        self.surface_mut(child)?
            .assign_role(SurfaceRole::Subsurface)
            .map_err(error)?;
        self.core.subsurfaces.add(child, parent).map_err(error)?;
        self.create_resource(
            resource.client(),
            context.client,
            "wl_subsurface",
            1,
            request.new_id(0).map_err(error)?,
            ResourceKind::Subsurface(child),
            true,
        )?;
        Ok(DispatchOutcome::default())
    }

    fn dispatch_subsurface(
        &mut self,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "set_position" => self
                .core
                .subsurfaces
                .set_position(
                    surface,
                    crate::compositor_wayland::SubsurfacePosition {
                        offset: PointI {
                            x: request.int(0).map_err(error)?,
                            y: request.int(1).map_err(error)?,
                        },
                        above: None,
                    },
                )
                .map_err(error)?,
            "set_sync" => {
                self.core
                    .subsurfaces
                    .set_synchronized(surface, true)
                    .map_err(error)?;
            }
            "set_desync" => {
                if let Some(commit) = self
                    .core
                    .subsurfaces
                    .set_synchronized(surface, false)
                    .map_err(error)?
                {
                    self.surface_mut(surface)?.stage(commit).map_err(error)?;
                    self.commit_surface(surface)?;
                }
            }
            "place_above" | "place_below" => {
                // Sibling ordering is policy-visible; exact ordering is applied by the shell scene.
                let sibling = self.surface_from_resource(
                    request
                        .object(0)
                        .map_err(error)?
                        .ok_or_else(|| NativeCompositorError::new("missing sibling"))?,
                )?;
                let position = crate::compositor_wayland::SubsurfacePosition {
                    offset: PointI::default(),
                    above: (request.message().name == "place_above").then_some(sibling),
                };
                self.core
                    .subsurfaces
                    .set_position(surface, position)
                    .map_err(error)?;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_xdg_wm_base(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "create_positioner" => {
                let object = self.peek_next_object()?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_positioner",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::XdgPositioner(object),
                    true,
                )?;
                self.positioners
                    .insert(object, NativeXdgPositioner::default());
            }
            "get_xdg_surface" => {
                let surface = self.surface_from_resource(
                    request
                        .object(1)
                        .map_err(error)?
                        .ok_or_else(|| NativeCompositorError::new("missing wl_surface"))?,
                )?;
                if self.surface_mut(surface)?.snapshot().attachment.is_some() {
                    return Err(NativeCompositorError::new(
                        "xdg_surface was created with a buffer attached",
                    ));
                }
                let object = self.peek_next_object()?;
                self.core
                    .create_xdg_surface(context.client, surface, object, resource.version())
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_surface",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::XdgSurface(surface),
                    false,
                )?;
                self.xdg_resources.insert(surface, object);
            }
            "pong" => {
                let _ = request.uint(0).map_err(error)?;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_xdg_positioner(
        &mut self,
        object: ProtocolObjectId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let positioner = self
            .positioners
            .get_mut(&object)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_positioner"))?;
        match request.message().name.as_str() {
            "set_size" => {
                positioner.size = Some(crate::core::SizeI {
                    width: request.int(0).map_err(error)?,
                    height: request.int(1).map_err(error)?,
                });
            }
            "set_anchor_rect" => {
                positioner.anchor_rect = Some(RectI {
                    x: request.int(0).map_err(error)?,
                    y: request.int(1).map_err(error)?,
                    width: request.int(2).map_err(error)?,
                    height: request.int(3).map_err(error)?,
                });
            }
            "set_anchor" => positioner.anchor = request.uint(0).map_err(error)?,
            "set_gravity" => positioner.gravity = request.uint(0).map_err(error)?,
            "set_constraint_adjustment" => {
                positioner.constraint_adjustment = request.uint(0).map_err(error)?;
            }
            "set_offset" => {
                positioner.offset = PointI {
                    x: request.int(0).map_err(error)?,
                    y: request.int(1).map_err(error)?,
                };
            }
            "set_reactive" => positioner.reactive = true,
            "set_parent_size" => {
                positioner.parent_size = Some(crate::core::SizeI {
                    width: request.int(0).map_err(error)?,
                    height: request.int(1).map_err(error)?,
                });
            }
            "set_parent_configure" => {
                positioner.parent_configure = Some(request.uint(0).map_err(error)?);
            }
            _ => return Err(unsupported_request(request)),
        }
        // Validate every field that can be checked before the two required fields are complete.
        if positioner.anchor > 8
            || positioner.gravity > 8
            || positioner.constraint_adjustment & !0x3f != 0
        {
            return Err(NativeCompositorError::new(
                "invalid xdg_positioner enum or flags",
            ));
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_xdg_surface(
        &mut self,
        resource: ResourceRef<'_>,
        context: &ResourceContext,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "get_toplevel" => {
                self.surface_mut(surface)?
                    .assign_role(SurfaceRole::XdgToplevel)
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_toplevel",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::XdgToplevel(surface),
                    true,
                )?;
                self.toplevels.insert(surface, XdgToplevelState::default());
            }
            "ack_configure" => {
                self.core
                    .xdg_surface_mut(surface)
                    .ok_or_else(|| NativeCompositorError::new("unknown xdg_surface"))?
                    .ack_configure(request.uint(0).map_err(error)?)
                    .map_err(error)?;
            }
            "set_window_geometry" => self
                .core
                .xdg_surface_mut(surface)
                .ok_or_else(|| NativeCompositorError::new("unknown xdg_surface"))?
                .set_window_geometry(RectI {
                    x: request.int(0).map_err(error)?,
                    y: request.int(1).map_err(error)?,
                    width: request.int(2).map_err(error)?,
                    height: request.int(3).map_err(error)?,
                })
                .map_err(error)?,
            "get_popup" => {
                let parent = request
                    .object(1)
                    .map_err(error)?
                    .map(|resource| match self.resource_kind(resource)? {
                        ResourceKind::XdgSurface(parent) => Ok(parent),
                        _ => Err(NativeCompositorError::new(
                            "xdg_popup parent is not an xdg_surface",
                        )),
                    })
                    .transpose()?;
                let positioner_resource = request
                    .object(2)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing xdg_positioner"))?;
                let ResourceKind::XdgPositioner(positioner_object) =
                    self.resource_kind(positioner_resource)?
                else {
                    return Err(NativeCompositorError::new(
                        "popup positioner is not an xdg_positioner",
                    ));
                };
                let positioner = self
                    .positioners
                    .get(&positioner_object)
                    .copied()
                    .ok_or_else(|| NativeCompositorError::new("unknown xdg_positioner"))?
                    .finish()?;
                self.surface_mut(surface)?
                    .assign_role(SurfaceRole::XdgPopup)
                    .map_err(error)?;
                self.create_resource(
                    resource.client(),
                    context.client,
                    "xdg_popup",
                    resource.version(),
                    request.new_id(0).map_err(error)?,
                    ResourceKind::XdgPopup(surface),
                    true,
                )?;
                self.popups.insert(
                    surface,
                    crate::compositor_wayland::XdgPopupState {
                        parent,
                        positioner,
                        grabbed: false,
                        reposition_token: None,
                    },
                );
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_xdg_popup(
        &mut self,
        context: &ResourceContext,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        match request.message().name.as_str() {
            "grab" => {
                let serial = request.uint(1).map_err(error)?;
                self.core
                    .serials
                    .consume(
                        context.client,
                        serial,
                        &[
                            crate::compositor_wayland::SerialKind::PointerButton,
                            crate::compositor_wayland::SerialKind::TouchDown,
                        ],
                        self.popups.get(&surface).and_then(|popup| popup.parent),
                    )
                    .map_err(error)?;
                self.popups
                    .get_mut(&surface)
                    .ok_or_else(|| NativeCompositorError::new("unknown xdg_popup"))?
                    .grabbed = true;
            }
            "reposition" => {
                let positioner_resource = request
                    .object(0)
                    .map_err(error)?
                    .ok_or_else(|| NativeCompositorError::new("missing xdg_positioner"))?;
                let ResourceKind::XdgPositioner(positioner_object) =
                    self.resource_kind(positioner_resource)?
                else {
                    return Err(NativeCompositorError::new(
                        "popup reposition object is not an xdg_positioner",
                    ));
                };
                let positioner = self
                    .positioners
                    .get(&positioner_object)
                    .copied()
                    .ok_or_else(|| NativeCompositorError::new("unknown xdg_positioner"))?
                    .finish()?;
                let token = request.uint(1).map_err(error)?;
                let popup = self
                    .popups
                    .get_mut(&surface)
                    .ok_or_else(|| NativeCompositorError::new("unknown xdg_popup"))?;
                popup.positioner = positioner;
                popup.reposition_token = Some(token);
                self.send_popup_configure(surface, Some(token))?;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn dispatch_xdg_toplevel(
        &mut self,
        context: &ResourceContext,
        surface: WaylandSurfaceId,
        request: &IncomingRequest<'_>,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        if !self.toplevels.contains_key(&surface) {
            return Err(NativeCompositorError::new("unknown xdg_toplevel"));
        }
        match request.message().name.as_str() {
            "set_title" => self
                .toplevels
                .get_mut(&surface)
                .expect("checked above")
                .set_title(c_string(request, 0)?)
                .map_err(error)?,
            "set_app_id" => self
                .toplevels
                .get_mut(&surface)
                .expect("checked above")
                .set_application_id(c_string(request, 0)?)
                .map_err(error)?,
            "set_min_size" => {
                let minimum = crate::core::SizeI {
                    width: request.int(0).map_err(error)?,
                    height: request.int(1).map_err(error)?,
                };
                let maximum = self
                    .toplevels
                    .get(&surface)
                    .expect("checked above")
                    .maximum_size;
                self.toplevels
                    .get_mut(&surface)
                    .expect("checked above")
                    .set_size_constraints(Some(minimum), maximum)
                    .map_err(error)?;
            }
            "set_max_size" => {
                let maximum = crate::core::SizeI {
                    width: request.int(0).map_err(error)?,
                    height: request.int(1).map_err(error)?,
                };
                let minimum = self
                    .toplevels
                    .get(&surface)
                    .expect("checked above")
                    .minimum_size;
                self.toplevels
                    .get_mut(&surface)
                    .expect("checked above")
                    .set_size_constraints(minimum, Some(maximum))
                    .map_err(error)?;
            }
            "move" | "resize" => {
                // Both requests carry `(seat, serial, ...)`; resize alone appends its edge.
                let serial = request.uint(1).map_err(error)?;
                self.core
                    .serials
                    .consume(
                        context.client,
                        serial,
                        &[crate::compositor_wayland::SerialKind::PointerButton],
                        Some(surface),
                    )
                    .map_err(error)?;
                if request.message().name == "move" {
                    self.core
                        .queue_action(CompositorAction::MoveToplevel(surface));
                } else {
                    let edge = resize_edge(request.uint(2).map_err(error)?)?;
                    self.core
                        .queue_action(CompositorAction::ResizeToplevel { surface, edge });
                }
            }
            "set_maximized" => {
                self.core.queue_action(CompositorAction::MaximizeToplevel {
                    surface,
                    maximized: true,
                });
            }
            "unset_maximized" => {
                self.core.queue_action(CompositorAction::MaximizeToplevel {
                    surface,
                    maximized: false,
                });
            }
            "set_fullscreen" => {
                let output = request
                    .object(0)
                    .map_err(error)?
                    .map(|resource| match self.resource_kind(resource)? {
                        ResourceKind::Output(output) => Ok(output),
                        _ => Err(NativeCompositorError::new(
                            "fullscreen target is not a wl_output",
                        )),
                    })
                    .transpose()?;
                self.core
                    .queue_action(CompositorAction::FullscreenToplevel {
                        surface,
                        fullscreen: true,
                        output,
                    });
            }
            "unset_fullscreen" => {
                self.core
                    .queue_action(CompositorAction::FullscreenToplevel {
                        surface,
                        fullscreen: false,
                        output: None,
                    });
            }
            "set_minimized" => {
                self.core
                    .queue_action(CompositorAction::MinimizeToplevel(surface));
            }
            "set_parent" => {
                let parent = request
                    .object(0)
                    .map_err(error)?
                    .map(|resource| match self.resource_kind(resource)? {
                        ResourceKind::XdgToplevel(parent) => Ok(parent),
                        _ => Err(NativeCompositorError::new(
                            "toplevel parent is not an xdg_toplevel",
                        )),
                    })
                    .transpose()?;
                let mut ancestor = parent;
                let mut visited = BTreeSet::new();
                while let Some(candidate) = ancestor {
                    if candidate == surface || !visited.insert(candidate) {
                        return Err(NativeCompositorError::new(
                            "toplevel parent relationship would form a cycle",
                        ));
                    }
                    ancestor = self
                        .toplevels
                        .get(&candidate)
                        .ok_or_else(|| NativeCompositorError::new("unknown toplevel parent"))?
                        .parent;
                }
                self.toplevels
                    .get_mut(&surface)
                    .expect("checked above")
                    .parent = parent;
            }
            "show_window_menu" => {
                let serial = request.uint(1).map_err(error)?;
                self.core
                    .serials
                    .consume(
                        context.client,
                        serial,
                        &[crate::compositor_wayland::SerialKind::PointerButton],
                        Some(surface),
                    )
                    .map_err(error)?;
            }
            _ => return Err(unsupported_request(request)),
        }
        Ok(DispatchOutcome::default())
    }

    fn commit_surface(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Result<DispatchOutcome, NativeCompositorError> {
        let pending_buffer = self
            .surface_mut(surface)?
            .pending()
            .attachment
            .flatten()
            .map(|attachment| attachment.buffer);
        if let Some(xdg) = self.core.xdg_surface_mut(surface) {
            xdg.validate_buffer_commit(pending_buffer.is_some())
                .map_err(error)?;
        }
        if let Some(lock_surface) = self.session_lock_surfaces.get(&surface) {
            if lock_surface.last_acked.is_none() {
                return Err(NativeCompositorError::new(
                    "session-lock surface committed before its first configure ack",
                ));
            }
            let resulting_buffer = match self.surface_mut(surface)?.pending().attachment {
                Some(attachment) => attachment.map(|attachment| attachment.buffer),
                None => self
                    .core
                    .world
                    .surface(surface)
                    .and_then(|surface| surface.snapshot().attachment)
                    .map(|attachment| attachment.buffer),
            };
            if resulting_buffer.is_none() {
                return Err(NativeCompositorError::new(
                    "session-lock surface committed a null buffer",
                ));
            }
        }
        if pending_buffer.is_none()
            && (self.pending_acquire_fences.contains_key(&surface)
                || self.pending_releases.contains_key(&surface))
        {
            return Err(NativeCompositorError::new(
                "explicit synchronization requires a buffer in the same commit",
            ));
        }
        if self.core.subsurfaces.parent(surface).is_some() {
            let pending = self.surface_mut(surface)?.pending().clone();
            if self
                .core
                .subsurfaces
                .stage_or_release(surface, pending)
                .map_err(error)?
                .is_none()
            {
                return Ok(DispatchOutcome::default());
            }
        }
        let outcome = self.surface_mut(surface)?.commit().map_err(error)?;
        if let Some((acknowledged_configure, window_geometry)) = self
            .core
            .xdg_surface_mut(surface)
            .map(|xdg_surface| xdg_surface.commit_state())
        {
            self.surface_mut(surface)?
                .apply_xdg_commit_state(acknowledged_configure, window_geometry);
        }
        if let Some(icon) = self.pending_toplevel_icons.remove(&surface) {
            match icon {
                PendingToplevelIcon::Reset => {
                    self.committed_toplevel_icons.remove(&surface);
                }
                PendingToplevelIcon::Icon(icon) => {
                    self.committed_toplevel_icons.insert(surface, icon);
                }
            }
        }
        self.commit_viewport_state(surface)?;
        if let Some(expected) = self
            .session_lock_surfaces
            .get(&surface)
            .and_then(|surface| surface.last_acked.map(|(_, size)| size))
            && self.surface_logical_size(surface)? != expected
        {
            return Err(NativeCompositorError::new(
                "session-lock surface dimensions do not match its acknowledged configure",
            ));
        }
        self.commit_feedback_state(surface, outcome.revision);
        if let Some(fence) = self.pending_acquire_fences.remove(&surface) {
            self.committed_acquire_fences
                .insert((surface, outcome.revision), fence);
        }
        if let Some(release) = self.pending_releases.remove(&surface) {
            self.committed_releases
                .insert((surface, outcome.revision), release);
        }
        self.core.queue_action(if outcome.mapped {
            CompositorAction::PublishSurface(surface)
        } else {
            CompositorAction::WithdrawSurface(surface)
        });
        if self.core.xdg_surface_mut(surface).is_some()
            && !self.initial_configures.contains(&surface)
            && !outcome.mapped
        {
            self.send_initial_configure(surface)?;
        }
        for (child, commit) in self.core.subsurfaces.release_children(surface) {
            self.surface_mut(child)?.stage(commit).map_err(error)?;
            let child_outcome = self.surface_mut(child)?.commit().map_err(error)?;
            self.commit_viewport_state(child)?;
            self.commit_feedback_state(child, child_outcome.revision);
            self.core.queue_action(if child_outcome.mapped {
                CompositorAction::PublishSurface(child)
            } else {
                CompositorAction::WithdrawSurface(child)
            });
        }
        Ok(DispatchOutcome::default())
    }

    fn commit_feedback_state(&mut self, surface: WaylandSurfaceId, revision: u64) {
        if let Some(callbacks) = self.callbacks.remove(&surface)
            && !callbacks.is_empty()
        {
            self.committed_callbacks
                .entry((surface, revision))
                .or_default()
                .extend(callbacks);
        }
        if let Some(feedbacks) = self.pending_presentation_feedbacks.remove(&surface)
            && !feedbacks.is_empty()
        {
            self.committed_presentation_feedbacks
                .entry((surface, revision))
                .or_default()
                .extend(feedbacks);
        }
    }

    fn commit_viewport_state(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Result<(), NativeCompositorError> {
        let Some(viewport) = self.viewports.get_mut(&surface) else {
            return self.validate_surface_buffer_geometry(surface, None);
        };
        viewport.commit();
        let current = viewport.current;
        self.validate_surface_buffer_geometry(surface, Some(current))
    }

    fn validate_surface_buffer_geometry(
        &self,
        surface: WaylandSurfaceId,
        viewport: Option<ViewportState>,
    ) -> Result<(), NativeCompositorError> {
        let snapshot = self
            .core
            .world
            .surface(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown wl_surface"))?
            .snapshot();
        let Some(attachment) = snapshot.attachment else {
            return Ok(());
        };
        let buffer_size = match self
            .core
            .buffer(attachment.buffer)
            .ok_or_else(|| NativeCompositorError::new("surface buffer is absent"))?
        {
            BufferDescriptor::Shm(buffer) => buffer.size,
            BufferDescriptor::DmaBuf(buffer) => buffer.size,
        };
        let transformed = transformed_size(buffer_size, snapshot.buffer_transform);
        if transformed.width % snapshot.buffer_scale != 0
            || transformed.height % snapshot.buffer_scale != 0
        {
            return Err(NativeCompositorError::new(
                "buffer dimensions are not divisible by wl_surface buffer scale",
            ));
        }
        let natural_width = f64::from(transformed.width / snapshot.buffer_scale);
        let natural_height = f64::from(transformed.height / snapshot.buffer_scale);
        let Some(viewport) = viewport else {
            return Ok(());
        };
        if let Some(source) = viewport.source {
            if source.x + source.width > natural_width || source.y + source.height > natural_height
            {
                return Err(NativeCompositorError::new(
                    "viewport source extends outside the surface buffer",
                ));
            }
            if viewport.destination.is_none()
                && (source.width.fract() != 0.0 || source.height.fract() != 0.0)
            {
                return Err(NativeCompositorError::new(
                    "fractional viewport source requires a destination size",
                ));
            }
        }
        Ok(())
    }

    fn surface_logical_size(
        &self,
        surface: WaylandSurfaceId,
    ) -> Result<crate::core::SizeI, NativeCompositorError> {
        if let Some(destination) = self
            .viewports
            .get(&surface)
            .and_then(|viewport| viewport.current.destination)
        {
            return Ok(destination);
        }
        if let Some(source) = self
            .viewports
            .get(&surface)
            .and_then(|viewport| viewport.current.source)
        {
            return Ok(crate::core::SizeI {
                width: source.width as i32,
                height: source.height as i32,
            });
        }
        let snapshot = self
            .core
            .world
            .surface(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown wl_surface"))?
            .snapshot();
        let attachment = snapshot
            .attachment
            .ok_or_else(|| NativeCompositorError::new("surface has no buffer"))?;
        let buffer_size = match self
            .core
            .buffer(attachment.buffer)
            .ok_or_else(|| NativeCompositorError::new("surface buffer is absent"))?
        {
            BufferDescriptor::Shm(buffer) => buffer.size,
            BufferDescriptor::DmaBuf(buffer) => buffer.size,
        };
        let transformed = transformed_size(buffer_size, snapshot.buffer_transform);
        Ok(crate::core::SizeI {
            width: transformed.width / snapshot.buffer_scale,
            height: transformed.height / snapshot.buffer_scale,
        })
    }

    fn send_initial_configure(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Result<(), NativeCompositorError> {
        let serial = unsafe { ffi::wl_display_next_serial(self.display.as_ptr()) };
        if serial == 0 {
            return Err(NativeCompositorError::new(
                "libwayland returned a zero configure serial",
            ));
        }
        self.core
            .xdg_surface_mut(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_surface"))?
            .queue_configure(XdgConfigure {
                serial,
                size: None,
                bounds: None,
                states: crate::compositor_wayland::ToplevelState::default(),
                decoration: crate::compositor_wayland::DecorationMode::ServerSide,
            })
            .map_err(error)?;
        if self.toplevels.contains_key(&surface)
            && let Some(toplevel_object) = self
            .resources
            .iter()
            .find_map(|(object, identity)| {
                let resource = unsafe { ResourceRef::from_raw(*identity as *mut ffi::wl_resource) }?;
                matches!(self.resource_kind(resource).ok()?, ResourceKind::XdgToplevel(candidate) if candidate == surface)
                    .then_some(*object)
            })
            && let Some(identity) = self.resources.get(&toplevel_object).copied()
            && let Some(resource) = unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
        {
            let mut states = ffi::wl_array {
                size: 0,
                alloc: 0,
                data: std::ptr::null_mut(),
            };
            self.post_event(
                resource,
                "xdg_toplevel",
                "configure",
                &mut [
                    ffi::wl_argument { i: 0 },
                    ffi::wl_argument { i: 0 },
                    ffi::wl_argument { a: &mut states },
                ],
            )?;
        }
        if self.popups.contains_key(&surface) {
            self.send_popup_configure(surface, None)?;
        }
        let object = *self
            .xdg_resources
            .get(&surface)
            .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is absent"))?;
        let identity = *self
            .resources
            .get(&object)
            .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is absent"))?;
        let resource = unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
            .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is stale"))?;
        self.post_event(
            resource,
            "xdg_surface",
            "configure",
            &mut [ffi::wl_argument { u: serial }],
        )?;
        self.initial_configures.insert(surface);
        Ok(())
    }

    fn send_toplevel_configure(
        &mut self,
        surface: WaylandSurfaceId,
        size: Option<crate::core::SizeI>,
        states: crate::compositor_wayland::ToplevelState,
    ) -> Result<u32, NativeCompositorError> {
        if !self.toplevels.contains_key(&surface) {
            return Err(NativeCompositorError::new("surface is not an xdg_toplevel"));
        }
        let (toplevel_identity, toplevel_version) = {
            let resource = self
                .resource_for_kind(
                    |kind| matches!(kind, ResourceKind::XdgToplevel(candidate) if candidate == surface),
                )?
                .ok_or_else(|| NativeCompositorError::new("xdg_toplevel resource is absent"))?;
            (resource.identity(), resource.version())
        };
        let xdg_surface_object = *self
            .xdg_resources
            .get(&surface)
            .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is absent"))?;
        let xdg_surface_identity = *self
            .resources
            .get(&xdg_surface_object)
            .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is absent"))?;
        let xdg_surface =
            unsafe { ResourceRef::from_raw(xdg_surface_identity as *mut ffi::wl_resource) }
                .ok_or_else(|| NativeCompositorError::new("xdg_surface resource is stale"))?;
        let serial = unsafe { ffi::wl_display_next_serial(self.display.as_ptr()) };
        if serial == 0 {
            return Err(NativeCompositorError::new(
                "libwayland returned a zero configure serial",
            ));
        }
        let decoration = self
            .toplevels
            .get(&surface)
            .expect("checked above")
            .decoration;
        self.core
            .xdg_surface_mut(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_surface"))?
            .queue_configure(XdgConfigure {
                serial,
                size,
                bounds: None,
                states,
                decoration,
            })
            .map_err(error)?;
        let mut state_values = Vec::<u32>::with_capacity(9);
        if states.maximized {
            state_values.push(1);
        }
        if states.fullscreen {
            state_values.push(2);
        }
        if states.resizing {
            state_values.push(3);
        }
        if states.activated {
            state_values.push(4);
        }
        if toplevel_version >= 2 {
            if states.tiled_left {
                state_values.push(5);
            }
            if states.tiled_right {
                state_values.push(6);
            }
            if states.tiled_top {
                state_values.push(7);
            }
            if states.tiled_bottom {
                state_values.push(8);
            }
        }
        if toplevel_version >= 6 && states.suspended {
            state_values.push(9);
        }
        let mut state_array = ffi::wl_array {
            size: state_values.len() * std::mem::size_of::<u32>(),
            alloc: state_values.len() * std::mem::size_of::<u32>(),
            data: state_values.as_mut_ptr().cast(),
        };
        let size = size.unwrap_or_default();
        let toplevel = unsafe { ResourceRef::from_raw(toplevel_identity as *mut ffi::wl_resource) }
            .ok_or_else(|| NativeCompositorError::new("xdg_toplevel resource is stale"))?;
        self.post_event(
            toplevel,
            "xdg_toplevel",
            "configure",
            &mut [
                ffi::wl_argument { i: size.width },
                ffi::wl_argument { i: size.height },
                ffi::wl_argument {
                    a: &mut state_array,
                },
            ],
        )?;
        self.post_event(
            xdg_surface,
            "xdg_surface",
            "configure",
            &mut [ffi::wl_argument { u: serial }],
        )?;
        Ok(serial)
    }

    fn send_popup_configure(
        &self,
        surface: WaylandSurfaceId,
        repositioned: Option<u32>,
    ) -> Result<(), NativeCompositorError> {
        let popup = self
            .popups
            .get(&surface)
            .ok_or_else(|| NativeCompositorError::new("unknown xdg_popup"))?;
        let geometry = popup_geometry(popup.positioner);
        let resource = self
            .resource_for_kind(
                |kind| matches!(kind, ResourceKind::XdgPopup(candidate) if candidate == surface),
            )?
            .ok_or_else(|| NativeCompositorError::new("xdg_popup resource is absent"))?;
        self.post_event(
            resource,
            "xdg_popup",
            "configure",
            &mut [
                ffi::wl_argument { i: geometry.x },
                ffi::wl_argument { i: geometry.y },
                ffi::wl_argument { i: geometry.width },
                ffi::wl_argument { i: geometry.height },
            ],
        )?;
        if let Some(token) = repositioned {
            self.post_event(
                resource,
                "xdg_popup",
                "repositioned",
                &mut [ffi::wl_argument { u: token }],
            )?;
        }
        Ok(())
    }

    fn data_source_from_resource(
        &self,
        resource: ResourceRef<'_>,
    ) -> Result<ProtocolObjectId, NativeCompositorError> {
        let ResourceKind::DataSource(source) = self.resource_kind(resource)? else {
            return Err(NativeCompositorError::new(
                "resource is not a wl_data_source",
            ));
        };
        Ok(source)
    }

    fn data_source_resource(
        &self,
        source: ProtocolObjectId,
    ) -> Result<ResourceRef<'_>, NativeCompositorError> {
        self.resource_for_kind(
            |kind| matches!(kind, ResourceKind::DataSource(candidate) if candidate == source),
        )?
        .ok_or_else(|| NativeCompositorError::new("wl_data_source resource is absent"))
    }

    fn cancel_data_source(&self, source: ProtocolObjectId) -> Result<(), NativeCompositorError> {
        if let Some(resource) = self.resource_for_kind(
            |kind| matches!(kind, ResourceKind::DataSource(candidate) if candidate == source),
        )? {
            self.post_event(resource, "wl_data_source", "cancelled", &mut [])?;
        }
        Ok(())
    }

    fn create_drag_offer(
        &mut self,
        device_identity: usize,
        client: ClientId,
        source_object: ProtocolObjectId,
    ) -> Result<(ProtocolObjectId, usize), NativeCompositorError> {
        let device = unsafe {
            ResourceRef::from_raw(device_identity as *mut ffi::wl_resource)
                .ok_or_else(|| NativeCompositorError::new("wl_data_device resource is absent"))?
        };
        let source = self
            .core
            .data_devices
            .source(source_object)
            .cloned()
            .ok_or_else(|| NativeCompositorError::new("drag source is absent"))?;
        let object = self.peek_next_object()?;
        let offer_resource = self.create_resource(
            device.client(),
            client,
            "wl_data_offer",
            device.version(),
            0,
            ResourceKind::DataOffer(object),
            true,
        )?;
        let legacy = offer_resource.version() < 3;
        let source_actions = if legacy {
            crate::compositor_wayland::DataAction::COPY
        } else {
            source.actions
        };
        if let Err(cause) =
            self.core
                .data_devices
                .create_offer(crate::compositor_wayland::DataOffer {
                    object,
                    source: source_object,
                    target: client,
                    drag: true,
                    accepted_mime_type: None,
                    source_actions,
                    target_actions: if legacy {
                        crate::compositor_wayland::DataAction::COPY
                    } else {
                        crate::compositor_wayland::DataAction::NONE
                    },
                    preferred_action: if legacy {
                        crate::compositor_wayland::DataAction::COPY
                    } else {
                        crate::compositor_wayland::DataAction::NONE
                    },
                    selected_action: crate::compositor_wayland::DataAction::NONE,
                    dropped: false,
                    finished: false,
                })
        {
            unsafe { offer_resource.destroy() };
            return Err(error(cause));
        }
        self.post_event(
            device,
            "wl_data_device",
            "data_offer",
            &mut [ffi::wl_argument {
                o: offer_resource.identity() as *mut ffi::wl_resource,
            }],
        )?;
        for mime in &source.mime_types {
            let mime = protocol_string(mime.as_str());
            self.post_event(
                offer_resource,
                "wl_data_offer",
                "offer",
                &mut [ffi::wl_argument { s: mime.as_ptr() }],
            )?;
        }
        if !legacy {
            self.post_event(
                offer_resource,
                "wl_data_offer",
                "source_actions",
                &mut [ffi::wl_argument {
                    u: u32::from(source_actions.bits()),
                }],
            )?;
        }
        Ok((object, offer_resource.identity()))
    }

    fn send_selection_to_client(
        &mut self,
        seat: u32,
        client: ClientId,
    ) -> Result<(), NativeCompositorError> {
        let devices = self
            .resources_for_client(
                client,
                |kind| matches!(kind, ResourceKind::DataDevice(candidate) if candidate == seat),
            )?
            .into_iter()
            .map(ResourceRef::identity)
            .collect::<Vec<_>>();
        for identity in devices {
            let Some(device) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            self.send_selection_to_device(device, client)?;
        }
        Ok(())
    }

    fn clear_selection_for_client(
        &mut self,
        seat: u32,
        client: ClientId,
    ) -> Result<(), NativeCompositorError> {
        self.core.data_devices.remove_offers_for_target(client);
        for resource in self.resources_for_client(
            client,
            |kind| matches!(kind, ResourceKind::DataDevice(candidate) if candidate == seat),
        )? {
            self.post_event(
                resource,
                "wl_data_device",
                "selection",
                &mut [ffi::wl_argument {
                    o: std::ptr::null_mut(),
                }],
            )?;
        }
        Ok(())
    }

    fn send_selection_to_device(
        &mut self,
        device: ResourceRef<'_>,
        client: ClientId,
    ) -> Result<(), NativeCompositorError> {
        let Some(selection) = self.core.data_devices.selection() else {
            return self.post_event(
                device,
                "wl_data_device",
                "selection",
                &mut [ffi::wl_argument {
                    o: std::ptr::null_mut(),
                }],
            );
        };
        let source = self
            .core
            .data_devices
            .source(selection)
            .cloned()
            .ok_or_else(|| NativeCompositorError::new("selection source is absent"))?;
        let object = self.peek_next_object()?;
        let offer_resource = self.create_resource(
            device.client(),
            client,
            "wl_data_offer",
            device.version(),
            0,
            ResourceKind::DataOffer(object),
            true,
        )?;
        if let Err(cause) =
            self.core
                .data_devices
                .create_offer(crate::compositor_wayland::DataOffer {
                    object,
                    source: selection,
                    target: client,
                    drag: false,
                    accepted_mime_type: None,
                    source_actions: source.actions,
                    target_actions: crate::compositor_wayland::DataAction::NONE,
                    preferred_action: crate::compositor_wayland::DataAction::NONE,
                    selected_action: crate::compositor_wayland::DataAction::NONE,
                    dropped: false,
                    finished: false,
                })
        {
            unsafe { offer_resource.destroy() };
            return Err(error(cause));
        }
        self.post_event(
            device,
            "wl_data_device",
            "data_offer",
            &mut [ffi::wl_argument {
                o: offer_resource.identity() as *mut ffi::wl_resource,
            }],
        )?;
        for mime in &source.mime_types {
            let mime = protocol_string(mime.as_str());
            self.post_event(
                offer_resource,
                "wl_data_offer",
                "offer",
                &mut [ffi::wl_argument { s: mime.as_ptr() }],
            )?;
        }
        self.post_event(
            device,
            "wl_data_device",
            "selection",
            &mut [ffi::wl_argument {
                o: offer_resource.identity() as *mut ffi::wl_resource,
            }],
        )
    }

    fn resource_for_kind(
        &self,
        predicate: impl Fn(ResourceKind) -> bool,
    ) -> Result<Option<ResourceRef<'_>>, NativeCompositorError> {
        for identity in self.resources.values().copied() {
            let Some(resource) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            if predicate(self.resource_kind(resource)?) {
                return Ok(Some(resource));
            }
        }
        Ok(None)
    }

    fn resource_for_object(
        &self,
        object: ProtocolObjectId,
    ) -> Result<Option<ResourceRef<'_>>, NativeCompositorError> {
        let Some(identity) = self.resources.get(&object).copied() else {
            return Ok(None);
        };
        let resource = unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) };
        if let Some(resource) = resource {
            let context_object = self.protocol_object_for_resource(resource)?;
            if context_object != object {
                return Err(NativeCompositorError::new(
                    "protocol resource identity is inconsistent",
                ));
            }
        }
        Ok(resource)
    }

    fn resources_for_kind(
        &self,
        predicate: impl Fn(ResourceKind) -> bool,
    ) -> Result<Vec<ResourceRef<'_>>, NativeCompositorError> {
        let mut matches = Vec::new();
        for identity in self.resources.values().copied() {
            let Some(resource) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            if predicate(self.resource_kind(resource)?) {
                matches.push(resource);
            }
        }
        Ok(matches)
    }

    fn resources_for_client(
        &self,
        client: ClientId,
        predicate: impl Fn(ResourceKind) -> bool,
    ) -> Result<Vec<ResourceRef<'_>>, NativeCompositorError> {
        let mut matches = Vec::new();
        for identity in self.resources.values().copied() {
            let Some(resource) =
                (unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) })
            else {
                continue;
            };
            let pointer = resource.user_data().cast::<ResourceContext>();
            if pointer.is_null() {
                continue;
            }
            let context = unsafe { &*pointer };
            if context.client == client && predicate(context.kind) {
                matches.push(resource);
            }
        }
        Ok(matches)
    }

    fn surface_resource(
        &self,
        surface: WaylandSurfaceId,
    ) -> Result<*mut ffi::wl_resource, NativeCompositorError> {
        self.resource_for_kind(
            |kind| matches!(kind, ResourceKind::Surface(candidate) if candidate == surface),
        )?
        .map(|resource| resource.identity() as *mut ffi::wl_resource)
        .ok_or_else(|| NativeCompositorError::new("wl_surface resource is absent"))
    }

    fn post_event(
        &self,
        resource: ResourceRef<'_>,
        interface: &str,
        event: &str,
        arguments: &mut [ffi::wl_argument],
    ) -> Result<(), NativeCompositorError> {
        let (opcode, schema) = self
            .protocol
            .interface_schema(interface)
            .and_then(|interface| interface.event_named(event))
            .ok_or_else(|| NativeCompositorError::new(format!("missing {interface}.{event}")))?;
        if schema.arguments.len() != arguments.len() {
            return Err(NativeCompositorError::new("event argument count mismatch"));
        }
        if schema.since > resource.version() {
            return Err(NativeCompositorError::new(format!(
                "event {interface}.{event} requires version {}, resource has version {}",
                schema.since,
                resource.version()
            )));
        }
        unsafe { resource.post_event(opcode, arguments) };
        Ok(())
    }

    fn resource_kind(
        &self,
        resource: ResourceRef<'_>,
    ) -> Result<ResourceKind, NativeCompositorError> {
        let context = resource.user_data().cast::<ResourceContext>();
        if context.is_null() {
            return Err(NativeCompositorError::new(
                "resource is not owned by Telorgon",
            ));
        }
        let context = unsafe { &*context };
        if !std::ptr::eq(context.state, self) {
            return Err(NativeCompositorError::new(
                "resource belongs to another compositor",
            ));
        }
        Ok(context.kind)
    }

    fn protocol_object_for_resource(
        &self,
        resource: ResourceRef<'_>,
    ) -> Result<ProtocolObjectId, NativeCompositorError> {
        let context = resource.user_data().cast::<ResourceContext>();
        if context.is_null() {
            return Err(NativeCompositorError::new(
                "resource is not owned by Telorgon",
            ));
        }
        let context = unsafe { &*context };
        if !std::ptr::eq(context.state, self) {
            return Err(NativeCompositorError::new(
                "resource belongs to another compositor",
            ));
        }
        Ok(context.object)
    }

    fn surface_from_resource(
        &self,
        resource: ResourceRef<'_>,
    ) -> Result<WaylandSurfaceId, NativeCompositorError> {
        match self.resource_kind(resource)? {
            ResourceKind::Surface(surface) => Ok(surface),
            _ => Err(NativeCompositorError::new("resource is not a wl_surface")),
        }
    }

    fn region_from_resource(
        &self,
        resource: ResourceRef<'_>,
    ) -> Result<Region, NativeCompositorError> {
        let ResourceKind::Region(object) = self.resource_kind(resource)? else {
            return Err(NativeCompositorError::new("resource is not a wl_region"));
        };
        Region::from_rectangles(
            self.regions
                .get(&object)
                .cloned()
                .ok_or_else(|| NativeCompositorError::new("unknown region"))?,
        )
        .map_err(error)
    }

    fn surface_mut(
        &mut self,
        surface: WaylandSurfaceId,
    ) -> Result<&mut crate::compositor_wayland::SurfaceState, NativeCompositorError> {
        self.core
            .world
            .surface_mut(surface)
            .ok_or_else(|| NativeCompositorError::new("unknown wl_surface"))
    }

    fn peek_next_object(&self) -> Result<ProtocolObjectId, NativeCompositorError> {
        let next = self
            .next_object
            .checked_add(1)
            .filter(|value| *value != 0)
            .ok_or_else(|| NativeCompositorError::new("protocol object identity exhausted"))?;
        Ok(ProtocolObjectId::from_raw(next).expect("nonzero"))
    }

    fn destroy_context(&mut self, context: &ResourceContext) {
        self.resources.remove(&context.object);
        let abort_drag = self
            .active_drag
            .as_ref()
            .is_some_and(|drag| match context.kind {
                ResourceKind::DataSource(source) => drag.source == Some(source),
                ResourceKind::Surface(surface) => {
                    drag.origin == surface
                        || drag
                            .target
                            .as_ref()
                            .is_some_and(|target| target.surface == surface)
                }
                _ => false,
            });
        if abort_drag && let Some(drag) = self.active_drag.take() {
            if let Some(target) = &drag.target {
                let _ = self.send_drag_leave(target, drag.source);
            }
            if !matches!(context.kind, ResourceKind::DataSource(_))
                && let Some(source) = drag.source
            {
                let _ = self.cancel_data_source(source);
            }
            self.finish_drag(drag.icon);
        }
        if let ResourceKind::Surface(surface) = context.kind
            && let Some(drag) = self.active_drag.as_mut()
            && drag.icon == Some(surface)
        {
            drag.icon = None;
        }
        match context.kind {
            ResourceKind::Surface(surface) => {
                self.callbacks.remove(&surface);
                self.committed_callbacks
                    .retain(|(candidate, _), _| *candidate != surface);
                self.pending_presentation_feedbacks.remove(&surface);
                self.committed_presentation_feedbacks
                    .retain(|(candidate, _), _| *candidate != surface);
                self.initial_configures.remove(&surface);
                self.xdg_resources.remove(&surface);
                self.toplevels.remove(&surface);
                self.pending_toplevel_icons.remove(&surface);
                self.committed_toplevel_icons.remove(&surface);
                self.viewports.remove(&surface);
                self.synchronized_surfaces.remove(&surface);
                self.pending_acquire_fences.remove(&surface);
                self.pending_releases.remove(&surface);
                self.committed_acquire_fences
                    .retain(|(candidate, _), _| *candidate != surface);
                self.committed_releases
                    .retain(|(candidate, _), _| *candidate != surface);
                self.core.buffer_uses.cancel_surface(surface);
                self.touch_points
                    .retain(|_, point| point.surface != surface);
                self.idle_inhibitors
                    .retain(|_, candidate| *candidate != surface);
                self.pointer_constraints
                    .retain(|_, constraint| constraint.surface != surface);
                self.session_lock_surfaces.remove(&surface);
                let _ = self.core.destroy_surface(context.client, surface);
            }
            ResourceKind::Region(object) => {
                self.regions.remove(&object);
            }
            ResourceKind::ShmPool(object) => {
                self.shm_pools.remove(&object);
            }
            ResourceKind::Buffer(buffer) => {
                let affected = self
                    .toplevel_icons
                    .iter()
                    .filter(|(_, icon)| icon.buffers.values().any(|candidate| *candidate == buffer))
                    .map(|(object, _)| *object)
                    .collect::<Vec<_>>();
                for object in affected {
                    if let Some(identity) = self.resources.get(&object).copied()
                        && let Some(icon) =
                            unsafe { ResourceRef::from_raw(identity as *mut ffi::wl_resource) }
                    {
                        icon.post_error(3, "an icon wl_buffer was destroyed before its icon");
                    }
                }
                self.buffer_files.remove(&buffer);
                self.dmabuf_files.remove(&buffer);
                let _ = self.core.destroy_buffer(context.client, buffer);
            }
            ResourceKind::LinuxBufferParams(object) => {
                self.dmabuf_params.remove(&object);
            }
            ResourceKind::DataSource(source) => {
                self.finished_drag_sources.remove(&source);
                if self.core.data_devices.remove_source(source) {
                    let focused = self
                        .core
                        .seats
                        .iter()
                        .filter_map(|(seat, state)| {
                            state.keyboard_focus.map(|focus| (*seat, focus.client))
                        })
                        .collect::<Vec<_>>();
                    for (seat, client) in focused {
                        let _ = self.send_selection_to_client(seat, client);
                    }
                }
            }
            ResourceKind::DataOffer(offer) => {
                self.core.data_devices.remove_offer(offer);
            }
            ResourceKind::Subsurface(surface) => {
                let _ = self.core.subsurfaces.remove(surface);
            }
            ResourceKind::XdgSurface(surface) => {
                self.xdg_resources.remove(&surface);
            }
            ResourceKind::XdgToplevel(surface) => {
                self.toplevels.remove(&surface);
                self.pending_toplevel_icons.remove(&surface);
                self.committed_toplevel_icons.remove(&surface);
            }
            ResourceKind::ToplevelIcon(object) => {
                self.toplevel_icons.remove(&object);
            }
            ResourceKind::XdgPositioner(object) => {
                self.positioners.remove(&object);
            }
            ResourceKind::XdgPopup(surface) => {
                self.popups.remove(&surface);
            }
            ResourceKind::Viewport(surface) => {
                self.viewports.remove(&surface);
            }
            ResourceKind::PresentationFeedback(surface) => {
                if let Some(feedbacks) = self.pending_presentation_feedbacks.get_mut(&surface) {
                    feedbacks.retain(|object| *object != context.object);
                }
                for ((candidate, _), feedbacks) in &mut self.committed_presentation_feedbacks {
                    if *candidate == surface {
                        feedbacks.retain(|object| *object != context.object);
                    }
                }
            }
            ResourceKind::ActivationToken(object) => {
                self.activation_tokens.remove(&object);
            }
            ResourceKind::SessionLock(object) => {
                if let Some(lock) = self.session_locks.remove(&object)
                    && self.active_session_lock == Some(object)
                {
                    self.active_session_lock = None;
                    if !lock.locked_event_sent {
                        self.core
                            .queue_action(CompositorAction::SessionLockCancelled(object));
                    }
                }
            }
            ResourceKind::SessionLockSurface(surface) => {
                self.session_lock_surfaces.remove(&surface);
            }
            ResourceKind::IdleInhibitor(object) => {
                self.idle_inhibitors.remove(&object);
            }
            ResourceKind::LockedPointer(object) | ResourceKind::ConfinedPointer(object) => {
                self.pointer_constraints.remove(&object);
            }
            ResourceKind::SurfaceSynchronization(surface) => {
                self.synchronized_surfaces.remove(&surface);
                self.pending_acquire_fences.remove(&surface);
                self.pending_releases.remove(&surface);
            }
            ResourceKind::ExplicitBufferRelease(surface) => {
                self.pending_releases
                    .retain(|candidate, object| *candidate != surface || *object != context.object);
                self.committed_releases.retain(|(candidate, _), object| {
                    *candidate != surface || *object != context.object
                });
            }
            ResourceKind::Callback(surface) => {
                if let Some(callbacks) = self.callbacks.get_mut(&surface) {
                    callbacks.retain(|object| *object != context.object);
                }
                for ((candidate, _), callbacks) in &mut self.committed_callbacks {
                    if *candidate == surface {
                        callbacks.retain(|object| *object != context.object);
                    }
                }
            }
            _ => {}
        }
        let _ = self.core.objects.remove(context.client, context.object);
        if self.core.objects.client_len(context.client) == 0 {
            self.touch_points
                .retain(|_, point| point.client != context.client);
            let _ = self.core.disconnect_client(context.client);
            self.clients.retain(|_, client| *client != context.client);
        }
    }
}

fn c_string(request: &IncomingRequest<'_>, index: usize) -> Result<String, NativeCompositorError> {
    Ok(request
        .string(index)
        .map_err(error)?
        .ok_or_else(|| NativeCompositorError::new("string must not be null"))?
        .to_string_lossy()
        .into_owned())
}

fn protocol_string(value: &str) -> std::ffi::CString {
    std::ffi::CString::new(value)
        .unwrap_or_else(|_| std::ffi::CString::new(value.replace('\0', "")).expect("sanitized"))
}

fn activation_token_handle() -> Result<String, NativeCompositorError> {
    let mut random = [0_u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|error| {
            NativeCompositorError::new(format!("activation-token entropy failed: {error}"))
        })?;
    let mut handle = String::with_capacity(7 + random.len() * 2);
    handle.push_str("telorgon-");
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut handle, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(handle)
}

fn output_transform_wire(transform: crate::compositor_wayland::OutputTransform) -> i32 {
    match transform {
        crate::compositor_wayland::OutputTransform::Normal => 0,
        crate::compositor_wayland::OutputTransform::Rotate90 => 1,
        crate::compositor_wayland::OutputTransform::Rotate180 => 2,
        crate::compositor_wayland::OutputTransform::Rotate270 => 3,
        crate::compositor_wayland::OutputTransform::Flipped => 4,
        crate::compositor_wayland::OutputTransform::Flipped90 => 5,
        crate::compositor_wayland::OutputTransform::Flipped180 => 6,
        crate::compositor_wayland::OutputTransform::Flipped270 => 7,
    }
}

fn resize_edge(value: u32) -> Result<crate::compositor_wayland::ResizeEdge, NativeCompositorError> {
    Ok(match value {
        1 => crate::compositor_wayland::ResizeEdge::Top,
        2 => crate::compositor_wayland::ResizeEdge::Bottom,
        4 => crate::compositor_wayland::ResizeEdge::Left,
        5 => crate::compositor_wayland::ResizeEdge::TopLeft,
        6 => crate::compositor_wayland::ResizeEdge::BottomLeft,
        8 => crate::compositor_wayland::ResizeEdge::Right,
        9 => crate::compositor_wayland::ResizeEdge::TopRight,
        10 => crate::compositor_wayland::ResizeEdge::BottomRight,
        _ => return Err(NativeCompositorError::new("invalid xdg resize edge")),
    })
}

#[repr(C)]
struct NativeTimespec {
    seconds: c_long,
    nanoseconds: c_long,
}

struct MonotonicTimestamp {
    seconds: u64,
    nanoseconds: u32,
}

#[link(name = "c")]
unsafe extern "C" {
    fn clock_gettime(clock: i32, time: *mut NativeTimespec) -> i32;
}

fn monotonic_timestamp() -> Result<MonotonicTimestamp, NativeCompositorError> {
    let mut time = NativeTimespec {
        seconds: 0,
        nanoseconds: 0,
    };
    let result = unsafe { clock_gettime(1, &mut time) };
    if result != 0 || time.seconds < 0 || !(0..1_000_000_000).contains(&time.nanoseconds) {
        return Err(NativeCompositorError::new(
            "CLOCK_MONOTONIC timestamp query failed",
        ));
    }
    Ok(MonotonicTimestamp {
        seconds: time.seconds as u64,
        nanoseconds: time.nanoseconds as u32,
    })
}

fn transformed_size(
    size: crate::core::SizeI,
    transform: crate::compositor_wayland::BufferTransform,
) -> crate::core::SizeI {
    match transform {
        crate::compositor_wayland::BufferTransform::Rotate90
        | crate::compositor_wayland::BufferTransform::Rotate270
        | crate::compositor_wayland::BufferTransform::Flipped90
        | crate::compositor_wayland::BufferTransform::Flipped270 => crate::core::SizeI {
            width: size.height,
            height: size.width,
        },
        _ => size,
    }
}

fn next_nonzero(value: u32) -> Result<u32, NativeCompositorError> {
    value
        .checked_add(1)
        .filter(|value| *value != 0)
        .ok_or_else(|| NativeCompositorError::new("native compositor identity exhausted"))
}

fn fixed(value: f32) -> i32 {
    let scaled = f64::from(value) * 256.0;
    scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn fixed_f64(value: f64) -> i32 {
    (value * 256.0).clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn rectangles_intersect(left: RectI, right: RectI) -> bool {
    left.x < right.x.saturating_add(right.width)
        && left.x.saturating_add(left.width) > right.x
        && left.y < right.y.saturating_add(right.height)
        && left.y.saturating_add(left.height) > right.y
}

fn subtract_rectangle(source: RectI, cut: RectI) -> Vec<RectI> {
    if !rectangles_intersect(source, cut) {
        return vec![source];
    }

    let source_right = source.x.saturating_add(source.width);
    let source_bottom = source.y.saturating_add(source.height);
    let cut_left = cut.x.max(source.x);
    let cut_top = cut.y.max(source.y);
    let cut_right = cut.x.saturating_add(cut.width).min(source_right);
    let cut_bottom = cut.y.saturating_add(cut.height).min(source_bottom);
    let mut result = Vec::with_capacity(4);
    let mut push = |x: i32, y: i32, width: i32, height: i32| {
        if width > 0 && height > 0 {
            result.push(RectI {
                x,
                y,
                width,
                height,
            });
        }
    };

    push(source.x, source.y, source.width, cut_top - source.y);
    push(
        source.x,
        cut_bottom,
        source.width,
        source_bottom - cut_bottom,
    );
    push(source.x, cut_top, cut_left - source.x, cut_bottom - cut_top);
    push(
        cut_right,
        cut_top,
        source_right - cut_right,
        cut_bottom - cut_top,
    );
    result
}

fn popup_geometry(positioner: crate::compositor_wayland::XdgPositioner) -> RectI {
    let anchor = positioner.anchor_rect;
    let horizontal_center = anchor.x.saturating_add(anchor.width / 2);
    let vertical_center = anchor.y.saturating_add(anchor.height / 2);
    let anchor_point = match positioner.anchor {
        1 => PointI {
            x: horizontal_center,
            y: anchor.y,
        },
        2 => PointI {
            x: horizontal_center,
            y: anchor.y.saturating_add(anchor.height),
        },
        3 => PointI {
            x: anchor.x,
            y: vertical_center,
        },
        4 => PointI {
            x: anchor.x.saturating_add(anchor.width),
            y: vertical_center,
        },
        5 => PointI {
            x: anchor.x,
            y: anchor.y,
        },
        6 => PointI {
            x: anchor.x,
            y: anchor.y.saturating_add(anchor.height),
        },
        7 => PointI {
            x: anchor.x.saturating_add(anchor.width),
            y: anchor.y,
        },
        8 => PointI {
            x: anchor.x.saturating_add(anchor.width),
            y: anchor.y.saturating_add(anchor.height),
        },
        _ => PointI {
            x: horizontal_center,
            y: vertical_center,
        },
    };
    let size = positioner.size;
    let (gravity_x, gravity_y) = match positioner.gravity {
        1 => (-size.width / 2, -size.height),
        2 => (-size.width / 2, 0),
        3 => (-size.width, -size.height / 2),
        4 => (0, -size.height / 2),
        5 => (-size.width, -size.height),
        6 => (-size.width, 0),
        7 => (0, -size.height),
        8 => (0, 0),
        _ => (-size.width / 2, -size.height / 2),
    };
    RectI {
        x: anchor_point
            .x
            .saturating_add(gravity_x)
            .saturating_add(positioner.offset.x),
        y: anchor_point
            .y
            .saturating_add(gravity_y)
            .saturating_add(positioner.offset.y),
        width: size.width,
        height: size.height,
    }
}

fn fd_size(fd: &OwnedFd) -> std::io::Result<u64> {
    let file = std::fs::File::from(fd.try_clone()?);
    Ok(file.metadata()?.len())
}

fn unsupported_request(request: &IncomingRequest<'_>) -> NativeCompositorError {
    NativeCompositorError::new(format!(
        "request {} is not implemented",
        request.message().name
    ))
}

fn error(error: impl fmt::Display) -> NativeCompositorError {
    NativeCompositorError::new(error.to_string())
}

#[derive(Debug)]
pub struct NativeCompositorError {
    context: String,
}

impl NativeCompositorError {
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            context: context.into(),
        }
    }
}

impl fmt::Display for NativeCompositorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.context)
    }
}

impl std::error::Error for NativeCompositorError {}
