use telorgon::application_components::{
    FieldMetadata, FieldMetadataError, FieldSemanticSupport, FieldValidation, ValidationKind,
    ValidationResult,
};
use telorgon::ui::{
    SemanticName, SemanticNode, SemanticRelationshipKind, SemanticRole, StringId, UiNodeId,
};

#[test]
fn public_form_field_metadata_associates_typed_validation_with_stable_identity() {
    let metadata = FieldMetadata::new("email", "Email address")
        .unwrap()
        .help("Used for account recovery")
        .unwrap()
        .required(true);
    let validation = FieldValidation::new(
        "email",
        ValidationResult::invalid("Enter a complete email address").unwrap(),
    );
    let help = UiNodeId::new(2, 1);
    let error = UiNodeId::new(3, 1);
    let semantic = metadata
        .decorate_semantics(
            SemanticNode::new(SemanticRole::TextInput),
            StringId(1),
            &validation,
            FieldSemanticSupport::new(Some(help), Some(error)),
        )
        .unwrap();

    assert_eq!(metadata.key(), &"email");
    assert_eq!(validation.result().kind(), ValidationKind::Invalid);
    assert_eq!(semantic.name, SemanticName::Text(StringId(1)));
    assert!(semantic.state.required);
    assert!(semantic.state.invalid);
    assert!(semantic.relationships.iter().any(|relationship| {
        relationship.kind == SemanticRelationshipKind::Help && relationship.target == help
    }));
    assert!(semantic.relationships.iter().any(|relationship| {
        relationship.kind == SemanticRelationshipKind::ErrorMessage && relationship.target == error
    }));

    let wrong_field = FieldValidation::new("password", ValidationResult::Valid);
    assert!(matches!(
        metadata.decorate_semantics(
            SemanticNode::new(SemanticRole::TextInput),
            StringId(1),
            &wrong_field,
            FieldSemanticSupport::new(Some(help), None),
        ),
        Err(FieldMetadataError::ValidationFieldMismatch)
    ));
}
