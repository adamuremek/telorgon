use telorgon::platform::stamp::{EventStampError, EventStampStream, MonotonicInstant};

#[test]
fn public_event_stamp_stream_orders_injected_host_time() {
    let mut stream = EventStampStream::new();
    let at = MonotonicInstant::from_nanos;

    let first = stream.stamp(at(40), None).expect("first host event");
    let second = stream
        .stamp(at(40), Some(at(39)))
        .expect("equal receipt instants use sequence order");

    assert_eq!((first.sequence, second.sequence), (1, 2));
    assert_eq!(second.source_at, Some(at(39)));
    assert_eq!(
        stream.stamp(at(38), None),
        Err(EventStampError::ReceiptTimeRegressed {
            previous: at(40),
            received: at(38),
        })
    );
    assert_eq!(stream.last(), Some(second));
}
