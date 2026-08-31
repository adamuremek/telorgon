use telorgon::shell::{ShellDiagnosticCollector, ShellDiagnosticKind, ShellRequestResult};

#[test]
fn public_shell_diagnostics_are_fixed_payload_free_counters() {
    let mut collector = ShellDiagnosticCollector::default();
    collector.record(ShellDiagnosticKind::WorkspaceRequest);
    collector.record_result(ShellRequestResult::Stale);

    let diagnostics = collector.diagnostics();
    assert_eq!(diagnostics.total(), 2);
    assert_eq!(diagnostics.count(ShellDiagnosticKind::WorkspaceRequest), 1);
    assert_eq!(diagnostics.count(ShellDiagnosticKind::RequestStale), 1);
    assert_eq!(diagnostics.iter().len(), ShellDiagnosticKind::ALL.len());
}
