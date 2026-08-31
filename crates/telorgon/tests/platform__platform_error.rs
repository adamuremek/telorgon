use std::error::Error;

use telorgon::platform::error::{
    PlatformError, PlatformErrorKind, PlatformErrorSource, PlatformResult,
};

fn failed(error: PlatformError) -> PlatformResult<()> {
    Err(error)
}

#[test]
fn public_error_path_keeps_branching_and_sanitized_causality_structured() {
    let source = PlatformErrorSource::new(
        PlatformErrorKind::TransportFailure,
        "host completion transport",
    );
    let error = PlatformError::with_source(
        PlatformErrorKind::Unavailable,
        "clipboard completion",
        source,
    );
    let error = failed(error).unwrap_err();
    assert_eq!(error.kind(), PlatformErrorKind::Unavailable);
    assert_eq!(error.context(), "clipboard completion");
    assert_eq!(error.source_record(), Some(source));
    assert_eq!(
        error.source().unwrap().to_string(),
        "host completion transport: transport failure"
    );
}
