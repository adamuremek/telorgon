use std::time::Duration;

use telorgon::application_components::prelude::{
    ScrollActivity, ScrollController, ScrollControllerCommand, ScrollInputSource,
    ScrollMotionRequest, ScrollPhysics,
};
use telorgon::core::{PointF, SizeF};

#[test]
fn public_scroll_controller_returns_unapplied_generation_checked_motion() {
    let mut controller = ScrollController::new(
        SizeF {
            width: 100.0,
            height: 100.0,
        },
        SizeF {
            width: 100.0,
            height: 600.0,
        },
    )
    .unwrap();
    controller
        .route(ScrollControllerCommand::ScrollBy {
            delta: PointF { x: 0.0, y: 80.0 },
            source: ScrollInputSource::Keyboard,
        })
        .unwrap();
    controller
        .route(ScrollControllerCommand::BeginDrag)
        .unwrap();
    let ended = controller
        .route(ScrollControllerCommand::EndDrag {
            velocity: PointF { x: 0.0, y: 240.0 },
            physics: ScrollPhysics::new(240.0, 0.0).unwrap(),
            reduced_motion: false,
        })
        .unwrap();
    let ScrollMotionRequest::Start(id) = ended.motion() else {
        panic!("the caller must receive the motion generation to schedule")
    };
    assert_eq!(controller.activity(), ScrollActivity::Ballistic(id));

    let stepped = controller
        .route(ScrollControllerCommand::StepMotion {
            id,
            elapsed: Duration::from_millis(100),
        })
        .unwrap();
    assert!(stepped.update().after.offset.y > 80.0);
    assert_eq!(controller.metrics(), stepped.update().after);
}
