use telorgon::core::RectF;
use telorgon::shell::{
    OutputId, SurfaceId, WorkspaceId, WorkspaceName, WorkspaceRevision, WorkspaceSnapshot,
    WorkspaceSnapshotError, WorkspaceSurface,
};

#[test]
fn public_workspace_snapshot_preserves_ordered_unique_host_membership() {
    let place = |surface| {
        WorkspaceSurface::new(
            SurfaceId::from_raw(surface).unwrap(),
            OutputId::from_raw(1).unwrap(),
            RectF {
                x: surface as f32 * 10.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        )
        .unwrap()
    };
    let snapshot = WorkspaceSnapshot::new(
        WorkspaceId::from_raw(1).unwrap(),
        WorkspaceRevision::from_raw(2).unwrap(),
        0,
        WorkspaceName::new("Main").unwrap(),
        true,
        vec![place(3), place(4)],
    )
    .unwrap();

    assert_eq!(snapshot.surfaces()[0].surface().get(), 3);
    assert_eq!(snapshot.surfaces()[1].surface().get(), 4);
    assert!(matches!(
        WorkspaceSnapshot::new(
            WorkspaceId::from_raw(1).unwrap(),
            WorkspaceRevision::from_raw(3).unwrap(),
            0,
            WorkspaceName::new("Main").unwrap(),
            true,
            vec![place(3), place(3)],
        ),
        Err(WorkspaceSnapshotError::DuplicateSurface { .. })
    ));
}
