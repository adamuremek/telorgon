use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{EdgeInsets, RectF, SizeI};
use telorgon::input::ChangeSource;
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    ExternalContentId, OutputColorCapabilities, OutputGeometry, OutputSnapshot, OutputTransform,
    ShellCapabilities, ShellCapabilityGrant, ShellGrantToken, SurfaceAlphaMode,
    SurfaceBufferTransform, SurfaceCapabilities, SurfaceColorDescription, SurfaceContent,
    SurfaceContentRevision, SurfaceDamage, SurfaceGeometry, SurfaceProtection, SurfaceRegions,
    SurfaceSampling, SurfaceStates, SurfaceTitle, WorkspaceName,
};
use telorgon::shell_components::prelude::*;
use telorgon::shell_primitives::{OutputView, ShellLayer, ShellLayerOrder, ShellRoot};
use telorgon::ui::{
    BoxStyle, LayoutStyle, SemanticAction, SemanticParticipation, SemanticRole, SizeRule, UiRoot,
};

fn output_id() -> OutputId {
    OutputId::from_raw(7).unwrap()
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output_id(),
        OutputRevision::from_raw(13).unwrap(),
        OutputGeometry::new(
            RectF {
                x: -100.0,
                y: 20.0,
                width: 800.0,
                height: 600.0,
            },
            RectF {
                x: -100.0,
                y: 44.0,
                width: 800.0,
                height: 576.0,
            },
            SizeI {
                width: 1600,
                height: 1200,
            },
            2.0,
            OutputTransform::Normal,
            EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        )
        .unwrap(),
    )
}

fn surface(raw: u64, bounds: RectF) -> ClientSurfaceSnapshot {
    ClientSurfaceSnapshot::new(
        SurfaceId::from_raw(raw).unwrap(),
        SurfaceRevision::from_raw(raw + 20).unwrap(),
        None,
        raw as i32,
        None,
        Some(SurfaceTitle::new(format!("Window {raw}")).unwrap()),
        SurfaceGeometry::new(
            bounds,
            SizeI {
                width: (bounds.width * 2.0) as i32,
                height: (bounds.height * 2.0) as i32,
            },
            2.0,
            SurfaceBufferTransform::Normal,
            1.0,
        )
        .unwrap(),
        SurfaceRegions::default(),
        SurfaceDamage::default(),
        SurfaceContent::new(
            ExternalContentId::from_raw(raw + 40).unwrap(),
            SurfaceContentRevision::from_raw(raw + 60).unwrap(),
            None,
            SurfaceColorDescription::default(),
            SurfaceAlphaMode::Premultiplied,
            SurfaceSampling::Linear,
            SurfaceProtection::Unprotected,
        ),
        SurfaceCapabilities::NONE,
        SurfaceStates::NONE,
    )
    .unwrap()
}

fn workspace(
    raw: u64,
    revision: u64,
    order: u32,
    active: bool,
    name: &str,
    placements: Vec<WorkspaceSurface>,
) -> WorkspaceSnapshot {
    WorkspaceSnapshot::new(
        WorkspaceId::from_raw(raw).unwrap(),
        WorkspaceRevision::from_raw(revision).unwrap(),
        order,
        WorkspaceName::new(name).unwrap(),
        active,
        placements,
    )
    .unwrap()
}

fn placement(raw: u64, bounds: RectF) -> WorkspaceSurface {
    WorkspaceSurface::new(SurfaceId::from_raw(raw).unwrap(), output_id(), bounds).unwrap()
}

fn development() -> (
    WorkspaceSnapshot,
    ClientSurfaceSnapshot,
    ClientSurfaceSnapshot,
) {
    let first_bounds = RectF {
        x: -80.0,
        y: 50.0,
        width: 300.0,
        height: 300.0,
    };
    let second_bounds = RectF {
        x: 350.0,
        y: 100.0,
        width: 250.0,
        height: 200.0,
    };
    (
        workspace(
            41,
            43,
            0,
            true,
            "Development",
            vec![placement(1, first_bounds), placement(2, second_bounds)],
        ),
        surface(1, first_bounds),
        surface(2, second_bounds),
    )
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(11).unwrap(),
        output_id(),
        ShellCapabilities::WORKSPACE_LAYER
            | ShellCapabilities::PANEL_LAYER
            | ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::SELECT_WORKSPACE
            | ShellCapabilities::RESERVE_OUTPUT_AREA,
    )
}

#[test]
fn stack_regions_and_catalog_reject_policy_ambiguity_without_rewriting_order() {
    let (development_workspace, first, second) = development();
    let reversed = WindowStack::new(
        development_workspace.clone(),
        output_id(),
        vec![
            WindowStackEntry::new("Second", second.clone()).unwrap(),
            WindowStackEntry::new("First", first.clone()).unwrap(),
        ],
    );
    assert!(matches!(
        reversed,
        Err(WindowStackError::PainterOrderMismatch { index: 0 })
    ));

    let overlapping = workspace(
        50,
        51,
        0,
        true,
        "Overlap",
        vec![
            placement(
                3,
                RectF {
                    x: 0.0,
                    y: 20.0,
                    width: 200.0,
                    height: 200.0,
                },
            ),
            placement(
                4,
                RectF {
                    x: 100.0,
                    y: 100.0,
                    width: 200.0,
                    height: 200.0,
                },
            ),
        ],
    );
    let region_bounds = output_snapshot().geometry().logical_bounds();
    assert!(matches!(
        TilingRegion::new(
            "Tiled",
            overlapping.clone(),
            output_id(),
            region_bounds,
            vec![
                SurfaceId::from_raw(3).unwrap(),
                SurfaceId::from_raw(4).unwrap()
            ],
        ),
        Err(TilingRegionError::OverlappingSurfaces { .. })
    ));
    let floating = FloatingRegion::new(
        "Floating",
        overlapping,
        output_id(),
        region_bounds,
        vec![
            SurfaceId::from_raw(4).unwrap(),
            SurfaceId::from_raw(3).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(floating.placements()[0].surface().get(), 3);
    assert_eq!(floating.placements()[1].surface().get(), 4);

    let duplicate = vec![
        development_workspace.clone(),
        workspace(41, 44, 1, false, "Duplicate", Vec::new()),
    ];
    assert!(matches!(
        WorkspaceCatalog::new(duplicate),
        Err(WorkspaceCatalogError::DuplicateWorkspace { .. })
    ));
}

#[derive(Clone)]
struct MountedRefs {
    tiled: TilingRegionRef,
    tiled_stack: WindowStackRef,
    floating: FloatingRegionRef,
    floating_stack: WindowStackRef,
    panel: PanelRef,
    switcher: WorkspaceSwitcherRef,
    overview: WorkspaceOverviewRef,
}

struct Fixture {
    references: Rc<RefCell<Option<MountedRefs>>>,
    received: Rc<RefCell<Vec<WorkspaceSelectionIntent>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = WorkspaceSelectionIntent;

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ShellRoot::new("Test shell", grant())
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        let output = OutputView::new(output_snapshot()).mount(ui, root).unwrap();
        let mut order = ShellLayerOrder::new(output_id());
        let workspace_layer =
            ShellLayer::new(root.authorize_layer(ShellLayerKind::Workspace).unwrap())
                .mount(ui, output, &mut order)
                .unwrap();

        let (development, first, second) = development();
        let view = WorkspaceView::new(development.clone())
            .mount(ui, workspace_layer, output)
            .unwrap();
        let output_bounds = output_snapshot().geometry().logical_bounds();
        let tiled = TilingRegion::new(
            "Tiled windows",
            development.clone(),
            output_id(),
            output_bounds,
            vec![first.id()],
        )
        .unwrap()
        .mount(ui, &view)
        .unwrap();
        let tiled_stack = WindowStack::new(
            development.clone(),
            output_id(),
            vec![WindowStackEntry::new("First window", first).unwrap()],
        )
        .unwrap()
        .mount(ui, &tiled, output)
        .unwrap();
        let floating = FloatingRegion::new(
            "Floating windows",
            development.clone(),
            output_id(),
            output_bounds,
            vec![second.id()],
        )
        .unwrap()
        .mount(ui, &view)
        .unwrap();
        let floating_stack = WindowStack::new(
            development.clone(),
            output_id(),
            vec![WindowStackEntry::new("Second window", second).unwrap()],
        )
        .unwrap()
        .mount(ui, &floating, output)
        .unwrap();

        let panel_layer = ShellLayer::new(root.authorize_layer(ShellLayerKind::Panel).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let panel = Panel::new(
            "System panel",
            ReservedAreaId::from_raw(61).unwrap(),
            OutputEdge::Top,
            ReservedAreaExtent::new(24.0).unwrap(),
        )
        .unwrap()
        .mount(ui, root, output, panel_layer)
        .unwrap();

        let writing = workspace(42, 44, 1, false, "Writing", Vec::new());
        let catalog = vec![development, writing];
        let overlay = ShellLayer::new(root.authorize_layer(ShellLayerKind::Overlay).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let switcher = WorkspaceSwitcher::new("Workspace switcher", catalog.clone())
            .unwrap()
            .mount(ui, root, overlay, |intent| intent)
            .unwrap();
        let overview = WorkspaceOverview::new("Workspace overview", catalog)
            .unwrap()
            .mount(ui, root, overlay, output, |intent| intent)
            .unwrap();

        *self.references.borrow_mut() = Some(MountedRefs {
            tiled,
            tiled_stack,
            floating,
            floating_stack,
            panel,
            switcher,
            overview,
        });
        host
    }

    fn action(&self, _: &mut Self::State, action: Self::Action, _: &mut UpdateContext<'_, Self>) {
        self.received.borrow_mut().push(action);
    }
}

#[test]
fn mounted_workspace_and_panel_components_preserve_host_truth_and_emit_only_requests() {
    let references = Rc::new(RefCell::new(None));
    let received = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(Fixture {
        references: Rc::clone(&references),
        received: Rc::clone(&received),
    })
    .unwrap();
    let refs = references.borrow().as_ref().unwrap().clone();

    assert_eq!(refs.tiled.placements()[0].surface().get(), 1);
    assert_eq!(refs.floating.placements()[0].surface().get(), 2);
    assert_eq!(refs.tiled_stack.frames().len(), 1);
    assert_eq!(refs.floating_stack.frames().len(), 1);
    let tiled_frame = &refs.tiled_stack.frames()[0];
    let tiled_style = runtime.ui().box_styles.get(tiled_frame.node()).unwrap();
    assert_eq!(tiled_style.transform.translation.x, 20.0);
    assert_eq!(tiled_style.transform.translation.y, 30.0);
    assert_eq!(
        runtime.ui().semantics.get(tiled_frame.node()).unwrap().role,
        SemanticRole::Window
    );

    assert_eq!(refs.panel.bounds().height, 24.0);
    assert_eq!(refs.panel.output_revision().get(), 13);
    assert!(matches!(
        refs.panel.propose(),
        OutputRequest::ProposeReservedArea {
            reservation,
            edge: OutputEdge::Top,
            ..
        } if reservation.get() == 61
    ));
    assert!(matches!(
        refs.panel.release(),
        OutputRequest::ReleaseReservedArea { reservation, .. } if reservation.get() == 61
    ));
    assert_eq!(
        runtime.ui().semantics.get(refs.panel.node()).unwrap().role,
        SemanticRole::Region
    );

    let writing = WorkspaceId::from_raw(42).unwrap();
    let switcher_item = refs.switcher.item(writing).unwrap();
    assert!(switcher_item.available());
    let semantic = runtime.ui().semantics.get(switcher_item.node()).unwrap();
    assert!(semantic.actions.contains(SemanticAction::Activate));
    assert_eq!(semantic.state.selected, Some(false));
    assert!(runtime.dispatch_activation(switcher_item.node(), ChangeSource::Keyboard));
    let intent = received.borrow()[0];
    assert!(matches!(
        intent.inferred_request(),
        Some(WorkspaceRequest::Select {
            workspace,
            revision,
            source: InputSource::Keyboard,
        }) if workspace == writing && revision.get() == 44
    ));

    let development_item = refs
        .overview
        .item(WorkspaceId::from_raw(41).unwrap())
        .unwrap();
    assert!(development_item.active());
    assert!(!development_item.available());
    assert_eq!(development_item.surfaces().len(), 2);
    let preview = development_item.surfaces()[0];
    assert_eq!(preview.source_bounds().x, -80.0);
    assert!((preview.preview_bounds().x - 3.2).abs() < 0.0001);
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(preview.node())
            .unwrap()
            .participation,
        SemanticParticipation::Exclude
    );
    let preview_style = runtime.ui().box_styles.get(preview.node()).unwrap();
    assert_eq!(preview_style.width, SizeRule::Px(48.0));
}
