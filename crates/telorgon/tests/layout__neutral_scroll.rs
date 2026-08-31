use std::time::Duration;

use telorgon::core::{PointF, RectF, SizeF};
use telorgon::layout::{
    RevealRequest, ScrollActivity, ScrollInputSource, ScrollMotionRequest, ScrollPhysics,
    ScrollState,
};

#[test]
fn public_scroll_path_clamps_reveals_and_advances_caller_timed_motion() {
    let mut scroll = ScrollState::new(
        SizeF {
            width: 100.0,
            height: 100.0,
        },
        SizeF {
            width: 100.0,
            height: 500.0,
        },
    )
    .unwrap();
    scroll
        .scroll_by(PointF { x: 0.0, y: 40.0 }, ScrollInputSource::Keyboard)
        .unwrap();
    scroll
        .reveal(RevealRequest::nearest(RectF {
            x: 0.0,
            y: 180.0,
            width: 20.0,
            height: 20.0,
        }))
        .unwrap();
    assert_eq!(scroll.metrics().offset.y, 100.0);

    scroll.begin_drag();
    let started = scroll
        .end_drag(
            PointF { x: 0.0, y: 200.0 },
            ScrollPhysics::new(200.0, 0.0).unwrap(),
            false,
        )
        .unwrap();
    let ScrollMotionRequest::Start(id) = started.motion else {
        panic!("caller should receive a motion generation");
    };
    let update = scroll.step_motion(id, Duration::from_millis(500)).unwrap();

    assert_eq!(update.motion, ScrollMotionRequest::Continue(id));
    assert_eq!(scroll.activity(), ScrollActivity::Ballistic(id));
    assert_eq!(scroll.metrics().offset.y, 175.0);
}
