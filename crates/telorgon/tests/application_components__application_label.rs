use telorgon::application_components::prelude::{
    Label, LabelContent, LabelStyle, LabelTextStyle, LabelTextStyleError,
};
use telorgon::core::ColorRgba8;

#[test]
fn public_label_preserves_visible_content_revision_and_validated_style() {
    let content = LabelContent::new("Project status", 18).unwrap();
    let text = LabelTextStyle::new(
        ColorRgba8::rgba(20, 30, 40, 255),
        16.0,
        21.0,
        "application-ui",
        500,
    )
    .unwrap();
    let label = Label::from_content(content).style(LabelStyle {
        text: text.clone(),
        ..LabelStyle::default()
    });

    assert_eq!(label.content().text(), "Project status");
    assert_eq!(label.content().revision(), 18);
    assert_eq!(text.family(), "application-ui");
    assert_eq!(text.weight(), 500);
    assert_eq!(
        LabelTextStyle::new(
            ColorRgba8::rgba(20, 30, 40, 255),
            16.0,
            21.0,
            "application-ui",
            0,
        ),
        Err(LabelTextStyleError::InvalidWeight)
    );
}
