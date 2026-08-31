use telorgon::core::PointF;
use telorgon::input::{ButtonState, PointerButton};
use telorgon::shell::{
    ClientInputRequest, ContactId, InputSource, SeatId, SurfaceId, SurfaceInputContact,
    SurfaceInputEvent, SurfaceInputKind,
};

#[test]
fn public_client_input_retains_neutral_contact_and_local_event() {
    let contact = SurfaceInputContact::new(
        SeatId::from_raw(1).unwrap(),
        ContactId::from_raw(2).unwrap(),
        InputSource::Mouse,
    )
    .unwrap();
    let event = SurfaceInputEvent::button(
        contact,
        PointF { x: 30.0, y: 40.0 },
        PointerButton::PRIMARY,
        ButtonState::Released,
    )
    .unwrap();
    let request = ClientInputRequest::new(SurfaceId::from_raw(3).unwrap(), event);

    assert_eq!(request.event().kind(), SurfaceInputKind::Button);
    assert_eq!(request.event().contact().seat().get(), 1);
    assert_eq!(request.event().contact().contact().get(), 2);
    assert_eq!(request.event().position().unwrap().y, 40.0);
}
