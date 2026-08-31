use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{PointF, RectF, RectI, SizeI};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    ExternalContentId, OutputColorCapabilities, OutputGeometry, OutputRevision, OutputTransform,
    ShellCapabilities, ShellGrantToken, SurfaceAlphaMode, SurfaceBufferTransform,
    SurfaceCapabilities, SurfaceColorDescription, SurfaceContent, SurfaceContentRevision,
    SurfaceDamage, SurfaceGeometry, SurfaceProtection, SurfaceRegion, SurfaceRegions,
    SurfaceSampling, SurfaceStates, SurfaceSynchronizationRef,
};
use telorgon::shell_primitives::prelude::*;
use telorgon::ui::{BoxStyle, LayoutStyle, NodeKind, SemanticName, SemanticParticipation, UiRoot};

fn output() -> OutputId {
    OutputId::from_raw(7).unwrap()
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(11).unwrap(),
        output(),
        ShellCapabilities::WORKSPACE_LAYER
            | ShellCapabilities::RETAIN_SURFACE_SNAPSHOT
            | ShellCapabilities::RESERVE_OUTPUT_AREA,
    )
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output(),
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
            telorgon::core::EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        )
        .unwrap(),
    )
}

fn surface(
    raw: u64,
    parent: Option<SurfaceId>,
    stacking_order: i32,
    protection: SurfaceProtection,
) -> ClientSurfaceSnapshot {
    let geometry = SurfaceGeometry::new(
        RectF {
            x: raw as f32 * 10.0,
            y: 30.0,
            width: 120.0,
            height: 80.0,
        },
        SizeI {
            width: 240,
            height: 160,
        },
        2.0,
        SurfaceBufferTransform::Normal,
        0.8,
    )
    .unwrap();
    let region = SurfaceRegion::new(vec![RectF {
        x: 4.0,
        y: 5.0,
        width: 100.0,
        height: 60.0,
    }])
    .unwrap();
    ClientSurfaceSnapshot::new(
        SurfaceId::from_raw(raw).unwrap(),
        SurfaceRevision::from_raw(raw + 100).unwrap(),
        parent,
        stacking_order,
        None,
        None,
        geometry,
        SurfaceRegions::new(Some(region.clone()), region.clone(), region),
        SurfaceDamage::new(vec![RectI {
            x: 2,
            y: 3,
            width: 20,
            height: 10,
        }])
        .unwrap(),
        SurfaceContent::new(
            ExternalContentId::from_raw(raw + 200).unwrap(),
            SurfaceContentRevision::from_raw(raw + 300).unwrap(),
            Some(SurfaceSynchronizationRef::from_raw(raw + 400).unwrap()),
            SurfaceColorDescription::default(),
            SurfaceAlphaMode::Premultiplied,
            SurfaceSampling::Linear,
            protection,
        ),
        SurfaceCapabilities::NONE,
        SurfaceStates::NONE,
    )
    .unwrap()
}

#[test]
fn tree_validation_preserves_host_order_and_rejects_ambiguous_parentage() {
    let root = surface(1, None, 0, SurfaceProtection::Unprotected);
    let child = surface(2, Some(root.id()), 8, SurfaceProtection::Unprotected);
    let tree = SurfaceTree::new(vec![root.clone(), child.clone()]).unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(
        tree.surfaces()
            .map(ClientSurfaceSnapshot::id)
            .collect::<Vec<_>>(),
        vec![root.id(), child.id()]
    );

    assert!(matches!(
        SurfaceTree::new(vec![child.clone(), root.clone()]),
        Err(SurfaceTreeError::RootHasParent { .. })
    ));
    assert!(matches!(
        SurfaceTree::new(vec![root.clone(), root]),
        Err(SurfaceTreeError::DuplicateSurface { .. })
    ));
    assert_eq!(SurfaceTree::new(Vec::new()), Err(SurfaceTreeError::Empty));
}

#[test]
fn authorization_is_revision_protection_and_capability_bound() {
    let protected = surface(4, None, 0, SurfaceProtection::Protected);
    let ordinary_only = SurfaceSnapshotAuthorization::from_host(
        SurfaceSnapshotToken::from_raw(1).unwrap(),
        grant(),
        protected.id(),
        protected.revision(),
        SurfaceSnapshotRevision::from_raw(1).unwrap(),
        SurfaceSnapshotPolicy::UnprotectedOnly,
    )
    .unwrap();
    assert_eq!(
        SurfaceSnapshot::new(protected.clone(), ordinary_only),
        Err(SurfaceSnapshotError::ProtectedContentDenied)
    );

    let weak_grant = ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(12).unwrap(),
        output(),
        ShellCapabilities::WORKSPACE_LAYER,
    );
    assert_eq!(
        SurfaceSnapshotAuthorization::from_host(
            SurfaceSnapshotToken::from_raw(2).unwrap(),
            weak_grant,
            protected.id(),
            protected.revision(),
            SurfaceSnapshotRevision::from_raw(2).unwrap(),
            SurfaceSnapshotPolicy::AllowProtected,
        ),
        Err(SurfaceSnapshotAuthorizationError::MissingCapability)
    );
}

#[test]
fn exclusive_geometry_has_deterministic_half_open_routing() {
    let region = ExclusiveRegionGeometry::new(vec![RectF {
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 40.0,
    }])
    .unwrap();
    assert_eq!(
        region.decision(PointF { x: 10.0, y: 20.0 }).unwrap(),
        ExclusiveHitDecision::BlockLowerLayers
    );
    assert_eq!(
        region.decision(PointF { x: 40.0, y: 20.0 }).unwrap(),
        ExclusiveHitDecision::PassThrough
    );
    assert_eq!(
        region.decision(PointF {
            x: f32::NAN,
            y: 0.0
        }),
        Err(ExclusiveRegionError::NonFinitePoint)
    );
}

struct MountedRefs {
    client: ClientSurfaceRef,
    tree: SurfaceTreeRef,
    placeholder: SurfacePlaceholderRef,
    retained: SurfaceSnapshotRef,
    exclusive: ExclusiveRegionRef,
    reservation: ReservedAreaRef,
}

struct Fixture(Rc<RefCell<Option<MountedRefs>>>);

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, ()>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let grant = grant();
        let root = ShellRoot::new("Test shell", grant)
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        let output_view = OutputView::new(output_snapshot()).mount(ui, root).unwrap();
        let mut order = ShellLayerOrder::new(output());
        let layer = ShellLayer::new(root.authorize_layer(ShellLayerKind::Workspace).unwrap())
            .mount(ui, output_view, &mut order)
            .unwrap();

        let source = surface(1, None, 0, SurfaceProtection::Unprotected);
        let client = ClientSurface::new(source.clone()).mount(ui, layer).unwrap();
        let tree_root = surface(2, None, 0, SurfaceProtection::Unprotected);
        let tree_child = surface(3, Some(tree_root.id()), 1, SurfaceProtection::Unprotected);
        let tree = SurfaceTree::new(vec![tree_root, tree_child])
            .unwrap()
            .mount(ui, layer)
            .unwrap();
        let placeholder = SurfacePlaceholder::new(
            SurfaceId::from_raw(8).unwrap(),
            SurfaceRevision::from_raw(9).unwrap(),
            SurfacePlaceholderReason::Lost,
        )
        .mount(ui, layer)
        .unwrap();
        let authorization = SurfaceSnapshotAuthorization::from_host(
            SurfaceSnapshotToken::from_raw(6).unwrap(),
            grant,
            source.id(),
            source.revision(),
            SurfaceSnapshotRevision::from_raw(7).unwrap(),
            SurfaceSnapshotPolicy::UnprotectedOnly,
        )
        .unwrap();
        let retained = SurfaceSnapshot::new(source, authorization)
            .unwrap()
            .mount(ui, layer)
            .unwrap();
        let exclusive = ExclusiveRegion::new(
            ExclusiveRegionGeometry::new(vec![RectF {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            }])
            .unwrap(),
        )
        .mount(ui, layer)
        .unwrap();
        let reservation = ReservedArea::new(
            ReservedAreaId::from_raw(5).unwrap(),
            OutputEdge::Top,
            ReservedAreaExtent::new(24.0).unwrap(),
        )
        .bind(root, output_view)
        .unwrap();

        *self.0.borrow_mut() = Some(MountedRefs {
            client,
            tree,
            placeholder,
            retained,
            exclusive,
            reservation,
        });
        host
    }

    fn action(&self, _: &mut (), _: (), _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn mounted_primitives_retain_exact_metadata_without_images_or_input_listeners() {
    let references = Rc::new(RefCell::new(None));
    let runtime = ViewRuntime::from_component(Fixture(references.clone())).unwrap();
    let references = references.borrow();
    let references = references.as_ref().unwrap();

    assert_eq!(references.client.output(), output());
    assert_eq!(references.client.snapshot().geometry().opacity(), 0.8);
    assert_eq!(references.client.snapshot().regions().input().len(), 1);
    assert_eq!(references.client.snapshot().damage().len(), 1);
    assert_eq!(references.tree.surfaces().len(), 2);
    let child = &references.tree.surfaces()[1];
    assert_eq!(
        runtime.ui().nodes.core(child.node()).unwrap().parent,
        Some(references.tree.root().node())
    );
    assert_eq!(
        references.placeholder.reason(),
        SurfacePlaceholderReason::Lost
    );
    assert!(matches!(
        runtime
            .ui()
            .semantics
            .get(references.placeholder.node())
            .unwrap()
            .name,
        SemanticName::Text(_)
    ));
    assert_eq!(references.retained.authorization().revision().get(), 7);
    assert_eq!(
        references
            .exclusive
            .decision(PointF { x: 1.0, y: 1.0 })
            .unwrap(),
        ExclusiveHitDecision::BlockLowerLayers
    );
    assert_eq!(references.reservation.output(), output());
    assert_eq!(references.reservation.revision().get(), 13);
    assert!(matches!(
        references.reservation.propose(),
        OutputRequest::ProposeReservedArea { .. }
    ));
    assert!(matches!(
        references.reservation.release(),
        OutputRequest::ReleaseReservedArea { .. }
    ));

    for node in [
        references.client.node(),
        references.placeholder.node(),
        references.retained.node(),
        references.exclusive.node(),
    ] {
        assert_eq!(runtime.ui().kinds.get(node), Some(&NodeKind::Box));
        assert!(runtime.ui().images.get(node).is_none());
        assert!(
            runtime
                .ui()
                .interactions
                .get(node)
                .is_none_or(|interaction| interaction.listener_mask == 0 && !interaction.focusable)
        );
    }
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(references.client.node())
            .unwrap()
            .participation,
        SemanticParticipation::Exclude
    );
}
