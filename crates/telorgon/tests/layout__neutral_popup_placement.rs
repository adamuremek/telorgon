use telorgon::core::{RectF, SizeF};
use telorgon::layout::{
    PopupOverflowPolicy, PopupPlacementAdjustment, PopupPlacementAlignment,
    PopupPlacementCandidate, PopupPlacementRequest, place_popup,
};

#[test]
fn public_popup_path_flips_before_using_typed_overflow() {
    let mut request = PopupPlacementRequest::new(
        RectF {
            x: 40.0,
            y: 80.0,
            width: 20.0,
            height: 10.0,
        },
        SizeF {
            width: 40.0,
            height: 30.0,
        },
        RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
        [
            PopupPlacementCandidate::below(PopupPlacementAlignment::Start),
            PopupPlacementCandidate::above(PopupPlacementAlignment::Start),
        ],
    );
    request.overflow = PopupOverflowPolicy::Shift;
    let placed = place_popup(&request).expect("the above candidate fits exactly");
    assert_eq!(
        placed.candidate,
        PopupPlacementCandidate::above(PopupPlacementAlignment::Start)
    );
    assert_eq!(placed.adjustment, PopupPlacementAdjustment::Exact);
}
