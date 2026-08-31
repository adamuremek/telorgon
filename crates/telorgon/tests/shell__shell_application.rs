use telorgon::shell::{
    ApplicationAction, ApplicationActionId, ApplicationActionKind, ApplicationEntry,
    ApplicationEntryError, ApplicationId, ApplicationLabel, ApplicationRevision, ApplicationStates,
};

#[test]
fn public_application_entry_exposes_only_host_described_typed_actions() {
    let action_id = ApplicationActionId::from_raw(7).unwrap();
    let action = ApplicationAction::new(
        action_id,
        ApplicationActionKind::Launch,
        ApplicationLabel::new("Launch").unwrap(),
        true,
    );
    let entry = ApplicationEntry::new(
        ApplicationId::from_raw(1).unwrap(),
        ApplicationRevision::from_raw(2).unwrap(),
        ApplicationLabel::new("Editor").unwrap(),
        None,
        None,
        ApplicationStates::PINNED,
        Some(action_id),
        vec![action.clone()],
    )
    .unwrap();

    assert_eq!(entry.primary_action(), Some(action_id));
    assert_eq!(entry.action(action_id), Some(&action));
    assert!(matches!(
        ApplicationEntry::new(
            ApplicationId::from_raw(1).unwrap(),
            ApplicationRevision::from_raw(3).unwrap(),
            ApplicationLabel::new("Editor").unwrap(),
            None,
            None,
            ApplicationStates::NONE,
            Some(ApplicationActionId::from_raw(8).unwrap()),
            vec![action],
        ),
        Err(ApplicationEntryError::UnknownPrimaryAction { .. })
    ));
}
