use telorgon::platform::NativeSurfaceGeneration;
use telorgon::platform::lifecycle::{
    ActivityState, LifecycleError, NativeSurfaceState, ViewLifecycle, ViewLifetime, VisibilityState,
};

fn generation(value: u64) -> NativeSurfaceGeneration {
    NativeSurfaceGeneration::from_raw(value).unwrap()
}

#[test]
fn public_lifecycle_path_keeps_axes_independent_and_surface_generations_fresh() {
    let mut lifecycle = ViewLifecycle::new();
    lifecycle.observe_lifetime(ViewLifetime::Live).unwrap();
    lifecycle.observe_surface_available(generation(4)).unwrap();
    lifecycle
        .observe_activity(ActivityState::Suspended)
        .unwrap();
    lifecycle
        .observe_visibility(VisibilityState::Hidden)
        .unwrap();

    assert_eq!(lifecycle.activity(), ActivityState::Suspended);
    assert_eq!(lifecycle.visibility(), VisibilityState::Hidden);
    assert_eq!(
        lifecycle.surface(),
        NativeSurfaceState::Available {
            generation: generation(4),
        }
    );

    lifecycle.observe_surface_unavailable().unwrap();
    assert_eq!(
        lifecycle.observe_surface_available(generation(4)),
        Err(LifecycleError::SurfaceGenerationDidNotAdvance {
            previous: generation(4),
            observed: generation(4),
        })
    );
    lifecycle.observe_surface_available(generation(5)).unwrap();
    assert_eq!(lifecycle.last_surface_generation(), Some(generation(5)));
}
