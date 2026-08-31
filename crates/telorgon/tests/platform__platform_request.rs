use telorgon::platform::request::{
    AdmittedRequest, RequestAdmission, RequestCompletion, RequestOutcome,
};
use telorgon::platform::{PlatformError, PlatformErrorKind, RequestId};

fn admit(request: RequestId) -> RequestAdmission<u32, &'static str> {
    Ok(AdmittedRequest::new(request))
}

#[test]
fn public_request_path_separates_admission_from_one_identity_bound_terminal_outcome() {
    let request = RequestId::from_raw(27).unwrap();
    let admitted = admit(request).expect("valid request is admitted but not yet applied");
    assert_eq!(admitted.request_id(), request);

    let completion: RequestCompletion<u64> = admitted
        .complete(RequestOutcome::Applied(5))
        .map_applied(u64::from);
    assert_eq!(completion.request_id(), request);
    assert_eq!(
        completion.into_parts(),
        (request, RequestOutcome::Applied(5))
    );

    let failed = AdmittedRequest::<u32>::new(request).complete(RequestOutcome::Failed(
        PlatformError::new(PlatformErrorKind::TimedOut, "request completion"),
    ));
    assert!(failed.outcome().is_failed());
    assert_eq!(
        failed.outcome().failure().unwrap().kind(),
        PlatformErrorKind::TimedOut
    );
}
