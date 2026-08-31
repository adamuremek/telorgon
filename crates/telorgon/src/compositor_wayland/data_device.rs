use std::collections::BTreeMap;
use std::fmt;

use crate::compositor_wayland::{ClientId, ProtocolObjectId, WaylandSurfaceId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MimeType(String);

impl MimeType {
    pub fn new(value: impl Into<String>) -> Result<Self, DataDeviceError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 1024
            || value.contains('\0')
            || value.chars().any(char::is_control)
        {
            Err(DataDeviceError::InvalidMimeType)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataAction(u8);

impl DataAction {
    pub const NONE: Self = Self(0);
    pub const COPY: Self = Self(1);
    pub const MOVE: Self = Self(2);
    pub const ASK: Self = Self(4);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn from_protocol(value: u32) -> Option<Self> {
        if value & !7 == 0 {
            Some(Self(value as u8))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataSource {
    pub owner: ClientId,
    pub object: ProtocolObjectId,
    pub mime_types: Vec<MimeType>,
    pub actions: DataAction,
    pub used: bool,
}

impl DataSource {
    pub fn offer(&mut self, mime_type: MimeType) -> Result<(), DataDeviceError> {
        if self.used {
            return Err(DataDeviceError::SourceAlreadyUsed);
        }
        if self.mime_types.len() >= 128 {
            return Err(DataDeviceError::TooManyMimeTypes);
        }
        if !self.mime_types.contains(&mime_type) {
            self.mime_types.push(mime_type);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DataOffer {
    pub object: ProtocolObjectId,
    pub source: ProtocolObjectId,
    pub target: ClientId,
    pub drag: bool,
    pub accepted_mime_type: Option<MimeType>,
    pub source_actions: DataAction,
    pub target_actions: DataAction,
    pub preferred_action: DataAction,
    pub selected_action: DataAction,
    pub dropped: bool,
    pub finished: bool,
}

#[derive(Debug, Default)]
pub struct DataDeviceState {
    sources: BTreeMap<ProtocolObjectId, DataSource>,
    offers: BTreeMap<ProtocolObjectId, DataOffer>,
    selection: Option<ProtocolObjectId>,
    drag_source: Option<ProtocolObjectId>,
    drag_origin: Option<WaylandSurfaceId>,
}

impl DataDeviceState {
    pub fn create_source(&mut self, source: DataSource) -> Result<(), DataDeviceError> {
        if self.sources.insert(source.object, source).is_some() {
            return Err(DataDeviceError::DuplicateObject);
        }
        Ok(())
    }

    pub fn source_mut(&mut self, object: ProtocolObjectId) -> Option<&mut DataSource> {
        self.sources.get_mut(&object)
    }

    pub fn source(&self, object: ProtocolObjectId) -> Option<&DataSource> {
        self.sources.get(&object)
    }

    pub fn selection(&self) -> Option<ProtocolObjectId> {
        self.selection
    }

    pub fn set_selection(
        &mut self,
        owner: ClientId,
        source: Option<ProtocolObjectId>,
    ) -> Result<(), DataDeviceError> {
        if let Some(source) = source {
            let source = self
                .sources
                .get_mut(&source)
                .ok_or(DataDeviceError::UnknownSource)?;
            if source.owner != owner || source.used {
                return Err(DataDeviceError::InvalidSource);
            }
            source.used = true;
            self.selection = Some(source.object);
        } else {
            self.selection = None;
        }
        Ok(())
    }

    pub fn start_drag(
        &mut self,
        owner: ClientId,
        source: Option<ProtocolObjectId>,
        origin: WaylandSurfaceId,
    ) -> Result<(), DataDeviceError> {
        if self.drag_origin.is_some() {
            return Err(DataDeviceError::DragAlreadyActive);
        }
        if let Some(source) = source {
            let source = self
                .sources
                .get_mut(&source)
                .ok_or(DataDeviceError::UnknownSource)?;
            if source.owner != owner || source.used {
                return Err(DataDeviceError::InvalidSource);
            }
            source.used = true;
            self.drag_source = Some(source.object);
        }
        self.drag_origin = Some(origin);
        Ok(())
    }

    pub fn drag_source(&self) -> Option<ProtocolObjectId> {
        self.drag_source
    }

    pub fn drag_origin(&self) -> Option<WaylandSurfaceId> {
        self.drag_origin
    }

    pub fn drag_active(&self) -> bool {
        self.drag_origin.is_some()
    }

    pub fn create_offer(&mut self, offer: DataOffer) -> Result<(), DataDeviceError> {
        if !self.sources.contains_key(&offer.source) {
            return Err(DataDeviceError::UnknownSource);
        }
        if self.offers.insert(offer.object, offer).is_some() {
            return Err(DataDeviceError::DuplicateObject);
        }
        Ok(())
    }

    pub fn offer(&self, object: ProtocolObjectId) -> Option<&DataOffer> {
        self.offers.get(&object)
    }

    pub fn offer_mut(&mut self, object: ProtocolObjectId) -> Option<&mut DataOffer> {
        self.offers.get_mut(&object)
    }

    /// Removes a source and every offer derived from it. Returns whether the selection changed.
    pub fn remove_source(&mut self, object: ProtocolObjectId) -> bool {
        if self.sources.remove(&object).is_none() {
            return false;
        }
        self.offers.retain(|_, offer| offer.source != object);
        let selection_changed = self.selection == Some(object);
        if selection_changed {
            self.selection = None;
        }
        if self.drag_source == Some(object) {
            self.finish_drag();
        }
        selection_changed
    }

    pub fn remove_offer(&mut self, object: ProtocolObjectId) -> bool {
        self.offers.remove(&object).is_some()
    }

    pub fn remove_offers_for_target(&mut self, target: ClientId) {
        self.offers.retain(|_, offer| offer.target != target);
    }

    pub fn choose_action(
        &mut self,
        offer: ProtocolObjectId,
    ) -> Result<DataAction, DataDeviceError> {
        let offer = self
            .offers
            .get_mut(&offer)
            .ok_or(DataDeviceError::UnknownOffer)?;
        let intersection = offer.source_actions.bits() & offer.target_actions.bits();
        let selected = if intersection & offer.preferred_action.bits() != 0 {
            offer.preferred_action
        } else if intersection & DataAction::COPY.bits() != 0 {
            DataAction::COPY
        } else if intersection & DataAction::MOVE.bits() != 0 {
            DataAction::MOVE
        } else if intersection & DataAction::ASK.bits() != 0 {
            DataAction::ASK
        } else {
            DataAction::NONE
        };
        offer.selected_action = selected;
        Ok(selected)
    }

    pub fn finish_drag(&mut self) {
        self.drag_source = None;
        self.drag_origin = None;
    }

    pub fn remove_client(&mut self, client: ClientId) {
        let removed_sources: Vec<_> = self
            .sources
            .iter()
            .filter_map(|(id, source)| (source.owner == client).then_some(*id))
            .collect();
        self.sources.retain(|_, source| source.owner != client);
        self.offers
            .retain(|_, offer| offer.target != client && !removed_sources.contains(&offer.source));
        if self
            .selection
            .is_some_and(|source| removed_sources.contains(&source))
        {
            self.selection = None;
        }
        if self
            .drag_source
            .is_some_and(|source| removed_sources.contains(&source))
        {
            self.finish_drag();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDeviceError {
    InvalidMimeType,
    TooManyMimeTypes,
    DuplicateObject,
    UnknownSource,
    UnknownOffer,
    InvalidSource,
    SourceAlreadyUsed,
    DragAlreadyActive,
}

impl fmt::Display for DataDeviceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Wayland data-device operation failed: {self:?}")
    }
}

impl std::error::Error for DataDeviceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(raw: u32) -> ClientId {
        ClientId::from_raw(raw).unwrap()
    }

    fn object(raw: u32) -> ProtocolObjectId {
        ProtocolObjectId::from_raw(raw).unwrap()
    }

    #[test]
    fn removing_the_selected_source_revokes_its_offers() {
        let mut state = DataDeviceState::default();
        state
            .create_source(DataSource {
                owner: client(1),
                object: object(1),
                mime_types: vec![MimeType::new("text/plain").unwrap()],
                actions: DataAction::COPY,
                used: false,
            })
            .unwrap();
        state.set_selection(client(1), Some(object(1))).unwrap();
        state
            .create_offer(DataOffer {
                object: object(2),
                source: object(1),
                target: client(2),
                drag: false,
                accepted_mime_type: None,
                source_actions: DataAction::COPY,
                target_actions: DataAction::NONE,
                preferred_action: DataAction::NONE,
                selected_action: DataAction::NONE,
                dropped: false,
                finished: false,
            })
            .unwrap();

        assert!(state.remove_source(object(1)));
        assert_eq!(state.selection(), None);
        assert!(state.offer(object(2)).is_none());
    }

    #[test]
    fn offer_action_prefers_an_available_target_choice() {
        let mut state = DataDeviceState::default();
        state
            .create_source(DataSource {
                owner: client(1),
                object: object(1),
                mime_types: Vec::new(),
                actions: DataAction::COPY.union(DataAction::MOVE),
                used: false,
            })
            .unwrap();
        state
            .create_offer(DataOffer {
                object: object(2),
                source: object(1),
                target: client(2),
                drag: true,
                accepted_mime_type: None,
                source_actions: DataAction::COPY.union(DataAction::MOVE),
                target_actions: DataAction::COPY.union(DataAction::MOVE),
                preferred_action: DataAction::MOVE,
                selected_action: DataAction::NONE,
                dropped: false,
                finished: false,
            })
            .unwrap();
        assert_eq!(state.choose_action(object(2)).unwrap(), DataAction::MOVE);
    }

    #[test]
    fn a_source_less_drag_is_still_exclusive() {
        let mut state = DataDeviceState::default();
        let origin = WaylandSurfaceId::from_raw(7).unwrap();
        state.start_drag(client(1), None, origin).unwrap();
        assert!(state.drag_active());
        assert_eq!(state.drag_origin(), Some(origin));
        assert_eq!(state.drag_source(), None);
        assert_eq!(
            state.start_drag(client(1), None, origin),
            Err(DataDeviceError::DragAlreadyActive)
        );
        state.finish_drag();
        assert!(!state.drag_active());
    }
}
