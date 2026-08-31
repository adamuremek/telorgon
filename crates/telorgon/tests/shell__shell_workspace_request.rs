use telorgon::shell::{
    InputSource, ShellCapabilities, SurfaceId, WorkspaceId, WorkspaceName, WorkspaceRequest,
    WorkspaceRevision,
};

#[test]
fn public_workspace_requests_preserve_revisions_order_and_authority() {
    let source = WorkspaceId::from_raw(1).unwrap();
    let destination = WorkspaceId::from_raw(2).unwrap();
    let request = WorkspaceRequest::MoveSurface {
        surface: SurfaceId::from_raw(3).unwrap(),
        from: source,
        from_revision: WorkspaceRevision::from_raw(4).unwrap(),
        to: destination,
        to_revision: WorkspaceRevision::from_raw(5).unwrap(),
    };

    assert_eq!(
        request.required_capability(),
        ShellCapabilities::MANAGE_WORKSPACES
    );
    assert!(matches!(
        request,
        WorkspaceRequest::MoveSurface {
            from,
            from_revision,
            to,
            to_revision,
            ..
        } if from == source
            && from_revision.get() == 4
            && to == destination
            && to_revision.get() == 5
    ));

    let select = WorkspaceRequest::Select {
        workspace: destination,
        revision: WorkspaceRevision::from_raw(5).unwrap(),
        source: InputSource::Keyboard,
    };
    assert_eq!(
        select.required_capability(),
        ShellCapabilities::SELECT_WORKSPACE
    );

    let create = WorkspaceRequest::Create {
        name: WorkspaceName::new("Public fixture").unwrap(),
        order: 6,
    };
    assert_eq!(create.observed_workspace(), None);
}
