use telorgon::application_components::scroll::{
    SplitViewBehavior, SplitViewCollapsePolicy, SplitViewCommand, SplitViewConstraints,
    SplitViewOperation, SplitViewOrientation, SplitViewPane, SplitViewValue,
};
use telorgon::input::ChangeSource;

#[test]
fn public_split_view_resizes_and_restores_without_mutating_controlled_state() {
    let behavior = SplitViewBehavior::new(
        SplitViewConstraints::new(800.0, 200.0, 240.0, 20.0, 100.0).unwrap(),
        SplitViewCollapsePolicy::Secondary,
        SplitViewOrientation::Horizontal,
        true,
    )
    .unwrap();
    let current = SplitViewValue::expanded(360.0);
    let resize = behavior
        .request(
            current,
            SplitViewCommand::Increment,
            ChangeSource::Accessibility,
        )
        .unwrap()
        .unwrap();
    assert_eq!(resize.value(), SplitViewValue::expanded(380.0));
    assert_eq!(resize.operation(), SplitViewOperation::Resize);
    assert_eq!(current, SplitViewValue::expanded(360.0));

    let collapsed = behavior
        .request(current, SplitViewCommand::Collapse, ChangeSource::Keyboard)
        .unwrap()
        .unwrap();
    assert_eq!(
        collapsed.value(),
        SplitViewValue::collapsed(360.0, SplitViewPane::Secondary)
    );
    let restored = behavior
        .request(
            collapsed.value(),
            SplitViewCommand::Restore,
            ChangeSource::Keyboard,
        )
        .unwrap()
        .unwrap();
    assert_eq!(restored.value(), current);
}
