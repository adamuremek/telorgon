//! Typed invocation requests for host-described application and system actions.

use crate::shell::{
    ApplicationActionId, ApplicationId, ApplicationRevision, InputSource, NotificationActionId,
    NotificationId, NotificationRevision, ShellCapabilities, StatusActionId, StatusEntryId,
    SystemStatusRevision,
};

/// An invocation of an action identity published by an exact host snapshot revision.
///
/// The host validates identity, revision, enabled state, session/lock policy, and input causality.
/// Constructing a request performs no launch, dismissal, service mutation, or optimistic update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemRequest {
    ApplicationAction {
        application: ApplicationId,
        revision: ApplicationRevision,
        action: ApplicationActionId,
        source: InputSource,
    },
    NotificationAction {
        notification: NotificationId,
        revision: NotificationRevision,
        action: NotificationActionId,
        source: InputSource,
    },
    StatusAction {
        revision: SystemStatusRevision,
        entry: StatusEntryId,
        action: StatusActionId,
        source: InputSource,
    },
}

impl SystemRequest {
    pub const fn required_capability(self) -> ShellCapabilities {
        ShellCapabilities::INVOKE_SYSTEM_ACTION
    }

    pub const fn source(self) -> InputSource {
        match self {
            Self::ApplicationAction { source, .. }
            | Self::NotificationAction { source, .. }
            | Self::StatusAction { source, .. } => source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_action_retains_its_parent_revision_and_source() {
        let application = SystemRequest::ApplicationAction {
            application: ApplicationId::from_raw(1).unwrap(),
            revision: ApplicationRevision::from_raw(10).unwrap(),
            action: ApplicationActionId::from_raw(11).unwrap(),
            source: InputSource::Keyboard,
        };
        let notification = SystemRequest::NotificationAction {
            notification: NotificationId::from_raw(2).unwrap(),
            revision: NotificationRevision::from_raw(20).unwrap(),
            action: NotificationActionId::from_raw(21).unwrap(),
            source: InputSource::Touch,
        };
        let status = SystemRequest::StatusAction {
            revision: SystemStatusRevision::from_raw(30).unwrap(),
            entry: StatusEntryId::from_raw(31).unwrap(),
            action: StatusActionId::from_raw(32).unwrap(),
            source: InputSource::Accessibility,
        };

        assert!(matches!(
            application,
            SystemRequest::ApplicationAction {
                revision,
                action,
                ..
            } if revision.get() == 10 && action.get() == 11
        ));
        assert!(matches!(
            notification,
            SystemRequest::NotificationAction {
                revision,
                action,
                ..
            } if revision.get() == 20 && action.get() == 21
        ));
        assert!(matches!(
            status,
            SystemRequest::StatusAction {
                revision,
                entry,
                action,
                ..
            } if revision.get() == 30 && entry.get() == 31 && action.get() == 32
        ));
        assert_eq!(application.source(), InputSource::Keyboard);
        assert_eq!(notification.source(), InputSource::Touch);
        assert_eq!(status.source(), InputSource::Accessibility);
    }

    #[test]
    fn every_host_described_action_requires_system_action_authority() {
        let request = SystemRequest::StatusAction {
            revision: SystemStatusRevision::INITIAL,
            entry: StatusEntryId::from_raw(1).unwrap(),
            action: StatusActionId::from_raw(2).unwrap(),
            source: InputSource::Programmatic,
        };

        assert_eq!(
            request.required_capability(),
            ShellCapabilities::INVOKE_SYSTEM_ACTION
        );
    }
}
