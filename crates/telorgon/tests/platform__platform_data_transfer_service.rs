use std::cell::{Cell, RefCell};
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};
use std::rc::Rc;

use telorgon::platform::services::data_transfer::{
    DataFormat, DataFormatReadRequest, DataOfferDescriptor, DataReadAdmission, DataReadCompletion,
    DataReadMode, DataReadProgress, DataReadValidationError, DataSourceKind,
    DataTransferAdmissionError, DataTransferCapability, DataTransferLimits, DataTransferOperations,
    DataTransferService, DataTransferServiceKey, SizeHint, TrustLevel,
};
use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, DataOfferId, ExecutionRequirement, PermissionState,
    RequestId, ServiceRegistry, Support, UserGestureRequirement,
};

#[test]
fn direct_path_preserves_formats_generations_bounds_and_content_free_metadata() {
    let plain = DataFormat::mime("text/plain;charset=utf-8").unwrap();
    let html = DataFormat::mime("text/html").unwrap();
    let offer = DataOfferDescriptor::new(
        DataOfferId::from_raw(12, 3).unwrap(),
        vec![plain.clone(), html.clone()],
        DataSourceKind::DragAndDrop,
        TrustLevel::Untrusted,
        vec![SizeHint::AtMost(64), SizeHint::Exact(300)],
    )
    .unwrap();

    assert_eq!(offer.formats(), [plain, html.clone()]);
    assert_eq!(
        offer.size_hints(),
        [SizeHint::AtMost(64), SizeHint::Exact(300)]
    );
    assert_eq!(offer.trust(), TrustLevel::Untrusted);

    assert_eq!(
        DataFormatReadRequest::for_offer(
            &offer,
            html.clone(),
            NonZeroU64::new(299).unwrap(),
            DataReadMode::Buffered,
        ),
        Err(DataReadValidationError::KnownSizeExceedsReadLimit)
    );

    let request = DataFormatReadRequest::for_offer(
        &offer,
        html,
        NonZeroU64::new(512).unwrap(),
        DataReadMode::Streamed {
            max_chunk_bytes: NonZeroU32::new(128).unwrap(),
        },
    )
    .unwrap();
    let progress = DataReadProgress::new(RequestId::MIN, &request, 128, 1).unwrap();
    let completion = DataReadCompletion::new(&request, 300, 3).unwrap();
    assert_eq!(progress.offer(), offer.id());
    assert_eq!(progress.request(), RequestId::MIN);
    assert_eq!(completion.bytes_read(), 300);

    let request_debug = format!("{request:?}");
    let progress_debug = format!("{progress:?}");
    let completion_debug = format!("{completion:?}");
    assert!(!request_debug.contains("text/html"));
    assert!(!progress_debug.contains("text/html"));
    assert!(!completion_debug.contains("text/html"));

    let replacement = DataOfferDescriptor::new(
        DataOfferId::from_raw(12, 4).unwrap(),
        vec![DataFormat::mime("text/html").unwrap()],
        DataSourceKind::DragAndDrop,
        TrustLevel::Untrusted,
        vec![SizeHint::Exact(300)],
    )
    .unwrap();
    assert_eq!(
        request.validate_against(&replacement),
        Err(DataReadValidationError::OfferMismatch)
    );
}

#[derive(Default)]
struct RecordingTransferService {
    next_request: Cell<u64>,
    reads: RefCell<Vec<DataFormatReadRequest>>,
    cancellations: RefCell<Vec<RequestId>>,
}

impl DataTransferService for RecordingTransferService {
    fn capability(&self) -> Support<DataTransferCapability> {
        Support::Available(CapabilityDescriptor::new(
            DataTransferOperations {
                inbound_read: true,
                outbound_offer: false,
                native_drag_and_drop: false,
                share: false,
                cancellation: true,
                streaming: true,
            },
            DataTransferLimits::new(
                NonZeroU16::new(4).unwrap(),
                NonZeroU64::new(4096).unwrap(),
                NonZeroU32::new(512).unwrap(),
            )
            .unwrap(),
            PermissionState::Granted,
            ExecutionRequirement::HostExecutor,
            UserGestureRequirement::NotRequired,
        ))
    }

    fn request_read(&self, request: DataFormatReadRequest) -> DataReadAdmission {
        self.reads.borrow_mut().push(request);
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        Ok(AdmittedRequest::new(RequestId::from_raw(next).unwrap()))
    }

    fn cancel_read(&self, request: RequestId) -> Result<(), DataTransferAdmissionError> {
        self.cancellations.borrow_mut().push(request);
        Ok(())
    }
}

#[test]
fn registry_handle_only_admits_commands_and_retains_no_content() {
    let format = DataFormat::mime("image/png").unwrap();
    let offer = DataOfferDescriptor::new(
        DataOfferId::MIN,
        vec![format.clone()],
        DataSourceKind::Clipboard,
        TrustLevel::Trusted,
        vec![SizeHint::AtMost(1024)],
    )
    .unwrap();
    let read = DataFormatReadRequest::for_offer(
        &offer,
        format,
        NonZeroU64::new(2048).unwrap(),
        DataReadMode::Buffered,
    )
    .unwrap();

    let concrete = Rc::new(RecordingTransferService::default());
    let handle: Rc<dyn DataTransferService> = concrete.clone();
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<DataTransferServiceKey>(handle)
            .is_registered()
    );

    let service = registry
        .lookup::<DataTransferServiceKey>()
        .into_available()
        .unwrap();
    let capability = service.capability().into_available().unwrap();
    assert!(capability.operations().inbound_read);
    assert!(capability.operations().cancellation);
    let admitted = service.request_read(read).unwrap();
    let request_id = admitted.request_id();
    service.cancel_read(request_id).unwrap();
    assert_eq!(concrete.reads.borrow().len(), 1);
    assert_eq!(concrete.cancellations.borrow().as_slice(), [request_id]);
}
