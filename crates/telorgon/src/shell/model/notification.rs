//! Privacy-aware, immutable notification snapshots supplied by a shell policy host.

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;

use crate::shell::ApplicationId;

macro_rules! define_notification_id {
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

define_notification_id!(NotificationId, "Stable host notification identity.");
define_notification_id!(
    NotificationRevision,
    "Monotonic revision of one host notification snapshot."
);
define_notification_id!(
    NotificationActionId,
    "Opaque typed host action identity exposed by a notification."
);
define_notification_id!(
    NotificationIconId,
    "Opaque logical notification icon identity resolved by an approved asset boundary."
);

impl NotificationRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);
}

/// Bounded notification presentation text that is always redacted from `Debug` output.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotificationText(Box<str>);

impl NotificationText {
    pub const MAX_BYTES: usize = 2048;

    pub fn new(value: impl AsRef<str>) -> Result<Self, NotificationTextError> {
        let value = value.as_ref();
        if value.trim().is_empty() || value.len() > Self::MAX_BYTES {
            return Err(NotificationTextError::InvalidText);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NotificationText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NotificationText(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationTextError {
    InvalidText,
}

impl fmt::Display for NotificationTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("notification text must be nonempty and at most 2048 bytes")
    }
}

impl std::error::Error for NotificationTextError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NotificationPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// Host classification controlling where notification content may be presented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NotificationPrivacy {
    #[default]
    Public,
    Sensitive,
    Secret,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NotificationPersistence {
    #[default]
    Transient,
    Persistent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NotificationDeliveryState {
    #[default]
    New,
    Presented,
    Acknowledged,
}

/// Lifecycle facts observed from the host. They do not schedule expiry or perform dismissal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NotificationLifecycle {
    pub persistence: NotificationPersistence,
    pub delivery: NotificationDeliveryState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NotificationActionKind {
    Default,
    Open,
    Reply,
    Dismiss,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationAction {
    id: NotificationActionId,
    kind: NotificationActionKind,
    label: NotificationText,
    enabled: bool,
}

impl NotificationAction {
    pub const fn new(
        id: NotificationActionId,
        kind: NotificationActionKind,
        label: NotificationText,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            kind,
            label,
            enabled,
        }
    }

    pub const fn id(&self) -> NotificationActionId {
        self.id
    }

    pub const fn kind(&self) -> NotificationActionKind {
        self.kind
    }

    pub const fn label(&self) -> &NotificationText {
        &self.label
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// Complete host notification record without delivery, persistence, or action execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSnapshot {
    id: NotificationId,
    revision: NotificationRevision,
    source: Option<ApplicationId>,
    title: NotificationText,
    body: Option<NotificationText>,
    icon: Option<NotificationIconId>,
    priority: NotificationPriority,
    privacy: NotificationPrivacy,
    lifecycle: NotificationLifecycle,
    actions: Arc<[NotificationAction]>,
}

impl NotificationSnapshot {
    pub const MAX_ACTIONS: usize = 16;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: NotificationId,
        revision: NotificationRevision,
        source: Option<ApplicationId>,
        title: NotificationText,
        body: Option<NotificationText>,
        icon: Option<NotificationIconId>,
        priority: NotificationPriority,
        privacy: NotificationPrivacy,
        lifecycle: NotificationLifecycle,
        actions: Vec<NotificationAction>,
    ) -> Result<Self, NotificationSnapshotError> {
        if actions.len() > Self::MAX_ACTIONS {
            return Err(NotificationSnapshotError::TooManyActions {
                count: actions.len(),
                max: Self::MAX_ACTIONS,
            });
        }
        let mut seen = HashSet::with_capacity(actions.len());
        if let Some(action) = actions
            .iter()
            .map(NotificationAction::id)
            .find(|action| !seen.insert(*action))
        {
            return Err(NotificationSnapshotError::DuplicateAction { action });
        }
        Ok(Self {
            id,
            revision,
            source,
            title,
            body,
            icon,
            priority,
            privacy,
            lifecycle,
            actions: actions.into(),
        })
    }

    pub const fn id(&self) -> NotificationId {
        self.id
    }

    pub const fn revision(&self) -> NotificationRevision {
        self.revision
    }

    pub const fn source(&self) -> Option<ApplicationId> {
        self.source
    }

    pub const fn title(&self) -> &NotificationText {
        &self.title
    }

    pub const fn body(&self) -> Option<&NotificationText> {
        self.body.as_ref()
    }

    pub const fn icon(&self) -> Option<NotificationIconId> {
        self.icon
    }

    pub const fn priority(&self) -> NotificationPriority {
        self.priority
    }

    pub const fn privacy(&self) -> NotificationPrivacy {
        self.privacy
    }

    pub const fn lifecycle(&self) -> NotificationLifecycle {
        self.lifecycle
    }

    pub fn actions(&self) -> &[NotificationAction] {
        &self.actions
    }

    pub fn action(&self, id: NotificationActionId) -> Option<&NotificationAction> {
        self.actions.iter().find(|action| action.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationSnapshotError {
    TooManyActions { count: usize, max: usize },
    DuplicateAction { action: NotificationActionId },
}

impl fmt::Display for NotificationSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyActions { count, max } => {
                write!(
                    formatter,
                    "notification has {count} actions; maximum is {max}"
                )
            }
            Self::DuplicateAction { action } => write!(
                formatter,
                "notification action {} appears more than once",
                action.get()
            ),
        }
    }
}

impl std::error::Error for NotificationSnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: u64) -> NotificationAction {
        NotificationAction::new(
            NotificationActionId::from_raw(id).unwrap(),
            NotificationActionKind::Open,
            NotificationText::new("Open").unwrap(),
            true,
        )
    }

    #[test]
    fn snapshot_retains_priority_privacy_lifecycle_and_typed_actions() {
        let snapshot = NotificationSnapshot::new(
            NotificationId::from_raw(1).unwrap(),
            NotificationRevision::from_raw(2).unwrap(),
            Some(ApplicationId::from_raw(3).unwrap()),
            NotificationText::new("New message").unwrap(),
            Some(NotificationText::new("Sensitive body").unwrap()),
            None,
            NotificationPriority::High,
            NotificationPrivacy::Sensitive,
            NotificationLifecycle {
                persistence: NotificationPersistence::Persistent,
                delivery: NotificationDeliveryState::Presented,
            },
            vec![action(4)],
        )
        .unwrap();

        assert_eq!(snapshot.priority(), NotificationPriority::High);
        assert_eq!(snapshot.privacy(), NotificationPrivacy::Sensitive);
        assert_eq!(snapshot.actions()[0].id().get(), 4);
        let debug = format!("{snapshot:?}");
        assert!(!debug.contains("New message"));
        assert!(!debug.contains("Sensitive body"));
    }

    #[test]
    fn duplicate_actions_are_rejected() {
        assert!(matches!(
            NotificationSnapshot::new(
                NotificationId::from_raw(1).unwrap(),
                NotificationRevision::INITIAL,
                None,
                NotificationText::new("Notice").unwrap(),
                None,
                None,
                NotificationPriority::Normal,
                NotificationPrivacy::Public,
                NotificationLifecycle::default(),
                vec![action(2), action(2)],
            ),
            Err(NotificationSnapshotError::DuplicateAction { .. })
        ));
    }
}
