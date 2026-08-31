use telorgon::application_components::{
    FieldMetadata, FieldValidation, Form, FormSubmission, ValidationResult,
};
use telorgon::layout::RevealAlignment;

#[test]
fn public_form_submission_uses_canonical_order_and_returns_unapplied_intents() {
    let mut form = Form::new(
        [
            FieldMetadata::new("account", "Account").unwrap(),
            FieldMetadata::new("email", "Email").unwrap(),
        ],
        [
            FieldValidation::new(
                "email",
                ValidationResult::invalid("Enter an email address").unwrap(),
            ),
            FieldValidation::new("account", ValidationResult::Valid),
        ],
    )
    .unwrap();

    assert_eq!(form.order(), ["account", "email"]);
    let FormSubmission::Invalid(invalid) = form.submit() else {
        panic!("invalid controlled input must reject submission");
    };
    assert_eq!(invalid.canonical_index(), 1);
    assert_eq!(invalid.focus().field(), &"email");
    assert_eq!(invalid.reveal().field(), &"email");
    assert_eq!(invalid.reveal().alignment(), RevealAlignment::Nearest);
}
