//! Typed output-policy intentions emitted by shell UI.

use std::fmt;
use std::num::NonZeroU64;

use crate::shell::{OutputId, OutputRevision, ShellCapabilities};

macro_rules! define_output_request_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

define_output_request_id!(
    ReservedAreaId,
    "Stable shell identity of one output reserved-area proposal."
);
define_output_request_id!(
    OutputAppearanceActionId,
    "Opaque host-supplied output appearance action identity."
);
define_output_request_id!(
    OutputModeActionId,
    "Opaque host-supplied output mode action identity."
);

/// Logical output edge from which a reserved-area extent is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Finite positive logical extent of an output reservation proposal.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReservedAreaExtent(f32);

impl ReservedAreaExtent {
    pub fn new(value: f32) -> Result<Self, ReservedAreaExtentError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(ReservedAreaExtentError::InvalidExtent);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservedAreaExtentError {
    InvalidExtent,
}

impl fmt::Display for ReservedAreaExtentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("reserved-area extent must be finite and positive")
    }
}

impl std::error::Error for ReservedAreaExtentError {}

/// A request against one observed output revision.
///
/// Reservations are proposals: only a later output snapshot decides usable geometry. Appearance
/// and mode values remain host-defined opaque actions rather than portable code inventing display
/// modes or color policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputRequest {
    ProposeReservedArea {
        output: OutputId,
        revision: OutputRevision,
        reservation: ReservedAreaId,
        edge: OutputEdge,
        extent: ReservedAreaExtent,
    },
    ReleaseReservedArea {
        output: OutputId,
        revision: OutputRevision,
        reservation: ReservedAreaId,
    },
    InvokeAppearanceAction {
        output: OutputId,
        revision: OutputRevision,
        action: OutputAppearanceActionId,
    },
    InvokeModeAction {
        output: OutputId,
        revision: OutputRevision,
        action: OutputModeActionId,
    },
}

impl OutputRequest {
    pub const fn output(self) -> OutputId {
        match self {
            Self::ProposeReservedArea { output, .. }
            | Self::ReleaseReservedArea { output, .. }
            | Self::InvokeAppearanceAction { output, .. }
            | Self::InvokeModeAction { output, .. } => output,
        }
    }

    pub const fn revision(self) -> OutputRevision {
        match self {
            Self::ProposeReservedArea { revision, .. }
            | Self::ReleaseReservedArea { revision, .. }
            | Self::InvokeAppearanceAction { revision, .. }
            | Self::InvokeModeAction { revision, .. } => revision,
        }
    }

    pub const fn required_capability(self) -> ShellCapabilities {
        match self {
            Self::ProposeReservedArea { .. } | Self::ReleaseReservedArea { .. } => {
                ShellCapabilities::RESERVE_OUTPUT_AREA
            }
            Self::InvokeAppearanceAction { .. } | Self::InvokeModeAction { .. } => {
                ShellCapabilities::CONFIGURE_OUTPUT
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> OutputId {
        OutputId::from_raw(4).unwrap()
    }

    fn revision() -> OutputRevision {
        OutputRevision::from_raw(9).unwrap()
    }

    #[test]
    fn reserved_area_extent_rejects_nonfinite_and_nonpositive_values() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert_eq!(
                ReservedAreaExtent::new(value),
                Err(ReservedAreaExtentError::InvalidExtent)
            );
        }
        assert_eq!(ReservedAreaExtent::new(32.0).unwrap().get(), 32.0);
    }

    #[test]
    fn reservation_is_revisioned_and_maps_to_reservation_authority() {
        let reservation = ReservedAreaId::from_raw(3).unwrap();
        let request = OutputRequest::ProposeReservedArea {
            output: output(),
            revision: revision(),
            reservation,
            edge: OutputEdge::Top,
            extent: ReservedAreaExtent::new(28.0).unwrap(),
        };

        assert_eq!(request.output(), output());
        assert_eq!(request.revision(), revision());
        assert_eq!(
            request.required_capability(),
            ShellCapabilities::RESERVE_OUTPUT_AREA
        );
    }

    #[test]
    fn host_actions_remain_typed_and_require_configuration_authority() {
        let appearance = OutputRequest::InvokeAppearanceAction {
            output: output(),
            revision: revision(),
            action: OutputAppearanceActionId::from_raw(1).unwrap(),
        };
        let mode = OutputRequest::InvokeModeAction {
            output: output(),
            revision: revision(),
            action: OutputModeActionId::from_raw(2).unwrap(),
        };

        assert_eq!(
            appearance.required_capability(),
            ShellCapabilities::CONFIGURE_OUTPUT
        );
        assert_eq!(
            mode.required_capability(),
            ShellCapabilities::CONFIGURE_OUTPUT
        );
    }
}
