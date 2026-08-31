use telorgon::application_components::prelude::{
    ChangePhase, ChangeSource, LabelContent, SelectableTextBehavior,
};
use telorgon::text::{TextAffinity, TextOffset, TextSelection};

#[test]
fn public_selectable_text_emits_only_valid_source_preserving_proposals() {
    let behavior = SelectableTextBehavior::new(LabelContent::new("Résumé", 25).unwrap()).unwrap();
    let current = TextSelection::collapsed(TextOffset::ZERO, TextAffinity::Downstream);
    let requested = TextSelection {
        anchor: TextOffset::ZERO,
        active: TextOffset(8),
        affinity: TextAffinity::Downstream,
    };
    let proposal = behavior
        .request(current, requested, ChangeSource::Accessibility)
        .unwrap()
        .unwrap();

    assert_eq!(proposal.value, requested);
    assert_eq!(proposal.phase, ChangePhase::Commit);
    assert_eq!(proposal.source, ChangeSource::Accessibility);
    assert_eq!(current.active, TextOffset::ZERO);
}
