use telorgon::shell::{
    ContactId, InputSource, ResizeEdge, ShellCapabilities, SurfaceCapabilities, SurfaceId,
    SurfaceRequest,
};

#[test]
fn public_surface_requests_retain_intent_and_required_authority() {
    let surface = SurfaceId::from_raw(1).unwrap();
    let contact = ContactId::from_raw(2).unwrap();
    let resize = SurfaceRequest::BeginResize {
        surface,
        edge: ResizeEdge::BottomRight,
        contact,
    };

    assert_eq!(resize.surface(), surface);
    assert_eq!(resize.contact(), Some(contact));
    assert_eq!(
        resize.required_shell_capability(),
        ShellCapabilities::RESIZE_SURFACE
    );
    assert_eq!(
        resize.required_surface_capability(),
        SurfaceCapabilities::RESIZE
    );

    let activate = SurfaceRequest::Activate {
        surface,
        source: InputSource::Keyboard,
    };
    assert_eq!(activate.input_source(), Some(InputSource::Keyboard));
}
