use std::cell::Cell;
use std::num::{NonZeroU8, NonZeroU32};
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, MAX_NOTIFICATION_ACTIONS,
    MAX_NOTIFICATION_BADGE_COUNT, MAX_NOTIFICATION_TITLE_BYTES, NotificationAction,
    NotificationActionError, NotificationActionId, NotificationActionKind, NotificationActionLabel,
    NotificationAdmissionError, NotificationAuthorizationAdmission,
    NotificationAuthorizationApplied, NotificationAuthorizationOptions,
    NotificationAuthorizationOptionsError, NotificationAuthorizationRequest, NotificationBadge,
    NotificationBadgeAdmission, NotificationBadgeApplied, NotificationBadgeError,
    NotificationBadgeRequest, NotificationBody, NotificationCapability, NotificationDescriptor,
    NotificationDescriptorError, NotificationId, NotificationLimits, NotificationOperations,
    NotificationPriority, NotificationPrivacy, NotificationPublicationAdmission,
    NotificationPublicationApplied, NotificationPublicationError, NotificationPublicationRequest,
    NotificationRemovalAdmission, NotificationRemovalApplied, NotificationRemovalRequest,
    NotificationReply, NotificationResponseAdmissionError, NotificationResponseEvent,
    NotificationResponseRequest, NotificationResponseSource, NotificationRevision,
    NotificationService, NotificationServiceKey, NotificationSnapshotId, NotificationTextError,
    NotificationTitle, PermissionState, RequestId, RequestOutcome, ServiceLookup, ServiceRegistry,
    Support, UserGestureRequirement,
};

fn notification_id(raw: u64) -> NotificationId {
    NotificationId::from_raw(raw).unwrap()
}

fn action_id(raw: u64) -> NotificationActionId {
    NotificationActionId::from_raw(raw).unwrap()
}

fn visible_action(raw: u64, kind: NotificationActionKind, text: &str) -> NotificationAction {
    NotificationAction::new(
        action_id(raw),
        kind,
        Some(NotificationActionLabel::new(text).unwrap()),
    )
    .unwrap()
}

fn default_action(raw: u64) -> NotificationAction {
    NotificationAction::new(action_id(raw), NotificationActionKind::Default, None).unwrap()
}

fn descriptor(id: u64, revision: NotificationRevision, body: &str) -> NotificationDescriptor {
    NotificationDescriptor::new(
        NotificationSnapshotId::new(notification_id(id), revision),
        NotificationTitle::new("Private account event").unwrap(),
        Some(NotificationBody::new(body).unwrap()),
        NotificationPriority::High,
        NotificationPrivacy::Sensitive,
        vec![
            default_action(1),
            visible_action(2, NotificationActionKind::Open, "Open private account"),
            visible_action(3, NotificationActionKind::Reply, "Reply privately"),
        ],
    )
    .unwrap()
}

#[test]
fn content_actions_and_descriptor_metadata_are_bounded_typed_and_redacted() {
    let sensitive_title = "Private payroll alert";
    let title = NotificationTitle::new(sensitive_title).unwrap();
    assert_eq!(title.as_str(), sensitive_title);
    assert!(!format!("{title:?}").contains(sensitive_title));
    assert_eq!(
        NotificationTitle::new(""),
        Err(NotificationTextError::Empty)
    );
    assert_eq!(
        NotificationTitle::new("x".repeat(MAX_NOTIFICATION_TITLE_BYTES + 1)),
        Err(NotificationTextError::TooLong {
            byte_len: MAX_NOTIFICATION_TITLE_BYTES + 1,
            maximum_bytes: MAX_NOTIFICATION_TITLE_BYTES,
        })
    );

    let private_label = NotificationActionLabel::new("Private operation").unwrap();
    assert!(!format!("{private_label:?}").contains("Private operation"));
    assert_eq!(
        NotificationAction::new(
            action_id(10),
            NotificationActionKind::Default,
            Some(private_label),
        ),
        Err(NotificationActionError::DefaultActionHasLabel)
    );
    assert_eq!(
        NotificationAction::new(action_id(10), NotificationActionKind::Open, None),
        Err(NotificationActionError::VisibleActionMissingLabel)
    );

    let notification = descriptor(
        9,
        NotificationRevision::INITIAL,
        "Private account and token details",
    );
    assert_eq!(notification.actions().len(), 3);
    assert_eq!(
        notification.action(action_id(3)).unwrap().kind(),
        NotificationActionKind::Reply
    );
    let debug = format!("{notification:?}");
    assert!(debug.contains("action_count"));
    assert!(!debug.contains("Private account"));
    assert!(!debug.contains("token details"));

    assert!(matches!(
        NotificationDescriptor::new(
            NotificationSnapshotId::new(notification_id(1), NotificationRevision::INITIAL),
            NotificationTitle::new("Notice").unwrap(),
            None,
            NotificationPriority::Normal,
            NotificationPrivacy::Public,
            vec![default_action(1), default_action(1)],
        ),
        Err(NotificationDescriptorError::DuplicateAction { .. })
    ));
    let too_many = (0..=MAX_NOTIFICATION_ACTIONS)
        .map(|index| visible_action(100 + index as u64, NotificationActionKind::Custom, "Action"))
        .collect();
    assert!(matches!(
        NotificationDescriptor::new(
            NotificationSnapshotId::new(notification_id(1), NotificationRevision::INITIAL),
            NotificationTitle::new("Notice").unwrap(),
            None,
            NotificationPriority::Normal,
            NotificationPrivacy::Public,
            too_many,
        ),
        Err(NotificationDescriptorError::TooManyActions { .. })
    ));
}

#[test]
fn publication_removal_badges_and_authorization_preserve_exact_linear_intentions() {
    let current = descriptor(20, NotificationRevision::INITIAL, "Private initial body");
    let initial = NotificationPublicationRequest::initial(current.clone()).unwrap();
    assert_eq!(initial.previous(), None);
    assert!(!format!("{initial:?}").contains("Private initial"));
    assert_eq!(
        NotificationPublicationApplied::from_request(&initial).snapshot(),
        current.snapshot()
    );

    let revision_2 = NotificationRevision::INITIAL.checked_next().unwrap();
    let next = descriptor(20, revision_2, "Private updated body");
    let update = NotificationPublicationRequest::advance(current.snapshot(), next).unwrap();
    assert_eq!(update.previous(), Some(current.snapshot()));
    assert_eq!(update.descriptor().snapshot().revision(), revision_2);

    assert!(matches!(
        NotificationPublicationRequest::initial(descriptor(20, revision_2, "Wrong initial")),
        Err(NotificationPublicationError::InitialRevisionRequired { .. })
    ));
    assert!(matches!(
        NotificationPublicationRequest::advance(
            current.snapshot(),
            descriptor(21, revision_2, "Wrong identity"),
        ),
        Err(NotificationPublicationError::NotificationMismatch { .. })
    ));

    let removal = NotificationRemovalRequest::new(current.snapshot());
    assert_eq!(
        NotificationRemovalApplied::from_request(removal).removed(),
        current.snapshot()
    );
    let badge = NotificationBadge::count(NonZeroU32::new(42).unwrap()).unwrap();
    let badge_request = NotificationBadgeRequest::new(badge);
    assert_eq!(
        NotificationBadgeApplied::from_request(badge_request).badge(),
        badge
    );
    assert_eq!(
        NotificationBadge::count(NonZeroU32::new(MAX_NOTIFICATION_BADGE_COUNT + 1).unwrap()),
        Err(NotificationBadgeError::CountTooLarge {
            supplied: MAX_NOTIFICATION_BADGE_COUNT + 1,
            maximum: MAX_NOTIFICATION_BADGE_COUNT,
        })
    );

    assert_eq!(
        NotificationAuthorizationOptions::new(false, false, false, false),
        Err(NotificationAuthorizationOptionsError::Empty)
    );
    assert_eq!(
        NotificationAuthorizationOptions::new(false, false, false, true),
        Err(NotificationAuthorizationOptionsError::CriticalAlertsRequireAlerts)
    );
    let options = NotificationAuthorizationOptions::new(true, true, false, true).unwrap();
    let request = NotificationAuthorizationRequest::new(options);
    assert!(!request.has_user_gesture());
    assert!(format!("{request:?}").contains("has_user_gesture"));
    let result = NotificationAuthorizationApplied::new(PermissionState::Granted);
    assert_eq!(result.permission(), PermissionState::Granted);
}

#[test]
fn response_events_require_the_exact_snapshot_action_and_source_relationship() {
    let current = descriptor(30, NotificationRevision::INITIAL, "Private response body");
    let reply_text = "Private reply content";
    let reply = NotificationResponseEvent::admit(
        &current,
        NotificationResponseRequest::new(
            current.snapshot(),
            Some(action_id(3)),
            NotificationResponseSource::InlineReply,
            Some(NotificationReply::new(reply_text).unwrap()),
        ),
    )
    .unwrap();
    assert_eq!(reply.snapshot(), current.snapshot());
    assert_eq!(reply.action(), Some(action_id(3)));
    assert_eq!(reply.source(), NotificationResponseSource::InlineReply);
    assert_eq!(reply.reply().unwrap().as_str(), reply_text);
    assert!(!format!("{reply:?}").contains(reply_text));

    let body = NotificationResponseEvent::admit(
        &current,
        NotificationResponseRequest::new(
            current.snapshot(),
            Some(action_id(1)),
            NotificationResponseSource::Body,
            None,
        ),
    )
    .unwrap();
    assert_eq!(body.action(), Some(action_id(1)));
    let dismissed = NotificationResponseEvent::admit(
        &current,
        NotificationResponseRequest::new(
            current.snapshot(),
            None,
            NotificationResponseSource::DismissedByUser,
            None,
        ),
    )
    .unwrap();
    assert!(dismissed.action().is_none());

    assert!(matches!(
        NotificationResponseEvent::admit(
            &current,
            NotificationResponseRequest::new(
                NotificationSnapshotId::new(
                    notification_id(30),
                    NotificationRevision::INITIAL.checked_next().unwrap(),
                ),
                Some(action_id(1)),
                NotificationResponseSource::Body,
                None,
            ),
        ),
        Err(NotificationResponseAdmissionError::StaleRevision { .. })
    ));
    assert!(matches!(
        NotificationResponseEvent::admit(
            &current,
            NotificationResponseRequest::new(
                current.snapshot(),
                Some(action_id(99)),
                NotificationResponseSource::Action,
                None,
            ),
        ),
        Err(NotificationResponseAdmissionError::UnknownAction { .. })
    ));
    assert!(matches!(
        NotificationResponseEvent::admit(
            &current,
            NotificationResponseRequest::new(
                current.snapshot(),
                Some(action_id(2)),
                NotificationResponseSource::InlineReply,
                Some(NotificationReply::new("Reply").unwrap()),
            ),
        ),
        Err(NotificationResponseAdmissionError::ReplyActionRequired)
    ));
    assert!(matches!(
        NotificationResponseEvent::admit(
            &current,
            NotificationResponseRequest::new(
                current.snapshot(),
                Some(action_id(2)),
                NotificationResponseSource::DismissedBySystem,
                None,
            ),
        ),
        Err(NotificationResponseAdmissionError::UnexpectedAction)
    ));
}

struct FixtureNotificationService {
    capability: NotificationCapability,
    observed: NotificationSnapshotId,
    next_request: Cell<u64>,
}

impl FixtureNotificationService {
    fn admit<T>(&self) -> AdmittedRequest<T> {
        let request = self.next_request.get() + 1;
        self.next_request.set(request);
        AdmittedRequest::new(RequestId::from_raw(request).unwrap())
    }
}

impl NotificationService for FixtureNotificationService {
    fn capability(&self) -> Support<NotificationCapability> {
        Support::Available(self.capability)
    }

    fn authorize(
        &self,
        _request: NotificationAuthorizationRequest,
    ) -> NotificationAuthorizationAdmission {
        if !self
            .capability
            .operations()
            .supports_authorization_request()
        {
            return Err(NotificationAdmissionError::UnsupportedOperation);
        }
        Ok(self.admit())
    }

    fn publish(&self, request: NotificationPublicationRequest) -> NotificationPublicationAdmission {
        let operations = *self.capability.operations();
        if request.previous().is_some() && !operations.supports_update()
            || request.previous().is_none() && !operations.supports_publish()
        {
            return Err(NotificationAdmissionError::UnsupportedOperation);
        }
        if self.capability.permission().blocks_use() {
            return Err(NotificationAdmissionError::PermissionDenied);
        }
        if self.capability.permission().requires_prompt() {
            return Err(NotificationAdmissionError::AuthorizationRequired);
        }
        let descriptor = request.descriptor();
        if !descriptor.actions().is_empty() && !operations.supports_actions() {
            return Err(NotificationAdmissionError::ActionsUnsupported);
        }
        if descriptor.reply_action_count() > 0 && !operations.supports_inline_reply() {
            return Err(NotificationAdmissionError::InlineReplyUnsupported);
        }
        let limits = *self.capability.limits();
        if descriptor.actions().len() > limits.maximum_actions().get() as usize {
            return Err(NotificationAdmissionError::ActionsExceedCapability);
        }
        if descriptor
            .body()
            .is_some_and(|body| body.byte_len() > limits.maximum_body_bytes().get() as usize)
        {
            return Err(NotificationAdmissionError::BodyExceedsCapability);
        }
        if request.previous() != Some(self.observed) {
            return Err(NotificationAdmissionError::RevisionMismatch {
                expected: request
                    .previous()
                    .unwrap_or(request.descriptor().snapshot()),
                observed: Some(self.observed),
            });
        }
        Ok(self.admit())
    }

    fn remove(&self, request: NotificationRemovalRequest) -> NotificationRemovalAdmission {
        if !self.capability.operations().supports_remove() {
            return Err(NotificationAdmissionError::UnsupportedOperation);
        }
        if request.expected() != self.observed {
            return Err(NotificationAdmissionError::RevisionMismatch {
                expected: request.expected(),
                observed: Some(self.observed),
            });
        }
        Ok(self.admit())
    }

    fn set_badge(&self, request: NotificationBadgeRequest) -> NotificationBadgeAdmission {
        if !self.capability.operations().supports_badges() {
            return Err(NotificationAdmissionError::UnsupportedOperation);
        }
        if request
            .badge()
            .value()
            .is_some_and(|count| count > self.capability.limits().maximum_badge_count())
        {
            return Err(NotificationAdmissionError::BadgeExceedsCapability);
        }
        Ok(self.admit())
    }
}

#[test]
fn service_capability_admissions_completions_and_registry_are_object_safe() {
    let current = descriptor(
        40,
        NotificationRevision::INITIAL,
        "Private current service body",
    );
    let capability = CapabilityDescriptor::new(
        NotificationOperations::new(true, true, true, true, true, true, true, true),
        NotificationLimits::new(
            NonZeroU8::new(4).unwrap(),
            NonZeroU32::new(512).unwrap(),
            NonZeroU32::new(512).unwrap(),
            NonZeroU32::new(99).unwrap(),
        )
        .unwrap(),
        PermissionState::Granted,
        ExecutionRequirement::HostExecutor,
        UserGestureRequirement::NotRequired,
    );
    let service: Rc<dyn NotificationService> = Rc::new(FixtureNotificationService {
        capability,
        observed: current.snapshot(),
        next_request: Cell::new(80),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<NotificationServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<NotificationServiceKey>() else {
        panic!("registered notification service must be available");
    };
    assert!(service.capability().is_available());

    let authorization = service
        .authorize(NotificationAuthorizationRequest::new(
            NotificationAuthorizationOptions::new(true, true, false, false).unwrap(),
        ))
        .unwrap()
        .complete(RequestOutcome::Applied(
            NotificationAuthorizationApplied::new(PermissionState::Granted),
        ));
    assert_eq!(authorization.request_id().get(), 81);
    assert_eq!(
        authorization.outcome().applied().unwrap().permission(),
        PermissionState::Granted
    );

    let next_revision = NotificationRevision::INITIAL.checked_next().unwrap();
    let next = descriptor(40, next_revision, "Private next service body");
    let publication = NotificationPublicationRequest::advance(current.snapshot(), next).unwrap();
    let applied = NotificationPublicationApplied::from_request(&publication);
    let completion = service
        .publish(publication)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id().get(), 82);
    assert_eq!(
        completion
            .outcome()
            .applied()
            .unwrap()
            .snapshot()
            .revision(),
        next_revision
    );

    let removal = NotificationRemovalRequest::new(current.snapshot());
    let removed = NotificationRemovalApplied::from_request(removal);
    assert!(
        service
            .remove(removal)
            .unwrap()
            .complete(RequestOutcome::Applied(removed))
            .outcome()
            .is_applied()
    );
    let badge_request = NotificationBadgeRequest::new(
        NotificationBadge::count(NonZeroU32::new(7).unwrap()).unwrap(),
    );
    let badge_applied = NotificationBadgeApplied::from_request(badge_request);
    assert!(
        service
            .set_badge(badge_request)
            .unwrap()
            .complete(RequestOutcome::Applied(badge_applied))
            .outcome()
            .is_applied()
    );
}
