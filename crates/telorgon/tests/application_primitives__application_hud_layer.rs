use telorgon::application_primitives::prelude::{
    HudCoordinateSpace, HudHitTestPolicy, HudLayer, HudLayerError, HudSemanticPolicy,
};
use telorgon::core::{PointF, SizeF};

#[test]
fn public_hud_policy_retains_explicit_coordinates_hits_and_semantics() {
    let layer = HudLayer::new(
        HudCoordinateSpace::Reference(SizeF {
            width: 1600.0,
            height: 900.0,
        }),
        HudHitTestPolicy::PassThrough,
        HudSemanticPolicy::IncludeContent,
    )
    .unwrap();
    assert_eq!(layer.hit_test_policy(), HudHitTestPolicy::PassThrough);
    assert_eq!(layer.semantic_policy(), HudSemanticPolicy::IncludeContent);
    assert_eq!(
        layer
            .coordinate_space()
            .resolve_point(
                PointF { x: 800.0, y: 450.0 },
                SizeF {
                    width: 1920.0,
                    height: 1080.0,
                },
            )
            .unwrap(),
        PointF { x: 960.0, y: 540.0 }
    );
    assert_eq!(
        HudLayer::new(
            HudCoordinateSpace::Reference(SizeF {
                width: f32::NAN,
                height: 900.0,
            }),
            HudHitTestPolicy::Content,
            HudSemanticPolicy::Exclude,
        ),
        Err(HudLayerError::InvalidReferenceSize)
    );
}
