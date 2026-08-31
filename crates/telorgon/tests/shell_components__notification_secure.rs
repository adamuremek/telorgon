use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{EdgeInsets, RectF, SizeI};
use telorgon::input::ChangeSource;
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    OutputColorCapabilities, OutputGeometry, OutputSnapshot, OutputTransform, ShellCapabilities,
    ShellCapabilityGrant, ShellGrantToken,
};
use telorgon::shell_components::prelude::*;
use telorgon::shell_primitives::{OutputView, ShellLayer, ShellLayerOrder, ShellRoot};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticParticipation, SemanticRole, UiRoot};

fn output_id() -> OutputId {
    OutputId::from_raw(43).unwrap()
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output_id(),
        OutputRevision::from_raw(44).unwrap(),
        OutputGeometry::new(
            RectF {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 720.0,
            },
            RectF {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 720.0,
            },
            SizeI {
                width: 2560,
                height: 1440,
            },
            2.0,
            OutputTransform::Normal,
            EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        )
        .unwrap(),
    )
}

fn action(raw: u64, label: &str) -> NotificationAction {
    NotificationAction::new(
        NotificationActionId::from_raw(raw).unwrap(),
        NotificationActionKind::Open,
        NotificationText::new(label).unwrap(),
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn notification(
    raw: u64,
    title: &str,
    body: &str,
    priority: NotificationPriority,
    privacy: NotificationPrivacy,
    delivery: NotificationDeliveryState,
    actions: Vec<NotificationAction>,
) -> NotificationSnapshot {
    NotificationSnapshot::new(
        NotificationId::from_raw(raw).unwrap(),
        NotificationRevision::from_raw(raw + 100).unwrap(),
        Some(ApplicationId::from_raw(raw + 200).unwrap()),
        NotificationText::new(title).unwrap(),
        Some(NotificationText::new(body).unwrap()),
        Some(NotificationIconId::from_raw(raw + 300).unwrap()),
        priority,
        privacy,
        NotificationLifecycle {
            persistence: NotificationPersistence::Transient,
            delivery,
        },
        actions,
    )
    .unwrap()
}

fn notifications() -> Vec<NotificationSnapshot> {
    vec![
        notification(
            1,
            "Build finished",
            "All checks passed",
            NotificationPriority::High,
            NotificationPrivacy::Public,
            NotificationDeliveryState::New,
            vec![action(11, "Open build")],
        ),
        notification(
            2,
            "Private message",
            "Sensitive message body",
            NotificationPriority::Normal,
            NotificationPrivacy::Sensitive,
            NotificationDeliveryState::Presented,
            vec![action(12, "Open message")],
        ),
        notification(
            3,
            "Secret title",
            "Secret body",
            NotificationPriority::Critical,
            NotificationPrivacy::Secret,
            NotificationDeliveryState::New,
            vec![action(13, "Secret action")],
        ),
        notification(
            4,
            "Earlier update",
            "Acknowledged body",
            NotificationPriority::Low,
            NotificationPrivacy::Public,
            NotificationDeliveryState::Acknowledged,
            vec![action(14, "Open update")],
        ),
    ]
}

fn critical_notification() -> NotificationSnapshot {
    notification(
        5,
        "Restart required",
        "Restart to finish updating",
        NotificationPriority::Critical,
        NotificationPrivacy::Public,
        NotificationDeliveryState::New,
        vec![action(15, "Restart")],
    )
}

fn osd_notification() -> NotificationSnapshot {
    notification(
        6,
        "Volume",
        "Fifty percent",
        NotificationPriority::Normal,
        NotificationPrivacy::Public,
        NotificationDeliveryState::Presented,
        Vec::new(),
    )
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(47).unwrap(),
        output_id(),
        ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::SYSTEM_MODAL_LAYER
            | ShellCapabilities::LOCK_LAYER
            | ShellCapabilities::INVOKE_SYSTEM_ACTION,
    )
}

#[test]
fn constructors_reject_ambiguous_catalogs_and_invalid_presentation_modes() {
    let duplicate = notifications()[0].clone();
    assert!(matches!(
        NotificationCatalog::new(vec![duplicate.clone(), duplicate]),
        Err(NotificationCatalogError::DuplicateNotification { .. })
    ));
    assert!(matches!(
        SystemDialog::new("System dialog", notifications()[0].clone()),
        Err(SystemDialogError::RequiresCriticalPriority)
    ));
    let acknowledged_critical = notification(
        7,
        "Old critical alert",
        "Already handled",
        NotificationPriority::Critical,
        NotificationPrivacy::Public,
        NotificationDeliveryState::Acknowledged,
        vec![action(17, "Open")],
    );
    assert!(matches!(
        SystemDialog::new("System dialog", acknowledged_critical),
        Err(SystemDialogError::AcknowledgedNotification)
    ));
    assert!(matches!(
        OnScreenDisplay::new("Volume", notifications()[0].clone(), true),
        Err(OnScreenDisplayError::InteractiveNotification)
    ));
}

#[derive(Clone)]
struct MountedRefs {
    host: NotificationHostRef,
    center: NotificationCenterRef,
    dialog: SystemDialogRef,
    osd: OnScreenDisplayRef,
    modal: SystemModalHostRef,
    lock: LockCompositionRef,
}

struct Fixture {
    references: Rc<RefCell<Option<MountedRefs>>>,
    received: Rc<RefCell<Vec<NotificationActionIntent>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = NotificationActionIntent;

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host_node = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ShellRoot::new("Notification test shell", grant())
            .unwrap()
            .mount(ui, host_node.0)
            .unwrap();
        let output = OutputView::new(output_snapshot()).mount(ui, root).unwrap();
        let mut order = ShellLayerOrder::new(output_id());
        let overlay = ShellLayer::new(root.authorize_layer(ShellLayerKind::Overlay).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let catalog = notifications();
        let notification_host = NotificationHost::new("Notifications", catalog.clone())
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        let center = NotificationCenter::new("Notification center", catalog)
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        let osd = OnScreenDisplay::new("Volume status", osd_notification(), true)
            .unwrap()
            .mount(ui, root, overlay)
            .unwrap();

        let modal_layer =
            ShellLayer::new(root.authorize_layer(ShellLayerKind::SystemModal).unwrap())
                .mount(ui, output, &mut order)
                .unwrap();
        let dialog = SystemDialog::new("System update", critical_notification())
            .unwrap()
            .mount(ui, root, modal_layer, |intent| intent)
            .unwrap();
        let modal = SystemModalHost::new("System modal host", true)
            .unwrap()
            .mount(ui, root, modal_layer)
            .unwrap();

        let lock_layer = ShellLayer::new(root.authorize_layer(ShellLayerKind::Lock).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let lock = LockComposition::new("Lock screen", true)
            .unwrap()
            .mount(ui, root, lock_layer)
            .unwrap();
        *self.references.borrow_mut() = Some(MountedRefs {
            host: notification_host,
            center,
            dialog,
            osd,
            modal,
            lock,
        });
        host_node
    }

    fn action(&self, _: &mut Self::State, action: Self::Action, _: &mut UpdateContext<'_, Self>) {
        self.received.borrow_mut().push(action);
    }
}

#[test]
fn mounted_family_preserves_privacy_lifecycle_authority_and_typed_actions() {
    let references = Rc::new(RefCell::new(None));
    let received = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(Fixture {
        references: Rc::clone(&references),
        received: Rc::clone(&received),
    })
    .unwrap();
    let refs = references.borrow().as_ref().unwrap().clone();

    assert_eq!(refs.host.notifications().len(), 4);
    assert_eq!(refs.host.notifications()[0].snapshot().id().get(), 1);
    assert_eq!(
        runtime.ui().semantics.get(refs.host.node()).unwrap().role,
        SemanticRole::Region
    );
    let public = refs
        .host
        .notification(NotificationId::from_raw(1).unwrap())
        .unwrap();
    assert_eq!(
        public.presented_body().unwrap().as_str(),
        "All checks passed"
    );
    assert_eq!(
        runtime.ui().semantics.get(public.node()).unwrap().role,
        SemanticRole::Alert
    );
    let sensitive = refs
        .host
        .notification(NotificationId::from_raw(2).unwrap())
        .unwrap();
    assert!(sensitive.presented_body().is_none());
    let secret = refs
        .host
        .notification(NotificationId::from_raw(3).unwrap())
        .unwrap();
    assert!(secret.redacted());
    assert!(!secret.actions()[0].available());
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(secret.node())
            .unwrap()
            .participation,
        SemanticParticipation::Exclude
    );
    let acknowledged = refs
        .host
        .notification(NotificationId::from_raw(4).unwrap())
        .unwrap();
    assert!(!acknowledged.presented());
    assert!(!acknowledged.actions()[0].available());
    let acknowledged_semantics = runtime.ui().semantics.get(acknowledged.node()).unwrap();
    assert!(acknowledged_semantics.state.hidden);
    assert!(acknowledged_semantics.state.inert);

    assert_eq!(
        runtime.ui().semantics.get(refs.center.node()).unwrap().role,
        SemanticRole::List
    );
    let center_acknowledged = refs
        .center
        .notification(NotificationId::from_raw(4).unwrap())
        .unwrap();
    assert!(center_acknowledged.presented());
    assert!(center_acknowledged.actions()[0].available());
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(center_acknowledged.node())
            .unwrap()
            .role,
        SemanticRole::ListItem
    );

    let open_build = public
        .action(NotificationActionId::from_raw(11).unwrap())
        .unwrap();
    assert!(runtime.dispatch_activation(open_build.node(), ChangeSource::Keyboard));
    let keyboard_intent = received.borrow()[0];
    assert!(matches!(
        keyboard_intent.inferred_request(),
        Some(SystemRequest::NotificationAction {
            notification,
            revision,
            action,
            source: InputSource::Keyboard,
        }) if notification.get() == 1 && revision.get() == 101 && action.get() == 11
    ));
    let open_message = sensitive
        .action(NotificationActionId::from_raw(12).unwrap())
        .unwrap();
    assert!(runtime.dispatch_activation(open_message.node(), ChangeSource::Pointer));
    let pointer_intent = received.borrow()[1];
    assert!(pointer_intent.inferred_request().is_none());
    assert!(matches!(
        pointer_intent.request(InputSource::Touch),
        Ok(SystemRequest::NotificationAction {
            notification,
            action,
            source: InputSource::Touch,
            ..
        }) if notification.get() == 2 && action.get() == 12
    ));
    assert_eq!(
        pointer_intent.request(InputSource::Keyboard),
        Err(NotificationActionSourceError::SourceMismatch)
    );

    assert_eq!(
        runtime.ui().semantics.get(refs.dialog.node()).unwrap().role,
        SemanticRole::Dialog
    );
    assert!(refs.dialog.requires_lower_layers_inert());
    let restart = refs
        .dialog
        .action(NotificationActionId::from_raw(15).unwrap())
        .unwrap();
    assert!(restart.available());
    assert!(runtime.dispatch_activation(restart.node(), ChangeSource::Accessibility));
    assert!(matches!(
        received.borrow()[2].inferred_request(),
        Some(SystemRequest::NotificationAction {
            notification,
            action,
            source: InputSource::Accessibility,
            ..
        }) if notification.get() == 5 && action.get() == 15
    ));

    assert!(refs.osd.visible());
    assert!(!refs.osd.redacted());
    assert_eq!(
        runtime.ui().semantics.get(refs.osd.node()).unwrap().role,
        SemanticRole::Status
    );
    assert_eq!(refs.modal.output(), output_id());
    assert_eq!(refs.modal.grant(), grant().token());
    assert!(refs.modal.requires_lower_layers_inert());
    assert_eq!(
        runtime.ui().semantics.get(refs.modal.node()).unwrap().role,
        SemanticRole::Dialog
    );
    assert_eq!(refs.lock.output(), output_id());
    assert_eq!(refs.lock.grant(), grant().token());
    assert!(refs.lock.requires_lower_layers_inert());
    assert_eq!(
        runtime.ui().semantics.get(refs.lock.node()).unwrap().role,
        SemanticRole::Application
    );
}
