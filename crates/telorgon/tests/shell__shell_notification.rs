use telorgon::shell::{
    NotificationAction, NotificationActionId, NotificationActionKind, NotificationDeliveryState,
    NotificationId, NotificationLifecycle, NotificationPersistence, NotificationPriority,
    NotificationPrivacy, NotificationRevision, NotificationSnapshot, NotificationSnapshotError,
    NotificationText,
};

#[test]
fn public_notification_is_revisioned_bounded_and_debug_redacted() {
    let action = NotificationAction::new(
        NotificationActionId::from_raw(3).unwrap(),
        NotificationActionKind::Open,
        NotificationText::new("Open").unwrap(),
        true,
    );
    let notification = NotificationSnapshot::new(
        NotificationId::from_raw(1).unwrap(),
        NotificationRevision::from_raw(2).unwrap(),
        None,
        NotificationText::new("Private title").unwrap(),
        Some(NotificationText::new("Private body").unwrap()),
        None,
        NotificationPriority::Critical,
        NotificationPrivacy::Secret,
        NotificationLifecycle {
            persistence: NotificationPersistence::Persistent,
            delivery: NotificationDeliveryState::New,
        },
        vec![action.clone()],
    )
    .unwrap();

    assert_eq!(notification.actions(), std::slice::from_ref(&action));
    assert!(!format!("{notification:?}").contains("Private body"));
    assert!(matches!(
        NotificationSnapshot::new(
            NotificationId::from_raw(1).unwrap(),
            NotificationRevision::from_raw(3).unwrap(),
            None,
            NotificationText::new("Title").unwrap(),
            None,
            None,
            NotificationPriority::Normal,
            NotificationPrivacy::Public,
            NotificationLifecycle::default(),
            vec![action.clone(), action],
        ),
        Err(NotificationSnapshotError::DuplicateAction { .. })
    ));
}
