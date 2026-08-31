use telorgon::application_components::navigation::{
    NavigationController, NavigationRestorationKey, RouteHost, RouteHostContentState,
    RouteHostKeepAlivePolicy, RouteHostRegistration, RouteHostRestorationIntent,
};
use telorgon::input::ChangeSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Details,
    Editor,
}

fn key(value: u64) -> NavigationRestorationKey {
    NavigationRestorationKey::from_raw(value).unwrap()
}

#[test]
fn public_route_host_bounds_retention_and_exposes_controller_restoration_intent() {
    let mut navigation = NavigationController::new(Route::Home, Some(key(1)));
    navigation
        .push(Route::Details, Some(key(2)), ChangeSource::Programmatic)
        .unwrap();
    navigation
        .push(Route::Editor, Some(key(3)), ChangeSource::Programmatic)
        .unwrap();
    let host = RouteHost::new(
        "Application content",
        [
            RouteHostRegistration::new(Route::Home, "Home content")
                .unwrap()
                .retained_bytes(40),
            RouteHostRegistration::new(Route::Details, "Details content")
                .unwrap()
                .retained_bytes(60),
            RouteHostRegistration::new(Route::Editor, "Editor content")
                .unwrap()
                .retained_bytes(80),
        ],
    )
    .unwrap()
    .keep_alive(RouteHostKeepAlivePolicy::bounded(1, 64).unwrap());

    let initial = host.plan(&navigation).unwrap();
    assert_eq!(initial.current().route(), &Route::Editor);
    assert_eq!(initial.kept_alive()[0].route(), &Route::Details);
    assert_eq!(
        initial.kept_alive()[0].state(),
        RouteHostContentState::KeptAlive
    );
    assert_eq!(initial.evicted()[0].route(), &Route::Home);
    assert_eq!(initial.retained_bytes(), 60);

    let popped = navigation.pop(ChangeSource::Accessibility).unwrap();
    assert_eq!(popped.restoration_key(), Some(key(2)));
    let revealed = host.plan(&navigation).unwrap();
    assert_eq!(revealed.current().route(), &Route::Details);
    assert_eq!(
        revealed.current().restoration(),
        RouteHostRestorationIntent::Restore(key(2))
    );
}
