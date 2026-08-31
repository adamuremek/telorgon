use telorgon::application_components::prelude::*;
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::SizeF;

#[test]
fn component_values_follow_the_neutral_environment_density() {
    let environment = EnvironmentValues::default();
    let metrics = DensityMetrics::baseline(environment.density);
    let assessment = metrics
        .assess(SizeF {
            width: 32.0,
            height: 32.0,
        })
        .unwrap();

    assert_eq!(metrics.class(), DensityClass::Standard);
    assert!(assessment.meets_minimum);

    let requested = ValueChange::committed(true, ChangeSource::Keyboard);
    assert_eq!(requested.phase, ChangePhase::Commit);
    assert!(requested.value);
}
