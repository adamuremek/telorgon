use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::rc::Rc;

use telorgon::platform::{
    AdmittedRequest, CapabilityDescriptor, ExecutionRequirement, MAX_RESTORATION_TOKEN_BYTES,
    PermissionState, RequestId, RequestOutcome, RestorationAdmissionError, RestorationCapability,
    RestorationCapabilityQuery, RestorationClearAdmission, RestorationClearApplied,
    RestorationClearRequest, RestorationConsumptionAdmission, RestorationConsumptionApplied,
    RestorationConsumptionRequest, RestorationLimitError, RestorationLimits, RestorationOperations,
    RestorationPublicationAdmission, RestorationPublicationApplied, RestorationPublicationError,
    RestorationPublicationRequest, RestorationRecord, RestorationRevision, RestorationScope,
    RestorationService, RestorationServiceKey, RestorationSessionId, RestorationSnapshotId,
    RestorationToken, RestorationTokenError, ServiceLookup, ServiceRegistry, Support,
    UnavailableReason, UserGestureRequirement, ViewId,
};

fn record(
    scope: RestorationScope,
    revision: RestorationRevision,
    bytes: &[u8],
) -> RestorationRecord {
    RestorationRecord::new(
        RestorationSnapshotId::new(scope, revision),
        RestorationToken::new(bytes.to_vec()).unwrap(),
    )
}

#[test]
fn opaque_tokens_are_nonempty_hard_bounded_and_redacted() {
    let secret = b"private serialized application state";
    let token = RestorationToken::new(secret.to_vec()).unwrap();
    assert_eq!(token.byte_len(), secret.len());
    assert_eq!(token.as_bytes(), secret);
    let debug = format!("{token:?}");
    assert!(debug.contains("redacted"));
    assert!(!debug.contains("private serialized application state"));
    assert_eq!(
        RestorationToken::new(vec![]),
        Err(RestorationTokenError::Empty)
    );
    assert!(matches!(
        RestorationToken::new(vec![1; MAX_RESTORATION_TOKEN_BYTES + 1]),
        Err(RestorationTokenError::TooLarge { .. })
    ));
    assert_eq!(
        RestorationLimits::new(NonZeroU32::new(MAX_RESTORATION_TOKEN_BYTES as u32 + 1).unwrap()),
        Err(RestorationLimitError::TokenBytesTooLarge)
    );
}

#[test]
fn scopes_and_publications_require_exact_independent_successor_histories() {
    let view = ViewId::from_raw(4, 3).unwrap();
    let session = RestorationSessionId::from_raw(7, 2).unwrap();
    assert_eq!(RestorationScope::View(view).view(), Some(view));
    assert_eq!(RestorationScope::Session(session).session(), Some(session));
    assert!(RestorationScope::Application.view().is_none());

    let initial = RestorationPublicationRequest::initial(record(
        RestorationScope::Application,
        RestorationRevision::INITIAL,
        b"first",
    ))
    .unwrap();
    assert!(initial.previous().is_none());
    assert_eq!(initial.snapshot().revision(), RestorationRevision::INITIAL);
    assert!(!format!("{initial:?}").contains("first"));

    let previous = initial.snapshot();
    let next_revision = previous.revision().checked_next().unwrap();
    let update = RestorationPublicationRequest::advance(
        previous,
        record(RestorationScope::Application, next_revision, b"second"),
    )
    .unwrap();
    assert_eq!(update.previous(), Some(previous));
    assert_eq!(update.snapshot().revision(), next_revision);
    assert_eq!(
        RestorationPublicationApplied::from_request(&update).snapshot(),
        update.snapshot()
    );

    assert!(matches!(
        RestorationPublicationRequest::initial(record(
            RestorationScope::Application,
            next_revision,
            b"wrong initial",
        )),
        Err(RestorationPublicationError::InitialRevisionRequired { .. })
    ));
    assert!(matches!(
        RestorationPublicationRequest::advance(
            previous,
            record(RestorationScope::View(view), next_revision, b"wrong scope"),
        ),
        Err(RestorationPublicationError::ScopeMismatch { .. })
    ));
    assert!(matches!(
        RestorationPublicationRequest::advance(
            previous,
            record(
                RestorationScope::Application,
                RestorationRevision::from_raw(3).unwrap(),
                b"skipped",
            ),
        ),
        Err(RestorationPublicationError::RevisionNotSuccessor { .. })
    ));
    assert!(matches!(
        RestorationPublicationRequest::advance(
            RestorationSnapshotId::new(
                RestorationScope::Application,
                RestorationRevision::from_raw(u64::MAX).unwrap(),
            ),
            record(
                RestorationScope::Application,
                RestorationRevision::INITIAL,
                b"exhausted",
            ),
        ),
        Err(RestorationPublicationError::RevisionExhausted { .. })
    ));
}

#[test]
fn consumption_returns_one_owned_exact_token_and_clear_cites_current_snapshot() {
    let session = RestorationSessionId::from_raw(9, 4).unwrap();
    let candidate = record(
        RestorationScope::Session(session),
        RestorationRevision::from_raw(5).unwrap(),
        b"single owner token",
    );
    let request = RestorationConsumptionRequest::new(candidate);
    let snapshot = request.snapshot();
    assert!(!format!("{request:?}").contains("single owner token"));
    let applied = RestorationConsumptionApplied::new(request.into_record());
    assert_eq!(applied.snapshot(), snapshot);
    assert_eq!(applied.token().as_bytes(), b"single owner token");
    assert!(!format!("{applied:?}").contains("single owner token"));
    let (_, token) = applied.into_record().into_parts();
    assert_eq!(&*token.into_bytes(), b"single owner token");

    let clear = RestorationClearRequest::new(snapshot);
    assert_eq!(clear.expected(), snapshot);
    assert_eq!(
        RestorationClearApplied::from_request(clear).cleared(),
        snapshot
    );
}

struct FixtureRestorationService {
    view: ViewId,
    session: RestorationSessionId,
    current: RestorationSnapshotId,
    capability: RestorationCapability,
    next_request: Cell<u64>,
    pending_consumption: RefCell<Option<RestorationRecord>>,
}

impl FixtureRestorationService {
    fn admit<T>(&self) -> AdmittedRequest<T> {
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        AdmittedRequest::new(RequestId::from_raw(next).unwrap())
    }

    fn validate_scope(&self, scope: RestorationScope) -> Result<(), RestorationAdmissionError> {
        match scope {
            RestorationScope::Application => {}
            RestorationScope::View(view) if view == self.view => {}
            RestorationScope::View(view) => {
                return Err(RestorationAdmissionError::ViewUnavailable { view });
            }
            RestorationScope::Session(session) if session == self.session => {}
            RestorationScope::Session(session) => {
                return Err(RestorationAdmissionError::SessionUnavailable { session });
            }
        }
        if !self.capability.operations().supports_scope(scope) {
            return Err(RestorationAdmissionError::UnsupportedScope { scope });
        }
        Ok(())
    }

    fn validate_record(&self, record: &RestorationRecord) -> Result<(), RestorationAdmissionError> {
        self.validate_scope(record.scope())?;
        if record.token().byte_len() > self.capability.limits().maximum_token_bytes().get() as usize
        {
            return Err(RestorationAdmissionError::TokenExceedsCapability);
        }
        Ok(())
    }
}

impl RestorationService for FixtureRestorationService {
    fn capability(&self, query: RestorationCapabilityQuery) -> Support<RestorationCapability> {
        if self.validate_scope(query.scope()).is_err() {
            return Support::Unavailable(UnavailableReason::UnavailableInScope);
        }
        Support::Available(self.capability)
    }

    fn publish(&self, request: RestorationPublicationRequest) -> RestorationPublicationAdmission {
        let update = request.previous().is_some();
        if update && !self.capability.operations().supports_update()
            || !update && !self.capability.operations().supports_publish()
        {
            return Err(RestorationAdmissionError::UnsupportedOperation);
        }
        self.validate_record(request.record())?;
        if request.previous() != Some(self.current) {
            return Err(RestorationAdmissionError::RevisionMismatch {
                expected: request.previous().unwrap_or(request.snapshot()),
                observed: Some(self.current),
            });
        }
        if self.capability.permission().blocks_use() {
            return Err(RestorationAdmissionError::PermissionDenied);
        }
        Ok(self.admit())
    }

    fn consume(&self, request: RestorationConsumptionRequest) -> RestorationConsumptionAdmission {
        if !self.capability.operations().supports_consume() {
            return Err(RestorationAdmissionError::UnsupportedOperation);
        }
        self.validate_record(request.record())?;
        if request.snapshot() != self.current {
            return Err(RestorationAdmissionError::RevisionMismatch {
                expected: request.snapshot(),
                observed: Some(self.current),
            });
        }
        self.pending_consumption
            .replace(Some(request.into_record()));
        Ok(self.admit())
    }

    fn clear(&self, request: RestorationClearRequest) -> RestorationClearAdmission {
        if !self.capability.operations().supports_clear() {
            return Err(RestorationAdmissionError::UnsupportedOperation);
        }
        self.validate_scope(request.expected().scope())?;
        if request.expected() != self.current {
            return Err(RestorationAdmissionError::RevisionMismatch {
                expected: request.expected(),
                observed: Some(self.current),
            });
        }
        Ok(self.admit())
    }
}

#[test]
fn service_capabilities_admissions_completions_and_registry_are_object_safe() {
    let view = ViewId::from_raw(12, 5).unwrap();
    let session = RestorationSessionId::from_raw(3, 2).unwrap();
    let current = RestorationSnapshotId::new(
        RestorationScope::Session(session),
        RestorationRevision::from_raw(8).unwrap(),
    );
    let capability = CapabilityDescriptor::new(
        RestorationOperations::new(true, true, true, true, true, true, true),
        RestorationLimits::new(NonZeroU32::new(128).unwrap()).unwrap(),
        PermissionState::Granted,
        ExecutionRequirement::HostExecutor,
        UserGestureRequirement::NotRequired,
    );
    let concrete = Rc::new(FixtureRestorationService {
        view,
        session,
        current,
        capability,
        next_request: Cell::new(90),
        pending_consumption: RefCell::new(None),
    });
    let service: Rc<dyn RestorationService> = concrete.clone();
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<RestorationServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<RestorationServiceKey>() else {
        panic!("registered restoration service must be available");
    };
    let queried = service
        .capability(RestorationCapabilityQuery::new(RestorationScope::Session(
            session,
        )))
        .into_available()
        .unwrap();
    assert!(queried.operations().supports_consume());
    assert_eq!(queried.limits().maximum_token_bytes().get(), 128);

    let next = current.revision().checked_next().unwrap();
    let publication = RestorationPublicationRequest::advance(
        current,
        record(RestorationScope::Session(session), next, b"next token"),
    )
    .unwrap();
    let published = RestorationPublicationApplied::from_request(&publication);
    let completion = service
        .publish(publication)
        .unwrap()
        .complete(RequestOutcome::Applied(published));
    assert_eq!(completion.request_id().get(), 91);
    assert_eq!(
        completion
            .outcome()
            .applied()
            .unwrap()
            .snapshot()
            .revision(),
        next
    );

    let consumption = RestorationConsumptionRequest::new(record(
        current.scope(),
        current.revision(),
        b"restored token",
    ));
    let token = service.consume(consumption).unwrap();
    let returned_record = concrete
        .pending_consumption
        .borrow_mut()
        .take()
        .expect("adapter must retain the admitted token until completion");
    let consumed = token.complete(RequestOutcome::Applied(RestorationConsumptionApplied::new(
        returned_record,
    )));
    assert_eq!(consumed.request_id().get(), 92);
    assert_eq!(
        consumed.outcome().applied().unwrap().token().as_bytes(),
        b"restored token"
    );

    let clear = RestorationClearRequest::new(current);
    let cleared = RestorationClearApplied::from_request(clear);
    assert!(
        service
            .clear(clear)
            .unwrap()
            .complete(RequestOutcome::Applied(cleared))
            .outcome()
            .is_applied()
    );
    assert!(matches!(
        service.consume(RestorationConsumptionRequest::new(record(
            current.scope(),
            RestorationRevision::INITIAL,
            b"stale",
        ))),
        Err(RestorationAdmissionError::RevisionMismatch { .. })
    ));
}
