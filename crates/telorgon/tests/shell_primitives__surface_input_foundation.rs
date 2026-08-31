use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{PointF, RectF, SizeI};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    ExternalContentId, OutputColorCapabilities, OutputGeometry, OutputRevision, OutputTransform,
    SeatId, ShellCapabilities, ShellGrantToken, SurfaceAlphaMode, SurfaceBufferTransform,
    SurfaceCapabilities, SurfaceColorDescription, SurfaceContent, SurfaceContentRevision,
    SurfaceDamage, SurfaceGeometry, SurfaceProtection, SurfaceRegions, SurfaceSampling,
    SurfaceStates,
};
use telorgon::shell_primitives::prelude::*;
use telorgon::ui::{BoxStyle, LayoutStyle, UiRoot};

fn output_id() -> OutputId {
    OutputId::from_raw(20).unwrap()
}

fn grant() -> ShellCapabilityGrant {
    ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(21).unwrap(),
        output_id(),
        ShellCapabilities::WORKSPACE_LAYER
            | ShellCapabilities::MOVE_SURFACE
            | ShellCapabilities::RESIZE_SURFACE
            | ShellCapabilities::RETAIN_SURFACE_SNAPSHOT,
    )
}

fn output_snapshot() -> OutputSnapshot {
    OutputSnapshot::new(
        output_id(),
        OutputRevision::from_raw(22).unwrap(),
        OutputGeometry::new(
            RectF {
                x: 100.0,
                y: 50.0,
                width: 100.0,
                height: 80.0,
            },
            RectF {
                x: 100.0,
                y: 50.0,
                width: 100.0,
                height: 80.0,
            },
            SizeI {
                width: 200,
                height: 160,
            },
            2.0,
            OutputTransform::Normal,
            telorgon::core::EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        )
        .unwrap(),
    )
}

fn surface(raw: u64, capabilities: SurfaceCapabilities) -> ClientSurfaceSnapshot {
    ClientSurfaceSnapshot::new(
        SurfaceId::from_raw(raw).unwrap(),
        SurfaceRevision::from_raw(raw + 100).unwrap(),
        None,
        0,
        None,
        None,
        SurfaceGeometry::new(
            RectF {
                x: 120.0,
                y: 80.0,
                width: 100.0,
                height: 60.0,
            },
            SizeI {
                width: 200,
                height: 120,
            },
            2.0,
            SurfaceBufferTransform::Normal,
            1.0,
        )
        .unwrap(),
        SurfaceRegions::new(
            None,
            SurfaceRegion::empty(),
            SurfaceRegion::new(vec![RectF {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 30.0,
            }])
            .unwrap(),
        ),
        SurfaceDamage::empty(),
        SurfaceContent::new(
            ExternalContentId::from_raw(raw + 200).unwrap(),
            SurfaceContentRevision::from_raw(raw + 300).unwrap(),
            None,
            SurfaceColorDescription::default(),
            SurfaceAlphaMode::Premultiplied,
            SurfaceSampling::Linear,
            SurfaceProtection::Unprotected,
        ),
        capabilities,
        SurfaceStates::NONE,
    )
    .unwrap()
}

struct Results {
    input: SurfaceInputRegion,
    drag: DragRegionIntent,
    resize: ResizeRegionIntent,
    edge_hit: OutputEdgeIntent,
    edge_alternative: OutputEdgeIntent,
    mounted_nodes: [telorgon::ui::UiNodeId; 5],
}

struct Fixture(Rc<RefCell<Option<Results>>>);

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, ()>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let grant = grant();
        let root = ui
            .mount_shell_root(host.0, &ShellRoot::new("Input shell", grant).unwrap())
            .unwrap();
        let output = ui
            .mount_output_view(root, &OutputView::new(output_snapshot()))
            .unwrap();
        let mut order = ShellLayerOrder::new(output_id());
        let layer = ui
            .mount_shell_layer(
                output,
                &mut order,
                &ShellLayer::new(root.authorize_layer(ShellLayerKind::Workspace).unwrap()),
            )
            .unwrap();
        let snapshot = surface(30, SurfaceCapabilities::MOVE | SurfaceCapabilities::RESIZE);
        let client = ui
            .mount_client_surface(layer, &ClientSurface::new(snapshot.clone()))
            .unwrap();

        let input = SurfaceInputRegion::from_surface(&client);
        let contact = SurfaceInputContact::new(
            SeatId::from_raw(1).unwrap(),
            ContactId::from_raw(2).unwrap(),
            InputSource::Touch,
        )
        .unwrap();
        let drag = DragRegion::new(
            &client,
            SurfaceRegion::new(vec![RectF {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            }])
            .unwrap(),
        )
        .unwrap()
        .intent(root, PointF { x: 130.0, y: 90.0 }, contact)
        .unwrap()
        .unwrap();
        let resize = ResizeRegion::new(
            &client,
            ResizeEdge::BottomRight,
            SurfaceRegion::new(vec![RectF {
                x: 90.0,
                y: 50.0,
                width: 10.0,
                height: 10.0,
            }])
            .unwrap(),
        )
        .unwrap()
        .intent(root, PointF { x: 215.0, y: 135.0 }, contact)
        .unwrap()
        .unwrap();
        let edge = OutputEdgeRegion::new(
            output,
            OutputEdgeKind::TopRight,
            OutputEdgeThickness::new(10.0).unwrap(),
        )
        .unwrap();
        let edge_hit = edge
            .hit(OutputEdgeActivation::Pointer, PointF { x: 95.0, y: 5.0 })
            .unwrap()
            .unwrap();
        let edge_alternative = edge
            .alternative(OutputEdgeActivation::Accessibility)
            .unwrap();

        let tree = ui
            .mount_surface_tree(
                layer,
                &SurfaceTree::new(vec![surface(31, SurfaceCapabilities::NONE)]).unwrap(),
            )
            .unwrap();
        let placeholder = ui
            .mount_surface_placeholder(
                layer,
                &SurfacePlaceholder::new(
                    SurfaceId::from_raw(32).unwrap(),
                    SurfaceRevision::from_raw(132).unwrap(),
                    SurfacePlaceholderReason::Unavailable,
                ),
            )
            .unwrap();
        let authorization = SurfaceSnapshotAuthorization::from_host(
            SurfaceSnapshotToken::from_raw(33).unwrap(),
            grant,
            snapshot.id(),
            snapshot.revision(),
            SurfaceSnapshotRevision::from_raw(133).unwrap(),
            SurfaceSnapshotPolicy::UnprotectedOnly,
        )
        .unwrap();
        let retained = ui
            .mount_surface_snapshot(
                layer,
                &SurfaceSnapshot::new(snapshot, authorization).unwrap(),
            )
            .unwrap();
        let exclusive = ui
            .mount_exclusive_region(
                layer,
                &ExclusiveRegion::new(
                    ExclusiveRegionGeometry::new(vec![RectF {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 10.0,
                    }])
                    .unwrap(),
                ),
            )
            .unwrap();

        *self.0.borrow_mut() = Some(Results {
            input,
            drag,
            resize,
            edge_hit,
            edge_alternative,
            mounted_nodes: [
                client.node(),
                tree.root().node(),
                placeholder.node(),
                retained.node(),
                exclusive.node(),
            ],
        });
        host
    }

    fn action(&self, _: &mut (), _: (), _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn mapping_requests_edges_and_extension_mounts_preserve_exact_boundaries() {
    let results = Rc::new(RefCell::new(None));
    let runtime = ViewRuntime::from_component(Fixture(results.clone())).unwrap();
    let results = results.borrow();
    let results = results.as_ref().unwrap();

    let eligible = results.input.map(PointF { x: 135.0, y: 95.0 }).unwrap();
    assert_eq!(eligible.local(), PointF { x: 15.0, y: 15.0 });
    assert!(eligible.is_eligible());
    assert_eq!(
        results.input.map(PointF { x: 125.0, y: 85.0 }).unwrap(),
        SurfaceInputMapping::OutsideInputRegion {
            local: PointF { x: 5.0, y: 5.0 }
        }
    );
    assert!(matches!(
        results.drag.request(),
        SurfaceRequest::BeginMove { .. }
    ));
    assert_eq!(results.drag.source(), InputSource::Touch);
    assert_eq!(results.drag.local_position(), PointF { x: 10.0, y: 10.0 });
    assert!(matches!(
        results.resize.request(),
        SurfaceRequest::BeginResize {
            edge: ResizeEdge::BottomRight,
            ..
        }
    ));
    assert_eq!(results.resize.local_position(), PointF { x: 95.0, y: 55.0 });
    assert_eq!(results.edge_hit.kind(), OutputEdgeKind::TopRight);
    assert_eq!(
        results.edge_hit.local_position(),
        Some(PointF { x: 95.0, y: 5.0 })
    );
    assert_eq!(
        results.edge_alternative.activation(),
        OutputEdgeActivation::Accessibility
    );
    assert_eq!(results.edge_alternative.local_position(), None);

    for node in results.mounted_nodes {
        assert!(runtime.ui().nodes.contains(node));
    }
}

#[test]
fn geometry_capability_and_diagnostics_fail_closed_without_payloads() {
    assert_eq!(
        OutputEdgeThickness::new(f32::NAN),
        Err(OutputEdgeRegionError::InvalidThickness)
    );
    assert_eq!(
        ExclusiveRegionGeometry::new(Vec::new()),
        Err(ExclusiveRegionError::Empty)
    );

    let mut diagnostics = ShellPrimitiveDiagnosticCollector::default();
    diagnostics.record_error(SurfaceInputRegionError::NonFiniteOutputPoint);
    diagnostics.record_error(DragRegionError::NotAuthorized);
    diagnostics.record_error(ResizeRegionError::SurfaceNotCapable);
    diagnostics.record_error(OutputEdgeRegionError::InvalidThickness);
    let snapshot = diagnostics.diagnostics();
    assert_eq!(snapshot.total(), 4);
    assert_eq!(
        snapshot.count(ShellPrimitiveDiagnosticKind::InvalidSurfaceInputMapping),
        1
    );
    assert_eq!(snapshot.iter().len(), 15);
    assert_eq!(diagnostics.clear(), snapshot);
    assert!(diagnostics.diagnostics().is_empty());
}
