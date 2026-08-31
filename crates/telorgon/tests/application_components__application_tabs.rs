use telorgon::application_components::{
    ChangeSource, NavigationController, Tab, TabActivationPolicy, TabNavigationKind, TabPolicy,
    Tabs,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Dashboard,
    Projects,
    Settings,
}

#[test]
fn public_tabs_keep_focus_and_selection_separate_over_navigation() {
    let navigation = NavigationController::new(Route::Dashboard, None);
    let tabs = Tabs::new(
        "Workspace sections",
        [
            Tab::new(Route::Dashboard, "Dashboard").unwrap(),
            Tab::new(Route::Projects, "Projects").unwrap(),
            Tab::new(Route::Settings, "Settings").unwrap(),
        ],
    )
    .unwrap();

    let mut automatic = tabs.behavior(&navigation).unwrap();
    let moved = automatic
        .navigate(
            CompositeNavigationCommand::Right,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(moved.kind(), TabNavigationKind::FocusMoved);
    assert_eq!(moved.focused(), Some(&Route::Projects));
    assert_eq!(moved.selection().unwrap().route(), &Route::Projects);
    assert_eq!(
        moved.selection().unwrap().source(),
        ChangeSource::Directional
    );
    assert_eq!(navigation.current(), &Route::Dashboard);

    let mut manual = tabs
        .policy(TabPolicy {
            activation: TabActivationPolicy::Manual,
            ..TabPolicy::default()
        })
        .behavior(&navigation)
        .unwrap();
    let focused = manual
        .navigate(
            CompositeNavigationCommand::End,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(focused.focused(), Some(&Route::Settings));
    assert!(focused.selection().is_none());
    let requested = manual
        .request_focused_selection(ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(requested.route(), &Route::Settings);
    assert_eq!(requested.source(), ChangeSource::Accessibility);
    assert_eq!(navigation.current(), &Route::Dashboard);
}
