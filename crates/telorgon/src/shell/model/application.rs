//! Immutable application and launcher entries supplied by a shell policy host.

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;

use crate::shell::ApplicationId;

macro_rules! define_application_id {
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

define_application_id!(
    ApplicationRevision,
    "Monotonic revision of one host application entry."
);
define_application_id!(
    ApplicationActionId,
    "Opaque typed host action identity exposed by an application entry."
);
define_application_id!(
    ApplicationIconId,
    "Opaque logical icon identity resolved by the shell's approved asset boundary."
);

impl ApplicationRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplicationLabel(Box<str>);

impl ApplicationLabel {
    pub const MAX_BYTES: usize = 256;

    pub fn new(value: impl AsRef<str>) -> Result<Self, ApplicationTextError> {
        let value = value.as_ref();
        if value.trim().is_empty() || value.len() > Self::MAX_BYTES {
            return Err(ApplicationTextError::InvalidLabel);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApplicationLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ApplicationLabel")
            .field(&self.as_str())
            .finish()
    }
}

impl fmt::Display for ApplicationLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplicationDescription(Box<str>);

impl ApplicationDescription {
    pub const MAX_BYTES: usize = 1024;

    pub fn new(value: impl AsRef<str>) -> Result<Self, ApplicationTextError> {
        let value = value.as_ref();
        if value.trim().is_empty() || value.len() > Self::MAX_BYTES {
            return Err(ApplicationTextError::InvalidDescription);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApplicationDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApplicationDescription(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationTextError {
    InvalidLabel,
    InvalidDescription,
}

impl fmt::Display for ApplicationTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLabel => "application label must be nonempty and at most 256 bytes",
            Self::InvalidDescription => {
                "application description must be nonempty and at most 1024 bytes"
            }
        })
    }
}

impl std::error::Error for ApplicationTextError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApplicationActionKind {
    Launch,
    Activate,
    NewInstance,
    Custom,
}

/// One host-described launcher operation. Invoking it remains a typed system request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationAction {
    id: ApplicationActionId,
    kind: ApplicationActionKind,
    label: ApplicationLabel,
    enabled: bool,
}

impl ApplicationAction {
    pub const fn new(
        id: ApplicationActionId,
        kind: ApplicationActionKind,
        label: ApplicationLabel,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            kind,
            label,
            enabled,
        }
    }

    pub const fn id(&self) -> ApplicationActionId {
        self.id
    }

    pub const fn kind(&self) -> ApplicationActionKind {
        self.kind
    }

    pub const fn label(&self) -> &ApplicationLabel {
        &self.label
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ApplicationStates(u8);

impl ApplicationStates {
    pub const NONE: Self = Self(0);
    pub const RUNNING: Self = Self(1 << 0);
    pub const ACTIVE: Self = Self(1 << 1);
    pub const URGENT: Self = Self(1 << 2);
    pub const PINNED: Self = Self(1 << 3);
    const ALL_BITS: u8 = (1 << 4) - 1;

    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::ALL_BITS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl fmt::Debug for ApplicationStates {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApplicationStates")
            .field("bits", &format_args!("{:#06b}", self.bits()))
            .finish()
    }
}

impl std::ops::BitOr for ApplicationStates {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

/// Complete host-described launcher entry without process discovery or command execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEntry {
    id: ApplicationId,
    revision: ApplicationRevision,
    label: ApplicationLabel,
    description: Option<ApplicationDescription>,
    icon: Option<ApplicationIconId>,
    states: ApplicationStates,
    primary_action: Option<ApplicationActionId>,
    actions: Arc<[ApplicationAction]>,
}

impl ApplicationEntry {
    pub const MAX_ACTIONS: usize = 32;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ApplicationId,
        revision: ApplicationRevision,
        label: ApplicationLabel,
        description: Option<ApplicationDescription>,
        icon: Option<ApplicationIconId>,
        states: ApplicationStates,
        primary_action: Option<ApplicationActionId>,
        actions: Vec<ApplicationAction>,
    ) -> Result<Self, ApplicationEntryError> {
        if actions.len() > Self::MAX_ACTIONS {
            return Err(ApplicationEntryError::TooManyActions {
                count: actions.len(),
                max: Self::MAX_ACTIONS,
            });
        }
        let mut seen = HashSet::with_capacity(actions.len());
        if let Some(action) = actions
            .iter()
            .map(ApplicationAction::id)
            .find(|action| !seen.insert(*action))
        {
            return Err(ApplicationEntryError::DuplicateAction { action });
        }
        if let Some(primary_action) = primary_action
            && !seen.contains(&primary_action)
        {
            return Err(ApplicationEntryError::UnknownPrimaryAction {
                action: primary_action,
            });
        }
        Ok(Self {
            id,
            revision,
            label,
            description,
            icon,
            states,
            primary_action,
            actions: actions.into(),
        })
    }

    pub const fn id(&self) -> ApplicationId {
        self.id
    }

    pub const fn revision(&self) -> ApplicationRevision {
        self.revision
    }

    pub const fn label(&self) -> &ApplicationLabel {
        &self.label
    }

    pub const fn description(&self) -> Option<&ApplicationDescription> {
        self.description.as_ref()
    }

    pub const fn icon(&self) -> Option<ApplicationIconId> {
        self.icon
    }

    pub const fn states(&self) -> ApplicationStates {
        self.states
    }

    pub const fn primary_action(&self) -> Option<ApplicationActionId> {
        self.primary_action
    }

    pub fn actions(&self) -> &[ApplicationAction] {
        &self.actions
    }

    pub fn action(&self, id: ApplicationActionId) -> Option<&ApplicationAction> {
        self.actions.iter().find(|action| action.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationEntryError {
    TooManyActions { count: usize, max: usize },
    DuplicateAction { action: ApplicationActionId },
    UnknownPrimaryAction { action: ApplicationActionId },
}

impl fmt::Display for ApplicationEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyActions { count, max } => {
                write!(
                    formatter,
                    "application has {count} actions; maximum is {max}"
                )
            }
            Self::DuplicateAction { action } => {
                write!(
                    formatter,
                    "application action {} appears more than once",
                    action.get()
                )
            }
            Self::UnknownPrimaryAction { action } => write!(
                formatter,
                "primary application action {} is not in the action list",
                action.get()
            ),
        }
    }
}

impl std::error::Error for ApplicationEntryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: u64, kind: ApplicationActionKind) -> ApplicationAction {
        ApplicationAction::new(
            ApplicationActionId::from_raw(id).unwrap(),
            kind,
            ApplicationLabel::new(format!("Action {id}")).unwrap(),
            true,
        )
    }

    #[test]
    fn entry_preserves_only_host_labels_state_assets_and_typed_actions() {
        let primary = ApplicationActionId::from_raw(2).unwrap();
        let entry = ApplicationEntry::new(
            ApplicationId::from_raw(1).unwrap(),
            ApplicationRevision::from_raw(3).unwrap(),
            ApplicationLabel::new("Browser").unwrap(),
            Some(ApplicationDescription::new("Browse the web").unwrap()),
            Some(ApplicationIconId::from_raw(4).unwrap()),
            ApplicationStates::RUNNING | ApplicationStates::PINNED,
            Some(primary),
            vec![
                action(2, ApplicationActionKind::Activate),
                action(3, ApplicationActionKind::NewInstance),
            ],
        )
        .unwrap();

        assert_eq!(entry.label().as_str(), "Browser");
        assert_eq!(entry.primary_action(), Some(primary));
        assert_eq!(entry.actions()[0].kind(), ApplicationActionKind::Activate);
        assert!(entry.states().contains(ApplicationStates::RUNNING));
    }

    #[test]
    fn actions_are_bounded_unique_and_primary_is_referentially_valid() {
        let duplicate = action(2, ApplicationActionKind::Launch);
        assert!(matches!(
            ApplicationEntry::new(
                ApplicationId::from_raw(1).unwrap(),
                ApplicationRevision::INITIAL,
                ApplicationLabel::new("Editor").unwrap(),
                None,
                None,
                ApplicationStates::NONE,
                None,
                vec![duplicate.clone(), duplicate],
            ),
            Err(ApplicationEntryError::DuplicateAction { .. })
        ));
        assert!(matches!(
            ApplicationEntry::new(
                ApplicationId::from_raw(1).unwrap(),
                ApplicationRevision::INITIAL,
                ApplicationLabel::new("Editor").unwrap(),
                None,
                None,
                ApplicationStates::NONE,
                Some(ApplicationActionId::from_raw(9).unwrap()),
                vec![],
            ),
            Err(ApplicationEntryError::UnknownPrimaryAction { .. })
        ));
    }
}
