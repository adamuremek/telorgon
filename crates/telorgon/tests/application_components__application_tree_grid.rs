use telorgon::application_components::{
    DataGrid, DataGridCell, SelectionFollowsFocus, SelectionMode, SelectionModel, Table, TableCell,
    TableColumn, TableRow, TreeGrid, TreeHierarchy, TreeItem,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

#[test]
fn public_tree_grid_reuses_the_grid_cell_owner_for_hierarchical_rows() {
    let hierarchy = TreeHierarchy::new(
        [
            TreeItem::new("root", "Root", None).unwrap(),
            TreeItem::new("child", "Child", Some("root")).unwrap(),
        ],
        ["root"],
    )
    .unwrap();
    let table = Table::new(
        "Files",
        [TableColumn::new("name", "Name").unwrap()],
        [
            TableRow::new("root", "Root", [TableCell::new("name", "Root")]).unwrap(),
            TableRow::new("child", "Child", [TableCell::new("name", "Child")]).unwrap(),
        ],
    )
    .unwrap();
    let cells = DataGrid::cells(&table);
    let selection = SelectionModel::new(
        SelectionMode::Single,
        SelectionFollowsFocus::Enabled,
        cells,
        [DataGridCell::new("root", "name")],
        None,
    )
    .unwrap();
    let grid = DataGrid::new(table, selection).unwrap();
    let mut tree_grid = TreeGrid::new(hierarchy, grid, "name").unwrap();

    let descend = tree_grid
        .navigate(
            CompositeNavigationCommand::Right,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(descend.current(), Some(DataGridCell::new("child", "name")));
    assert_eq!(tree_grid.grid().selection().selected().len(), 1);
}
