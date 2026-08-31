use telorgon::shell::{
    InputSource, NotificationActionId, NotificationId, NotificationRevision, ShellCapabilities,
    SystemRequest,
};

#[test]
fn public_system_request_references_the_exact_host_action_snapshot() {
    let request = SystemRequest::NotificationAction {
        notification: NotificationId::from_raw(1).unwrap(),
        revision: NotificationRevision::from_raw(2).unwrap(),
        action: NotificationActionId::from_raw(3).unwrap(),
        source: InputSource::Accessibility,
    };

    assert_eq!(request.source(), InputSource::Accessibility);
    assert_eq!(
        request.required_capability(),
        ShellCapabilities::INVOKE_SYSTEM_ACTION
    );
    assert!(matches!(
        request,
        SystemRequest::NotificationAction {
            notification,
            revision,
            action,
            ..
        } if notification.get() == 1 && revision.get() == 2 && action.get() == 3
    ));
}
