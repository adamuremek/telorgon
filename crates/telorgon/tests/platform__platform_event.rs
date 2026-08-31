use telorgon::core::MonotonicInstant;
use telorgon::platform::event::{
    CoalescingMetadata, CollapsedEventCount, MetricsCitation, PlatformEvent,
};
use telorgon::platform::{EventStamp, MetricsRevision, ViewId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestPayload {
    PointerMoved,
}

#[test]
fn public_event_path_preserves_newest_ordering_conversion_and_coalescing_evidence() {
    let view = ViewId::from_raw(9, 2).unwrap();
    let stamp = EventStamp {
        sequence: 17,
        received_at: MonotonicInstant::from_nanos(300),
        source_at: Some(MonotonicInstant::from_nanos(280)),
    };
    let metrics = MetricsRevision::from_raw(6).unwrap();
    let collapsed = CollapsedEventCount::from_raw(3).unwrap();
    let event = PlatformEvent::from_coalescing(
        view,
        CoalescingMetadata::coalesced(stamp, collapsed),
        MetricsCitation::converted_using(metrics),
        TestPayload::PointerMoved,
    );

    assert_eq!(event.view(), view);
    assert_eq!(event.stamp(), stamp);
    assert_eq!(event.metrics_revision(), Some(metrics));
    assert_eq!(event.coalescing().collapsed_count(), 3);
    assert_eq!(event.payload(), &TestPayload::PointerMoved);

    let mapped = event.map_payload(|_| 41_u64);
    assert_eq!(mapped.stamp(), stamp);
    assert_eq!(mapped.metrics_revision(), Some(metrics));
    assert_eq!(mapped.coalescing().collapsed_event_count(), Some(collapsed));
    assert_eq!(mapped.into_payload(), 41);
}
