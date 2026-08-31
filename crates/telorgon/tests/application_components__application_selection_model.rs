use telorgon::application_components::{
    ChangeSource, SelectionFollowsFocus, SelectionMode, SelectionModel, SelectionProposalKind,
};

#[test]
fn public_selection_model_preserves_multiple_selection_across_focus_and_key_updates() {
    let mut selection = SelectionModel::new(
        SelectionMode::Multiple,
        SelectionFollowsFocus::Enabled,
        ["alpha", "beta", "charlie", "delta"],
        ["alpha", "charlie"],
        Some("alpha"),
    )
    .unwrap();

    let focus = selection
        .propose_focus(&"delta", ChangeSource::Directional)
        .unwrap()
        .unwrap();
    assert_eq!(focus.kind(), SelectionProposalKind::Focus);
    assert_eq!(focus.source(), ChangeSource::Directional);
    assert_eq!(focus.selected(), &["alpha", "charlie", "delta"]);
    assert_eq!(selection.selected(), &["alpha", "charlie"]);
    selection.apply(focus).unwrap();

    let update = selection
        .update_items(["delta", "charlie", "beta"])
        .unwrap();
    assert_eq!(update.removed_selected(), &["alpha"]);
    assert_eq!(selection.selected(), &["delta", "charlie"]);
    assert_eq!(selection.anchor(), Some(&"charlie"));
    assert_eq!(selection.diagnostics().anchor_recoveries, 1);
}
