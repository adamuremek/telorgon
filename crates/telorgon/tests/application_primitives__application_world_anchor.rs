use telorgon::application_primitives::prelude::{
    WorldAnchor, WorldAnchorProjection, WorldAnchorProjectionError, WorldAnchorVisibility,
};
use telorgon::core::{PointF, Transform2D};

#[test]
fn public_world_anchor_retains_only_host_projected_inputs() {
    let projection = WorldAnchorProjection::new(
        Transform2D {
            translation: PointF { x: 480.0, y: 270.0 },
            scale: PointF { x: 0.5, y: 0.5 },
            ..Transform2D::default()
        },
        WorldAnchorVisibility::Occluded,
        18.0,
    )
    .unwrap();
    let anchor = WorldAnchor::new(projection);

    assert_eq!(anchor.projection(), projection);
    assert_eq!(
        anchor.projection().visibility(),
        WorldAnchorVisibility::Occluded
    );
    assert_eq!(anchor.projection().depth_hint(), 18.0);
    assert_eq!(
        WorldAnchorProjection::new(
            Transform2D {
                translation: PointF {
                    x: f32::INFINITY,
                    y: 0.0,
                },
                ..Transform2D::default()
            },
            WorldAnchorVisibility::Visible,
            0.0,
        ),
        Err(WorldAnchorProjectionError::NonFiniteTranslation)
    );
}
