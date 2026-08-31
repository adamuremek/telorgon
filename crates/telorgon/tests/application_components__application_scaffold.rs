use telorgon::application_components::structure::{Scaffold, ScaffoldSlot, ScaffoldSlotSpec};
use telorgon::ui::SemanticRole;

fn slot(slot: ScaffoldSlot, label: &str) -> ScaffoldSlotSpec {
    ScaffoldSlotSpec::new(slot, label).unwrap()
}

#[test]
fn public_scaffold_canonicalizes_named_application_landmarks() {
    let scaffold = Scaffold::new(
        "Editor",
        [
            slot(ScaffoldSlot::Overlay, "Editor overlays"),
            slot(ScaffoldSlot::Status, "Build status"),
            slot(ScaffoldSlot::Content, "Source editor"),
            slot(ScaffoldSlot::Navigation, "Project files"),
            slot(ScaffoldSlot::Top, "Editor commands"),
            slot(ScaffoldSlot::Secondary, "Inspector"),
            slot(ScaffoldSlot::FloatingAction, "Quick actions"),
        ],
    )
    .unwrap();

    assert_eq!(scaffold.slots().len(), ScaffoldSlot::ALL.len());
    assert_eq!(scaffold.slots()[0].slot(), ScaffoldSlot::Navigation);
    assert_eq!(scaffold.slots()[2].slot(), ScaffoldSlot::Content);
    assert_eq!(
        scaffold.slot(ScaffoldSlot::Content).unwrap().label(),
        "Source editor"
    );
    assert_eq!(
        ScaffoldSlot::Navigation.semantic_role(),
        SemanticRole::Navigation
    );
    assert_eq!(ScaffoldSlot::Content.semantic_role(), SemanticRole::Main);
    assert_eq!(ScaffoldSlot::Status.semantic_role(), SemanticRole::Status);
}
