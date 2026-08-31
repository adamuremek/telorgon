//! Opaque imported-accessibility attachment metadata for externally owned surfaces.
//!
//! The actual semantic tree belongs to the later neutral accessibility package. This module only
//! identifies a host-provided attachment, its namespace/revision, coordinate mapping, and focus.

use std::fmt;
use std::num::NonZeroU64;

use crate::shell::SurfaceId;

macro_rules! define_accessibility_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[repr(transparent)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            pub const fn new(value: NonZeroU64) -> Self {
                Self(value)
            }

            pub const fn from_raw(value: u64) -> Option<Self> {
                match NonZeroU64::new(value) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

define_accessibility_id!(
    AccessibilityAttachmentId,
    "Opaque identity of one host-provided semantic attachment."
);
define_accessibility_id!(
    AccessibilityAttachmentRevision,
    "Monotonic revision of one imported semantic attachment."
);
define_accessibility_id!(
    AccessibilityNamespaceId,
    "Stable namespace separating an imported tree from shell-authored nodes."
);
define_accessibility_id!(
    ImportedSemanticNodeId,
    "Stable node identity interpreted only within an imported namespace."
);

impl AccessibilityAttachmentRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);
}

impl fmt::Debug for AccessibilityAttachmentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AccessibilityAttachmentId(..)")
    }
}

impl fmt::Debug for AccessibilityAttachmentRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AccessibilityAttachmentRevision")
            .field(&self.get())
            .finish()
    }
}

impl fmt::Debug for AccessibilityNamespaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AccessibilityNamespaceId")
            .field(&self.get())
            .finish()
    }
}

impl fmt::Debug for ImportedSemanticNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ImportedSemanticNodeId")
            .field(&self.get())
            .finish()
    }
}

/// Full two-dimensional affine mapping from imported coordinates into shell logical coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportedSemanticTransform {
    coefficients: [f32; 6],
}

impl ImportedSemanticTransform {
    pub const IDENTITY: Self = Self {
        coefficients: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub fn new(coefficients: [f32; 6]) -> Result<Self, ImportedSemanticTransformError> {
        if !coefficients.iter().all(|value| value.is_finite()) {
            return Err(ImportedSemanticTransformError::NonFinite);
        }
        let determinant = coefficients[0] * coefficients[3] - coefficients[1] * coefficients[2];
        if !determinant.is_finite() || determinant == 0.0 {
            return Err(ImportedSemanticTransformError::NotInvertible);
        }
        Ok(Self { coefficients })
    }

    pub const fn coefficients(self) -> [f32; 6] {
        self.coefficients
    }

    pub fn determinant(self) -> f32 {
        self.coefficients[0] * self.coefficients[3] - self.coefficients[1] * self.coefficients[2]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportedSemanticTransformError {
    NonFinite,
    NotInvertible,
}

impl fmt::Display for ImportedSemanticTransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NonFinite => "imported semantic transform must be finite",
            Self::NotInvertible => "imported semantic transform must be invertible",
        })
    }
}

impl std::error::Error for ImportedSemanticTransformError {}

/// Distinct host-observed keyboard and assistive-technology focus within an imported tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImportedAccessibilityFocus {
    pub keyboard: Option<ImportedSemanticNodeId>,
    pub assistive_technology: Option<ImportedSemanticNodeId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImportedAccessibilityPrivacy {
    #[default]
    Ordinary,
    Redacted,
}

/// Host attachment metadata to be merged later by the accessibility tree owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportedAccessibilityAttachment {
    id: AccessibilityAttachmentId,
    revision: AccessibilityAttachmentRevision,
    surface: SurfaceId,
    namespace: AccessibilityNamespaceId,
    root: ImportedSemanticNodeId,
    transform: ImportedSemanticTransform,
    focus: ImportedAccessibilityFocus,
    privacy: ImportedAccessibilityPrivacy,
}

impl ImportedAccessibilityAttachment {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: AccessibilityAttachmentId,
        revision: AccessibilityAttachmentRevision,
        surface: SurfaceId,
        namespace: AccessibilityNamespaceId,
        root: ImportedSemanticNodeId,
        transform: ImportedSemanticTransform,
        focus: ImportedAccessibilityFocus,
        privacy: ImportedAccessibilityPrivacy,
    ) -> Self {
        Self {
            id,
            revision,
            surface,
            namespace,
            root,
            transform,
            focus,
            privacy,
        }
    }

    pub const fn id(self) -> AccessibilityAttachmentId {
        self.id
    }

    pub const fn revision(self) -> AccessibilityAttachmentRevision {
        self.revision
    }

    pub const fn surface(self) -> SurfaceId {
        self.surface
    }

    pub const fn namespace(self) -> AccessibilityNamespaceId {
        self.namespace
    }

    pub const fn root(self) -> ImportedSemanticNodeId {
        self.root
    }

    pub const fn transform(self) -> ImportedSemanticTransform {
        self.transform
    }

    pub const fn focus(self) -> ImportedAccessibilityFocus {
        self.focus
    }

    pub const fn privacy(self) -> ImportedAccessibilityPrivacy {
        self.privacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_preserves_namespaced_focus_and_coordinate_ownership() {
        let keyboard = ImportedSemanticNodeId::from_raw(8).unwrap();
        let attachment = ImportedAccessibilityAttachment::new(
            AccessibilityAttachmentId::from_raw(1).unwrap(),
            AccessibilityAttachmentRevision::from_raw(2).unwrap(),
            SurfaceId::from_raw(3).unwrap(),
            AccessibilityNamespaceId::from_raw(4).unwrap(),
            ImportedSemanticNodeId::from_raw(5).unwrap(),
            ImportedSemanticTransform::new([2.0, 0.0, 0.0, 2.0, 10.0, 20.0]).unwrap(),
            ImportedAccessibilityFocus {
                keyboard: Some(keyboard),
                assistive_technology: None,
            },
            ImportedAccessibilityPrivacy::Redacted,
        );

        assert_eq!(attachment.surface().get(), 3);
        assert_eq!(attachment.namespace().get(), 4);
        assert_eq!(attachment.focus().keyboard, Some(keyboard));
        assert_eq!(attachment.transform().determinant(), 4.0);
        assert_eq!(attachment.privacy(), ImportedAccessibilityPrivacy::Redacted);
    }

    #[test]
    fn invalid_coordinate_mappings_are_rejected() {
        assert_eq!(
            ImportedSemanticTransform::new([1.0, 0.0, 0.0, f32::NAN, 0.0, 0.0]),
            Err(ImportedSemanticTransformError::NonFinite)
        );
        assert_eq!(
            ImportedSemanticTransform::new([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]),
            Err(ImportedSemanticTransformError::NotInvertible)
        );
    }
}
