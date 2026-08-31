use telorgon::application_components::range::{
    RangeModel, RangeSliderBehavior, RangeSliderCrossingPolicy, RangeSliderThumb, RangeSliderValue,
    SliderCommand, SliderOrientation,
};
use telorgon::input::{ChangeSource, WritingDirection};

#[test]
fn public_range_slider_preserves_controlled_values_and_reports_role_swaps() {
    let behavior = RangeSliderBehavior::new(
        RangeModel::new(0.0_f64, 100.0, 5.0, 20.0).unwrap(),
        RangeSliderCrossingPolicy::Swap,
        SliderOrientation::Horizontal,
        WritingDirection::LeftToRight,
        false,
        true,
    )
    .unwrap();
    let current = RangeSliderValue::new(25.0, 75.0);
    let swap = behavior
        .request(
            current,
            RangeSliderThumb::Lower,
            SliderCommand::End,
            ChangeSource::Accessibility,
        )
        .unwrap()
        .unwrap();

    assert_eq!(current, RangeSliderValue::new(25.0, 75.0));
    assert_eq!(swap.value(), &RangeSliderValue::new(75.0, 100.0));
    assert_eq!(swap.requested_thumb(), RangeSliderThumb::Lower);
    assert_eq!(swap.active_thumb(), RangeSliderThumb::Upper);
    assert!(swap.role_swapped());
    assert_eq!(swap.source(), ChangeSource::Accessibility);
}
