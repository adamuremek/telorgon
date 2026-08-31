use telorgon::application_components::{
    SelectionFollowsFocus, SelectionMode, SelectionModel, TreeHierarchy, TreeItem, TreeView,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

#[test]
fn public_tree_keeps_expansion_selection_and_active_item_controlled_and_distinct() {
    let hierarchy = TreeHierarchy::new(
        [
            TreeItem::new("root", "Root", None).unwrap(),
            TreeItem::new("child", "Child", Some("root")).unwrap(),
        ],
        [],
    )
    .unwrap();
    let selection = SelectionModel::new(
        SelectionMode::Single,
        SelectionFollowsFocus::Enabled,
        ["root", "child"],
        ["root"],
        Some("root"),
    )
    .unwrap();
    let mut tree = TreeView::new("Files", hierarchy, selection).unwrap();

    let open = tree
        .navigate(
            CompositeNavigationCommand::Right,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(open.expansion().unwrap().key(), &"root");
    assert!(!tree.hierarchy().is_expanded(&"root"));
    tree.apply_expansion(open.into_expansion().unwrap())
        .unwrap();
    let descend = tree
        .navigate(
            CompositeNavigationCommand::Right,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(tree.active_item(), Some("child"));
    assert_eq!(descend.selection().unwrap().selected(), &["child"]);
    assert_eq!(tree.selection().selected(), &["root"]);
}
