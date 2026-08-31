use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use telorgon::accessibility::{
    AssistiveActionData, AssistiveActionError, AssistiveActionRequest, ResolvedSemanticString,
    SemanticAction, SemanticActions, SemanticName, SemanticNode, SemanticNodeGeometry,
    SemanticNodeId, SemanticRole, SemanticTreeGeneration, SemanticTreeNode,
    SemanticTreePublication, SemanticTreePublicationKind, SemanticTreeRevision,
    SemanticTreeSnapshot, StringId,
};
use telorgon::core::RectF;
use telorgon::platform::{
    AccessibilityActionAdmissionError, AccessibilityActionEvent, AccessibilityAdmissionError,
    AccessibilityApplied, AccessibilityCapability, AccessibilityCapabilityQuery,
    AccessibilityLimits, AccessibilityOperations, AccessibilityPublicationAdmission,
    AccessibilityPublicationRequest, AccessibilityService, AccessibilityServiceKey,
    AdmittedRequest, ExecutionRequirement, PermissionState, RequestId, RequestOutcome,
    ServiceLookup, ServiceRegistry, Support, UnavailableReason, UserGestureRequirement, ViewId,
};

struct FixtureAccessibilityService {
    view: ViewId,
    next_request: Cell<u64>,
}

impl AccessibilityService for FixtureAccessibilityService {
    fn capability(&self, query: AccessibilityCapabilityQuery) -> Support<AccessibilityCapability> {
        if query.view() != self.view {
            return Support::Unavailable(UnavailableReason::UnavailableInScope);
        }
        Support::Available(AccessibilityCapability::new(
            AccessibilityOperations::new(true, true),
            AccessibilityLimits::default(),
            PermissionState::NotRequired,
            ExecutionRequirement::HostEventLoop,
            UserGestureRequirement::NotRequired,
        ))
    }

    fn publish(
        &self,
        request: AccessibilityPublicationRequest,
    ) -> AccessibilityPublicationAdmission {
        if request.view() != self.view {
            return Err(AccessibilityAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        let request_id = self.next_request.get() + 1;
        self.next_request.set(request_id);
        Ok(AdmittedRequest::new(
            RequestId::from_raw(request_id).unwrap(),
        ))
    }
}

fn node(index: u32) -> SemanticNodeId {
    SemanticNodeId::new(index, 2)
}

fn snapshot() -> SemanticTreeSnapshot {
    let mut button = SemanticNode::new(SemanticRole::Button);
    button.name = SemanticName::Text(StringId(1));
    button.actions = SemanticActions::ACTIVATE | SemanticActions::SET_TEXT;
    let root = SemanticTreeNode::new(
        node(0),
        None,
        vec![node(1)],
        SemanticNode::default(),
        SemanticNodeGeometry::view_logical(RectF::ZERO).unwrap(),
    )
    .unwrap();
    let button = SemanticTreeNode::new(
        node(1),
        Some(node(0)),
        vec![],
        button,
        SemanticNodeGeometry::view_logical(RectF {
            x: 8.0,
            y: 8.0,
            width: 80.0,
            height: 30.0,
        })
        .unwrap(),
    )
    .unwrap();
    SemanticTreeSnapshot::new(
        SemanticTreeGeneration::INITIAL,
        SemanticTreeRevision::INITIAL,
        node(0),
        vec![root, button],
        vec![ResolvedSemanticString::new(StringId(1), "private label").unwrap()],
        Some(node(1)),
        None,
    )
    .unwrap()
}

#[test]
fn public_service_path_advertises_per_view_support_and_linearly_admits_publication() {
    let view = ViewId::from_raw(5, 4).unwrap();
    let service: Rc<dyn AccessibilityService> = Rc::new(FixtureAccessibilityService {
        view,
        next_request: Cell::new(20),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<AccessibilityServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<AccessibilityServiceKey>() else {
        panic!("registered accessibility service must be available")
    };
    let capability = service
        .capability(AccessibilityCapabilityQuery::new(view))
        .into_available()
        .unwrap();
    assert!(capability.operations().supports_tree_publication());
    assert!(capability.operations().supports_assistive_actions());

    let request =
        AccessibilityPublicationRequest::new(view, SemanticTreePublication::Activate(snapshot()));
    assert_eq!(request.kind(), SemanticTreePublicationKind::Activate);
    assert!(!format!("{request:?}").contains("private label"));
    let applied = AccessibilityApplied::from_request(&request);
    let completion = service
        .publish(request)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id(), RequestId::from_raw(21).unwrap());
    assert_eq!(completion.outcome().applied().unwrap().view(), view);

    let stale_view = ViewId::from_raw(view.slot(), view.generation() + 1).unwrap();
    let stale = AccessibilityPublicationRequest::new(
        stale_view,
        SemanticTreePublication::Activate(snapshot()),
    );
    assert_eq!(
        service.publish(stale),
        Err(AccessibilityAdmissionError::ViewUnavailable { view: stale_view })
    );
}

#[test]
fn action_event_admission_rejects_stale_or_unadvertised_targets_and_redacts_text() {
    let view = ViewId::from_raw(7, 3).unwrap();
    let snapshot = snapshot();
    let action = AssistiveActionRequest::new(
        snapshot.generation(),
        snapshot.revision(),
        node(1),
        SemanticAction::SetText,
        AssistiveActionData::Text(Arc::from("secret replacement")),
    )
    .unwrap();
    let event = AccessibilityActionEvent::admit(view, &snapshot, action).unwrap();
    assert_eq!(event.view(), view);
    assert_eq!(event.target(), node(1));
    assert!(!format!("{event:?}").contains("secret replacement"));

    let stale = AssistiveActionRequest::new(
        snapshot.generation(),
        SemanticTreeRevision::from_raw(2).unwrap(),
        node(1),
        SemanticAction::Activate,
        AssistiveActionData::None,
    )
    .unwrap();
    assert!(matches!(
        AccessibilityActionEvent::admit(view, &snapshot, stale),
        Err(AccessibilityActionAdmissionError::InvalidAction(
            AssistiveActionError::StaleTreeRevision { .. }
        ))
    ));
}
