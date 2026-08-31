//! Structured, payload-free shell boundary errors.

use std::fmt;

use crate::shell::{ShellRequestResult, ShellSnapshotError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShellErrorKind {
    InvalidSnapshot,
    RequestDenied,
    RequestStale,
    RequestUnsupported,
    HostUnavailable,
    CapacityExceeded,
    InvariantViolation,
}

/// Redaction-safe shell boundary failure with static diagnostic context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellError {
    kind: ShellErrorKind,
    context: &'static str,
}

impl ShellError {
    pub const fn new(kind: ShellErrorKind, context: &'static str) -> Self {
        Self { kind, context }
    }

    pub const fn kind(self) -> ShellErrorKind {
        self.kind
    }

    pub const fn context(self) -> &'static str {
        self.context
    }

    pub const fn from_rejection(result: ShellRequestResult, context: &'static str) -> Option<Self> {
        let kind = match result {
            ShellRequestResult::Accepted(_) => return None,
            ShellRequestResult::Denied => ShellErrorKind::RequestDenied,
            ShellRequestResult::Stale => ShellErrorKind::RequestStale,
            ShellRequestResult::Unsupported => ShellErrorKind::RequestUnsupported,
        };
        Some(Self::new(kind, context))
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.context, self.kind)
    }
}

impl std::error::Error for ShellError {}

impl From<ShellSnapshotError> for ShellError {
    fn from(_: ShellSnapshotError) -> Self {
        Self::new(
            ShellErrorKind::InvalidSnapshot,
            "invalid shell host snapshot",
        )
    }
}

pub type ShellResult<T> = Result<T, ShellError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejections_map_without_display_string_branching() {
        assert_eq!(
            ShellError::from_rejection(ShellRequestResult::Stale, "surface request")
                .unwrap()
                .kind(),
            ShellErrorKind::RequestStale
        );
        assert_eq!(
            ShellError::from_rejection(
                ShellRequestResult::accepted(crate::shell::AcceptedRequestId::MIN),
                "accepted",
            ),
            None
        );
    }
}
