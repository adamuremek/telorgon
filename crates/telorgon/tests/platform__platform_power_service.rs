use std::any::Any;
use std::cell::Cell;
use std::num::NonZeroU16;
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, MAX_POWER_INHIBITION_LEASES,
    PermissionState, PowerAdmissionError, PowerCapability, PowerCapabilityQuery,
    PowerInhibitionAdmission, PowerInhibitionKind, PowerInhibitionLease,
    PowerInhibitionLeaseHandle, PowerInhibitionLeaseId, PowerInhibitionLeaseStatus,
    PowerInhibitionReason, PowerInhibitionRequest, PowerInhibitionRevocation, PowerInhibitionScope,
    PowerLimitError, PowerLimits, PowerOperations, PowerPolicyState, PowerService, PowerServiceKey,
    RequestId, RequestOutcome, ServiceLookup, ServiceRegistry, Support, UnavailableReason,
    UserGestureGrant, UserGestureRequirement, ViewId,
};

fn capability(
    operations: PowerOperations,
    policy: PowerPolicyState,
    gesture: UserGestureRequirement,
    maximum: u16,
) -> PowerCapability {
    PowerCapability::new(
        CapabilityDescriptor::new(
            operations,
            PowerLimits::new(NonZeroU16::new(maximum).unwrap()).unwrap(),
            PermissionState::Granted,
            ExecutionRequirement::HostEventLoop,
            gesture,
        ),
        policy,
    )
}

#[test]
fn operations_policy_and_capacity_keep_kind_and_scope_support_explicit() {
    let view = ViewId::from_raw(4, 2).unwrap();
    let operations = PowerOperations::new(true, false, true, true);
    assert!(operations.supports_idle_inhibition());
    assert!(!operations.supports_system_sleep_inhibition());
    assert!(operations.supports_application_scope());
    assert!(operations.supports_view_scope());
    assert!(operations.supports_kind(PowerInhibitionKind::Idle));
    assert!(!operations.supports_kind(PowerInhibitionKind::SystemSleep));
    assert!(operations.supports_scope(PowerInhibitionScope::Application));
    assert!(operations.supports_scope(PowerInhibitionScope::View(view)));

    assert_eq!(PowerLimits::default().maximum_concurrent_leases().get(), 64);
    assert_eq!(
        PowerLimits::new(NonZeroU16::new(MAX_POWER_INHIBITION_LEASES + 1).unwrap()),
        Err(PowerLimitError::LeaseLimitTooLarge)
    );
    let capability = capability(
        operations,
        PowerPolicyState::Allowed,
        UserGestureRequirement::NotRequired,
        3,
    );
    assert_eq!(capability.policy(), PowerPolicyState::Allowed);
    assert!(capability.policy().allows_inhibition());
    assert_eq!(capability.limits().maximum_concurrent_leases().get(), 3);
    assert_eq!(capability.permission(), PermissionState::Granted);
    assert_eq!(capability.execution(), ExecutionRequirement::HostEventLoop);
    assert!(!PowerPolicyState::Denied.allows_inhibition());
    assert!(!PowerPolicyState::Unknown.allows_inhibition());
}

struct Gesture;

impl UserGestureGrant for Gesture {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[test]
fn requests_preserve_semantic_scope_kind_reason_and_opaque_gesture_evidence() {
    let view = ViewId::from_raw(8, 5).unwrap();
    let request = PowerInhibitionRequest::new(
        PowerInhibitionScope::View(view),
        PowerInhibitionKind::Idle,
        PowerInhibitionReason::MediaPlayback,
    );
    assert_eq!(request.scope().view(), Some(view));
    assert_eq!(request.kind(), PowerInhibitionKind::Idle);
    assert_eq!(request.reason(), PowerInhibitionReason::MediaPlayback);
    assert!(!request.has_user_gesture());

    let with_gesture = PowerInhibitionRequest::with_user_gesture(
        PowerInhibitionScope::Application,
        PowerInhibitionKind::SystemSleep,
        PowerInhibitionReason::Presentation,
        Box::new(Gesture),
    );
    assert!(with_gesture.has_user_gesture());
    let debug = format!("{with_gesture:?}");
    assert!(debug.contains("has_user_gesture"));
    assert!(!debug.contains("Gesture"));
    let (scope, kind, reason, gesture) = with_gesture.into_parts();
    assert_eq!(scope, PowerInhibitionScope::Application);
    assert_eq!(kind, PowerInhibitionKind::SystemSleep);
    assert_eq!(reason, PowerInhibitionReason::Presentation);
    assert!(gesture.is_some());
}

#[derive(Debug)]
struct FixtureLease {
    id: PowerInhibitionLeaseId,
    scope: PowerInhibitionScope,
    kind: PowerInhibitionKind,
    reason: PowerInhibitionReason,
    status: Rc<Cell<PowerInhibitionLeaseStatus>>,
    active: Rc<Cell<u16>>,
}

impl PowerInhibitionLease for FixtureLease {
    fn id(&self) -> PowerInhibitionLeaseId {
        self.id
    }

    fn scope(&self) -> PowerInhibitionScope {
        self.scope
    }

    fn kind(&self) -> PowerInhibitionKind {
        self.kind
    }

    fn reason(&self) -> PowerInhibitionReason {
        self.reason
    }

    fn status(&self) -> PowerInhibitionLeaseStatus {
        self.status.get()
    }
}

impl Drop for FixtureLease {
    fn drop(&mut self) {
        self.active.set(self.active.get() - 1);
    }
}

#[test]
fn lease_status_reports_revocation_and_drop_releases_exactly_once() {
    let active = Rc::new(Cell::new(1));
    let status = Rc::new(Cell::new(PowerInhibitionLeaseStatus::Active));
    let lease: PowerInhibitionLeaseHandle = Box::new(FixtureLease {
        id: PowerInhibitionLeaseId::from_raw(2, 3).unwrap(),
        scope: PowerInhibitionScope::Application,
        kind: PowerInhibitionKind::SystemSleep,
        reason: PowerInhibitionReason::UserInitiatedWork,
        status: Rc::clone(&status),
        active: Rc::clone(&active),
    });
    assert_eq!(lease.id().slot(), 2);
    assert_eq!(lease.id().generation(), 3);
    assert_eq!(lease.status(), PowerInhibitionLeaseStatus::Active);
    assert_eq!(lease.kind(), PowerInhibitionKind::SystemSleep);
    assert_eq!(lease.scope(), PowerInhibitionScope::Application);
    assert_eq!(lease.reason(), PowerInhibitionReason::UserInitiatedWork);

    status.set(PowerInhibitionLeaseStatus::Revoked(
        PowerInhibitionRevocation::PolicyChanged,
    ));
    assert_eq!(
        lease.status(),
        PowerInhibitionLeaseStatus::Revoked(PowerInhibitionRevocation::PolicyChanged)
    );
    drop(lease);
    assert_eq!(active.get(), 0);
}

struct FixturePowerService {
    view: ViewId,
    capability: PowerCapability,
    next_request: Cell<u64>,
    active: Rc<Cell<u16>>,
}

impl FixturePowerService {
    fn admit<T>(&self) -> AdmittedRequest<T> {
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        AdmittedRequest::new(RequestId::from_raw(next).unwrap())
    }
}

impl PowerService for FixturePowerService {
    fn capability(&self, query: PowerCapabilityQuery) -> Support<PowerCapability> {
        if query.scope().view().is_some_and(|view| view != self.view) {
            return Support::Unavailable(UnavailableReason::UnavailableInScope);
        }
        Support::Available(self.capability)
    }

    fn acquire_inhibition(&self, request: PowerInhibitionRequest) -> PowerInhibitionAdmission {
        if request.scope().view().is_some_and(|view| view != self.view) {
            return Err(PowerAdmissionError::ViewUnavailable {
                view: request.scope().view().unwrap(),
            });
        }
        let operations = *self.capability.operations();
        if !operations.supports_scope(request.scope()) {
            return Err(PowerAdmissionError::UnsupportedScope {
                scope: request.scope(),
            });
        }
        if !operations.supports_kind(request.kind()) {
            return Err(PowerAdmissionError::UnsupportedOperation {
                kind: request.kind(),
            });
        }
        match self.capability.policy() {
            PowerPolicyState::Allowed => {}
            PowerPolicyState::Denied => return Err(PowerAdmissionError::PolicyDenied),
            PowerPolicyState::Unknown => return Err(PowerAdmissionError::PolicyUnknown),
        }
        if self.capability.permission().blocks_use() {
            return Err(PowerAdmissionError::PermissionDenied);
        }
        if self.capability.permission().requires_prompt() {
            return Err(PowerAdmissionError::AuthorizationRequired);
        }
        if self.capability.user_gesture().is_required() && !request.has_user_gesture() {
            return Err(PowerAdmissionError::UserGestureRequired);
        }
        let maximum = self.capability.limits().maximum_concurrent_leases();
        if self.active.get() >= maximum.get() {
            return Err(PowerAdmissionError::LeaseLimitReached { maximum });
        }
        self.active.set(self.active.get() + 1);
        let _adapter_owned_gesture = request.into_parts().3;
        Ok(self.admit())
    }
}

#[test]
fn service_capability_admission_completion_and_registry_are_object_safe_and_linear() {
    let view = ViewId::from_raw(12, 4).unwrap();
    let active = Rc::new(Cell::new(0));
    let capability = capability(
        PowerOperations::new(true, false, true, true),
        PowerPolicyState::Allowed,
        UserGestureRequirement::RecentRequired,
        1,
    );
    let service: Rc<dyn PowerService> = Rc::new(FixturePowerService {
        view,
        capability,
        next_request: Cell::new(70),
        active: Rc::clone(&active),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<PowerServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<PowerServiceKey>() else {
        panic!("registered power service must be available");
    };
    let queried = service
        .capability(PowerCapabilityQuery::new(PowerInhibitionScope::View(view)))
        .into_available()
        .unwrap();
    assert_eq!(queried.policy(), PowerPolicyState::Allowed);

    assert!(matches!(
        service.acquire_inhibition(PowerInhibitionRequest::new(
            PowerInhibitionScope::View(view),
            PowerInhibitionKind::Idle,
            PowerInhibitionReason::InteractiveActivity,
        )),
        Err(PowerAdmissionError::UserGestureRequired)
    ));
    let request = PowerInhibitionRequest::with_user_gesture(
        PowerInhibitionScope::View(view),
        PowerInhibitionKind::Idle,
        PowerInhibitionReason::InteractiveActivity,
        Box::new(Gesture),
    );
    let token = service.acquire_inhibition(request).unwrap();
    assert_eq!(token.request_id().get(), 71);
    assert_eq!(active.get(), 1);
    assert!(matches!(
        service.acquire_inhibition(PowerInhibitionRequest::with_user_gesture(
            PowerInhibitionScope::Application,
            PowerInhibitionKind::Idle,
            PowerInhibitionReason::MediaPlayback,
            Box::new(Gesture),
        )),
        Err(PowerAdmissionError::LeaseLimitReached { maximum }) if maximum.get() == 1
    ));

    let status = Rc::new(Cell::new(PowerInhibitionLeaseStatus::Active));
    let lease: PowerInhibitionLeaseHandle = Box::new(FixtureLease {
        id: PowerInhibitionLeaseId::from_raw(5, 1).unwrap(),
        scope: PowerInhibitionScope::View(view),
        kind: PowerInhibitionKind::Idle,
        reason: PowerInhibitionReason::InteractiveActivity,
        status,
        active: Rc::clone(&active),
    });
    let completion = token.complete(RequestOutcome::Applied(lease));
    assert_eq!(completion.request_id().get(), 71);
    assert_eq!(
        completion.outcome().applied().unwrap().status(),
        PowerInhibitionLeaseStatus::Active
    );
    drop(completion);
    assert_eq!(active.get(), 0);

    assert!(matches!(
        service.acquire_inhibition(PowerInhibitionRequest::with_user_gesture(
            PowerInhibitionScope::View(view),
            PowerInhibitionKind::SystemSleep,
            PowerInhibitionReason::Presentation,
            Box::new(Gesture),
        )),
        Err(PowerAdmissionError::UnsupportedOperation {
            kind: PowerInhibitionKind::SystemSleep,
        })
    ));
}
