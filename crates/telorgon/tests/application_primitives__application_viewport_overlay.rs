use telorgon::application_primitives::prelude::{
    ViewportOverlay, ViewportOverlayPlacement, ViewportOverlayPlacementError,
};
use telorgon::core::{PointF, RectF};

#[test]
fn public_viewport_overlay_retains_host_geometry_and_resolved_anchor() {
    let placement = ViewportOverlayPlacement::new(
        RectF {
            x: 40.0,
            y: 20.0,
            width: 1000.0,
            height: 500.0,
        },
        PointF { x: 1.0, y: 0.5 },
        PointF { x: -16.0, y: 4.0 },
    )
    .unwrap();
    let overlay = ViewportOverlay::new(placement);

    assert_eq!(overlay.placement().viewport(), placement.viewport());
    assert_eq!(
        overlay.placement().resolved_anchor(),
        PointF {
            x: 1024.0,
            y: 274.0
        }
    );
    assert_eq!(
        ViewportOverlayPlacement::new(
            RectF {
                width: 100.0,
                height: 100.0,
                ..RectF::ZERO
            },
            PointF { x: -0.1, y: 0.5 },
            PointF::default(),
        ),
        Err(ViewportOverlayPlacementError::AnchorOutOfBounds)
    );
}
