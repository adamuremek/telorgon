use telorgon::shell::{
    OutputEdge, OutputId, OutputRequest, OutputRevision, ReservedAreaExtent,
    ReservedAreaExtentError, ReservedAreaId, ShellCapabilities,
};

#[test]
fn public_output_reservation_is_validated_revisioned_and_unapplied() {
    assert_eq!(
        ReservedAreaExtent::new(f32::NAN),
        Err(ReservedAreaExtentError::InvalidExtent)
    );
    let request = OutputRequest::ProposeReservedArea {
        output: OutputId::from_raw(1).unwrap(),
        revision: OutputRevision::from_raw(2).unwrap(),
        reservation: ReservedAreaId::from_raw(3).unwrap(),
        edge: OutputEdge::Bottom,
        extent: ReservedAreaExtent::new(48.0).unwrap(),
    };

    assert_eq!(request.output().get(), 1);
    assert_eq!(request.revision().get(), 2);
    assert_eq!(
        request.required_capability(),
        ShellCapabilities::RESERVE_OUTPUT_AREA
    );
}
