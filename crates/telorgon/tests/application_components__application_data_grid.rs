use telorgon::application_components::{
    DataGrid, DataGridCell, SelectionFollowsFocus, SelectionMode, SelectionModel, Table, TableCell,
    TableColumn, TableRow,
};
use telorgon::input::{CompositeNavigationCommand, WritingDirection};

#[test]
fn public_data_grid_keeps_active_cell_and_controlled_selection_distinct() {
    let table = Table::new(
        "Services",
        [
            TableColumn::new("name", "Name").unwrap(),
            TableColumn::new("status", "Status").unwrap(),
        ],
        [
            TableRow::new(
                "api",
                "API",
                [
                    TableCell::new("name", "Gateway"),
                    TableCell::new("status", "Ready"),
                ],
            )
            .unwrap(),
            TableRow::new(
                "jobs",
                "Jobs",
                [
                    TableCell::new("name", "Worker"),
                    TableCell::new("status", "Paused"),
                ],
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let cells = DataGrid::cells(&table);
    let first = DataGridCell::new("api", "name");
    let selection = SelectionModel::new(
        SelectionMode::Multiple,
        SelectionFollowsFocus::Enabled,
        cells,
        [first],
        Some(first),
    )
    .unwrap();
    let mut grid = DataGrid::new(table, selection).unwrap();

    let moved = grid
        .navigate(
            CompositeNavigationCommand::Down,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(moved.current(), Some(DataGridCell::new("jobs", "name")));
    assert_eq!(moved.selection().unwrap().selected().len(), 2);
    assert_eq!(grid.selection().selected(), &[first]);
}
