use telorgon::application_components::prelude::{
    ScrollController, ScrollControllerCommand, ScrollInputSource, ScrollViewAxis,
    ScrollViewBehavior, ScrollViewCommand,
};
use telorgon::core::{PointF, SizeF};
use telorgon::ui::SemanticAction;

#[test]
fn public_scroll_view_returns_applicable_unapplied_controller_commands() {
    let mut controller = ScrollController::new(
        SizeF {
            width: 120.0,
            height: 80.0,
        },
        SizeF {
            width: 120.0,
            height: 400.0,
        },
    )
    .unwrap();
    controller
        .route(ScrollControllerCommand::ScrollTo {
            offset: PointF { x: 0.0, y: 40.0 },
            source: ScrollInputSource::Programmatic,
        })
        .unwrap();

    let behavior = ScrollViewBehavior::from_controller(&controller, ScrollViewAxis::Vertical, true);
    let command = behavior
        .semantic_request(SemanticAction::ScrollForward)
        .expect("the snapshot can scroll forward");
    assert_eq!(
        command,
        ScrollControllerCommand::ScrollBy {
            delta: PointF { x: 0.0, y: 80.0 },
            source: ScrollInputSource::Semantic,
        }
    );
    assert!(behavior.request(ScrollViewCommand::Backward).is_some());
    assert_eq!(controller.metrics().offset, PointF { x: 0.0, y: 40.0 });
}
