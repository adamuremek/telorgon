use telorgon::application_components::prelude::{
    ScrollBarBehavior, ScrollBarCommand, ScrollBarTrackGeometry, ScrollController,
    ScrollControllerCommand, ScrollInputSource, ScrollViewAxis,
};
use telorgon::core::{PointF, SizeF};
use telorgon::ui::SemanticAction;

#[test]
fn public_scrollbar_projects_snapshot_and_returns_unapplied_source_preserving_commands() {
    let mut controller = ScrollController::new(
        SizeF {
            width: 80.0,
            height: 50.0,
        },
        SizeF {
            width: 400.0,
            height: 50.0,
        },
    )
    .unwrap();
    controller
        .route(ScrollControllerCommand::ScrollTo {
            offset: PointF { x: 40.0, y: 0.0 },
            source: ScrollInputSource::Programmatic,
        })
        .unwrap();

    let behavior =
        ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Horizontal, 16.0, true)
            .unwrap();
    assert_eq!(behavior.model().thumb_fraction(), 0.2);
    assert_eq!(
        behavior
            .semantic_request(SemanticAction::Increment)
            .unwrap(),
        Some(ScrollControllerCommand::ScrollBy {
            delta: PointF { x: 16.0, y: 0.0 },
            source: ScrollInputSource::Semantic,
        })
    );
    assert_eq!(
        behavior
            .request(ScrollBarCommand::ToEnd, ScrollInputSource::Keyboard)
            .unwrap(),
        Some(ScrollControllerCommand::ScrollTo {
            offset: PointF { x: 320.0, y: 0.0 },
            source: ScrollInputSource::Keyboard,
        })
    );
    let track = ScrollBarTrackGeometry::new(0.0, 200.0, 24.0).unwrap();
    assert!(behavior.drag_to_offset(80.0, track).unwrap().is_some());
    assert_eq!(controller.metrics().offset, PointF { x: 40.0, y: 0.0 });
}
