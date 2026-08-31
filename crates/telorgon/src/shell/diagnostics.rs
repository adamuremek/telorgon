//! Fixed, payload-free diagnostic counters for the shell model/transport boundary.

use crate::shell::{ShellError, ShellRequestResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ShellDiagnosticKind {
    SnapshotPublished,
    SnapshotRejected,
    ClientInputRequest,
    SurfaceRequest,
    WorkspaceRequest,
    OutputRequest,
    SystemRequest,
    RequestAccepted,
    RequestDenied,
    RequestStale,
    RequestUnsupported,
    HostError,
}

impl ShellDiagnosticKind {
    pub const ALL: [Self; 12] = [
        Self::SnapshotPublished,
        Self::SnapshotRejected,
        Self::ClientInputRequest,
        Self::SurfaceRequest,
        Self::WorkspaceRequest,
        Self::OutputRequest,
        Self::SystemRequest,
        Self::RequestAccepted,
        Self::RequestDenied,
        Self::RequestStale,
        Self::RequestUnsupported,
        Self::HostError,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellDiagnostics {
    counts: [u64; ShellDiagnosticKind::ALL.len()],
    total: u64,
}

impl ShellDiagnostics {
    pub const fn total(self) -> u64 {
        self.total
    }

    pub const fn is_empty(self) -> bool {
        self.total == 0
    }

    pub const fn count(self, kind: ShellDiagnosticKind) -> u64 {
        self.counts[kind.index()]
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = (ShellDiagnosticKind, u64)> {
        ShellDiagnosticKind::ALL
            .into_iter()
            .map(move |kind| (kind, self.count(kind)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellDiagnosticCollector {
    diagnostics: ShellDiagnostics,
}

impl ShellDiagnosticCollector {
    pub const fn diagnostics(self) -> ShellDiagnostics {
        self.diagnostics
    }

    pub fn record(&mut self, kind: ShellDiagnosticKind) {
        let count = &mut self.diagnostics.counts[kind.index()];
        *count = count.saturating_add(1);
        self.diagnostics.total = self.diagnostics.total.saturating_add(1);
    }

    pub fn record_result(&mut self, result: ShellRequestResult) {
        self.record(match result {
            ShellRequestResult::Accepted(_) => ShellDiagnosticKind::RequestAccepted,
            ShellRequestResult::Denied => ShellDiagnosticKind::RequestDenied,
            ShellRequestResult::Stale => ShellDiagnosticKind::RequestStale,
            ShellRequestResult::Unsupported => ShellDiagnosticKind::RequestUnsupported,
        });
    }

    pub fn record_error(&mut self, _: ShellError) {
        self.record(ShellDiagnosticKind::HostError);
    }

    pub fn clear(&mut self) -> ShellDiagnostics {
        let previous = self.diagnostics;
        self.diagnostics = ShellDiagnostics::default();
        previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_bounded_deterministic_and_payload_free() {
        let mut collector = ShellDiagnosticCollector::default();
        collector.record(ShellDiagnosticKind::SurfaceRequest);
        collector.record_result(ShellRequestResult::Denied);
        collector.record_result(ShellRequestResult::Stale);

        let diagnostics = collector.diagnostics();
        assert_eq!(diagnostics.total(), 3);
        assert_eq!(diagnostics.count(ShellDiagnosticKind::SurfaceRequest), 1);
        assert_eq!(diagnostics.count(ShellDiagnosticKind::RequestDenied), 1);
        assert_eq!(diagnostics.count(ShellDiagnosticKind::RequestStale), 1);
        assert_eq!(diagnostics.iter().len(), ShellDiagnosticKind::ALL.len());
        assert_eq!(collector.clear(), diagnostics);
        assert!(collector.diagnostics().is_empty());
    }
}
