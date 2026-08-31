use telorgon::application_primitives::{
    EnvironmentChangeSet, EnvironmentState, EnvironmentValues, InputCapabilities, LocaleTag,
};
use telorgon::core::SizeF;

#[test]
fn application_environment_is_platform_neutral_and_revisioned() {
    let values = EnvironmentValues {
        available_size: SizeF {
            width: 800.0,
            height: 600.0,
        },
        locale: LocaleTag::parse("en-US").unwrap(),
        input_capabilities: InputCapabilities::MOUSE
            | InputCapabilities::KEYBOARD
            | InputCapabilities::TOUCH,
        ..EnvironmentValues::default()
    };
    let mut state = EnvironmentState::new(values.clone()).unwrap();
    let before = state.snapshot();
    let mut next = values;
    next.available_size.width = 640.0;
    let update = state.update(next).unwrap();

    assert_eq!(before.revision().get(), 1);
    assert_eq!(before.values().available_size.width, 800.0);
    assert_eq!(update.snapshot.revision().get(), 2);
    assert_eq!(update.snapshot.values().available_size.width, 640.0);
    assert!(update.changed.contains(EnvironmentChangeSet::GEOMETRY));
    assert!(
        update
            .snapshot
            .values()
            .input_capabilities
            .contains(InputCapabilities::MOUSE | InputCapabilities::TOUCH)
    );
}
