use telorgon::core::{EdgeInsets, RectF, SizeI};
use telorgon::shell::{
    OutputColorCapabilities, OutputGeometry, OutputGeometryError, OutputId, OutputRevision,
    OutputSnapshot, OutputTransform,
};

#[test]
fn public_output_snapshot_is_validated_revisioned_host_truth() {
    let logical = RectF {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    };
    let geometry = OutputGeometry::new(
        logical,
        RectF {
            y: 24.0,
            height: 696.0,
            ..logical
        },
        SizeI {
            width: 2560,
            height: 1440,
        },
        2.0,
        OutputTransform::Normal,
        EdgeInsets::all(4.0),
        OutputColorCapabilities::SRGB | OutputColorCapabilities::HDR_STATIC_METADATA,
    )
    .unwrap();
    let snapshot = OutputSnapshot::new(
        OutputId::from_raw(5).unwrap(),
        OutputRevision::from_raw(12).unwrap(),
        geometry,
    );

    assert_eq!(snapshot.id().get(), 5);
    assert_eq!(snapshot.revision().get(), 12);
    assert_eq!(snapshot.geometry().usable_bounds().y, 24.0);
    assert_eq!(snapshot.geometry().safe_bounds().width, 1272.0);
    assert!(
        snapshot
            .geometry()
            .color_capabilities()
            .contains(OutputColorCapabilities::HDR_STATIC_METADATA)
    );

    assert_eq!(
        OutputGeometry::new(
            logical,
            logical,
            SizeI {
                width: 2560,
                height: 1440,
            },
            0.0,
            OutputTransform::Normal,
            EdgeInsets::ZERO,
            OutputColorCapabilities::SRGB,
        ),
        Err(OutputGeometryError::InvalidScale)
    );
}
