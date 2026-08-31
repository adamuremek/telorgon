use telorgon::application_components::{
    ApplicationPopupPlacementPolicy, ApplicationPopupPlacementRequest, place_application_popup,
};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{EdgeInsets, RectF, SizeF};
use telorgon::input::WritingDirection;
use telorgon::layout::{
    PopupOverflowPolicy, PopupPlacementAdjustment, PopupPlacementAlignment, PopupPlacementCandidate,
};

#[test]
fn public_application_path_adapts_environment_and_preserves_solver_policy() {
    let environment = EnvironmentValues {
        available_size: SizeF {
            width: 240.0,
            height: 160.0,
        },
        device_scale: 1.5,
        writing_direction: WritingDirection::RightToLeft,
        safe_area: EdgeInsets::all(8.0),
        occlusions: vec![RectF {
            x: 8.0,
            y: 80.0,
            width: 224.0,
            height: 20.0,
        }],
        ..EnvironmentValues::default()
    };
    let policy = ApplicationPopupPlacementPolicy::new(
        [
            PopupPlacementCandidate::below(PopupPlacementAlignment::Start),
            PopupPlacementCandidate::above(PopupPlacementAlignment::Start),
        ],
        PopupOverflowPolicy::Shift,
    )
    .gap(4.0);
    let request = ApplicationPopupPlacementRequest::new(
        RectF {
            x: 160.0,
            y: 68.0,
            width: 40.0,
            height: 12.0,
        },
        SizeF {
            width: 72.0,
            height: 36.0,
        },
        &environment,
    )
    .policy(policy);

    let placed = place_application_popup(&request).expect("the exact above candidate remains free");
    assert_eq!(
        placed.placement.candidate,
        PopupPlacementCandidate::above(PopupPlacementAlignment::Start)
    );
    assert_eq!(placed.placement.adjustment, PopupPlacementAdjustment::Exact);
    assert_eq!(placed.placement.rect.x, 128.0);
    assert_eq!(placed.placement.rect.y, 28.0);
    assert_eq!(placed.device_scale, 1.5);
    assert_eq!(placed.writing_direction, WritingDirection::RightToLeft);
}
