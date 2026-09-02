use std::collections::BTreeMap;
use std::fmt;

use crate::compositor_wayland::{
    BufferDescriptor, ClientId, ClientLimits, ObjectRegistry, ObjectRegistryError, OutputState,
    ProtocolObjectId, SeatState, SerialLedger, SubsurfaceGraph, WaylandBufferId, WaylandSurfaceId,
    WaylandWorld, WaylandWorldError, XdgSurfaceState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositorAction {
    PublishSurface(WaylandSurfaceId),
    WithdrawSurface(WaylandSurfaceId),
    ImportBuffer(WaylandBufferId),
    ReleaseBuffer(WaylandBufferId),
    ActivateSurface {
        surface: WaylandSurfaceId,
        application_id: Option<String>,
        source_surface: Option<WaylandSurfaceId>,
    },
    SessionLockRequested(ProtocolObjectId),
    SessionLockCancelled(ProtocolObjectId),
    SessionUnlockRequested(ProtocolObjectId),
    MoveToplevel(WaylandSurfaceId),
    ResizeToplevel {
        surface: WaylandSurfaceId,
        edge: crate::compositor_wayland::ResizeEdge,
    },
    MaximizeToplevel {
        surface: WaylandSurfaceId,
        maximized: bool,
    },
    FullscreenToplevel {
        surface: WaylandSurfaceId,
        fullscreen: bool,
        output: Option<u32>,
    },
    MinimizeToplevel(WaylandSurfaceId),
    StartDrag {
        seat: u32,
        origin: WaylandSurfaceId,
        icon: Option<WaylandSurfaceId>,
    },
    FinishDrag {
        icon: Option<WaylandSurfaceId>,
    },
    RepaintOutput(u32),
    DisconnectClient(ClientId),
}

/// Protocol state owned by Telorgon. Native callbacks only decode requests into calls on this type.
#[derive(Debug)]
pub struct CompositorCore {
    pub world: WaylandWorld,
    pub objects: ObjectRegistry,
    pub serials: SerialLedger,
    pub subsurfaces: SubsurfaceGraph,
    pub data_devices: crate::compositor_wayland::DataDeviceState,
    pub buffer_uses: crate::compositor_wayland::BufferUseTracker,
    pub seats: BTreeMap<u32, SeatState>,
    pub outputs: BTreeMap<u32, OutputState>,
    buffers: BTreeMap<WaylandBufferId, (ClientId, BufferDescriptor)>,
    xdg_surfaces: BTreeMap<WaylandSurfaceId, XdgSurfaceState>,
    actions: Vec<CompositorAction>,
}

impl CompositorCore {
    pub fn new(limits: ClientLimits) -> Result<Self, CompositorCoreError> {
        Ok(Self {
            world: WaylandWorld::with_limits(limits)?,
            objects: ObjectRegistry::new(limits.maximum_protocol_objects)?,
            serials: SerialLedger::default(),
            subsurfaces: SubsurfaceGraph::default(),
            data_devices: crate::compositor_wayland::DataDeviceState::default(),
            buffer_uses: crate::compositor_wayland::BufferUseTracker::default(),
            seats: BTreeMap::new(),
            outputs: BTreeMap::new(),
            buffers: BTreeMap::new(),
            xdg_surfaces: BTreeMap::new(),
            actions: Vec::new(),
        })
    }

    pub fn connect_client(&mut self, client: ClientId) -> Result<(), CompositorCoreError> {
        self.world.add_client(client)?;
        Ok(())
    }

    pub fn disconnect_client(&mut self, client: ClientId) -> Result<(), CompositorCoreError> {
        self.world.remove_client(client)?;
        self.objects.remove_client(client);
        self.serials.remove_client(client);
        self.data_devices.remove_client(client);
        self.buffers.retain(|_, (owner, _)| *owner != client);
        self.xdg_surfaces
            .retain(|surface, _| self.world.surface(*surface).is_some());
        for seat in self.seats.values_mut() {
            seat.remove_client(client);
        }
        self.actions
            .push(CompositorAction::DisconnectClient(client));
        Ok(())
    }

    pub fn register_buffer(
        &mut self,
        client: ClientId,
        buffer: WaylandBufferId,
        descriptor: BufferDescriptor,
    ) -> Result<(), CompositorCoreError> {
        if self.buffers.contains_key(&buffer) {
            return Err(CompositorCoreError::DuplicateBuffer);
        }
        self.buffers.insert(buffer, (client, descriptor));
        self.actions.push(CompositorAction::ImportBuffer(buffer));
        Ok(())
    }

    pub fn destroy_buffer(
        &mut self,
        client: ClientId,
        buffer: WaylandBufferId,
    ) -> Result<BufferDescriptor, CompositorCoreError> {
        if self.buffers.get(&buffer).map(|entry| entry.0) != Some(client) {
            return Err(CompositorCoreError::BufferOwnershipMismatch);
        }
        let (_, descriptor) = self.buffers.remove(&buffer).expect("checked above");
        self.actions.push(CompositorAction::ReleaseBuffer(buffer));
        Ok(descriptor)
    }

    pub fn buffer(&self, buffer: WaylandBufferId) -> Option<&BufferDescriptor> {
        self.buffers.get(&buffer).map(|(_, descriptor)| descriptor)
    }

    pub fn buffer_owner(&self, buffer: WaylandBufferId) -> Option<ClientId> {
        self.buffers.get(&buffer).map(|(owner, _)| *owner)
    }

    pub fn create_xdg_surface(
        &mut self,
        client: ClientId,
        surface: WaylandSurfaceId,
        object: ProtocolObjectId,
        version: u32,
    ) -> Result<&mut XdgSurfaceState, CompositorCoreError> {
        if self.world.surface(surface).is_none() {
            return Err(CompositorCoreError::UnknownSurface);
        }
        if self.world.surface_owner(surface) != Some(client) {
            return Err(CompositorCoreError::UnknownSurface);
        }
        self.objects.insert(
            object,
            crate::compositor_wayland::ObjectMetadata {
                owner: client,
                kind: crate::compositor_wayland::ProtocolObjectKind::XdgSurface,
                version,
            },
        )?;
        if self.xdg_surfaces.contains_key(&surface) {
            return Err(CompositorCoreError::DuplicateXdgSurface);
        }
        self.xdg_surfaces
            .insert(surface, XdgSurfaceState::new(surface));
        Ok(self.xdg_surfaces.get_mut(&surface).expect("inserted"))
    }

    pub fn xdg_surface_mut(&mut self, surface: WaylandSurfaceId) -> Option<&mut XdgSurfaceState> {
        self.xdg_surfaces.get_mut(&surface)
    }

    pub fn xdg_surface(&self, surface: WaylandSurfaceId) -> Option<&XdgSurfaceState> {
        self.xdg_surfaces.get(&surface)
    }

    pub fn queue_action(&mut self, action: CompositorAction) {
        self.actions.push(action);
    }

    pub fn drain_actions(&mut self) -> impl Iterator<Item = CompositorAction> + '_ {
        self.actions.drain(..)
    }
}

impl Default for CompositorCore {
    fn default() -> Self {
        Self::new(ClientLimits::default()).expect("default compositor limits are valid")
    }
}

#[derive(Debug)]
pub enum CompositorCoreError {
    World(WaylandWorldError),
    Objects(ObjectRegistryError),
    DuplicateBuffer,
    BufferOwnershipMismatch,
    UnknownSurface,
    DuplicateXdgSurface,
}

impl fmt::Display for CompositorCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Wayland compositor core operation failed: {self:?}"
        )
    }
}

impl std::error::Error for CompositorCoreError {}

impl From<WaylandWorldError> for CompositorCoreError {
    fn from(value: WaylandWorldError) -> Self {
        Self::World(value)
    }
}

impl From<ObjectRegistryError> for CompositorCoreError {
    fn from(value: ObjectRegistryError) -> Self {
        Self::Objects(value)
    }
}
