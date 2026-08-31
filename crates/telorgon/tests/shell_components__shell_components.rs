use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{ColorRgba8, EdgeInsets, PointF, RectF, SizeI};
use telorgon::input::ChangeSource;
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    ContactId, ExternalContentId, OutputColorCapabilities, OutputGeometry, OutputSnapshot,
    OutputTransform, SeatId, ShellCapabilities, ShellCapabilityGrant, ShellGrantToken,
    SurfaceAlphaMode, SurfaceBufferTransform, SurfaceCapabilities, SurfaceColorDescription,
    SurfaceContent, SurfaceContentRevision, SurfaceDamage, SurfaceGeometry, SurfaceProtection,
    SurfaceRegions, SurfaceSampling, SurfaceStates, SurfaceTitle, WorkspaceName,
};
use telorgon::shell_components::prelude::*;
use telorgon::shell_primitives::{OutputView, ShellLayer, ShellLayerOrder, ShellRoot};
use telorgon::ui::{
    BoxStyle, LayoutStyle, NodeKind, SemanticAction, SemanticName, SemanticParticipation,
    SemanticRelationshipKind, SemanticRole, Shadow, SizeRule, UiRoot,
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

fn surface(states: SurfaceStates) -> ClientSurfaceSnapshot {
    ClientSurfaceSnapshot::new(
        SurfaceId::from_raw(19).unwrap(),
        SurfaceRevision::from_raw(23).unwrap(),
        None,
        4,
        None,
        Some(SurfaceTitle::new("Terminal").unwrap()),
        SurfaceGeometry::new(
            RectF {
                x: -80.0,
                y: 50.0,
                width: 640.0,
                height: 480.0,
            },
            SizeI {
                width: 1280,
                height: 960,
            },
            2.0,
            SurfaceBufferTransform::Normal,
            1.0,
        )
        .unwrap(),
        SurfaceRegions::default(),
        SurfaceDamage::default(),
        SurfaceContent::new(
            ExternalContentId::from_raw(31).unwrap(),
            SurfaceContentRevision::from_raw(37).unwrap(),
            None,
            SurfaceColorDescription::default(),
            SurfaceAlphaMode::Premultiplied,
            SurfaceSampling::Linear,
            SurfaceProtection::Unprotected,
        ),
        SurfaceCapabilities::MOVE
            | SurfaceCapabilities::MINIMIZE
            | SurfaceCapabilities::MAXIMIZE
            | SurfaceCapabilities::CLOSE,
        states,
    )
    .unwrap()
}

fn workspace_snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot::new(
        WorkspaceId::from_raw(41).unwrap(),
        WorkspaceRevision::from_raw(43).unwrap(),
        2,
        WorkspaceName::new("Development").unwrap(),
        true,
        vec![
            WorkspaceSurface::new(
                SurfaceId::from_raw(19).unwrap(),
                output_id(),
                RectF {
                    x: -80.0,
                    y: 50.0,
                    width: 640.0,
                    height: 480.0,
                },
            )
            .unwrap(),
            WorkspaceSurface::new(
                SurfaceId::from_raw(29).unwrap(),
                OutputId::from_raw(8).unwrap(),
                RectF {
                    x: 900.0,
                    y: 10.0,
                    width: 100.0,
                    height: 80.0,
                },
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(11).unwrap(),
        output_id(),
        ShellCapabilities::WORKSPACE_LAYER
            | ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::MOVE_SURFACE
            | ShellCapabilities::MINIMIZE_SURFACE
            | ShellCapabilities::MAXIMIZE_SURFACE
            | ShellCapabilities::CLOSE_SURFACE,
    )
}

#[derive(Clone)]
struct MountedRefs {
    root: ShellRootRef,
    frame: WindowFrameRef,
    titlebar: WindowTitlebarRef,
    controls: WindowControlsRef,
    shadow: ShadowFrameRef,
    preview: SnapPreviewRef,
    workspace: WorkspaceViewRef,
}

struct Fixture {
    references: Rc<RefCell<Option<MountedRefs>>>,
    received: Rc<RefCell<Vec<WindowControlIntent>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = WindowControlIntent;

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

        let workspace = WorkspaceView::new(workspace_snapshot())
            .mount(ui, workspace_layer, output)
            .unwrap();
        let snapshot = surface(SurfaceStates::ACTIVE);
        let frame = WindowFrame::new("Terminal window", snapshot.clone())
            .unwrap()
            .mount(ui, workspace_layer, output)
            .unwrap();
        let shadow = ShadowFrame::new(
            snapshot.clone(),
            Shadow {
                offset: PointF { x: 0.0, y: 8.0 },
                blur: 18.0,
                spread: 2.0,
                color: ColorRgba8::rgba(0, 0, 0, 120),
            },
        )
        .unwrap()
        .mount(ui, &frame)
        .unwrap();
        let titlebar = WindowTitlebar::from_snapshot_title(snapshot.clone())
            .unwrap()
            .mount(ui, &frame)
            .unwrap();
        let controls = WindowControls::new(snapshot.clone())
            .mount(ui, &titlebar, root, |intent| intent)
            .unwrap();

        let overlay = ShellLayer::new(root.authorize_layer(ShellLayerKind::Overlay).unwrap())
            .mount(ui, output, &mut order)
            .unwrap();
        let preview = SnapPreview::from_snapshots(
            &snapshot,
            output_snapshot(),
            RectF {
                x: 400.0,
                y: 0.0,
                width: 400.0,
                height: 600.0,
            },
        )
        .unwrap()
        .mount(ui, overlay, output)
        .unwrap();

        *self.references.borrow_mut() = Some(MountedRefs {
            root,
            frame,
            titlebar,
            controls,
            shadow,
            preview,
            workspace,
        });
        host
    }

    fn action(&self, _: &mut Self::State, action: Self::Action, _: &mut UpdateContext<'_, Self>) {
        self.received.borrow_mut().push(action);
    }
}

#[test]
fn constructors_validate_and_controls_follow_the_exact_surface_state() {
    assert!(WindowFrame::new(" ", surface(SurfaceStates::NONE)).is_err());
    assert!(WindowTitlebar::new(" ", surface(SurfaceStates::NONE)).is_err());
    assert_eq!(
        WindowControls::new(surface(SurfaceStates::MAXIMIZED)).available(),
        [
            WindowControl::Minimize,
            WindowControl::Restore,
            WindowControl::Close,
        ]
    );
    assert!(matches!(
        ShadowFrame::new(
            surface(SurfaceStates::NONE),
            Shadow {
                blur: -1.0,
                ..Shadow::default()
            }
        ),
        Err(ShadowFrameError::InvalidBlur)
    ));
    assert!(matches!(
        SnapPreview::new(
            SurfaceId::from_raw(1).unwrap(),
            SurfaceRevision::from_raw(1).unwrap(),
            output_id(),
            OutputRevision::from_raw(1).unwrap(),
            RectF {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 10.0,
            }
        ),
        Err(SnapPreviewError::InvalidBounds)
    ));
}

#[test]
fn mounted_catalog_preserves_structure_geometry_semantics_and_typed_requests() {
    let references = Rc::new(RefCell::new(None));
    let received = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(Fixture {
        references: Rc::clone(&references),
        received: Rc::clone(&received),
    })
    .unwrap();
    let refs = references.borrow().as_ref().unwrap().clone();

    let window = runtime.ui().semantics.get(refs.frame.node()).unwrap();
    assert_eq!(window.role, SemanticRole::Window);
    assert!(!window.state.focusable);
    assert!(window.actions.is_empty());
    assert_eq!(window.relationships.len(), 2);
    assert!(
        window
            .relationships
            .iter()
            .all(|relationship| relationship.kind == SemanticRelationshipKind::Owns)
    );
    assert_eq!(
        runtime
            .ui()
            .nodes
            .core(refs.frame.chrome_node())
            .unwrap()
            .parent,
        Some(refs.frame.node())
    );
    assert_eq!(
        runtime
            .ui()
            .nodes
            .core(refs.frame.client_content_node())
            .unwrap()
            .parent,
        Some(refs.frame.node())
    );
    let frame_style = runtime.ui().box_styles.get(refs.frame.node()).unwrap();
    assert_eq!(frame_style.width, SizeRule::Px(640.0));
    assert_eq!(frame_style.height, SizeRule::Px(480.0));
    assert_eq!(
        frame_style.transform.translation,
        PointF { x: 20.0, y: 30.0 }
    );

    let title = runtime
        .ui()
        .semantics
        .get(refs.titlebar.title_node())
        .unwrap();
    assert_eq!(title.role, SemanticRole::Text);
    let SemanticName::Text(title_name) = title.name else {
        panic!("title must be named");
    };
    assert_eq!(runtime.ui().string(title_name), Some("Terminal"));

    let contact = SurfaceInputContact::new(
        SeatId::from_raw(2).unwrap(),
        ContactId::from_raw(3).unwrap(),
        InputSource::Touch,
    )
    .unwrap();
    let move_intent = refs.titlebar.begin_move_intent(refs.root, contact).unwrap();
    assert_eq!(move_intent.source(), InputSource::Touch);
    assert_eq!(
        move_intent.revision(),
        SurfaceRevision::from_raw(23).unwrap()
    );
    assert_eq!(
        move_intent.request(),
        SurfaceRequest::BeginMove {
            surface: SurfaceId::from_raw(19).unwrap(),
            contact: ContactId::from_raw(3).unwrap(),
        }
    );

    assert_eq!(refs.controls.controls().len(), 3);
    let maximize = refs.controls.control(WindowControl::Maximize).unwrap();
    let close = refs.controls.control(WindowControl::Close).unwrap();
    for control in [maximize, close] {
        let semantics = runtime.ui().semantics.get(control.node()).unwrap();
        assert_eq!(semantics.role, SemanticRole::Button);
        assert!(semantics.actions.contains(SemanticAction::Activate));
        assert!(control.enabled());
    }
    assert!(runtime.dispatch_activation(maximize.node(), ChangeSource::Keyboard));
    assert!(runtime.dispatch_activation(close.node(), ChangeSource::Accessibility));
    assert_eq!(received.borrow().len(), 2);
    assert_eq!(received.borrow()[0].control(), WindowControl::Maximize);
    assert_eq!(
        received.borrow()[0].request(),
        SurfaceRequest::SetMaximized {
            surface: SurfaceId::from_raw(19).unwrap(),
            maximized: true,
        }
    );
    assert_eq!(
        refs.controls.snapshot().states(),
        SurfaceStates::ACTIVE,
        "request emission must not optimistically mutate host state"
    );

    assert_eq!(
        runtime.ui().kinds.get(refs.shadow.node()),
        Some(&NodeKind::Box)
    );
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(refs.shadow.node())
            .unwrap()
            .participation,
        SemanticParticipation::Exclude
    );
    assert_eq!(
        runtime
            .ui()
            .box_styles
            .get(refs.shadow.node())
            .unwrap()
            .shadows
            .as_slice()[0]
            .blur,
        18.0
    );

    assert_eq!(refs.preview.bounds().x, 400.0);
    assert_eq!(refs.preview.output_revision().get(), 13);
    let preview_style = runtime.ui().box_styles.get(refs.preview.node()).unwrap();
    assert_eq!(preview_style.width, SizeRule::Px(400.0));
    assert_eq!(preview_style.transform.translation.x, 400.0);
    assert!(runtime.ui().interactions.get(refs.preview.node()).is_none());

    assert_eq!(refs.workspace.placements().len(), 1);
    assert_eq!(refs.workspace.placements()[0].surface().get(), 19);
    assert_eq!(
        refs.workspace
            .local_bounds(SurfaceId::from_raw(19).unwrap()),
        Some(RectF {
            x: 20.0,
            y: 30.0,
            width: 640.0,
            height: 480.0,
        })
    );
    let workspace = runtime.ui().semantics.get(refs.workspace.node()).unwrap();
    assert_eq!(workspace.role, SemanticRole::Region);
    assert!(!workspace.state.hidden);
}
