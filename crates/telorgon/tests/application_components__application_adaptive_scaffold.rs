use telorgon::application_components::structure::{
    AdaptiveScaffold, AdaptiveSlotPresentation, AdaptiveWidthClass, Scaffold, ScaffoldSlot,
    ScaffoldSlotSpec,
};
use telorgon::application_primitives::{EnvironmentState, EnvironmentValues, InputCapabilities};
use telorgon::core::SizeF;

fn slot(slot: ScaffoldSlot, label: &str) -> ScaffoldSlotSpec {
    ScaffoldSlotSpec::new(slot, label).unwrap()
}

#[test]
fn public_adaptive_scaffold_reports_environment_derived_presentations() {
    let scaffold = Scaffold::new(
        "Workspace",
        [
            slot(ScaffoldSlot::Secondary, "Inspector"),
            slot(ScaffoldSlot::Content, "Document"),
            slot(ScaffoldSlot::Navigation, "Files"),
        ],
    )
    .unwrap();
    let adaptive = AdaptiveScaffold::new(scaffold);
    let environment = EnvironmentState::new(EnvironmentValues {
        available_size: SizeF {
            width: 1_280.0,
            height: 800.0,
        },
        input_capabilities: InputCapabilities::MOUSE | InputCapabilities::KEYBOARD,
        ..EnvironmentValues::default()
    })
    .unwrap();

    let plan = adaptive.plan(&environment.snapshot());
    assert_eq!(plan.width_class(), AdaptiveWidthClass::Expanded);
    assert_eq!(
        plan.presentation(ScaffoldSlot::Navigation),
        Some(AdaptiveSlotPresentation::NavigationRail)
    );
    assert_eq!(
        plan.presentation(ScaffoldSlot::Secondary),
        Some(AdaptiveSlotPresentation::SecondaryAlongside)
    );
    assert_eq!(plan.slots().len(), 3);
}
