use telorgon::application_components::prelude::{
    Separator, SeparatorGeometry, SeparatorOrientation, SeparatorSemanticPolicy,
};

#[test]
fn public_separator_keeps_decorative_and_meaningful_policy_explicit() {
    let geometry = SeparatorGeometry::new(160.0, 2.0).unwrap();
    let decorative = Separator::decorative(SeparatorOrientation::Horizontal, geometry);
    assert_eq!(
        decorative.semantic_policy(),
        SeparatorSemanticPolicy::Decorative
    );
    assert_eq!(decorative.accessible_name(), None);

    let named = Separator::named(
        "Navigation and content",
        SeparatorOrientation::Vertical,
        geometry,
    )
    .unwrap();
    assert_eq!(named.semantic_policy(), SeparatorSemanticPolicy::Named);
    assert_eq!(named.accessible_name(), Some("Navigation and content"));
    assert_eq!(named.geometry(), geometry);
}
