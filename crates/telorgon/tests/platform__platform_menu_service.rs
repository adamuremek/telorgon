use std::cell::Cell;
use std::num::{NonZeroU8, NonZeroU16};
use std::rc::Rc;

use telorgon::input::{Modifiers, PhysicalKey, ShortcutChord};
use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, MAX_MENU_DEPTH,
    MAX_MENU_LABEL_BYTES, MenuAccelerator, MenuAcceleratorError, MenuAcceleratorLabel,
    MenuActionAdmissionError, MenuActionEvent, MenuActionRequest, MenuActionSource,
    MenuAdmissionError, MenuCapability, MenuCapabilityQuery, MenuCheckState, MenuItem,
    MenuItemError, MenuItemId, MenuItemKind, MenuItemState, MenuLabel, MenuLimits, MenuOperations,
    MenuPublicationAdmission, MenuPublicationApplied, MenuPublicationError, MenuPublicationRequest,
    MenuRevision, MenuRole, MenuScope, MenuService, MenuServiceKey, MenuSnapshotId, MenuTextError,
    MenuTree, MenuTreeError, PermissionState, RequestId, RequestOutcome, ServiceLookup,
    ServiceRegistry, StatusMenuId, Support, UnavailableReason, UserGestureRequirement, ViewId,
};

fn item_id(raw: u64) -> MenuItemId {
    MenuItemId::from_raw(raw).unwrap()
}

fn label(value: &str) -> MenuLabel {
    MenuLabel::new(value).unwrap()
}

fn accelerator(key: u32, display: &str) -> MenuAccelerator {
    MenuAccelerator::new(
        ShortcutChord::pressed(PhysicalKey::new(key), Modifiers::CONTROL),
        MenuAcceleratorLabel::new(display).unwrap(),
    )
    .unwrap()
}

fn action(
    id: u64,
    name: &str,
    role: Option<MenuRole>,
    state: MenuItemState,
    accelerator: Option<MenuAccelerator>,
) -> MenuItem {
    MenuItem::action(item_id(id), label(name), role, state, accelerator).unwrap()
}

#[test]
fn labels_accelerators_roles_and_kind_specific_state_are_typed_and_redacted() {
    let sensitive_label = "Private customer export";
    let private_label = MenuLabel::new(sensitive_label).unwrap();
    assert_eq!(private_label.as_str(), sensitive_label);
    assert!(!format!("{private_label:?}").contains(sensitive_label));
    assert_eq!(MenuLabel::new(""), Err(MenuTextError::Empty));
    assert_eq!(
        MenuLabel::new("x".repeat(MAX_MENU_LABEL_BYTES + 1)),
        Err(MenuTextError::TooLong {
            byte_len: MAX_MENU_LABEL_BYTES + 1,
            maximum_bytes: MAX_MENU_LABEL_BYTES,
        })
    );

    let sensitive_accelerator = "Control+Private";
    let display = MenuAcceleratorLabel::new(sensitive_accelerator).unwrap();
    assert!(!format!("{display:?}").contains(sensitive_accelerator));
    assert_eq!(
        MenuAccelerator::new(
            ShortcutChord::pressed(PhysicalKey::new(0), Modifiers::CONTROL),
            display.clone(),
        ),
        Err(MenuAcceleratorError::UnknownPhysicalKey)
    );
    assert_eq!(
        MenuAccelerator::new(
            ShortcutChord::released(PhysicalKey::new(9), Modifiers::CONTROL),
            display,
        ),
        Err(MenuAcceleratorError::ReleasedTriggerUnsupported)
    );

    assert!(matches!(
        MenuItem::action(
            item_id(1),
            label("Wrong role"),
            Some(MenuRole::File),
            MenuItemState::default(),
            None,
        ),
        Err(MenuItemError::RoleKindMismatch {
            required: MenuItemKind::Submenu,
            actual: MenuItemKind::Action,
            ..
        })
    ));
    assert!(matches!(
        MenuItem::submenu(
            item_id(2),
            label("Wrong role"),
            Some(MenuRole::Copy),
            true,
            true,
            vec![],
        ),
        Err(MenuItemError::RoleKindMismatch {
            required: MenuItemKind::Action,
            actual: MenuItemKind::Submenu,
            ..
        })
    ));

    let separator = MenuItem::separator(item_id(3));
    assert_eq!(separator.kind(), MenuItemKind::Separator);
    assert!(!separator.state().enabled());
    assert_eq!(separator.state().check(), MenuCheckState::NotCheckable);
    assert!(separator.label().is_none());
}

fn application_tree(revision: MenuRevision) -> MenuTree {
    let save = action(
        2,
        "Private save operation",
        None,
        MenuItemState::default(),
        Some(accelerator(10, "Ctrl+S private")),
    );
    let quit = action(
        4,
        "Private quit operation",
        Some(MenuRole::QuitApplication),
        MenuItemState::default(),
        None,
    );
    let file = MenuItem::submenu(
        item_id(1),
        label("Private file menu"),
        Some(MenuRole::File),
        true,
        true,
        vec![save, MenuItem::separator(item_id(3)), quit],
    )
    .unwrap();
    MenuTree::new(MenuScope::Application, revision, vec![file]).unwrap()
}

#[test]
fn complete_trees_validate_topology_identity_accelerators_and_exact_revision_history() {
    let tree = application_tree(MenuRevision::INITIAL);
    assert_eq!(tree.item_count(), 4);
    assert_eq!(tree.depth(), 2);
    assert_eq!(tree.accelerator_count(), 1);
    assert!(tree.has_native_roles());
    assert_eq!(tree.item(item_id(2)).unwrap().kind(), MenuItemKind::Action);
    let debug = format!("{tree:?}");
    assert!(!debug.contains("Private save"));
    assert!(!debug.contains("Ctrl+S private"));

    let duplicate_id = MenuTree::new(
        MenuScope::Application,
        MenuRevision::INITIAL,
        vec![
            action(5, "First", None, MenuItemState::default(), None),
            action(5, "Second", None, MenuItemState::default(), None),
        ],
    );
    assert!(matches!(
        duplicate_id,
        Err(MenuTreeError::DuplicateItem { item }) if item == item_id(5)
    ));
    let duplicate_accelerator = MenuTree::new(
        MenuScope::Application,
        MenuRevision::INITIAL,
        vec![
            action(
                6,
                "First",
                None,
                MenuItemState::default(),
                Some(accelerator(20, "First")),
            ),
            action(
                7,
                "Second",
                None,
                MenuItemState::default(),
                Some(accelerator(20, "Second")),
            ),
        ],
    );
    assert!(matches!(
        duplicate_accelerator,
        Err(MenuTreeError::DuplicateAccelerator { .. })
    ));
    assert!(matches!(
        MenuTree::new(
            MenuScope::Application,
            MenuRevision::INITIAL,
            vec![MenuItem::separator(item_id(8))],
        ),
        Err(MenuTreeError::LeadingSeparator { .. })
    ));

    let mut nested = action(100, "Leaf", None, MenuItemState::default(), None);
    for depth in 0..MAX_MENU_DEPTH {
        nested = MenuItem::submenu(
            item_id(101 + u64::from(depth)),
            label("Nested"),
            None,
            true,
            true,
            vec![nested],
        )
        .unwrap();
    }
    assert!(matches!(
        MenuTree::new(MenuScope::Application, MenuRevision::INITIAL, vec![nested],),
        Err(MenuTreeError::TooDeep { .. })
    ));

    let initial = MenuPublicationRequest::initial(tree.clone()).unwrap();
    assert_eq!(initial.previous(), None);
    let applied = MenuPublicationApplied::from_request(&initial);
    assert_eq!(applied.snapshot(), tree.id());
    assert_eq!(applied.item_count(), tree.item_count());

    let revision_2 = MenuRevision::INITIAL.checked_next().unwrap();
    let empty = MenuTree::new(MenuScope::Application, revision_2, vec![]).unwrap();
    let removal = MenuPublicationRequest::advance(tree.id(), empty).unwrap();
    assert_eq!(removal.previous(), Some(tree.id()));
    assert!(removal.tree().is_empty());
    assert!(!format!("{removal:?}").contains("Private"));

    assert!(matches!(
        MenuPublicationRequest::initial(application_tree(revision_2)),
        Err(MenuPublicationError::InitialRevisionRequired { .. })
    ));
    let wrong_scope = MenuTree::new(
        MenuScope::View(ViewId::from_raw(3, 1).unwrap()),
        revision_2,
        vec![],
    )
    .unwrap();
    assert!(matches!(
        MenuPublicationRequest::advance(tree.id(), wrong_scope),
        Err(MenuPublicationError::ScopeMismatch { .. })
    ));
}

fn action_tree(view: ViewId) -> MenuTree {
    let enabled = action(
        1,
        "Copy private",
        Some(MenuRole::Copy),
        MenuItemState::default(),
        Some(accelerator(30, "Ctrl+C private")),
    );
    let disabled = action(
        2,
        "Disabled private",
        None,
        MenuItemState::new(false, MenuCheckState::Unchecked, true),
        None,
    );
    let hidden = action(
        3,
        "Hidden private",
        None,
        MenuItemState::new(true, MenuCheckState::NotCheckable, false),
        None,
    );
    let ordinary = action(4, "Ordinary private", None, MenuItemState::default(), None);
    let submenu = MenuItem::submenu(
        item_id(5),
        label("Submenu private"),
        None,
        true,
        true,
        vec![action(6, "Child", None, MenuItemState::default(), None)],
    )
    .unwrap();
    MenuTree::new(
        MenuScope::View(view),
        MenuRevision::INITIAL,
        vec![enabled, disabled, hidden, ordinary, submenu],
    )
    .unwrap()
}

#[test]
fn action_events_require_the_exact_current_actionable_item_and_preserve_source() {
    let view = ViewId::from_raw(7, 2).unwrap();
    let tree = action_tree(view);
    let accelerator_event = MenuActionEvent::admit(
        &tree,
        MenuActionRequest::new(tree.id(), item_id(1), MenuActionSource::Accelerator),
    )
    .unwrap();
    assert_eq!(accelerator_event.snapshot(), tree.id());
    assert_eq!(accelerator_event.item(), item_id(1));
    assert_eq!(accelerator_event.role(), Some(MenuRole::Copy));
    assert_eq!(accelerator_event.source(), MenuActionSource::Accelerator);

    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(
                MenuSnapshotId::new(tree.scope(), MenuRevision::INITIAL.checked_next().unwrap(),),
                item_id(1),
                MenuActionSource::Pointer,
            ),
        ),
        Err(MenuActionAdmissionError::StaleRevision { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(99), MenuActionSource::Pointer),
        ),
        Err(MenuActionAdmissionError::UnknownItem { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(5), MenuActionSource::Pointer),
        ),
        Err(MenuActionAdmissionError::ItemNotActionable { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(2), MenuActionSource::Pointer),
        ),
        Err(MenuActionAdmissionError::ItemDisabled { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(3), MenuActionSource::Pointer),
        ),
        Err(MenuActionAdmissionError::ItemHidden { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(4), MenuActionSource::Accelerator),
        ),
        Err(MenuActionAdmissionError::AcceleratorNotAdvertised { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(4), MenuActionSource::PlatformRole),
        ),
        Err(MenuActionAdmissionError::RoleNotAdvertised { .. })
    ));
    assert!(matches!(
        MenuActionEvent::admit(
            &tree,
            MenuActionRequest::new(tree.id(), item_id(4), MenuActionSource::StatusItem),
        ),
        Err(MenuActionAdmissionError::StatusSourceOutsideStatusScope)
    ));
}

struct FixtureMenuService {
    available_scope: MenuScope,
    capability: MenuCapability,
    observed: Option<MenuSnapshotId>,
    next_request: Cell<u64>,
}

impl MenuService for FixtureMenuService {
    fn capability(&self, query: MenuCapabilityQuery) -> Support<MenuCapability> {
        if query.scope() == self.available_scope {
            Support::Available(self.capability)
        } else {
            Support::Unavailable(UnavailableReason::UnavailableInScope)
        }
    }

    fn publish(&self, request: MenuPublicationRequest) -> MenuPublicationAdmission {
        let scope = request.tree().scope();
        if scope != self.available_scope {
            return Err(MenuAdmissionError::ScopeUnavailable { scope });
        }
        if !self.capability.operations().supports_scope(scope) {
            return Err(MenuAdmissionError::UnsupportedScope { scope });
        }
        if self.capability.permission().blocks_use() {
            return Err(MenuAdmissionError::PermissionDenied);
        }
        let operations = *self.capability.operations();
        let tree = request.tree();
        if tree.has_native_roles() && !operations.supports_native_roles() {
            return Err(MenuAdmissionError::NativeRolesUnsupported);
        }
        if tree.accelerator_count() > 0 && !operations.supports_accelerators() {
            return Err(MenuAdmissionError::AcceleratorsUnsupported);
        }
        if tree.has_mixed_check_state() && !operations.supports_mixed_check_state() {
            return Err(MenuAdmissionError::MixedCheckStateUnsupported);
        }
        let limits = *self.capability.limits();
        if tree.item_count() > limits.maximum_items().get() {
            return Err(MenuAdmissionError::ItemsExceedCapability);
        }
        if tree.depth() > limits.maximum_depth().get() {
            return Err(MenuAdmissionError::DepthExceedsCapability);
        }
        if tree.accelerator_count() > limits.maximum_accelerators().get() {
            return Err(MenuAdmissionError::AcceleratorsExceedCapability);
        }
        if request.previous() != self.observed {
            return Err(MenuAdmissionError::RevisionMismatch {
                expected_previous: request.previous().map(MenuSnapshotId::revision),
                observed: self.observed.map(MenuSnapshotId::revision),
            });
        }

        let request_id = self.next_request.get() + 1;
        self.next_request.set(request_id);
        Ok(AdmittedRequest::new(
            RequestId::from_raw(request_id).unwrap(),
        ))
    }
}

#[test]
fn service_capability_publication_completion_and_registry_are_object_safe() {
    let current = application_tree(MenuRevision::INITIAL);
    let next_revision = MenuRevision::INITIAL.checked_next().unwrap();
    let next = application_tree(next_revision);
    let capability = CapabilityDescriptor::new(
        MenuOperations::new(true, true, true, true, true, true, true),
        MenuLimits::new(
            NonZeroU16::new(8).unwrap(),
            NonZeroU8::new(3).unwrap(),
            NonZeroU16::new(2).unwrap(),
        )
        .unwrap(),
        PermissionState::NotRequired,
        ExecutionRequirement::HostEventLoop,
        UserGestureRequirement::NotRequired,
    );
    let service: Rc<dyn MenuService> = Rc::new(FixtureMenuService {
        available_scope: MenuScope::Application,
        capability,
        observed: Some(current.id()),
        next_request: Cell::new(70),
    });
    let mut registry = ServiceRegistry::new();
    assert!(registry.register::<MenuServiceKey>(service).is_registered());
    let ServiceLookup::Available(service) = registry.lookup::<MenuServiceKey>() else {
        panic!("registered menu service must be available");
    };
    assert!(
        service
            .capability(MenuCapabilityQuery::new(MenuScope::Application))
            .is_available()
    );
    assert!(
        service
            .capability(MenuCapabilityQuery::new(MenuScope::Status(
                StatusMenuId::from_raw(5).unwrap(),
            )))
            .is_unavailable()
    );

    let request = MenuPublicationRequest::advance(current.id(), next).unwrap();
    let applied = MenuPublicationApplied::from_request(&request);
    let admitted = service.publish(request).unwrap();
    assert_eq!(admitted.request_id().get(), 71);
    let completion = admitted.complete(RequestOutcome::Applied(applied));
    assert_eq!(
        completion
            .outcome()
            .applied()
            .unwrap()
            .snapshot()
            .revision(),
        next_revision
    );

    assert!(matches!(
        service.publish(MenuPublicationRequest::initial(current).unwrap()),
        Err(MenuAdmissionError::RevisionMismatch {
            expected_previous: None,
            observed: Some(MenuRevision::INITIAL),
        })
    ));
}
