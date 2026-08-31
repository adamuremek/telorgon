use std::num::NonZeroU32;

use telorgon::platform::capability::{
    CapabilityDescriptor, CapabilityLimit, ExecutionRequirement, PermissionState, Support,
    UnavailableReason, UserGestureRequirement,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowOperation {
    RequestAttention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowLimits {
    maximum_title_bytes: CapabilityLimit<NonZeroU32>,
}

#[test]
fn public_capability_path_keeps_availability_permission_and_requirements_separate() {
    let descriptor = CapabilityDescriptor::new(
        WindowOperation::RequestAttention,
        WindowLimits {
            maximum_title_bytes: CapabilityLimit::Bounded(NonZeroU32::new(512).unwrap()),
        },
        PermissionState::Denied,
        ExecutionRequirement::HostEventLoop,
        UserGestureRequirement::RecentRequired,
    );
    let support = Support::Available(descriptor);

    assert!(support.is_available());
    assert!(descriptor.permission().blocks_use());
    assert_eq!(descriptor.execution(), ExecutionRequirement::HostEventLoop);
    assert!(descriptor.user_gesture().is_required());
    assert_eq!(
        descriptor
            .limits()
            .maximum_title_bytes
            .into_bound()
            .unwrap()
            .get(),
        512
    );

    let unavailable: telorgon::platform::Support<WindowOperation> =
        Support::Unavailable(UnavailableReason::AdapterNotCompiled);
    assert_eq!(
        unavailable.unavailable_reason(),
        Some(UnavailableReason::AdapterNotCompiled)
    );
}
