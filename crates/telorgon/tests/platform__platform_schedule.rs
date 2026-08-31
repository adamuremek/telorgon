use telorgon::platform::schedule::{PendingHostFacts, PostTurnSchedule, RemainingWork};
use telorgon::platform::{MonotonicInstant, ViewId};

fn view(slot: u32, generation: u32) -> ViewId {
    ViewId::from_raw(slot, generation).unwrap()
}

#[test]
fn public_schedule_path_normalizes_views_and_merges_policy_free_facts() {
    let first = PostTurnSchedule::new(
        RemainingWork::new(true, false, false, false),
        &[view(3, 1), view(1, 1), view(3, 1)],
        Some(MonotonicInstant::from_nanos(40)),
        PendingHostFacts::new(true, false),
    )
    .unwrap();
    let second = PostTurnSchedule::new(
        RemainingWork::new(false, false, true, true),
        &[view(2, 1)],
        Some(MonotonicInstant::from_nanos(30)),
        PendingHostFacts::new(false, true),
    )
    .unwrap();

    assert_eq!(first.redraw_views(), &[view(1, 1), view(3, 1)]);
    let merged = first.merged(&second).unwrap();
    assert_eq!(merged.redraw_views(), &[view(1, 1), view(2, 1), view(3, 1)]);
    assert_eq!(
        merged.remaining_work(),
        RemainingWork::new(true, false, true, true)
    );
    assert_eq!(
        merged.next_deadline(),
        Some(MonotonicInstant::from_nanos(30))
    );
    assert_eq!(merged.pending_host(), PendingHostFacts::new(true, true));
}
