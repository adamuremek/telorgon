use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, CommandShortcut, CommandShortcutOutcome, CommandShortcutScope, CommandSpec,
    ShortcutDisplayBinding, ShortcutSet,
};
use telorgon::input::{
    ActiveShortcutScope, ButtonState, KeyEvent, Modifiers, PhysicalKey, ShortcutChord,
    ShortcutResolution, ShortcutScopeId,
};
use telorgon::runtime::{
    Component, CreateContext, NoAction, State, Ui, UpdateContext, ViewRuntime,
};
use telorgon::ui::{BoxStyle, LayoutStyle, UiRoot};

struct ShortcutFixture {
    outcome: Rc<RefCell<Option<CommandShortcutOutcome<u32, u32>>>>,
    factory_calls: Rc<Cell<u32>>,
}

struct ShortcutFixtureState {
    command: CommandSpec<u32, ()>,
    _enabled: State<bool>,
}

impl Component for ShortcutFixture {
    type State = ShortcutFixtureState;
    type Action = NoAction;

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        let factory_calls = self.factory_calls.clone();
        let shortcut = CommandShortcut::new(
            ShortcutChord::pressed(PhysicalKey::new(31), Modifiers::CONTROL),
            ShortcutDisplayBinding::new("Ctrl+S").unwrap(),
        );
        let command = CommandSpec::new(
            7,
            "Save",
            enabled.read(),
            ActionFactory::new(context.component(), move |_| {
                factory_calls.set(factory_calls.get() + 1)
            }),
        )
        .unwrap()
        .shortcuts(ShortcutSet::single(shortcut));
        ShortcutFixtureState {
            command,
            _enabled: enabled,
        }
    }

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let scope_id = ShortcutScopeId::from_raw(1, 1).unwrap();
        let mut scope = CommandShortcutScope::new();
        scope
            .update_controlled(
                ui,
                [state
                    .command
                    .shortcut_registration(11, scope_id, 0)
                    .unwrap()],
            )
            .unwrap();
        let outcome = scope
            .resolve(
                KeyEvent {
                    physical_key: PhysicalKey::new(31),
                    state: ButtonState::Pressed,
                    repeat: false,
                    modifiers: Modifiers::CONTROL,
                    ..KeyEvent::new(PhysicalKey::new(31), ButtonState::Pressed)
                },
                [ActiveShortcutScope::bubble(scope_id)],
            )
            .unwrap();
        self.outcome.replace(Some(outcome));
        ui.foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {})
    }

    fn action(
        &self,
        _state: &mut Self::State,
        action: Self::Action,
        _context: &mut UpdateContext<'_, Self>,
    ) {
        match action {}
    }
}

#[test]
fn public_command_shortcut_scope_resolves_without_invoking_the_command() {
    let outcome = Rc::new(RefCell::new(None));
    let factory_calls = Rc::new(Cell::new(0));
    let _runtime = ViewRuntime::from_component(ShortcutFixture {
        outcome: outcome.clone(),
        factory_calls: factory_calls.clone(),
    })
    .unwrap();

    let outcome = outcome.borrow();
    assert!(matches!(
        outcome.as_ref().unwrap().resolution(),
        ShortcutResolution::Matched {
            binding: 11,
            command: 7,
            ..
        }
    ));
    assert_eq!(
        outcome.as_ref().unwrap().display_binding().unwrap().label(),
        "Ctrl+S"
    );
    assert_eq!(factory_calls.get(), 0);
}
