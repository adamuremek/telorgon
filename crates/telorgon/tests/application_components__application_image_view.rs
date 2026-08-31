use telorgon::application_components::prelude::{
    ImageView, ImageViewContent, ImageViewSemanticPolicy,
};
use telorgon::ui::ImageId;

#[test]
fn public_image_view_preserves_retained_identity_and_explicit_semantics() {
    let content = ImageViewContent::new(ImageId(73), 16);
    let decorative = ImageView::decorative(content);
    assert_eq!(
        decorative.semantic_policy(),
        ImageViewSemanticPolicy::Decorative
    );
    assert_eq!(decorative.content(), content);
    assert_eq!(decorative.accessible_description(), None);

    let described = ImageView::described(content, "A deployment success chart").unwrap();
    assert_eq!(
        described.semantic_policy(),
        ImageViewSemanticPolicy::Described
    );
    assert_eq!(
        described.accessible_description(),
        Some("A deployment success chart")
    );
    assert_eq!(described.content().content_version(), 16);
}
