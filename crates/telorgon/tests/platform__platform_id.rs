use telorgon::platform::id::{DataOfferId, NativeSurfaceGeneration, RequestId, ViewId};

#[test]
fn public_platform_identities_are_typed_nonzero_and_generation_aware() {
    let view = ViewId::from_raw(1, 2).expect("nonzero view slot and generation");
    let offer = DataOfferId::from_raw(1, 2).expect("nonzero offer slot and generation");
    let request = RequestId::from_raw(3).expect("nonzero request sequence");
    let surface = NativeSurfaceGeneration::from_raw(4).expect("nonzero surface generation");

    assert_eq!((view.slot(), view.generation()), (1, 2));
    assert_eq!((offer.slot(), offer.generation()), (1, 2));
    assert_eq!(request.get(), 3);
    assert_eq!(surface.get(), 4);
    assert_eq!(ViewId::from_raw(0, 2), None);
    assert_eq!(DataOfferId::from_raw(1, 0), None);
}
