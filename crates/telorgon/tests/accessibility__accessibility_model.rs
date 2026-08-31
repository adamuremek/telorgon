use std::sync::Arc;

use telorgon::accessibility::{
    AssistiveActionData, AssistiveActionError, AssistiveActionRequest, ResolvedSemanticString,
    SemanticAction, SemanticActions, SemanticCoordinateSpace, SemanticFocusUpdate, SemanticName,
    SemanticNode, SemanticNodeGeometry, SemanticNodeId, SemanticParticipation,
    SemanticRelationship, SemanticRelationshipKind, SemanticRole, SemanticTreeDelta,
    SemanticTreeError, SemanticTreeGeneration, SemanticTreeNode, SemanticTreeRevision,
    SemanticTreeSnapshot, StringId,
};
use telorgon::core::{PointF, RectF, Transform2D};

fn id(index: u32) -> SemanticNodeId {
    SemanticNodeId::new(index, 3)
}

fn geometry(x: f32) -> SemanticNodeGeometry {
    SemanticNodeGeometry::new(
        RectF {
            x,
            y: 4.0,
            width: 80.0,
            height: 24.0,
        },
        Transform2D {
            translation: PointF { x: 1.0, y: 2.0 },
            scale: PointF { x: 1.0, y: 1.0 },
            ..Transform2D::default()
        },
        SemanticCoordinateSpace::ViewLogical,
    )
    .unwrap()
}

fn button_semantics(name: StringId) -> SemanticNode {
    let mut semantic = SemanticNode::new(SemanticRole::Button);
    semantic.name = SemanticName::Text(name);
    semantic.actions = SemanticActions::ACTIVATE | SemanticActions::SET_TEXT;
    semantic.state.focusable = true;
    semantic
}

fn snapshot() -> SemanticTreeSnapshot {
    let root = SemanticTreeNode::new(
        id(0),
        None,
        vec![id(1)],
        SemanticNode::new(SemanticRole::Generic),
        geometry(0.0),
    )
    .unwrap();
    let button = SemanticTreeNode::new(
        id(1),
        Some(id(0)),
        vec![],
        button_semantics(StringId(1)),
        geometry(10.0),
    )
    .unwrap();
    SemanticTreeSnapshot::new(
        SemanticTreeGeneration::INITIAL,
        SemanticTreeRevision::INITIAL,
        id(0),
        vec![button, root],
        vec![ResolvedSemanticString::new(StringId(1), "private button label").unwrap()],
        Some(id(1)),
        None,
    )
    .unwrap()
}

#[test]
fn complete_snapshot_resolves_topology_geometry_strings_and_distinct_focus() {
    let snapshot = snapshot();

    assert_eq!(snapshot.nodes().len(), 2);
    assert_eq!(snapshot.node(id(1)).unwrap().parent(), Some(id(0)));
    assert_eq!(
        snapshot.node(id(1)).unwrap().geometry().coordinate_space(),
        SemanticCoordinateSpace::ViewLogical
    );
    assert_eq!(
        snapshot.resolved_string(StringId(1)),
        Some("private button label")
    );
    assert_eq!(snapshot.keyboard_focus(), Some(id(1)));
    assert_eq!(snapshot.assistive_focus(), None);

    let diagnostics = format!("{snapshot:?}");
    assert!(diagnostics.contains("node_count"));
    assert!(!diagnostics.contains("private button label"));
    assert!(!format!("{:?}", snapshot.strings()[0]).contains("private button label"));
}

#[test]
fn revisioned_delta_applies_atomically_and_rejects_a_stale_base() {
    let initial = snapshot();
    let updated_button = SemanticTreeNode::new(
        id(1),
        Some(id(0)),
        vec![],
        button_semantics(StringId(2)),
        geometry(20.0),
    )
    .unwrap();
    let next = SemanticTreeRevision::from_raw(2).unwrap();
    let delta = SemanticTreeDelta::new(
        initial.generation(),
        initial.revision(),
        next,
        vec![updated_button],
        vec![],
        vec![ResolvedSemanticString::new(StringId(2), "updated label").unwrap()],
        vec![StringId(1)],
        SemanticFocusUpdate::Unchanged,
        SemanticFocusUpdate::Set(Some(id(1))),
    )
    .unwrap();

    let updated = initial.apply_delta(&delta).unwrap();
    assert_eq!(updated.revision(), next);
    assert_eq!(updated.resolved_string(StringId(1)), None);
    assert_eq!(updated.resolved_string(StringId(2)), Some("updated label"));
    assert_eq!(updated.assistive_focus(), Some(id(1)));
    assert_eq!(updated.node(id(1)).unwrap().geometry().bounds().x, 20.0);
    assert_eq!(
        updated.apply_delta(&delta),
        Err(SemanticTreeError::DeltaBaseRevisionMismatch)
    );
}

#[test]
fn malformed_cross_node_references_are_rejected_before_publication() {
    let mut related = button_semantics(StringId(1));
    related.relationships.push(SemanticRelationship {
        kind: SemanticRelationshipKind::Controls,
        target: id(9),
    });
    let root = SemanticTreeNode::new(
        id(0),
        None,
        vec![id(1)],
        SemanticNode::default(),
        geometry(0.0),
    )
    .unwrap();
    let child = SemanticTreeNode::new(id(1), Some(id(0)), vec![], related, geometry(10.0)).unwrap();
    assert!(matches!(
        SemanticTreeSnapshot::new(
            SemanticTreeGeneration::INITIAL,
            SemanticTreeRevision::INITIAL,
            id(0),
            vec![root, child],
            vec![ResolvedSemanticString::new(StringId(1), "label").unwrap()],
            None,
            None,
        ),
        Err(SemanticTreeError::UnknownRelationshipTarget { .. })
    ));

    let excluded = SemanticNode {
        participation: SemanticParticipation::Exclude,
        ..SemanticNode::default()
    };
    assert_eq!(
        SemanticTreeNode::new(id(2), None, vec![], excluded, geometry(0.0)),
        Err(SemanticTreeError::UnresolvedParticipation { node: id(2) })
    );
}

#[test]
fn assistive_actions_require_exact_tree_state_advertisement_and_typed_data() {
    let snapshot = snapshot();
    let exact = AssistiveActionRequest::new(
        snapshot.generation(),
        snapshot.revision(),
        id(1),
        SemanticAction::SetText,
        AssistiveActionData::Text(Arc::from("secret replacement")),
    )
    .unwrap();
    assert_eq!(exact.validate_against(&snapshot), Ok(()));
    assert!(!format!("{exact:?}").contains("secret replacement"));

    let stale = AssistiveActionRequest::new(
        snapshot.generation(),
        SemanticTreeRevision::from_raw(2).unwrap(),
        id(1),
        SemanticAction::Activate,
        AssistiveActionData::None,
    )
    .unwrap();
    assert!(matches!(
        stale.validate_against(&snapshot),
        Err(AssistiveActionError::StaleTreeRevision { .. })
    ));

    let unadvertised = AssistiveActionRequest::new(
        snapshot.generation(),
        snapshot.revision(),
        id(1),
        SemanticAction::Increment,
        AssistiveActionData::None,
    )
    .unwrap();
    assert!(matches!(
        unadvertised.validate_against(&snapshot),
        Err(AssistiveActionError::ActionNotAdvertised { .. })
    ));
    assert_eq!(
        AssistiveActionRequest::new(
            snapshot.generation(),
            snapshot.revision(),
            id(1),
            SemanticAction::SetSelection,
            AssistiveActionData::None,
        ),
        Err(AssistiveActionError::MissingData {
            action: SemanticAction::SetSelection
        })
    );
}
