use std::num::{NonZeroU16, NonZeroU32};
use std::rc::Rc;
use std::time::Duration;

use telorgon::platform::{
    ActivityState, AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, HapticApplied,
    HapticCapability, HapticDeviceSupport, HapticEffect, HapticEffectSupport, HapticIntensity,
    HapticLimits, HapticOperations, HapticRequest, HapticUserSettingState, HapticsService,
    HapticsServiceKey, MetricsCitation, MonotonicClock, MonotonicInstant, PermissionState,
    RequestId, RequestOutcome, RestorationCapability, RestorationClearApplied,
    RestorationClearRequest, RestorationConsumptionApplied, RestorationConsumptionRequest,
    RestorationLimits, RestorationOperations, RestorationPublicationApplied,
    RestorationPublicationRequest, RestorationRecord, RestorationRevision, RestorationScope,
    RestorationService, RestorationServiceKey, RestorationSnapshotId, RestorationToken,
    ServiceLookup, ServiceRegistry, UserGestureRequirement, ViewId, ViewLifetime, ViewMetrics,
};
use telorgon::platform_conformance::{
    BoundedCapture, CaptureLimitError, CompletionCapture, DeterministicHost, FakeClock,
    FakeClockError, FakeHapticsService, FakeRestorationOperation, FakeRestorationService,
    HostEmitErrorKind, MAX_CAPTURE_ITEMS, MAX_CONFORMANCE_VIEWS, ViewDriver, ViewDriverError,
    ViewDriverLimitError, ViewObservation,
};

fn nonzero(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap()
}

fn view(slot: u32) -> ViewId {
    ViewId::from_raw(slot, 1).unwrap()
}

#[test]
fn fake_clock_and_bounded_capture_are_explicit_atomic_and_payload_safe() {
    let mut clock = FakeClock::from_nanos(10);
    assert_eq!(clock.now().as_nanos(), 10);
    assert_eq!(clock.now().as_nanos(), 10, "sampling must not advance time");
    assert_eq!(
        clock.advance(Duration::from_nanos(5)).unwrap().as_nanos(),
        15
    );
    assert_eq!(
        clock.set(MonotonicInstant::from_nanos(14)),
        Err(FakeClockError::Regression {
            current: MonotonicInstant::from_nanos(15),
            requested: MonotonicInstant::from_nanos(14),
        })
    );
    let mut edge = FakeClock::from_nanos(u64::MAX);
    assert!(matches!(
        edge.advance(Duration::from_nanos(1)),
        Err(FakeClockError::Overflow { .. })
    ));

    assert_eq!(
        BoundedCapture::<u8>::new(nonzero(MAX_CAPTURE_ITEMS + 1)).unwrap_err(),
        CaptureLimitError::AboveHardLimit {
            requested: MAX_CAPTURE_ITEMS + 1,
            maximum: MAX_CAPTURE_ITEMS,
        }
    );
    let mut capture = BoundedCapture::new(nonzero(1)).unwrap();
    capture.push("first private payload").unwrap();
    let rejected = capture.push("second private payload").unwrap_err();
    assert_eq!(rejected.capacity().get(), 1);
    assert!(!format!("{rejected:?}").contains("second private payload"));
    assert_eq!(rejected.into_item(), "second private payload");
    assert_eq!(capture.pop_front(), Some("first private payload"));

    let mut completions: CompletionCapture<u32> = BoundedCapture::new(nonzero(1)).unwrap();
    let completion =
        AdmittedRequest::new(RequestId::from_raw(9).unwrap()).complete(RequestOutcome::Applied(42));
    completions.push(completion).unwrap();
    assert_eq!(completions.front().unwrap().request_id().get(), 9);
    assert_eq!(completions.front().unwrap().outcome().applied(), Some(&42));
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TracePayload {
    Ready,
    Input(u8),
}

fn deterministic_trace() -> (
    Vec<u64>,
    Vec<telorgon::platform::PlatformEvent<TracePayload>>,
) {
    let mut host = DeterministicHost::new(
        MonotonicInstant::from_nanos(100),
        nonzero(2),
        nonzero(8),
        nonzero(4),
    )
    .unwrap();
    host.views_mut()
        .add_view(view(1), ViewMetrics::default())
        .unwrap();
    host.views_mut()
        .add_view(view(2), ViewMetrics::default())
        .unwrap();
    host.views_mut()
        .observe(view(1), ViewObservation::Lifetime(ViewLifetime::Live))
        .unwrap();
    host.views_mut()
        .observe(view(1), ViewObservation::Lifetime(ViewLifetime::Live))
        .unwrap();
    host.emit(
        view(1),
        MetricsCitation::NOT_CONVERTED,
        Some(MonotonicInstant::from_nanos(90)),
        TracePayload::Ready,
    )
    .unwrap();
    host.clock_mut().advance(Duration::from_nanos(5)).unwrap();
    host.emit(
        view(2),
        MetricsCitation::NOT_CONVERTED,
        None,
        TracePayload::Input(7),
    )
    .unwrap();
    let revisions = host
        .views()
        .updates()
        .iter()
        .map(|update| update.current().revision().get())
        .collect();
    (revisions, host.events_mut().take_all())
}

#[test]
fn lifecycle_and_event_host_reproduce_identical_bounded_multi_view_traces() {
    assert_eq!(deterministic_trace(), deterministic_trace());

    assert!(matches!(
        ViewDriver::new(nonzero(MAX_CONFORMANCE_VIEWS + 1), nonzero(1)),
        Err(ViewDriverLimitError::ViewLimitTooLarge { .. })
    ));
    let mut driver = ViewDriver::new(nonzero(1), nonzero(1)).unwrap();
    driver.add_view(view(3), ViewMetrics::default()).unwrap();
    driver
        .observe(view(3), ViewObservation::Lifetime(ViewLifetime::Live))
        .unwrap();
    let before = driver.snapshot(view(3)).unwrap();
    assert!(matches!(
        driver.observe(view(3), ViewObservation::Activity(ActivityState::Active)),
        Err(ViewDriverError::UpdateCaptureFull { .. })
    ));
    assert_eq!(driver.snapshot(view(3)).unwrap(), before);
    driver.updates_mut().clear();
    driver
        .observe(view(3), ViewObservation::Activity(ActivityState::Active))
        .unwrap();
    assert_eq!(
        driver.snapshot(view(3)).unwrap().activity(),
        ActivityState::Active
    );

    let mut host =
        DeterministicHost::new(MonotonicInstant::ZERO, nonzero(1), nonzero(1), nonzero(1)).unwrap();
    host.views_mut()
        .add_view(view(4), ViewMetrics::default())
        .unwrap();
    let first = host
        .emit(view(4), MetricsCitation::NOT_CONVERTED, None, "first")
        .unwrap();
    let full = host
        .emit(view(4), MetricsCitation::NOT_CONVERTED, None, "second")
        .unwrap_err();
    assert!(matches!(
        full.kind(),
        HostEmitErrorKind::EventCaptureFull { .. }
    ));
    assert_eq!(full.into_payload(), "second");
    assert_eq!(host.last_stamp(), Some(first));
}

fn haptic_capability() -> HapticCapability {
    HapticCapability::new(
        CapabilityDescriptor::new(
            HapticOperations::new(true),
            HapticLimits::default(),
            PermissionState::Granted,
            ExecutionRequirement::RuntimeOwner,
            UserGestureRequirement::NotRequired,
        ),
        HapticDeviceSupport::available(HapticEffectSupport::only(HapticEffect::Selection), true)
            .unwrap(),
        HapticUserSettingState::Enabled,
    )
    .unwrap()
}

#[test]
fn fake_haptics_validates_capability_and_produces_deterministic_linear_admissions() {
    let service = Rc::new(FakeHapticsService::new(haptic_capability(), nonzero(1)).unwrap());
    let handle: Rc<dyn HapticsService> = service.clone();
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<HapticsServiceKey>(handle)
            .is_registered()
    );
    assert!(matches!(
        registry.lookup::<HapticsServiceKey>(),
        ServiceLookup::Available(_)
    ));
    let request = HapticRequest::new(
        HapticEffect::Selection,
        HapticIntensity::from_units(500).unwrap(),
    );
    let applied = HapticApplied::from_request(&request);
    let completion = service
        .trigger(request)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id().get(), 1);
    let invocation = *service.invocations().front().unwrap();
    assert_eq!(invocation.request_id().get(), 1);
    assert_eq!(invocation.effect(), HapticEffect::Selection);
    assert_eq!(invocation.intensity().units(), 500);
    assert!(!invocation.had_user_gesture());
    assert!(matches!(
        service.trigger(HapticRequest::new(
            HapticEffect::HeavyImpact,
            HapticIntensity::FULL,
        )),
        Err(telorgon::platform::HapticAdmissionError::EffectUnsupported { .. })
    ));
    assert!(matches!(
        service.trigger(HapticRequest::new(
            HapticEffect::Selection,
            HapticIntensity::FULL,
        )),
        Err(telorgon::platform::HapticAdmissionError::CapacityExceeded)
    ));
}

fn restoration_capability() -> RestorationCapability {
    CapabilityDescriptor::new(
        RestorationOperations::new(true, true, true, true, true, true, true),
        RestorationLimits::new(NonZeroU32::new(64).unwrap()).unwrap(),
        PermissionState::Granted,
        ExecutionRequirement::RuntimeOwner,
        UserGestureRequirement::NotRequired,
    )
}

fn restoration_record(snapshot: RestorationSnapshotId, bytes: &[u8]) -> RestorationRecord {
    RestorationRecord::new(snapshot, RestorationToken::new(bytes.to_vec()).unwrap())
}

#[test]
fn fake_restoration_keeps_admission_separate_from_observed_truth_and_token_completion() {
    let service =
        Rc::new(FakeRestorationService::new(restoration_capability(), nonzero(8)).unwrap());
    let handle: Rc<dyn RestorationService> = service.clone();
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<RestorationServiceKey>(handle)
            .is_registered()
    );
    let initial =
        RestorationSnapshotId::new(RestorationScope::Application, RestorationRevision::INITIAL);
    service.observe_snapshot(initial).unwrap();

    let consume = RestorationConsumptionRequest::new(restoration_record(initial, b"private"));
    let admitted = service.consume(consume).unwrap();
    let request_id = admitted.request_id();
    let owned = service
        .take_pending_consumption(request_id)
        .expect("admitted token must remain owned until terminal completion");
    let completion = admitted.complete(RequestOutcome::Applied(
        RestorationConsumptionApplied::new(owned),
    ));
    assert_eq!(
        completion.outcome().applied().unwrap().token().as_bytes(),
        b"private"
    );
    let first = *service.invocations().front().unwrap();
    assert_eq!(first.operation(), FakeRestorationOperation::Consume);
    assert_eq!(first.snapshot(), initial);
    assert_eq!(first.token_byte_len(), Some(7));
    assert!(!format!("{first:?}").contains("private"));

    let next =
        RestorationSnapshotId::new(initial.scope(), initial.revision().checked_next().unwrap());
    let publication =
        RestorationPublicationRequest::advance(initial, restoration_record(next, b"next state"))
            .unwrap();
    let published = RestorationPublicationApplied::from_request(&publication);
    let publication_completion = service
        .publish(publication)
        .unwrap()
        .complete(RequestOutcome::Applied(published));
    assert!(publication_completion.outcome().is_applied());
    assert_eq!(service.current(initial.scope()), Some(initial));
    service.observe_snapshot(next).unwrap();
    assert_eq!(service.current(next.scope()), Some(next));

    assert!(matches!(
        service.consume(RestorationConsumptionRequest::new(restoration_record(
            initial, b"stale",
        ))),
        Err(telorgon::platform::RestorationAdmissionError::RevisionMismatch { .. })
    ));
    let clear = RestorationClearRequest::new(next);
    let cleared = RestorationClearApplied::from_request(clear);
    let clear_completion = service
        .clear(clear)
        .unwrap()
        .complete(RequestOutcome::Applied(cleared));
    assert!(clear_completion.outcome().is_applied());
    assert_eq!(service.current(next.scope()), Some(next));
    service.observe_clear(next).unwrap();
    assert_eq!(service.current(next.scope()), None);
}
