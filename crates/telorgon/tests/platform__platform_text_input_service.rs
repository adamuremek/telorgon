use std::cell::Cell;
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, ExecutionRequirement, PermissionState, RequestId, RequestOutcome,
    ServiceLookup, ServiceRegistry, Support, TextInputAdmission, TextInputAdmissionError,
    TextInputApplied, TextInputCapability, TextInputCapabilityQuery, TextInputDeltaEvent,
    TextInputDeltaKind, TextInputLimits, TextInputOperations, TextInputService,
    TextInputServiceKey, TextInputSyncKind, TextInputSyncRequest, UnavailableReason,
    UserGestureRequirement, ViewId,
};
use telorgon::text::{
    TextBuffer, TextInputConfiguration, TextInputRequest, TextInputSession, TextRevision,
    TextSessionCommand, TextSessionDelta, TextSessionId,
};

struct FixtureTextInputService {
    view: ViewId,
    next_request: Cell<u64>,
}

impl TextInputService for FixtureTextInputService {
    fn capability(&self, query: TextInputCapabilityQuery) -> Support<TextInputCapability> {
        if query.view() != self.view {
            return Support::Unavailable(UnavailableReason::UnavailableInScope);
        }
        Support::Available(TextInputCapability::new(
            TextInputOperations::new(true, true, true, true, true, true),
            TextInputLimits::default(),
            PermissionState::NotRequired,
            ExecutionRequirement::PlatformMainThread,
            UserGestureRequirement::NotRequired,
        ))
    }

    fn synchronize(&self, request: TextInputSyncRequest) -> TextInputAdmission {
        if request.view() != self.view {
            return Err(TextInputAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        Ok(AdmittedRequest::new(RequestId::from_raw(next).unwrap()))
    }
}

fn open_request(session: TextSessionId, secure: bool) -> TextInputRequest {
    let configuration = TextInputConfiguration {
        secure_entry: secure,
        ..TextInputConfiguration::default()
    };
    let buffer = TextBuffer::from_text("private text").unwrap();
    let mut state = TextInputSession::new(session, configuration, 128);
    state.open(&buffer).unwrap()
}

#[test]
fn public_text_input_path_reuses_revisioned_sessions_and_only_admits_native_synchronization() {
    let view = ViewId::from_raw(12, 5).unwrap();
    let session = TextSessionId::from_raw(8, 2).unwrap();
    let request = TextInputSyncRequest::new(view, open_request(session, false)).unwrap();
    assert_eq!(request.kind(), TextInputSyncKind::Open);
    assert_eq!(request.revision(), Some(TextRevision::INITIAL));
    assert!(!format!("{request:?}").contains("private text"));

    let service: Rc<dyn TextInputService> = Rc::new(FixtureTextInputService {
        view,
        next_request: Cell::new(70),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<TextInputServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<TextInputServiceKey>() else {
        panic!("registered text-input service must be available")
    };
    let capability = service
        .capability(TextInputCapabilityQuery::new(view))
        .into_available()
        .unwrap();
    assert!(capability.operations().supports_input_method());
    assert!(capability.operations().supports_virtual_keyboard());

    let applied = TextInputApplied::from_request(&request);
    let completion = service
        .synchronize(request)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(completion.request_id(), RequestId::from_raw(71).unwrap());
    assert_eq!(completion.outcome().applied().unwrap().session(), session);

    let stale_view = ViewId::from_raw(view.slot(), view.generation() + 1).unwrap();
    let stale = TextInputSyncRequest::new(stale_view, TextInputRequest::Close { session }).unwrap();
    assert_eq!(
        service.synchronize(stale),
        Err(TextInputAdmissionError::ViewUnavailable { view: stale_view })
    );
}

#[test]
fn canonical_delta_envelope_cites_view_session_and_revision_without_native_ranges() {
    let view = ViewId::from_raw(4, 3).unwrap();
    let session = TextSessionId::from_raw(9, 6).unwrap();
    let event = TextInputDeltaEvent::new(
        view,
        TextSessionDelta {
            session,
            command: TextSessionCommand::PerformAction {
                base_revision: TextRevision(14),
            },
        },
    );

    assert_eq!(event.view(), view);
    assert_eq!(event.session(), session);
    assert_eq!(event.observed_revision(), TextRevision(14));
    assert_eq!(event.kind(), TextInputDeltaKind::PerformAction);
}
