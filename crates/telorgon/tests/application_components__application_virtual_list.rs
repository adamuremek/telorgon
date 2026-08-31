use telorgon::application_components::{
    ListViewItem, VirtualListPolicy, VirtualListTotal, VirtualListView, VirtualListViewport,
};

fn item(key: &'static str) -> ListViewItem<&'static str> {
    ListViewItem::new(key, key.to_uppercase()).unwrap()
}

#[test]
fn public_virtual_list_exposes_bounded_keyed_materialization_without_scroll_ownership() {
    let list = VirtualListView::new(
        "Results",
        ["alpha", "beta", "charlie", "delta", "echo"].map(item),
        VirtualListTotal::Known(5),
        VirtualListPolicy::new(40.0, 80.0, 2).unwrap(),
    )
    .unwrap();
    let plan = list.plan(VirtualListViewport::new(80.0, 40.0).unwrap());

    assert!(plan.visible_keys().contains(&"charlie"));
    assert!(plan.cached_keys().len() <= 2);
    assert_eq!(plan.total(), VirtualListTotal::Known(5));
    assert_eq!(list.items().len(), 5);
}
