//! Immediate admission results for shell-policy requests.

use std::num::NonZeroU64;

/// Opaque host identity assigned to an admitted shell request.
///
/// This identity correlates later platform completion once Gate 9 supplies that channel. Its
/// presence does not mean that the requested state change has completed or may be reflected
/// optimistically in a shell snapshot.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AcceptedRequestId(NonZeroU64);

impl AcceptedRequestId {
    pub const MIN: Self = Self(NonZeroU64::MIN);

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

/// Immediate host admission result for a shell request.
///
/// `Accepted` means only that the host admitted the request. Observable state continues to come
/// from later host snapshots, and Gate 9 owns any eventual terminal platform outcome.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShellRequestResult {
    Accepted(AcceptedRequestId),
    Denied,
    Stale,
    Unsupported,
}

impl ShellRequestResult {
    pub const fn accepted(id: AcceptedRequestId) -> Self {
        Self::Accepted(id)
    }

    pub const fn accepted_id(self) -> Option<AcceptedRequestId> {
        match self {
            Self::Accepted(id) => Some(id),
            Self::Denied | Self::Stale | Self::Unsupported => None,
        }
    }

    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_identity_is_nonzero_and_retained() {
        assert_eq!(AcceptedRequestId::from_raw(0), None);
        let id = AcceptedRequestId::from_raw(95).unwrap();
        let result = ShellRequestResult::accepted(id);

        assert!(result.is_accepted());
        assert_eq!(result.accepted_id(), Some(id));
        assert_eq!(id.get(), 95);
    }

    #[test]
    fn immediate_rejections_never_fabricate_an_accepted_identity() {
        for result in [
            ShellRequestResult::Denied,
            ShellRequestResult::Stale,
            ShellRequestResult::Unsupported,
        ] {
            assert!(!result.is_accepted());
            assert_eq!(result.accepted_id(), None);
        }
    }
}
