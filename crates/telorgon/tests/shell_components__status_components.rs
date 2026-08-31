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
use telorgon::ui::{
    BoxStyle, LayoutStyle, SemanticAction, SemanticParticipation, SemanticRole, UiRoot,
};

fn output_id() -> OutputId {
    OutputId::from_raw(27).unwrap()
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output_id(),
        OutputRevision::from_raw(33).unwrap(),
        OutputGeometry::new(
            RectF {
                x: 0.0,
                y: 0.0,
                width: 1024.0,
                height: 768.0,
            },
            RectF {
                x: 0.0,
                y: 0.0,
                width: 1024.0,
                height: 736.0,
            },
            SizeI {
                width: 2048,
                height: 1536,
            },
            2.0,
            OutputTransform::Normal,
            EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        )
        .unwrap(),
    )
}

fn action(raw: u64, label: &str, enabled: bool) -> StatusAction {
    StatusAction::new(
        StatusActionId::from_raw(raw).unwrap(),
        StatusActionKind::OpenDetails,
        StatusText::new(label).unwrap(),
        enabled,
    )
}

#[allow(clippy::too_many_arguments)]
fn entry(
    raw: u64,
    kind: StatusEntryKind,
    label: &str,
    value: &str,
    privacy: StatusPrivacy,
    severity: StatusSeverity,
    active: bool,
    actions: Vec<StatusAction>,
) -> StatusEntry {
    StatusEntry::new(
        StatusEntryId::from_raw(raw).unwrap(),
        kind,
        StatusText::new(label).unwrap(),
        Some(StatusText::new(value).unwrap()),
        Some(StatusIconId::from_raw(raw + 100).unwrap()),
        severity,
        privacy,
        active,
        actions.first().map(StatusAction::id),
        actions,
    )
    .unwrap()
}

fn status_snapshot() -> SystemStatusSnapshot {
    SystemStatusSnapshot::new(
        SystemStatusRevision::from_raw(70).unwrap(),
        vec![
            entry(
                1,
                StatusEntryKind::Clock,
                "Clock",
                "10:30",
                StatusPrivacy::Public,
                StatusSeverity::Normal,
                false,
                vec![action(11, "Open calendar", true)],
            ),
            entry(
                2,
                StatusEntryKind::Connectivity,
                "Network",
                "Office Wi-Fi",
                StatusPrivacy::Sensitive,
                StatusSeverity::Attention,
                true,
                vec![action(12, "Network settings", true)],
            ),
            entry(
                3,
                StatusEntryKind::Media,
                "Media",
                "Private track",
                StatusPrivacy::Sensitive,
                StatusSeverity::Normal,
                true,
                vec![action(13, "Open media", true), action(14, "Pause", true)],
            ),
            entry(
                4,
                StatusEntryKind::Extension,
                "Weather",
                "37 degrees",
                StatusPrivacy::Public,
                StatusSeverity::Normal,
                false,
                vec![action(15, "Open weather", true)],
            ),
            entry(
                5,
                StatusEntryKind::Privacy,
                "Camera user",
                "Recording user",
                StatusPrivacy::Secret,
                StatusSeverity::Critical,
                true,
                vec![action(16, "Secret action", true)],
            ),
            entry(
                6,
                StatusEntryKind::Power,
                "Power",
                "Unavailable",
                StatusPrivacy::Public,
                StatusSeverity::Unavailable,
                false,
                vec![action(17, "Power settings", true)],
            ),
        ],
    )
    .unwrap()
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(41).unwrap(),
        output_id(),
        ShellCapabilities::PANEL_LAYER
            | ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::RESERVE_OUTPUT_AREA
            | ShellCapabilities::INVOKE_SYSTEM_ACTION,
    )
}

#[test]
fn typed_status_views_reject_wrong_kinds_and_keep_clock_and_privacy_host_owned() {
    assert!(matches!(
        StatusArea::new("", status_snapshot()),
        Err(StatusAreaError::MissingAccessibleName)
    ));
    let snapshot = status_snapshot();
    assert_eq!(snapshot.entries()[0].kind(), StatusEntryKind::Clock);
    assert_eq!(snapshot.entries()[4].privacy(), StatusPrivacy::Secret);
    assert!(!format!("{snapshot:?}").contains("Private track"));
}

#[derive(Clone)]
struct MountedRefs {
    area: StatusAreaRef,
    clock: StatusClockRef,
    indicator: StatusIndicatorRef,
    media: MediaStatusRef,
    extension: StatusExtensionSlotRef,
    quick: QuickSettingsRef,
}

struct Fixture {
    references: Rc<RefCell<Option<MountedRefs>>>,
    received: Rc<RefCell<Vec<StatusActionIntent>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = StatusActionIntent;

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ShellRoot::new("Status test shell", grant())
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        let output = OutputView::new(output_snapshot()).mount(ui, root).unwrap();
        let mut order = ShellLayerOrder::new(output_id());
        let panel_layer = ShellLayer::new(root.authorize_layer(ShellLayerKind::Panel).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let panel = Panel::new(
            "System panel",
            ReservedAreaId::from_raw(61).unwrap(),
            OutputEdge::Top,
            ReservedAreaExtent::new(32.0).unwrap(),
        )
        .unwrap()
        .mount(ui, root, output, panel_layer)
        .unwrap();
        let snapshot = status_snapshot();
        let area = StatusArea::new("System status", snapshot.clone())
            .unwrap()
            .mount(ui, root, panel, |intent| intent)
            .unwrap();
        let clock = StatusClock::new(StatusEntryId::from_raw(1).unwrap())
            .bind(&area)
            .unwrap();
        let indicator = StatusIndicator::new(StatusEntryId::from_raw(2).unwrap())
            .bind(&area)
            .unwrap();
        let media = MediaStatus::new(StatusEntryId::from_raw(3).unwrap())
            .bind(&area)
            .unwrap();
        let extension = StatusExtensionSlot::new(StatusEntryId::from_raw(4).unwrap())
            .bind(&area)
            .unwrap();
        assert!(matches!(
            StatusClock::new(StatusEntryId::from_raw(2).unwrap()).bind(&area),
            Err(StatusClockError::WrongKind { .. })
        ));

        let overlay = ShellLayer::new(root.authorize_layer(ShellLayerKind::Overlay).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let quick = QuickSettings::new("Quick settings", snapshot)
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        *self.references.borrow_mut() = Some(MountedRefs {
            area,
            clock,
            indicator,
            media,
            extension,
            quick,
        });
        host
    }

    fn action(&self, _: &mut Self::State, action: Self::Action, _: &mut UpdateContext<'_, Self>) {
        self.received.borrow_mut().push(action);
    }
}

#[test]
fn mounted_status_family_preserves_order_privacy_semantics_and_typed_requests() {
    let references = Rc::new(RefCell::new(None));
    let received = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(Fixture {
        references: Rc::clone(&references),
        received: Rc::clone(&received),
    })
    .unwrap();
    let refs = references.borrow().as_ref().unwrap().clone();

    assert_eq!(refs.area.entries().len(), 6);
    assert_eq!(refs.area.entries()[0].entry().id().get(), 1);
    assert_eq!(
        runtime.ui().semantics.get(refs.area.node()).unwrap().role,
        SemanticRole::Status
    );
    assert_eq!(refs.clock.presented_time().unwrap().as_str(), "10:30");
    assert_eq!(refs.indicator.kind(), StatusEntryKind::Connectivity);
    assert!(refs.indicator.presented_value().is_none());
    assert!(refs.media.presented_summary().is_none());
    assert_eq!(refs.media.actions().len(), 2);
    assert_eq!(
        refs.extension.presented_value().unwrap().as_str(),
        "37 degrees"
    );

    let secret = refs
        .area
        .entry(StatusEntryId::from_raw(5).unwrap())
        .unwrap();
    assert!(secret.redacted());
    assert!(!secret.available());
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(secret.node())
            .unwrap()
            .participation,
        SemanticParticipation::Exclude
    );
    let unavailable = refs
        .area
        .entry(StatusEntryId::from_raw(6).unwrap())
        .unwrap();
    assert!(!unavailable.available());

    let clock_item = refs
        .area
        .entry(StatusEntryId::from_raw(1).unwrap())
        .unwrap();
    assert!(
        runtime
            .ui()
            .semantics
            .get(clock_item.node())
            .unwrap()
            .actions
            .contains(SemanticAction::Activate)
    );
    assert!(runtime.dispatch_activation(clock_item.node(), ChangeSource::Keyboard));
    let intent = received.borrow()[0];
    assert!(matches!(
        intent.inferred_request(),
        Some(SystemRequest::StatusAction {
            revision,
            entry,
            action,
            source: InputSource::Keyboard,
        }) if revision.get() == 70 && entry.get() == 1 && action.get() == 11
    ));

    assert_eq!(refs.quick.actions().len(), 7);
    assert_eq!(
        runtime.ui().semantics.get(refs.quick.node()).unwrap().role,
        SemanticRole::Menu
    );
    let pause = refs
        .quick
        .action(StatusActionId::from_raw(14).unwrap())
        .unwrap();
    assert!(pause.available());
    assert!(runtime.dispatch_activation(pause.node(), ChangeSource::Pointer));
    let pointer_intent = received.borrow()[1];
    assert!(pointer_intent.inferred_request().is_none());
    assert!(matches!(
        pointer_intent.request(InputSource::Pen),
        Ok(SystemRequest::StatusAction {
            entry,
            action,
            source: InputSource::Pen,
            ..
        }) if entry.get() == 3 && action.get() == 14
    ));
    assert_eq!(
        pointer_intent.request(InputSource::Keyboard),
        Err(StatusActionSourceError::SourceMismatch)
    );
    let secret_action = refs
        .quick
        .action(StatusActionId::from_raw(16).unwrap())
        .unwrap();
    assert!(secret_action.redacted());
    assert!(!secret_action.available());
}
