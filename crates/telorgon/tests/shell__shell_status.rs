use telorgon::shell::{
    StatusAction, StatusActionId, StatusActionKind, StatusEntry, StatusEntryId, StatusEntryKind,
    StatusPrivacy, StatusSeverity, StatusText, SystemStatusError, SystemStatusRevision,
    SystemStatusSnapshot,
};

fn entry(id: u64, action_id: u64) -> StatusEntry {
    let action_id = StatusActionId::from_raw(action_id).unwrap();
    StatusEntry::new(
        StatusEntryId::from_raw(id).unwrap(),
        StatusEntryKind::Media,
        StatusText::new("Media").unwrap(),
        Some(StatusText::new("Private track").unwrap()),
        None,
        StatusSeverity::Normal,
        StatusPrivacy::Sensitive,
        true,
        Some(action_id),
        vec![StatusAction::new(
            action_id,
            StatusActionKind::OpenDetails,
            StatusText::new("Open media").unwrap(),
            true,
        )],
    )
    .unwrap()
}

#[test]
fn public_system_status_preserves_order_and_global_action_identity() {
    let snapshot = SystemStatusSnapshot::new(
        SystemStatusRevision::from_raw(1).unwrap(),
        vec![entry(1, 10), entry(2, 11)],
    )
    .unwrap();
    assert_eq!(snapshot.entries()[0].id().get(), 1);
    assert!(
        snapshot
            .action(StatusActionId::from_raw(11).unwrap())
            .is_some()
    );
    assert!(!format!("{snapshot:?}").contains("Private track"));

    assert!(matches!(
        SystemStatusSnapshot::new(
            SystemStatusRevision::from_raw(2).unwrap(),
            vec![entry(1, 10), entry(2, 10)],
        ),
        Err(SystemStatusError::DuplicateAction { .. })
    ));
}
