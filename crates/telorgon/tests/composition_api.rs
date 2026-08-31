use telorgon::app::*;
use telorgon::{ChangeSource, NodeKind, SemanticCheckState, ValueChangePhase, ViewRuntime};

#[component]
struct Counter {
    #[input]
    title: String,
    #[state]
    count: u32,
}

impl Counter {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            count: 0,
        }
    }
}

impl Component for Counter {
    fn view(&self) -> impl View {
        column()
            .gap(16.0)
            .padding(24.0)
            .child(text(&self.title).size(20.0))
            .child(
                text(format!("Count: {}", self.count))
                    .style(TextStyle::new().size(32.0).weight(600)),
            )
            .child(
                button("Increment")
                    .primary()
                    .on_press(|this: &mut Self| this.count += 1),
            )
    }
}

#[component]
struct DefaultCounter {
    #[state]
    count: u32,
}

impl Component for DefaultCounter {
    fn view(&self) -> impl View {
        text(format!("Count: {}", self.count))
    }
}

#[test]
fn component_derives_default_automatically() {
    let runtime = ViewRuntime::from_composed(DefaultCounter::default()).unwrap();
    assert!(
        runtime
            .ui()
            .texts
            .values()
            .iter()
            .any(|value| runtime.ui().string(value.content) == Some("Count: 0"))
    );
}

#[component]
struct ForeignCallbackOwner {}

impl Component for ForeignCallbackOwner {
    fn view(&self) -> impl View {
        text("foreign")
    }
}

#[component]
struct InvalidCallbackOwner {}

impl Component for InvalidCallbackOwner {
    fn view(&self) -> impl View {
        button("Invalid").on_press(|_: &mut ForeignCallbackOwner| {})
    }
}

#[test]
fn callback_type_mismatches_are_rejected_before_mount() {
    let error = match ViewRuntime::from_composed(InvalidCallbackOwner::default()) {
        Ok(_) => panic!("mismatched callback owner should fail"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("callback component type mismatch")
    );
}

#[test]
fn counter_rerenders_dynamic_text_without_remounting_the_button() {
    let mut runtime = ViewRuntime::from_composed(Counter::new("Counter")).unwrap();
    let button = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| runtime.ui().kinds.get(*node) == Some(&NodeKind::Button))
        .unwrap();
    let count_text = runtime
        .ui()
        .texts
        .values()
        .iter()
        .find(|text| runtime.ui().string(text.content) == Some("Count: 0"))
        .copied()
        .unwrap();
    let count_node = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| runtime.ui().texts.get(*node) == Some(&count_text))
        .unwrap();

    assert_eq!(count_text.style.size, 32.0);
    assert_eq!(count_text.style.weight, 600);

    assert!(runtime.dispatch_activation(button, ChangeSource::Programmatic));

    assert!(runtime.ui().nodes.contains(button));
    assert_eq!(
        runtime
            .ui()
            .texts
            .get(count_node)
            .and_then(|text| runtime.ui().string(text.content)),
        Some("Count: 1")
    );
    let diagnostics = runtime.composition_diagnostics();
    assert_eq!(diagnostics.events_delivered, 1);
    assert!(diagnostics.elements_reused >= 3);
}

#[test]
fn sealed_application_declaration_owns_initial_content() {
    let application = Application::gui("Counter").renderer(Renderer::Auto).window(
        Window::new("Counter")
            .size(480, 320)
            .content(Counter::new("Example")),
    );
    let debug = format!("{application:?}");
    assert!(debug.contains("has_content: true"));
    assert!(debug.contains("renderer: Auto"));
}

#[test]
fn app_facade_includes_common_composition_styling() {
    let style = BoxStyle {
        width: SizeRule::Fill(1.0),
        background: Background::Color(ColorRgba8::rgba(20, 22, 28, 255)),
        ..BoxStyle::default()
    };

    assert!(matches!(style.width, SizeRule::Fill(1.0)));
}

#[component]
struct Controls {
    #[state]
    first: bool,
    #[state]
    second: bool,
    #[state]
    level: f32,
}

impl Component for Controls {
    fn view(&self) -> impl View {
        column()
            .child(
                checkbox("First", self.first).on_change(|this: &mut Self, checked| {
                    this.first = checked;
                }),
            )
            .child(
                switch("Second", self.second).on_change(|this: &mut Self, checked| {
                    this.second = checked;
                }),
            )
            .child(
                slider("Level", self.level).on_change(|this: &mut Self, value| {
                    this.level = value;
                }),
            )
    }
}

#[test]
fn controls_reconcile_independently_and_slider_thumb_tracks_the_value() {
    let mut runtime = ViewRuntime::from_composed(Controls {
        first: false,
        second: true,
        level: 0.25,
    })
    .unwrap();
    let toggles: Vec<_> = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .filter(|node| runtime.ui().kinds.get(*node) == Some(&NodeKind::Toggle))
        .collect();
    assert_eq!(toggles.len(), 2);
    let slider = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| runtime.ui().kinds.get(*node) == Some(&NodeKind::Slider))
        .unwrap();

    let checkbox_style = telorgon::ComponentStyleId::named(
        telorgon::ThemeDomainId::APPLICATION,
        "checkbox",
        "default",
    );
    let switch_style = telorgon::ComponentStyleId::named(
        telorgon::ThemeDomainId::APPLICATION,
        "switch",
        "default",
    );
    let slider_style = telorgon::ComponentStyleId::named(
        telorgon::ThemeDomainId::APPLICATION,
        "slider",
        "default",
    );
    assert!(runtime.ui().style_bindings().iter().any(|binding| {
        binding.state_root == toggles[0]
            && binding.component_style == checkbox_style
            && binding.slots.len() == 6
    }));
    assert!(runtime.ui().style_bindings().iter().any(|binding| {
        binding.state_root == toggles[1]
            && binding.component_style == switch_style
            && binding.slots.len() == 4
    }));
    assert!(runtime.ui().style_bindings().iter().any(|binding| {
        binding.state_root == slider
            && binding.component_style == slider_style
            && binding.slots.len() == 5
    }));

    assert!(runtime.dispatch_activation(toggles[0], ChangeSource::Pointer));
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(toggles[0])
            .unwrap()
            .state
            .checked,
        Some(SemanticCheckState::Checked)
    );
    assert_eq!(
        runtime
            .ui()
            .semantics
            .get(toggles[1])
            .unwrap()
            .state
            .checked,
        Some(SemanticCheckState::Checked)
    );

    assert!(runtime.dispatch_value(
        slider,
        0.75,
        ValueChangePhase::Commit,
        ChangeSource::Pointer,
    ));
    assert_eq!(runtime.ui().interactions.get(slider).unwrap().value, 0.75);
}

#[component(no_default)]
struct SignalLabel {
    #[input]
    value: Signal<u32>,
}

impl Component for SignalLabel {
    fn view(&self) -> impl View {
        let value = self.watch(&self.value);
        text(format!("External: {}", *value))
    }
}

#[test]
fn watched_signal_invalidates_only_after_a_changed_publication() {
    let (value, writer) = Signal::new(4_u32);
    let mut runtime = ViewRuntime::from_composed(SignalLabel { value }).unwrap();
    let label = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| {
            runtime
                .ui()
                .texts
                .get(*node)
                .and_then(|text| runtime.ui().string(text.content))
                == Some("External: 4")
        })
        .unwrap();

    writer.publish_if_changed(4);
    assert!(!runtime.external_updates_ready());

    writer.publish(9);
    assert!(runtime.external_updates_ready());
    assert_eq!(runtime.process_external_updates(), 1);
    assert!(!runtime.external_updates_ready());
    assert_eq!(
        runtime
            .ui()
            .texts
            .get(label)
            .and_then(|text| runtime.ui().string(text.content)),
        Some("External: 9")
    );
    let diagnostics = runtime.composition_diagnostics();
    assert_eq!(diagnostics.externally_invalidated_components, 1);
    assert_eq!(diagnostics.externally_reconciled_components, 1);
}

#[component]
struct KeyedItem {
    #[input]
    name: String,
    #[state]
    count: u32,
}

impl KeyedItem {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            count: 0,
        }
    }
}

impl Component for KeyedItem {
    fn view(&self) -> impl View {
        button(format!("{}: {}", self.name, self.count)).on_press(|this: &mut Self| this.count += 1)
    }
}

#[component]
struct KeyedList {
    #[state]
    reversed: bool,
}

impl Component for KeyedList {
    fn view(&self) -> impl View {
        let first = if self.reversed { "B" } else { "A" };
        let second = if self.reversed { "A" } else { "B" };
        column()
            .child(button("Reverse").on_press(|this: &mut Self| this.reversed = !this.reversed))
            .child(KeyedItem::new(first).keyed(first))
            .child(KeyedItem::new(second).keyed(second))
    }
}

#[test]
fn keyed_component_reorder_preserves_local_state_and_control_identity() {
    let mut runtime = ViewRuntime::from_composed(KeyedList { reversed: false }).unwrap();
    let named_button = |runtime: &ViewRuntime<telorgon::CompositionDriver>, name: &str| {
        runtime
            .ui()
            .semantics
            .iter()
            .find_map(|(node, semantic)| {
                let telorgon::SemanticName::Text(text) = semantic.name else {
                    return None;
                };
                (semantic.role == telorgon::SemanticRole::Button
                    && runtime.ui().string(text) == Some(name))
                .then_some(node)
            })
            .unwrap()
    };

    let item_a = named_button(&runtime, "A: 0");
    assert!(runtime.dispatch_activation(item_a, ChangeSource::Programmatic));
    let item_a = named_button(&runtime, "A: 1");
    let reverse = named_button(&runtime, "Reverse");
    assert!(runtime.dispatch_activation(reverse, ChangeSource::Programmatic));

    assert!(runtime.ui().nodes.contains(item_a));
    assert_eq!(named_button(&runtime, "A: 1"), item_a);
    assert_eq!(runtime.composition_diagnostics().live_components, 3);
}

#[component]
struct MountedUpdate {
    #[state]
    value: u32,
}

impl Component for MountedUpdate {
    fn mounted(&mut self, cx: &mut telorgon::MountContext<Self>) {
        self.value = 7;
        cx.request_update();
    }

    fn view(&self) -> impl View {
        text(format!("Mounted: {}", self.value))
    }
}

#[test]
fn mounted_lifecycle_can_request_one_coalesced_followup_view() {
    let runtime = ViewRuntime::from_composed(MountedUpdate { value: 0 }).unwrap();
    assert!(
        runtime
            .ui()
            .texts
            .values()
            .iter()
            .any(|visual| { runtime.ui().string(visual.content) == Some("Mounted: 7") })
    );
    assert_eq!(runtime.composition_diagnostics().view_evaluations, 2);
}

#[component]
struct InvalidInitialView {
    #[state]
    unused: bool,
}

impl Component for InvalidInitialView {
    fn view(&self) -> impl View {
        button("")
    }
}

#[test]
fn invalid_initial_view_returns_an_error_instead_of_panicking() {
    let result = ViewRuntime::from_composed(InvalidInitialView { unused: false });
    assert!(result.is_err());
}

#[component]
struct InputAuthority {
    #[input]
    title: String,
    #[state]
    attempts: u32,
}

impl Component for InputAuthority {
    fn view(&self) -> impl View {
        column()
            .child(text(&self.title))
            .child(button("Attempt input write").on_press(|this: &mut Self| {
                this.title = "child-owned".to_owned();
                this.attempts += 1;
            }))
            .child(text(format!("Attempts: {}", self.attempts)))
    }
}

#[test]
fn event_callbacks_cannot_take_authority_over_input_fields() {
    let mut runtime = ViewRuntime::from_composed(InputAuthority {
        title: "parent-owned".to_owned(),
        attempts: 0,
    })
    .unwrap();
    let button = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| runtime.ui().kinds.get(*node) == Some(&NodeKind::Button))
        .unwrap();

    assert!(runtime.dispatch_activation(button, ChangeSource::Programmatic));
    let copy = runtime
        .ui()
        .texts
        .values()
        .iter()
        .filter_map(|visual| runtime.ui().string(visual.content))
        .collect::<Vec<_>>();
    assert!(copy.contains(&"parent-owned"));
    assert!(copy.contains(&"Attempts: 1"));
    assert!(!copy.contains(&"child-owned"));
    assert_eq!(
        runtime.composition_diagnostics().input_mutations_restored,
        1
    );
}
