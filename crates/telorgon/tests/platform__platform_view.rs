use telorgon::platform::lifecycle::{
    ActivityState, NativeSurfaceState, ViewLifetime, VisibilityState,
};
use telorgon::platform::view::{
    CloseRequest, CloseRequestDecision, CloseRequestReason, ForcedDestruction,
    ForcedDestructionPhase, ViewRevision, ViewState,
};
use telorgon::platform::{NativeSurfaceGeneration, ViewId};

#[test]
fn public_view_path_publishes_atomic_snapshots_and_distinct_close_facts() {
    let view = ViewId::from_raw(3, 2).unwrap();
    let generation = NativeSurfaceGeneration::from_raw(9).unwrap();
    let mut state = ViewState::new(view);

    assert_eq!(state.snapshot().revision(), ViewRevision::INITIAL);
    state.observe_lifetime(ViewLifetime::Live).unwrap();
    state.observe_activity(ActivityState::Suspended).unwrap();
    state.observe_visibility(VisibilityState::Hidden).unwrap();
    let update = state.observe_surface_available(generation).unwrap();
    assert_eq!(update.current().view(), view);
    assert_eq!(update.current().activity(), ActivityState::Suspended);
    assert_eq!(update.current().visibility(), VisibilityState::Hidden);
    assert_eq!(
        update.current().surface(),
        NativeSurfaceState::Available { generation }
    );

    let request = CloseRequest::from_snapshot(update.current(), CloseRequestReason::System);
    let decision = CloseRequestDecision::Defer;
    let forced =
        ForcedDestruction::from_snapshot(update.current(), ForcedDestructionPhase::Destroying);
    assert_eq!(request.view(), forced.view());
    assert_eq!(request.observed_revision(), forced.observed_revision());
    assert_eq!(decision, CloseRequestDecision::Defer);
}
