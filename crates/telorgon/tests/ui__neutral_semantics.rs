use telorgon::ui::{
    BoxStyle, LayoutStyle, MountWriter, MountedUi, SemanticAction, SemanticActions, SemanticError,
    SemanticName, SemanticNode, SemanticRelationship, SemanticRelationshipKind, SemanticRole,
    SemanticValue, StringId, UiNodeId,
};

#[test]
fn mounted_semantic_updates_are_validated_and_atomic() {
    let mut ui = MountedUi::default();
    let root =
        MountWriter::<()>::new(&mut ui).root(BoxStyle::default(), LayoutStyle::default(), |_| {});
    let name = ui.intern("Volume");
    let mut slider = SemanticNode::named(SemanticRole::Slider, name);
    slider.actions = SemanticActions::INCREMENT | SemanticActions::DECREMENT;
    slider.value = SemanticValue::Number {
        current: 50.0,
        minimum: 0.0,
        maximum: 100.0,
        step: Some(1.0),
        value_text: None,
    };

    assert_eq!(ui.set_semantics(root.0, slider.clone()), Ok(true));
    assert!(
        ui.semantics
            .get(root.0)
            .unwrap()
            .effective_actions()
            .contains(SemanticAction::Increment)
    );

    let mut invalid = slider.clone();
    invalid.name = SemanticName::Unspecified;
    assert_eq!(
        ui.set_semantics(root.0, invalid),
        Err(SemanticError::MissingAccessibleName)
    );
    assert_eq!(ui.semantics.get(root.0), Some(&slider));

    let mut unknown_string = slider.clone();
    unknown_string.name = SemanticName::Text(StringId(u32::MAX));
    assert_eq!(
        ui.set_semantics(root.0, unknown_string),
        Err(SemanticError::UnknownString(StringId(u32::MAX)))
    );
    assert_eq!(ui.semantics.get(root.0), Some(&slider));

    let mut stale_relationship = slider.clone();
    stale_relationship.relationships.push(SemanticRelationship {
        kind: SemanticRelationshipKind::DescribedBy,
        target: UiNodeId::new(99, 7),
    });
    assert_eq!(
        ui.set_semantics(root.0, stale_relationship),
        Err(SemanticError::UnknownRelationshipTarget(UiNodeId::new(
            99, 7
        )))
    );
    assert_eq!(ui.semantics.get(root.0), Some(&slider));
    assert_eq!(ui.diagnostics.semantic_updates, 1);
    assert_eq!(ui.diagnostics.semantic_failures, 3);
}
