use telorgon::application_components::{
    ChangeSource, ListBox, ListBoxOption, SelectionFollowsFocus, SelectionMode, SelectionModel,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

fn option(key: u8, enabled: bool) -> ListBoxOption<u8> {
    ListBoxOption::new(key, format!("Option {key}"))
        .unwrap()
        .enabled(enabled)
}

#[test]
fn public_listbox_keeps_focus_and_selection_separate_over_disabled_options() {
    let selection = SelectionModel::new(
        SelectionMode::Multiple,
        SelectionFollowsFocus::Enabled,
        [1_u8, 2, 3],
        [1],
        Some(1),
    )
    .unwrap();
    let mut listbox = ListBox::new(
        "Options",
        [option(1, true), option(2, false), option(3, true)],
        selection,
    )
    .unwrap();

    let transition = listbox
        .navigate(
            CompositeNavigationCommand::Down,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    let proposal = transition.selection().unwrap();
    assert_eq!(listbox.active_descendant(), Some(3));
    assert_eq!(proposal.selected(), &[1, 3]);
    assert_eq!(proposal.source(), ChangeSource::Directional);
    assert_eq!(listbox.selection().selected(), &[1]);
}
