use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_primitives::{
    EnvironmentReadBinding, EnvironmentSnapshot, EnvironmentState, EnvironmentUpdate,
    EnvironmentValues, InputCapabilities, LocaleTag,
};
use telorgon::core::SizeF;
use telorgon::input::WritingDirection;
use telorgon::runtime::{Component, CreateContext, Read, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, UiRoot};

#[derive(Clone, Debug, PartialEq)]
enum Observation {
    Geometry(f32),
    Scale(f32),
    Language(String),
    Input(u16),
    Preferences(bool),
    View(bool),
    Coherent(f32, bool),
}

struct EnvironmentHarness {
    initial: EnvironmentSnapshot,
    binding: Rc<Cell<Option<EnvironmentReadBinding>>>,
    observations: Rc<RefCell<Vec<Observation>>>,
    errors: Rc<RefCell<Vec<String>>>,
}

struct EnvironmentHarnessState {
    binding: EnvironmentReadBinding,
    coherent: Read<(f32, bool)>,
}

enum HarnessAction {
    Publish(EnvironmentUpdate),
    PublishThrough(Box<EnvironmentReadBinding>, EnvironmentUpdate),
    Observed(Observation),
}

impl Component for EnvironmentHarness {
    type State = EnvironmentHarnessState;
    type Action = HarnessAction;

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let binding = EnvironmentReadBinding::new(context, self.initial.clone()).unwrap();
        let reads = binding.reads();
        let coherent = context
            .zip(
                reads.geometry(),
                reads.preferences(),
                |geometry, preferences| {
                    (
                        geometry.available_size().width,
                        preferences.preferences().reduced_motion,
                    )
                },
            )
            .unwrap();
        self.binding.set(Some(binding));
        EnvironmentHarnessState { binding, coherent }
    }

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let reads = state.binding.reads();
        ui.observe(reads.geometry(), |value| {
            HarnessAction::Observed(Observation::Geometry(value.available_size().width))
        })
        .unwrap();
        ui.observe(reads.scale_and_density(), |value| {
            HarnessAction::Observed(Observation::Scale(value.text_scale()))
        })
        .unwrap();
        ui.observe(reads.language_and_direction(), |value| {
            HarnessAction::Observed(Observation::Language(value.locale().as_str().into()))
        })
        .unwrap();
        ui.observe(reads.input(), |value| {
            HarnessAction::Observed(Observation::Input(value.capabilities().bits()))
        })
        .unwrap();
        ui.observe(reads.preferences(), |value| {
            HarnessAction::Observed(Observation::Preferences(value.preferences().reduced_motion))
        })
        .unwrap();
        ui.observe(reads.view(), |value| {
            HarnessAction::Observed(Observation::View(value.view_state().focused))
        })
        .unwrap();
        ui.observe(state.coherent, |(width, reduced_motion)| {
            HarnessAction::Observed(Observation::Coherent(*width, *reduced_motion))
        })
        .unwrap();
        ui.foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {})
    }

    fn action(
        &self,
        state: &mut Self::State,
        action: Self::Action,
        context: &mut UpdateContext<'_, Self>,
    ) {
        match action {
            HarnessAction::Publish(update) => {
                if let Err(error) = state.binding.publish(&update, context) {
                    self.errors.borrow_mut().push(error.to_string());
                }
            }
            HarnessAction::PublishThrough(binding, update) => {
                if let Err(error) = binding.publish(&update, context) {
                    self.errors.borrow_mut().push(error.to_string());
                }
            }
            HarnessAction::Observed(observation) => {
                self.observations.borrow_mut().push(observation);
            }
        }
    }
}

fn values() -> EnvironmentValues {
    EnvironmentValues {
        available_size: SizeF {
            width: 800.0,
            height: 600.0,
        },
        locale: LocaleTag::parse("en-US").unwrap(),
        input_capabilities: InputCapabilities::MOUSE | InputCapabilities::KEYBOARD,
        ..EnvironmentValues::default()
    }
}

type HarnessParts = (
    ViewRuntime<telorgon::runtime::ComponentRuntimeDriver<EnvironmentHarness>>,
    Rc<Cell<Option<EnvironmentReadBinding>>>,
    Rc<RefCell<Vec<Observation>>>,
    Rc<RefCell<Vec<String>>>,
);

fn harness(initial: EnvironmentSnapshot) -> HarnessParts {
    let binding = Rc::new(Cell::new(None));
    let observations = Rc::new(RefCell::new(Vec::new()));
    let errors = Rc::new(RefCell::new(Vec::new()));
    let runtime = ViewRuntime::from_component(EnvironmentHarness {
        initial,
        binding: binding.clone(),
        observations: observations.clone(),
        errors: errors.clone(),
    })
    .unwrap();
    (runtime, binding, observations, errors)
}

#[test]
fn aspect_reads_invalidate_selectively_and_publish_one_coherent_snapshot() {
    let mut environment = EnvironmentState::new(values()).unwrap();
    let (mut runtime, _, observations, errors) = harness(environment.snapshot());

    let mut next = environment.values().clone();
    next.available_size.width = 720.0;
    runtime
        .send_component_action(HarnessAction::Publish(environment.update(next).unwrap()))
        .unwrap();
    assert_eq!(
        observations.take(),
        vec![
            Observation::Geometry(720.0),
            Observation::Coherent(720.0, false),
        ]
    );

    let mut next = environment.values().clone();
    next.preferences.reduced_motion = true;
    runtime
        .send_component_action(HarnessAction::Publish(environment.update(next).unwrap()))
        .unwrap();
    assert_eq!(
        observations.take(),
        vec![
            Observation::Preferences(true),
            Observation::Coherent(720.0, true),
        ]
    );

    let mut next = environment.values().clone();
    next.text_scale = 1.25;
    next.locale = LocaleTag::parse("ar-EG").unwrap();
    next.writing_direction = WritingDirection::RightToLeft;
    next.input_capabilities |= InputCapabilities::TOUCH;
    next.view.focused = false;
    runtime
        .send_component_action(HarnessAction::Publish(environment.update(next).unwrap()))
        .unwrap();
    assert_eq!(
        observations.take(),
        vec![
            Observation::Scale(1.25),
            Observation::Language("ar-EG".into()),
            Observation::Input(
                (InputCapabilities::MOUSE | InputCapabilities::KEYBOARD | InputCapabilities::TOUCH)
                    .bits(),
            ),
            Observation::View(false),
        ]
    );

    runtime
        .send_component_action(HarnessAction::Publish(
            environment.update(environment.values().clone()).unwrap(),
        ))
        .unwrap();
    assert!(observations.borrow().is_empty());
    assert!(errors.borrow().is_empty());
}

#[test]
fn stale_skipped_and_cross_view_publications_are_rejected_without_read_changes() {
    let mut environment = EnvironmentState::new(values()).unwrap();
    let initial = environment.snapshot();
    let (mut first, first_binding, first_observations, first_errors) = harness(initial.clone());

    let mut next = environment.values().clone();
    next.available_size.width = 700.0;
    let first_update = environment.update(next).unwrap();
    first
        .send_component_action(HarnessAction::Publish(first_update.clone()))
        .unwrap();
    first_observations.take();

    first
        .send_component_action(HarnessAction::Publish(first_update))
        .unwrap();
    assert!(first_errors.borrow()[0].contains("change set"));
    assert!(first_observations.borrow().is_empty());

    let mut next = environment.values().clone();
    next.available_size.width = 690.0;
    environment.update(next).unwrap();
    let mut next = environment.values().clone();
    next.available_size.width = 680.0;
    let skipped = environment.update(next).unwrap();
    first
        .send_component_action(HarnessAction::Publish(skipped.clone()))
        .unwrap();
    assert!(
        first_errors
            .borrow()
            .iter()
            .any(|error| error.contains("does not continue"))
    );
    assert!(first_observations.borrow().is_empty());

    let (mut second, _, second_observations, second_errors) = harness(initial);
    second
        .send_component_action(HarnessAction::PublishThrough(
            Box::new(first_binding.get().unwrap()),
            skipped,
        ))
        .unwrap();
    assert!(second_errors.borrow()[0].contains("non-owner"));
    assert!(second_observations.borrow().is_empty());
}
