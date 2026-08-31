use telorgon::application_components::{
    ChangeSource, NavigationController, NavigationError, NavigationRestorationKey,
    NavigationTransitionKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Library,
    Document(u64),
}

fn restoration(value: u64) -> NavigationRestorationKey {
    NavigationRestorationKey::from_raw(value).expect("fixture keys are nonzero")
}

#[test]
fn public_controller_owns_one_atomic_source_preserving_route_stack() {
    let mut navigation = NavigationController::new(Route::Home, Some(restoration(1)));
    navigation
        .push(Route::Library, Some(restoration(2)), ChangeSource::Pointer)
        .unwrap();
    navigation
        .push(
            Route::Document(7),
            Some(restoration(3)),
            ChangeSource::Keyboard,
        )
        .unwrap();

    let revision = navigation.revision();
    let request = navigation
        .request_selection(Route::Library, ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(navigation.current(), &Route::Document(7));
    assert_eq!(navigation.revision(), revision);

    let selected = navigation.select(request).unwrap();
    assert_eq!(selected.kind(), NavigationTransitionKind::Select);
    assert_eq!(selected.source(), ChangeSource::Accessibility);
    assert_eq!(selected.current(), &Route::Library);
    assert_eq!(selected.restoration_key(), Some(restoration(2)));
    assert_eq!(
        selected
            .removed()
            .iter()
            .map(|entry| *entry.route())
            .collect::<Vec<_>>(),
        vec![Route::Document(7)]
    );

    let revision = navigation.revision();
    assert_eq!(
        navigation.push(
            Route::Home,
            Some(restoration(4)),
            ChangeSource::Programmatic,
        ),
        Err(NavigationError::DuplicateRoute(Route::Home))
    );
    assert_eq!(navigation.revision(), revision);
    assert_eq!(navigation.current(), &Route::Library);

    let popped = navigation.pop(ChangeSource::Keyboard).unwrap();
    assert_eq!(popped.kind(), NavigationTransitionKind::Pop);
    assert_eq!(popped.source(), ChangeSource::Keyboard);
    assert_eq!(popped.current(), &Route::Home);
    assert_eq!(popped.restoration_key(), Some(restoration(1)));
}
