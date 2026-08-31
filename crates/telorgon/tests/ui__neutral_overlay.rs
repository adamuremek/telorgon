use telorgon::ui::{
    BoxStyle, DismissReason, LayoutStyle, MountWriter, MountedUi, OutsidePressPolicy,
    OverlayDismissResult, OverlayFocusLifecycle, OverlayFocusRestoration, OverlayHost,
    OverlayInitialFocus, OverlayOpenRequest,
};

#[test]
fn public_overlay_path_closes_before_consuming_an_outside_press() {
    let mut ui = MountedUi::default();
    let root =
        MountWriter::<()>::new(&mut ui).root(BoxStyle::default(), LayoutStyle::default(), |_| {});
    let mut request = OverlayOpenRequest::anchored(root.0);
    request.dismissal.outside_press = OutsidePressPolicy::DismissAndConsume;
    request.focus = OverlayFocusLifecycle {
        initial: OverlayInitialFocus::FirstFocusable,
        restoration: OverlayFocusRestoration::TargetThenNearest(root.0),
        ..OverlayFocusLifecycle::default()
    };

    let mut host = OverlayHost::default();
    let opened = host.open(&ui, request).unwrap();
    let OverlayDismissResult::Dismissed(closed) = host
        .dismiss(opened.id, DismissReason::OutsidePress)
        .unwrap()
    else {
        panic!("outside press should close the configured overlay");
    };
    assert!(closed.consume_input);
    assert_eq!(closed.dismissed[0].id, opened.id);
    assert!(host.entries().is_empty());
}
