use telorgon::application_components::{ListView, ListViewItem};

fn item(key: &'static str, label: &str) -> ListViewItem<&'static str> {
    ListViewItem::new(key, label).unwrap()
}

#[test]
fn public_list_view_reports_stable_key_snapshot_work_without_owning_row_data() {
    let mut list = ListView::new(
        "Recent documents",
        [
            item("alpha", "Alpha"),
            item("beta", "Beta"),
            item("charlie", "Charlie"),
        ],
    )
    .unwrap();

    let update = list
        .update_items([
            item("charlie", "Charlie renamed"),
            item("beta", "Beta"),
            item("delta", "Delta"),
        ])
        .unwrap();
    assert_eq!(update.inserted(), &["delta"]);
    assert_eq!(update.removed(), &["alpha"]);
    assert_eq!(update.updated(), &["charlie"]);
    assert_eq!(update.reused(), 2);
    assert_eq!(list.items()[0].key(), &"charlie");
    assert_eq!(list.items()[0].label(), "Charlie renamed");
    assert_eq!(list.diagnostics().updates, 1);
}
