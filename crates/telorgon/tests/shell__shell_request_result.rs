use telorgon::shell::{AcceptedRequestId, ShellRequestResult};

#[test]
fn public_immediate_result_distinguishes_admission_from_rejection() {
    let id = AcceptedRequestId::from_raw(95).expect("nonzero accepted request identity");
    let accepted = ShellRequestResult::accepted(id);

    assert_eq!(accepted.accepted_id(), Some(id));
    assert_eq!(ShellRequestResult::Denied.accepted_id(), None);
    assert_eq!(ShellRequestResult::Stale.accepted_id(), None);
    assert_eq!(ShellRequestResult::Unsupported.accepted_id(), None);
}
