use telorgon::application_primitives::prelude::{
    ApplicationPrimitiveDiagnosticCollector, ApplicationPrimitiveDiagnosticKind,
    RenderTargetViewError, VideoSurfaceError,
};

#[test]
fn public_primitive_diagnostics_are_bounded_typed_and_payload_free() {
    let mut collector = ApplicationPrimitiveDiagnosticCollector::default();
    collector.record_error(RenderTargetViewError::ZeroContentVersion);
    collector.record_error(VideoSurfaceError::InvalidFrameSize);
    collector.record(ApplicationPrimitiveDiagnosticKind::ProtectedVideoUnavailable);
    let diagnostics = collector.diagnostics();

    assert_eq!(diagnostics.total(), 3);
    assert_eq!(
        diagnostics.count(ApplicationPrimitiveDiagnosticKind::InvalidRenderTargetContent),
        1
    );
    assert_eq!(
        diagnostics.count(ApplicationPrimitiveDiagnosticKind::InvalidVideoSurfaceContent),
        1
    );
    assert_eq!(diagnostics.iter().len(), 8);
    assert_eq!(collector.clear(), diagnostics);
    assert!(collector.diagnostics().is_empty());
}
