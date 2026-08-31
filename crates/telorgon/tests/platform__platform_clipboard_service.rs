use std::cell::Cell;
use std::rc::Rc;

use telorgon::platform::services::clipboard::{
    ClipboardAdmissionError, ClipboardCapabilities, ClipboardCapability, ClipboardChange,
    ClipboardClearApplied, ClipboardClearRequest, ClipboardKind, ClipboardLimits,
    ClipboardOperations, ClipboardPublishApplied, ClipboardPublishRequest,
    ClipboardRequestAdmission, ClipboardRevision, ClipboardService, ClipboardServiceKey,
    ClipboardSnapshot, ClipboardSnapshotId, ClipboardSnapshotStatus,
};
use telorgon::platform::services::data_transfer::{
    DataFormat, DataOfferDescriptor, DataSourceKind, SizeHint, TrustLevel,
};
use telorgon::platform::{
    ExecutionRequirement, PermissionState, RequestId, RequestOutcome, ServiceLookup,
    ServiceRegistry, Support, UnavailableReason, UserGestureRequirement,
};

fn offer() -> DataOfferDescriptor {
    DataOfferDescriptor::new(
        telorgon::platform::DataOfferId::from_raw(8, 3).unwrap(),
        vec![
            DataFormat::mime("text/plain;charset=utf-8").unwrap(),
            DataFormat::mime("text/html").unwrap(),
        ],
        DataSourceKind::Clipboard,
        TrustLevel::Trusted,
        vec![SizeHint::AtMost(128), SizeHint::Unknown],
    )
    .unwrap()
}

fn capability() -> ClipboardCapability {
    ClipboardCapability::new(
        ClipboardOperations::new(true, true, true, true),
        offer().formats().to_vec(),
        ClipboardLimits::default(),
        PermissionState::Granted,
        PermissionState::NotRequired,
        ExecutionRequirement::RuntimeOwner,
        UserGestureRequirement::NotRequired,
    )
    .unwrap()
}

struct FixtureClipboard {
    capabilities: ClipboardCapabilities,
    snapshot: ClipboardSnapshot,
    next_request: Cell<u64>,
}

impl FixtureClipboard {
    fn admit<T>(&self) -> ClipboardRequestAdmission<T> {
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        Ok(telorgon::platform::AdmittedRequest::new(
            RequestId::from_raw(next).unwrap(),
        ))
    }
}

impl ClipboardService for FixtureClipboard {
    fn capability(&self, clipboard: ClipboardKind) -> Support<ClipboardCapability> {
        self.capabilities.for_clipboard(clipboard).map(Clone::clone)
    }

    fn current_snapshot(&self, clipboard: ClipboardKind) -> ClipboardSnapshotStatus {
        match clipboard {
            ClipboardKind::System => ClipboardSnapshotStatus::Current(self.snapshot.clone()),
            ClipboardKind::Selection => {
                ClipboardSnapshotStatus::Unavailable(UnavailableReason::UnsupportedByPlatform)
            }
        }
    }

    fn publish(
        &self,
        _request: ClipboardPublishRequest,
    ) -> ClipboardRequestAdmission<ClipboardPublishApplied> {
        self.admit()
    }

    fn clear(
        &self,
        request: ClipboardClearRequest,
    ) -> ClipboardRequestAdmission<ClipboardClearApplied> {
        if request.clipboard() == ClipboardKind::Selection {
            return Err(ClipboardAdmissionError::ClipboardUnavailable {
                clipboard: ClipboardKind::Selection,
            });
        }
        self.admit()
    }
}

#[test]
fn typed_clipboard_service_publishes_metadata_without_synchronous_content_or_fallback() {
    let empty = ServiceRegistry::new();
    assert!(matches!(
        empty.lookup::<ClipboardServiceKey>(),
        ServiceLookup::Unavailable(_)
    ));

    let first_id = ClipboardSnapshotId::new(ClipboardKind::System, ClipboardRevision::INITIAL);
    let first = ClipboardSnapshot::new(first_id, None).unwrap();
    let service: Rc<dyn ClipboardService> = Rc::new(FixtureClipboard {
        capabilities: ClipboardCapabilities::new(
            Support::Available(capability()),
            Support::Unavailable(UnavailableReason::UnsupportedByPlatform),
        ),
        snapshot: first,
        next_request: Cell::new(100),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<ClipboardServiceKey>(Rc::clone(&service))
            .is_registered()
    );

    let ServiceLookup::Available(service) = registry.lookup::<ClipboardServiceKey>() else {
        panic!("registered clipboard service must be found")
    };
    let Support::Available(system) = service.capability(ClipboardKind::System) else {
        panic!("system clipboard must be available")
    };
    assert!(system.operations().supports_publish());
    assert!(system.supports_format(&DataFormat::mime("text/html").unwrap()));
    assert_eq!(system.read_permission(), PermissionState::Granted);
    assert_eq!(
        service
            .capability(ClipboardKind::Selection)
            .unavailable_reason(),
        Some(UnavailableReason::UnsupportedByPlatform)
    );

    let descriptor = offer();
    let publish =
        ClipboardPublishRequest::new(ClipboardKind::System, descriptor.clone(), Some(first_id))
            .unwrap();
    let debug = format!("{publish:?}");
    assert!(debug.contains("format_count: 2"));
    assert!(!debug.contains("clipboard payload"));

    let published = ClipboardPublishApplied::from_request(&publish);
    let admitted = service.publish(publish).unwrap();
    assert_eq!(admitted.request_id(), RequestId::from_raw(101).unwrap());
    let completion = admitted.complete(RequestOutcome::Applied(published));
    assert_eq!(
        completion.outcome().applied().unwrap().offer(),
        descriptor.id()
    );

    let next_id = ClipboardSnapshotId::new(
        ClipboardKind::System,
        ClipboardRevision::from_raw(2).unwrap(),
    );
    let next = ClipboardSnapshot::new(next_id, Some(descriptor)).unwrap();
    let changed = ClipboardChange::new(Some(first_id), next).unwrap();
    assert_eq!(changed.current().id(), next_id);

    let clear = ClipboardClearRequest::new(ClipboardKind::System, Some(next_id)).unwrap();
    let cleared = ClipboardClearApplied::from_request(clear);
    let admitted = service.clear(clear).unwrap();
    assert_eq!(admitted.request_id(), RequestId::from_raw(102).unwrap());
    assert_eq!(
        admitted
            .complete(RequestOutcome::Applied(cleared))
            .outcome()
            .applied()
            .unwrap()
            .clipboard(),
        ClipboardKind::System
    );
}
