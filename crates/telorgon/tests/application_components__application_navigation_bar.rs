use telorgon::application_components::{
    ChangeSource, NavigationBar, NavigationBarDestination, NavigationBarNavigationKind,
    NavigationController,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Library,
    Disabled,
    Settings,
}

#[test]
fn public_navigation_bar_is_rtl_aware_and_proposes_controller_selection() {
    let mut navigation = NavigationController::new(Route::Home, None);
    let bar = NavigationBar::new(
        "Primary compact navigation",
        [
            NavigationBarDestination::new(Route::Home, "Home").unwrap(),
            NavigationBarDestination::new(Route::Library, "Library").unwrap(),
            NavigationBarDestination::new(Route::Disabled, "Disabled")
                .unwrap()
                .enabled(false),
            NavigationBarDestination::new(Route::Settings, "Settings").unwrap(),
        ],
    )
    .unwrap();

    let mut behavior = bar.behavior(&navigation).unwrap();
    let moved = behavior
        .navigate(
            CompositeNavigationCommand::Left,
            WritingDirection::RightToLeft,
        )
        .unwrap();
    assert_eq!(moved.kind(), NavigationBarNavigationKind::FocusMoved);
    assert_eq!(moved.focused(), Some(&Route::Library));
    behavior
        .navigate(
            CompositeNavigationCommand::Left,
            WritingDirection::RightToLeft,
        )
        .unwrap();
    assert_eq!(behavior.focused_route(), Some(&Route::Settings));
    assert_eq!(navigation.current(), &Route::Home);

    let request = behavior
        .request_focused_selection(ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(request.route(), &Route::Settings);
    assert_eq!(request.source(), ChangeSource::Accessibility);
    navigation
        .push(*request.route(), None, request.source())
        .unwrap();
    assert_eq!(navigation.current(), &Route::Settings);
}
