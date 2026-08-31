use telorgon::application_components::{
    ChangeSource, NavigationController, NavigationRail, NavigationRailDestination,
    NavigationRailNavigationKind,
};
use telorgon::input::CompositeNavigationCommand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Projects,
    Disabled,
    Settings,
}

#[test]
fn public_navigation_rail_proposes_controller_selection_without_owning_it() {
    let mut navigation = NavigationController::new(Route::Home, None);
    let rail = NavigationRail::new(
        "Primary navigation",
        [
            NavigationRailDestination::new(Route::Home, "Home").unwrap(),
            NavigationRailDestination::new(Route::Projects, "Projects").unwrap(),
            NavigationRailDestination::new(Route::Disabled, "Disabled")
                .unwrap()
                .enabled(false),
            NavigationRailDestination::new(Route::Settings, "Settings").unwrap(),
        ],
    )
    .unwrap();

    let mut behavior = rail.behavior(&navigation).unwrap();
    assert_eq!(
        behavior
            .navigate(CompositeNavigationCommand::Down)
            .unwrap()
            .kind(),
        NavigationRailNavigationKind::FocusMoved
    );
    assert_eq!(behavior.focused_route(), Some(&Route::Projects));
    behavior.navigate(CompositeNavigationCommand::Down).unwrap();
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
