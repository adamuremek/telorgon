use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use telorgon::core::{EdgeInsets, RectF, SizeI};
use telorgon::input::ChangeSource;
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    OutputColorCapabilities, OutputGeometry, OutputSnapshot, OutputTransform, ShellCapabilities,
    ShellCapabilityGrant, ShellGrantToken,
};
use telorgon::shell_components::prelude::*;
use telorgon::shell_primitives::{OutputView, ShellLayer, ShellLayerOrder, ShellRoot};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticAction, SemanticRole, UiRoot};

fn output_id() -> OutputId {
    OutputId::from_raw(17).unwrap()
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output_id(),
        OutputRevision::from_raw(23).unwrap(),
        OutputGeometry::new(
            RectF {
                x: 40.0,
                y: -20.0,
                width: 1280.0,
                height: 720.0,
            },
            RectF {
                x: 40.0,
                y: -20.0,
                width: 1280.0,
                height: 680.0,
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

fn application(
    raw: u64,
    label: &str,
    enabled: Option<bool>,
    states: ApplicationStates,
) -> ApplicationEntry {
    let action_id = ApplicationActionId::from_raw(raw + 100).unwrap();
    let actions = enabled.map_or_else(Vec::new, |enabled| {
        vec![ApplicationAction::new(
            action_id,
            ApplicationActionKind::Activate,
            ApplicationLabel::new(format!("Open {label}")).unwrap(),
            enabled,
        )]
    });
    ApplicationEntry::new(
        ApplicationId::from_raw(raw).unwrap(),
        ApplicationRevision::from_raw(raw + 10).unwrap(),
        ApplicationLabel::new(label).unwrap(),
        Some(ApplicationDescription::new(format!("{label} application")).unwrap()),
        None,
        states,
        enabled.map(|_| action_id),
        actions,
    )
    .unwrap()
}

fn applications() -> Vec<ApplicationEntry> {
    vec![
        application(1, "Editor", Some(true), ApplicationStates::ACTIVE),
        application(2, "Music", Some(false), ApplicationStates::RUNNING),
        application(3, "Terminal", None, ApplicationStates::PINNED),
    ]
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(31).unwrap(),
        output_id(),
        ShellCapabilities::PANEL_LAYER
            | ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::RESERVE_OUTPUT_AREA
            | ShellCapabilities::INVOKE_SYSTEM_ACTION,
    )
}

#[test]
fn auto_hide_and_application_catalogs_reject_ambiguous_caller_state() {
    let policy = PanelAutoHidePolicy::new(
        Duration::from_millis(100),
        Duration::from_millis(200),
        Duration::from_millis(300),
    )
    .unwrap();
    let hidden = PanelAutoHideSnapshot::hidden(MonotonicInstant::ZERO);
    let armed = policy
        .transition(
            hidden,
            PanelAutoHideInput::Reveal(PanelRevealSource::Touch),
            MonotonicInstant::from_nanos(10_000_000),
        )
        .unwrap()
        .next();
    assert_eq!(armed.state(), PanelAutoHideState::RevealArmed);
    assert_eq!(armed.deadline().unwrap().as_nanos(), 110_000_000);
    assert!(matches!(
        policy.transition(
            armed,
            PanelAutoHideInput::DeadlineElapsed,
            MonotonicInstant::from_nanos(109_000_000),
        ),
        Err(PanelAutoHideError::DeadlineNotReached { .. })
    ));
    let revealing = policy
        .transition(
            armed,
            PanelAutoHideInput::DeadlineElapsed,
            MonotonicInstant::from_nanos(110_000_000),
        )
        .unwrap()
        .next();
    assert_eq!(revealing.state(), PanelAutoHideState::Revealing);
    let shown = policy
        .transition(
            revealing,
            PanelAutoHideInput::DeadlineElapsed,
            MonotonicInstant::from_nanos(310_000_000),
        )
        .unwrap()
        .next();
    assert_eq!(shown.state(), PanelAutoHideState::Shown);
    let hiding = policy
        .transition(
            shown,
            PanelAutoHideInput::Conceal,
            MonotonicInstant::from_nanos(400_000_000),
        )
        .unwrap()
        .next();
    assert_eq!(hiding.state(), PanelAutoHideState::Hiding);
    assert_eq!(
        policy
            .transition(
                hiding,
                PanelAutoHideInput::Reveal(PanelRevealSource::Directional),
                MonotonicInstant::from_nanos(500_000_000),
            )
            .unwrap()
            .next()
            .state(),
        PanelAutoHideState::Revealing
    );

    let first = application(1, "Editor", Some(true), ApplicationStates::NONE);
    assert!(matches!(
        ApplicationCatalog::new(vec![first.clone(), first]),
        Err(ApplicationCatalogError::DuplicateApplication { .. })
    ));
    assert!(matches!(
        ApplicationGrid::new("Applications", applications(), 0),
        Err(ApplicationGridError::InvalidColumnCount { columns: 0 })
    ));
}

#[derive(Clone)]
struct MountedRefs {
    panel: PanelRef,
    taskbar: TaskbarRef,
    dock: DockRef,
    launcher: LauncherRef,
    grid: ApplicationGridRef,
    menu: StartMenuRef,
}

struct Fixture {
    references: Rc<RefCell<Option<MountedRefs>>>,
    received: Rc<RefCell<Vec<ApplicationActionIntent>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = ApplicationActionIntent;

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ShellRoot::new("Launcher test shell", grant())
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        let output = OutputView::new(output_snapshot()).mount(ui, root).unwrap();
        let mut order = ShellLayerOrder::new(output_id());
        let panel_layer = ShellLayer::new(root.authorize_layer(ShellLayerKind::Panel).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let panel = Panel::new(
            "Application panel",
            ReservedAreaId::from_raw(51).unwrap(),
            OutputEdge::Bottom,
            ReservedAreaExtent::new(40.0).unwrap(),
        )
        .unwrap()
        .mount(ui, root, output, panel_layer)
        .unwrap();
        let catalog = ApplicationCatalog::new(applications()).unwrap();
        let taskbar = Taskbar::from_catalog("Taskbar", catalog.clone())
            .unwrap()
            .mount(ui, root, panel, |intent| intent)
            .unwrap();
        let dock = Dock::from_catalog("Dock", catalog.clone())
            .unwrap()
            .mount(ui, root, panel, |intent| intent)
            .unwrap();

        let overlay = ShellLayer::new(root.authorize_layer(ShellLayerKind::Overlay).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let launcher = Launcher::from_catalog("Launcher", catalog.clone())
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        let grid = ApplicationGrid::from_catalog("Application grid", catalog.clone(), 2)
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        let menu = StartMenu::from_catalog("Start menu", catalog)
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        *self.references.borrow_mut() = Some(MountedRefs {
            panel,
            taskbar,
            dock,
            launcher,
            grid,
            menu,
        });
        host
    }

    fn action(&self, _: &mut Self::State, action: Self::Action, _: &mut UpdateContext<'_, Self>) {
        self.received.borrow_mut().push(action);
    }
}

#[test]
fn panel_and_launcher_presentations_preserve_exact_entries_and_emit_typed_requests() {
    let references = Rc::new(RefCell::new(None));
    let received = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(Fixture {
        references: Rc::clone(&references),
        received: Rc::clone(&received),
    })
    .unwrap();
    let refs = references.borrow().as_ref().unwrap().clone();
    let editor = ApplicationId::from_raw(1).unwrap();
    let music = ApplicationId::from_raw(2).unwrap();

    assert_eq!(refs.panel.edge(), OutputEdge::Bottom);
    assert_eq!(refs.panel.extent().get(), 40.0);
    assert_eq!(refs.taskbar.items().len(), 3);
    assert_eq!(refs.dock.items().len(), 3);
    assert_eq!(refs.launcher.items().len(), 3);
    assert_eq!(refs.grid.items().len(), 3);
    assert_eq!(refs.menu.items().len(), 3);
    assert!(refs.taskbar.item(editor).unwrap().available());
    assert!(!refs.taskbar.item(music).unwrap().available());
    assert_eq!(refs.grid.items()[2].row(), 1);
    assert_eq!(refs.grid.items()[2].column(), 0);

    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(refs.taskbar.node())
            .unwrap()
            .role,
        SemanticRole::Toolbar
    );
    assert_eq!(
        runtime.ui().semantics.get(refs.dock.node()).unwrap().role,
        SemanticRole::Navigation
    );
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(refs.launcher.node())
            .unwrap()
            .role,
        SemanticRole::List
    );
    assert_eq!(
        runtime.ui().semantics.get(refs.grid.node()).unwrap().role,
        SemanticRole::Grid
    );
    assert_eq!(
        runtime.ui().semantics.get(refs.menu.node()).unwrap().role,
        SemanticRole::Menu
    );
    let editor_item = refs.launcher.item(editor).unwrap();
    assert!(
        runtime
            .ui()
            .semantics
            .get(editor_item.node())
            .unwrap()
            .actions
            .contains(SemanticAction::Activate)
    );
    assert!(runtime.dispatch_activation(editor_item.node(), ChangeSource::Keyboard));
    let intent = received.borrow()[0];
    assert!(matches!(
        intent.inferred_request(),
        Some(SystemRequest::ApplicationAction {
            application,
            revision,
            action,
            source: InputSource::Keyboard,
        }) if application == editor && revision.get() == 11 && action.get() == 101
    ));

    let taskbar_editor = refs.taskbar.item(editor).unwrap();
    assert!(runtime.dispatch_activation(taskbar_editor.node(), ChangeSource::Pointer));
    let pointer_intent = received.borrow()[1];
    assert!(pointer_intent.inferred_request().is_none());
    assert!(matches!(
        pointer_intent.request(InputSource::Touch),
        Ok(SystemRequest::ApplicationAction {
            source: InputSource::Touch,
            ..
        })
    ));
    assert_eq!(
        pointer_intent.request(InputSource::Keyboard),
        Err(ApplicationActionSourceError::SourceMismatch)
    );
}
