use telorgon::shell::{
    ClientInputRequest, OutputRequest, ShellHost, ShellRequestResult, ShellSnapshot,
    ShellSnapshotParts, ShellSnapshotRevision, SurfaceRequest, SystemRequest, SystemStatusRevision,
    SystemStatusSnapshot, WorkspaceRequest,
};

struct UnsupportedHost {
    snapshot: ShellSnapshot,
}

impl ShellHost for UnsupportedHost {
    fn snapshot(&self) -> ShellSnapshot {
        self.snapshot.clone()
    }

    fn request_client_input(&mut self, _: ClientInputRequest) -> ShellRequestResult {
        ShellRequestResult::Unsupported
    }

    fn request_surface(&mut self, _: SurfaceRequest) -> ShellRequestResult {
        ShellRequestResult::Unsupported
    }

    fn request_workspace(&mut self, _: WorkspaceRequest) -> ShellRequestResult {
        ShellRequestResult::Unsupported
    }

    fn request_output(&mut self, _: OutputRequest) -> ShellRequestResult {
        ShellRequestResult::Unsupported
    }

    fn request_system(&mut self, _: SystemRequest) -> ShellRequestResult {
        ShellRequestResult::Unsupported
    }
}

#[test]
fn public_host_transports_one_atomic_immutable_snapshot() {
    let snapshot = ShellSnapshot::new(
        ShellSnapshotRevision::from_raw(101).unwrap(),
        ShellSnapshotParts {
            grants: Vec::new(),
            outputs: Vec::new(),
            surfaces: Vec::new(),
            workspaces: Vec::new(),
            applications: Vec::new(),
            notifications: Vec::new(),
            system_status: SystemStatusSnapshot::new(SystemStatusRevision::INITIAL, Vec::new())
                .unwrap(),
            accessibility: Vec::new(),
        },
    )
    .unwrap();
    let host = UnsupportedHost { snapshot };

    assert_eq!(host.snapshot().revision().get(), 101);
    assert!(host.snapshot().outputs().is_empty());
}
