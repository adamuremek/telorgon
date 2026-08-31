use telorgon::application_primitives::prelude::{
    RenderTargetToken, RenderTargetView, RenderTargetViewContent, RenderTargetViewError,
    RenderTargetViewSemanticPolicy,
};

#[test]
fn public_render_target_view_keeps_host_identity_revision_and_semantics_explicit() {
    assert_eq!(RenderTargetToken::new(0), None);
    let content = RenderTargetViewContent::new(RenderTargetToken::new(83).unwrap(), 11).unwrap();
    let view = RenderTargetView::described(content, "Host-rendered editor viewport").unwrap();

    assert_eq!(view.content(), content);
    assert_eq!(view.content().target().get(), 83);
    assert_eq!(view.content().content_version(), 11);
    assert_eq!(
        view.semantic_policy(),
        RenderTargetViewSemanticPolicy::Described
    );
    assert_eq!(
        RenderTargetViewContent::new(RenderTargetToken::new(83).unwrap(), 0),
        Err(RenderTargetViewError::ZeroContentVersion)
    );
}
