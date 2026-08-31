use telorgon::core::{EdgeInsets, PointF, RectF, SizeI};
use telorgon::shell::{
    OutputColorCapabilities, OutputGeometry, OutputId, OutputRevision, OutputSnapshot,
    OutputTransform,
};
use telorgon::shell_primitives::prelude::{OutputView, OutputViewMappingError};

#[test]
fn public_output_view_maps_global_host_geometry_to_local_logical_coordinates() {
    let view = OutputView::new(OutputSnapshot::new(
        OutputId::from_raw(1).unwrap(),
        OutputRevision::from_raw(2).unwrap(),
        OutputGeometry::new(
            RectF {
                x: 100.0,
                y: -50.0,
                width: 800.0,
                height: 600.0,
            },
            RectF {
                x: 100.0,
                y: -20.0,
                width: 800.0,
                height: 570.0,
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
    ));

    assert_eq!(
        view.to_local(PointF { x: 125.0, y: 0.0 }).unwrap(),
        PointF { x: 25.0, y: 50.0 }
    );
    assert_eq!(view.local_usable_bounds().y, 30.0);
    assert_eq!(
        view.to_local(PointF {
            x: f32::NAN,
            y: 0.0,
        }),
        Err(OutputViewMappingError::NonFinitePoint)
    );
}
