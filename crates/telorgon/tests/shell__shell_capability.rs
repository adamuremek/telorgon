use telorgon::shell::{
    LayerAuthorityError, OutputId, ShellCapabilities, ShellCapabilityGrant, ShellGrantToken,
    ShellLayerKind,
};

#[test]
fn public_grants_narrow_layer_authority_to_the_host_capability_set() {
    let output = OutputId::from_raw(4).unwrap();
    let grant = ShellCapabilityGrant::from_host(
        ShellGrantToken::from_raw(8).unwrap(),
        output,
        ShellCapabilities::WORKSPACE_LAYER
            | ShellCapabilities::OVERLAY_LAYER
            | ShellCapabilities::ACTIVATE_SURFACE,
    );

    let overlay = grant.authorize_layer(ShellLayerKind::Overlay).unwrap();
    assert_eq!(overlay.output(), output);
    assert_eq!(overlay.layer(), ShellLayerKind::Overlay);
    assert!(grant.permits(ShellCapabilities::ACTIVATE_SURFACE));
    assert_eq!(
        grant.authorize_layer(ShellLayerKind::Cursor),
        Err(LayerAuthorityError::NotGranted {
            layer: ShellLayerKind::Cursor,
        })
    );
}
