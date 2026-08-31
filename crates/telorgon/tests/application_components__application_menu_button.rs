use telorgon::application_components::prelude::{
    MenuButton, MenuButtonError, MenuButtonOpenRequest, MenuOpeningFocus,
};
use telorgon::input::{ChangeSource, CompositeItem};
use telorgon::ui::{OverlayAnchor, UiNodeId};

#[test]
fn public_menu_button_builds_an_unapplied_source_preserving_open_request() {
    let button = MenuButton::new(
        "Actions",
        [
            CompositeItem {
                key: 1_u8,
                enabled: true,
            },
            CompositeItem {
                key: 2,
                enabled: false,
            },
        ],
    )
    .unwrap()
    .selected(1)
    .unwrap()
    .opening_focus(MenuOpeningFocus::None);
    let anchor = UiNodeId::new(8, 1);
    let request: MenuButtonOpenRequest<u8> = button.open_request(anchor, ChangeSource::Keyboard);

    assert_eq!(request.source(), ChangeSource::Keyboard);
    assert_eq!(request.menu().anchor, OverlayAnchor::Node(anchor));
    assert_eq!(request.menu().selected, Some(1));
    assert_eq!(request.menu().opening_focus, MenuOpeningFocus::None);
    assert_eq!(button.items().len(), 2);
    assert_eq!(
        MenuButton::<u8>::new("Empty", []),
        Err(MenuButtonError::EmptyItems)
    );
}
