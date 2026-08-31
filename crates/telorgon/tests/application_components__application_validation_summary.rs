use telorgon::application_components::{
    FieldMetadata, FieldValidation, Form, ValidationKind, ValidationResult, ValidationSummary,
};
use telorgon::input::ChangeSource;
use telorgon::layout::RevealAlignment;

#[test]
fn public_validation_summary_derives_order_and_returns_source_preserving_attention() {
    let form = Form::new(
        [
            FieldMetadata::new("name", "Name").unwrap(),
            FieldMetadata::new("email", "Email").unwrap(),
        ],
        [
            FieldValidation::new(
                "email",
                ValidationResult::invalid("Enter an email").unwrap(),
            ),
            FieldValidation::new(
                "name",
                ValidationResult::warning("Check the display name").unwrap(),
            ),
        ],
    )
    .unwrap();
    let summary = ValidationSummary::new("Review fields", &form).unwrap();

    assert_eq!(summary.entries()[0].field(), &"name");
    assert_eq!(summary.entries()[1].field(), &"email");
    let action = summary
        .activate(&"email", ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(action.kind(), ValidationKind::Invalid);
    assert_eq!(action.source(), ChangeSource::Accessibility);
    assert_eq!(action.focus().field(), &"email");
    assert_eq!(action.reveal().alignment(), RevealAlignment::Nearest);
}
