use telorgon::shell::{
    AccessibilityAttachmentId, AccessibilityAttachmentRevision, AccessibilityNamespaceId,
    ImportedAccessibilityAttachment, ImportedAccessibilityFocus, ImportedAccessibilityPrivacy,
    ImportedSemanticNodeId, ImportedSemanticTransform, ImportedSemanticTransformError, SurfaceId,
};

#[test]
fn public_accessibility_attachment_is_opaque_namespaced_and_transform_validated() {
    let transform = ImportedSemanticTransform::new([1.0, 0.0, 0.0, 1.0, 40.0, 20.0]).unwrap();
    let attachment = ImportedAccessibilityAttachment::new(
        AccessibilityAttachmentId::from_raw(1).unwrap(),
        AccessibilityAttachmentRevision::from_raw(2).unwrap(),
        SurfaceId::from_raw(3).unwrap(),
        AccessibilityNamespaceId::from_raw(4).unwrap(),
        ImportedSemanticNodeId::from_raw(5).unwrap(),
        transform,
        ImportedAccessibilityFocus::default(),
        ImportedAccessibilityPrivacy::Ordinary,
    );

    assert_eq!(attachment.surface().get(), 3);
    assert_eq!(attachment.transform(), transform);
    assert_eq!(
        ImportedSemanticTransform::new([0.0; 6]),
        Err(ImportedSemanticTransformError::NotInvertible)
    );
}
