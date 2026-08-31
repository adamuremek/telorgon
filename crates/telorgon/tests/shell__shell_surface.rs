use telorgon::core::{RectF, RectI, SizeI};
use telorgon::shell::{
    ClientSurfaceSnapshot, ExternalContentId, SurfaceAlphaMode, SurfaceBufferTransform,
    SurfaceCapabilities, SurfaceColorDescription, SurfaceContent, SurfaceContentRevision,
    SurfaceDamage, SurfaceGeometry, SurfaceId, SurfaceProtection, SurfaceRegion, SurfaceRegions,
    SurfaceRevision, SurfaceSampling, SurfaceStates, SurfaceSynchronizationRef,
};

#[test]
fn public_surface_snapshot_retains_bounded_protocol_neutral_host_truth() {
    let geometry = SurfaceGeometry::new(
        RectF {
            x: 10.0,
            y: 20.0,
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
    .unwrap();
    let full = SurfaceRegion::new(vec![RectF {
        x: 0.0,
        y: 0.0,
        width: 640.0,
        height: 480.0,
    }])
    .unwrap();
    let content = SurfaceContent::new(
        ExternalContentId::from_raw(3).unwrap(),
        SurfaceContentRevision::from_raw(4).unwrap(),
        Some(SurfaceSynchronizationRef::from_raw(5).unwrap()),
        SurfaceColorDescription::default(),
        SurfaceAlphaMode::Premultiplied,
        SurfaceSampling::Linear,
        SurfaceProtection::Unprotected,
    );
    let snapshot = ClientSurfaceSnapshot::new(
        SurfaceId::from_raw(1).unwrap(),
        SurfaceRevision::from_raw(2).unwrap(),
        None,
        0,
        None,
        None,
        geometry,
        SurfaceRegions::new(Some(full.clone()), full.clone(), full),
        SurfaceDamage::new(vec![RectI {
            x: 0,
            y: 0,
            width: 1280,
            height: 960,
        }])
        .unwrap(),
        content,
        SurfaceCapabilities::ACTIVATE | SurfaceCapabilities::CLOSE,
        SurfaceStates::ACTIVE,
    )
    .unwrap();

    assert_eq!(snapshot.revision().get(), 2);
    assert_eq!(snapshot.content().id().get(), 3);
    assert_eq!(snapshot.damage().len(), 1);
    assert!(snapshot.capabilities().contains(SurfaceCapabilities::CLOSE));
}
