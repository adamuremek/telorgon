use telorgon::application_components::{Table, TableCell, TableColumn, TableRow};

#[test]
fn public_table_addresses_rectangular_presentation_cells_by_stable_row_and_column_keys() {
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

    assert_eq!(table.cell(&"jobs", &"status").unwrap().text(), "Paused");
    assert_eq!(table.row(&"api").unwrap().label(), "API");
    assert_eq!(table.column(&"name").unwrap().label(), "Name");
}
