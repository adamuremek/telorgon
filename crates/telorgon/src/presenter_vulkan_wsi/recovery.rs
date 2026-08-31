pub use crate::presentation::{
    PresentationRecovery as PresenterRecovery, PresentationState as PresenterState,
};

pub(crate) use crate::presentation::is_zero_extent as is_zero;

#[cfg(test)]
mod tests {
    use crate::core::SizeI;

    use super::*;

    #[test]
    fn zero_extent_suspends_and_nonzero_extent_requests_reconfigure() {
        let mut recovery = PresenterRecovery::new(SizeI {
            width: 0,
            height: 9,
        });
        assert_eq!(recovery.state(), PresenterState::Suspended);
        assert!(recovery.resize(SizeI {
            width: 640,
            height: 480,
        }));
        assert_eq!(recovery.state(), PresenterState::NeedsReconfigure);
        assert!(!recovery.resize(SizeI {
            width: 640,
            height: 480,
        }));
        assert!(recovery.resize(SizeI {
            width: 640,
            height: 0,
        }));
        assert_eq!(recovery.state(), PresenterState::Suspended);
        assert!(!recovery.resize(SizeI {
            width: 640,
            height: 0,
        }));
    }
}
