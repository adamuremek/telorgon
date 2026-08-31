//! Immutable system-status entries supplied by a shell policy host.

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;

macro_rules! define_status_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            pub const fn new(value: NonZeroU64) -> Self {
                Self(value)
            }

            pub const fn from_raw(value: u64) -> Option<Self> {
                match NonZeroU64::new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

define_status_id!(
    SystemStatusRevision,
    "Monotonic revision of one complete system-status snapshot."
);
define_status_id!(StatusEntryId, "Stable host identity of one status entry.");
define_status_id!(
    StatusActionId,
    "Opaque typed host action identity exposed by a status entry."
);
define_status_id!(
    StatusIconId,
    "Opaque logical status icon identity resolved by an approved asset boundary."
);

impl SystemStatusRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);
}

/// Bounded status text that is always redacted from `Debug` output.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusText(Box<str>);

impl StatusText {
    pub const MAX_BYTES: usize = 512;

    pub fn new(value: impl AsRef<str>) -> Result<Self, StatusTextError> {
        let value = value.as_ref();
        if value.trim().is_empty() || value.len() > Self::MAX_BYTES {
            return Err(StatusTextError::InvalidText);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StatusText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StatusText(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTextError {
    InvalidText,
}

impl fmt::Display for StatusTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("status text must be nonempty and at most 512 bytes")
    }
}

impl std::error::Error for StatusTextError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusEntryKind {
    Clock,
    Connectivity,
    Power,
    Audio,
    Input,
    Media,
    Session,
    Privacy,
    Extension,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatusSeverity {
    #[default]
    Normal,
    Attention,
    Critical,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatusPrivacy {
    #[default]
    Public,
    Sensitive,
    Secret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusActionKind {
    Primary,
    Toggle,
    OpenDetails,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusAction {
    id: StatusActionId,
    kind: StatusActionKind,
    label: StatusText,
    enabled: bool,
}

impl StatusAction {
    pub const fn new(
        id: StatusActionId,
        kind: StatusActionKind,
        label: StatusText,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            kind,
            label,
            enabled,
        }
    }

    pub const fn id(&self) -> StatusActionId {
        self.id
    }

    pub const fn kind(&self) -> StatusActionKind {
        self.kind
    }

    pub const fn label(&self) -> &StatusText {
        &self.label
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// One status indicator, media/session summary, or approved extension entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    id: StatusEntryId,
    kind: StatusEntryKind,
    label: StatusText,
    value: Option<StatusText>,
    icon: Option<StatusIconId>,
    severity: StatusSeverity,
    privacy: StatusPrivacy,
    active: bool,
    primary_action: Option<StatusActionId>,
    actions: Arc<[StatusAction]>,
}

impl StatusEntry {
    pub const MAX_ACTIONS: usize = 16;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StatusEntryId,
        kind: StatusEntryKind,
        label: StatusText,
        value: Option<StatusText>,
        icon: Option<StatusIconId>,
        severity: StatusSeverity,
        privacy: StatusPrivacy,
        active: bool,
        primary_action: Option<StatusActionId>,
        actions: Vec<StatusAction>,
    ) -> Result<Self, StatusEntryError> {
        if actions.len() > Self::MAX_ACTIONS {
            return Err(StatusEntryError::TooManyActions {
                count: actions.len(),
                max: Self::MAX_ACTIONS,
            });
        }
        let mut seen = HashSet::with_capacity(actions.len());
        if let Some(action) = actions
            .iter()
            .map(StatusAction::id)
            .find(|action| !seen.insert(*action))
        {
            return Err(StatusEntryError::DuplicateAction { action });
        }
        if let Some(primary_action) = primary_action
            && !seen.contains(&primary_action)
        {
            return Err(StatusEntryError::UnknownPrimaryAction {
                action: primary_action,
            });
        }
        Ok(Self {
            id,
            kind,
            label,
            value,
            icon,
            severity,
            privacy,
            active,
            primary_action,
            actions: actions.into(),
        })
    }

    pub const fn id(&self) -> StatusEntryId {
        self.id
    }

    pub const fn kind(&self) -> StatusEntryKind {
        self.kind
    }

    pub const fn label(&self) -> &StatusText {
        &self.label
    }

    pub const fn value(&self) -> Option<&StatusText> {
        self.value.as_ref()
    }

    pub const fn icon(&self) -> Option<StatusIconId> {
        self.icon
    }

    pub const fn severity(&self) -> StatusSeverity {
        self.severity
    }

    pub const fn privacy(&self) -> StatusPrivacy {
        self.privacy
    }

    pub const fn active(&self) -> bool {
        self.active
    }

    pub const fn primary_action(&self) -> Option<StatusActionId> {
        self.primary_action
    }

    pub fn actions(&self) -> &[StatusAction] {
        &self.actions
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusEntryError {
    TooManyActions { count: usize, max: usize },
    DuplicateAction { action: StatusActionId },
    UnknownPrimaryAction { action: StatusActionId },
}

impl fmt::Display for StatusEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyActions { count, max } => {
                write!(
                    formatter,
                    "status entry has {count} actions; maximum is {max}"
                )
            }
            Self::DuplicateAction { action } => {
                write!(
                    formatter,
                    "status action {} appears more than once",
                    action.get()
                )
            }
            Self::UnknownPrimaryAction { action } => write!(
                formatter,
                "primary status action {} is not in the action list",
                action.get()
            ),
        }
    }
}

impl std::error::Error for StatusEntryError {}

/// Complete ordered system-status snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemStatusSnapshot {
    revision: SystemStatusRevision,
    entries: Arc<[StatusEntry]>,
}

impl SystemStatusSnapshot {
    pub const MAX_ENTRIES: usize = 128;

    pub fn new(
        revision: SystemStatusRevision,
        entries: Vec<StatusEntry>,
    ) -> Result<Self, SystemStatusError> {
        if entries.len() > Self::MAX_ENTRIES {
            return Err(SystemStatusError::TooManyEntries {
                count: entries.len(),
                max: Self::MAX_ENTRIES,
            });
        }
        let mut entry_ids = HashSet::with_capacity(entries.len());
        if let Some(entry) = entries
            .iter()
            .map(StatusEntry::id)
            .find(|entry| !entry_ids.insert(*entry))
        {
            return Err(SystemStatusError::DuplicateEntry { entry });
        }
        let action_count = entries.iter().map(|entry| entry.actions.len()).sum();
        let mut action_ids = HashSet::with_capacity(action_count);
        if let Some(action) = entries
            .iter()
            .flat_map(|entry| entry.actions.iter().map(StatusAction::id))
            .find(|action| !action_ids.insert(*action))
        {
            return Err(SystemStatusError::DuplicateAction { action });
        }
        Ok(Self {
            revision,
            entries: entries.into(),
        })
    }

    pub const fn revision(&self) -> SystemStatusRevision {
        self.revision
    }

    pub fn entries(&self) -> &[StatusEntry] {
        &self.entries
    }

    pub fn entry(&self, id: StatusEntryId) -> Option<&StatusEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn action(&self, id: StatusActionId) -> Option<&StatusAction> {
        self.entries
            .iter()
            .flat_map(StatusEntry::actions)
            .find(|action| action.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemStatusError {
    TooManyEntries { count: usize, max: usize },
    DuplicateEntry { entry: StatusEntryId },
    DuplicateAction { action: StatusActionId },
}

impl fmt::Display for SystemStatusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEntries { count, max } => {
                write!(
                    formatter,
                    "system status has {count} entries; maximum is {max}"
                )
            }
            Self::DuplicateEntry { entry } => {
                write!(
                    formatter,
                    "status entry {} appears more than once",
                    entry.get()
                )
            }
            Self::DuplicateAction { action } => write!(
                formatter,
                "status action {} appears in more than one entry",
                action.get()
            ),
        }
    }
}

impl std::error::Error for SystemStatusError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: u64) -> StatusAction {
        StatusAction::new(
            StatusActionId::from_raw(id).unwrap(),
            StatusActionKind::OpenDetails,
            StatusText::new("Open details").unwrap(),
            true,
        )
    }

    fn entry(id: u64, action_id: u64) -> StatusEntry {
        StatusEntry::new(
            StatusEntryId::from_raw(id).unwrap(),
            StatusEntryKind::Connectivity,
            StatusText::new("Network").unwrap(),
            Some(StatusText::new("Connected").unwrap()),
            None,
            StatusSeverity::Normal,
            StatusPrivacy::Sensitive,
            true,
            Some(StatusActionId::from_raw(action_id).unwrap()),
            vec![action(action_id)],
        )
        .unwrap()
    }

    #[test]
    fn snapshot_preserves_order_and_resolves_globally_unique_actions() {
        let snapshot = SystemStatusSnapshot::new(
            SystemStatusRevision::from_raw(2).unwrap(),
            vec![entry(1, 11), entry(2, 12)],
        )
        .unwrap();

        assert_eq!(snapshot.entries()[0].id().get(), 1);
        assert_eq!(snapshot.entries()[1].id().get(), 2);
        assert_eq!(
            snapshot
                .action(StatusActionId::from_raw(12).unwrap())
                .unwrap()
                .id()
                .get(),
            12
        );
        let debug = format!("{snapshot:?}");
        assert!(!debug.contains("Connected"));
    }

    #[test]
    fn duplicate_entry_or_cross_entry_action_is_rejected() {
        assert!(matches!(
            SystemStatusSnapshot::new(
                SystemStatusRevision::INITIAL,
                vec![entry(1, 11), entry(1, 12)],
            ),
            Err(SystemStatusError::DuplicateEntry { .. })
        ));
        assert!(matches!(
            SystemStatusSnapshot::new(
                SystemStatusRevision::INITIAL,
                vec![entry(1, 11), entry(2, 11)],
            ),
            Err(SystemStatusError::DuplicateAction { .. })
        ));
    }
}
