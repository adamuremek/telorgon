use telorgon::core::{EdgeInsets, RectF, SizeF};
use telorgon::platform::metrics::{
    AvoidRegion, AvoidRegionKind, CoordinateSpace, DisplayColorSpace, DisplayOrientation,
    DisplayProperties, DisplayTransform, HdrState, MetricInsets, MetricsRevision, PhysicalExtent,
    ScaleFactor, ViewMetrics, ViewMetricsState,
};

#[test]
fn public_metrics_path_preserves_spaces_zero_extent_and_atomic_revisions() {
    let display = DisplayProperties::new(
        DisplayTransform::MirrorRotate270,
        DisplayColorSpace::Rec2020,
        HdrState::Active,
    );
    let drawing = MetricInsets::new(
        CoordinateSpace::ViewLogical,
        EdgeInsets {
            top: 8.0,
            right: 12.0,
            bottom: 16.0,
            left: 12.0,
        },
    )
    .unwrap();
    let ime = AvoidRegion::new(
        AvoidRegionKind::Ime,
        CoordinateSpace::ViewPhysical,
        RectF {
            x: 0.0,
            y: 1200.0,
            width: 2560.0,
            height: 240.0,
        },
    )
    .unwrap();
    let initial = ViewMetrics::new(
        PhysicalExtent::new(2560, 1440),
        ScaleFactor::new(2.0).unwrap(),
        display,
    )
    .unwrap()
    .with_safe_drawing_insets(drawing)
    .unwrap()
    .with_avoid_regions(vec![ime])
    .unwrap();
    assert_eq!(
        initial.logical_extent(),
        SizeF {
            width: 1280.0,
            height: 720.0,
        }
    );
    assert_eq!(
        initial.display().orientation(),
        DisplayOrientation::Clockwise270
    );
    assert!(initial.display().transform().is_mirrored());
    assert_eq!(
        initial.avoid_regions()[0].space(),
        CoordinateSpace::ViewPhysical
    );

    let mut state = ViewMetricsState::new(initial.clone());
    assert_eq!(state.snapshot().revision(), MetricsRevision::INITIAL);
    assert!(!state.update(initial).unwrap().is_changed());

    let zero_extent = ViewMetrics::new(
        PhysicalExtent::new(0, 1440),
        ScaleFactor::new(2.0).unwrap(),
        display,
    )
    .unwrap();
    let update = state.update(zero_extent).unwrap();
    assert!(update.is_changed());
    assert_eq!(update.current().revision().get(), 2);
    assert_eq!(
        update.current().metrics().physical_extent(),
        PhysicalExtent::new(0, 1440)
    );
    assert!(!update.current().is_renderable());
}
