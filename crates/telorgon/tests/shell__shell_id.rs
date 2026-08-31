use telorgon::shell::{ApplicationId, OutputId, SurfaceId, WorkspaceId};

#[test]
fn public_shell_identities_are_nonzero_and_domain_distinct() {
    let output = OutputId::from_raw(1).expect("nonzero output identity");
    let surface = SurfaceId::from_raw(1).expect("nonzero surface identity");
    let workspace = WorkspaceId::from_raw(1).expect("nonzero workspace identity");
    let application = ApplicationId::from_raw(1).expect("nonzero application identity");

    assert_eq!(output.get(), 1);
    assert_eq!(surface.get(), 1);
    assert_eq!(workspace.get(), 1);
    assert_eq!(application.get(), 1);
    assert_eq!(OutputId::from_raw(0), None);
}
