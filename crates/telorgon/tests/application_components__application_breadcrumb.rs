use telorgon::application_components::{
    Breadcrumb, BreadcrumbError, BreadcrumbItem, ChangeSource, NavigationController,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Projects,
    Document(u64),
}

#[test]
fn public_breadcrumb_validates_and_selects_only_controller_ancestors() {
    let mut navigation = NavigationController::new(Route::Home, None);
    navigation
        .push(Route::Projects, None, ChangeSource::Programmatic)
        .unwrap();
    navigation
        .push(Route::Document(7), None, ChangeSource::Programmatic)
        .unwrap();
    let breadcrumb = Breadcrumb::new(
        "Document location",
        [
            BreadcrumbItem::new(Route::Home, "Home").unwrap(),
            BreadcrumbItem::new(Route::Projects, "Projects").unwrap(),
            BreadcrumbItem::new(Route::Document(7), "Document 7").unwrap(),
        ],
    )
    .unwrap();
    breadcrumb.validate(&navigation).unwrap();

    let request = breadcrumb
        .request_ancestor(&navigation, &Route::Projects, ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(request.route(), &Route::Projects);
    assert_eq!(request.source(), ChangeSource::Accessibility);
    assert_eq!(navigation.current(), &Route::Document(7));

    let navigation_request = navigation
        .request_selection(*request.route(), request.source())
        .unwrap();
    let selected = navigation.select(navigation_request).unwrap();
    assert_eq!(selected.current(), &Route::Projects);
    assert_eq!(selected.source(), ChangeSource::Accessibility);
    assert_eq!(selected.removed()[0].route(), &Route::Document(7));

    assert!(matches!(
        breadcrumb.request_ancestor(
            &NavigationController::new(Route::Home, None),
            &Route::Home,
            ChangeSource::Keyboard,
        ),
        Err(BreadcrumbError::TrailLengthMismatch { .. })
    ));
}
