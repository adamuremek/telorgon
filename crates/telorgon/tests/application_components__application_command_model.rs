use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ChangeSource, CheckState, CommandInvocation, CommandSpec, ResolvedCommandState,
};
use telorgon::runtime::{
    Component, CreateContext, NoAction, State, Ui, UpdateContext, ViewRuntime,
};
use telorgon::ui::{BoxStyle, LayoutStyle, UiRoot};

#[derive(Debug, PartialEq, Eq)]
struct SaveRequested {
    source: ChangeSource,
}

struct CommandFixture {
    resolved: Rc<RefCell<Option<ResolvedCommandState>>>,
    invoked: Rc<RefCell<Option<CommandInvocation<SaveRequested>>>>,
}

struct CommandFixtureState {
    command: CommandSpec<u32, SaveRequested>,
    _enabled: State<bool>,
    _checked: State<CheckState>,
}

impl Component for CommandFixture {
    type State = CommandFixtureState;
    type Action = NoAction;

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        let checked = context.state(CheckState::Checked);
        let command = CommandSpec::new(
            17,
            "Save",
            enabled.read(),
            ActionFactory::new(context.component(), |source| SaveRequested { source }),
        )
        .unwrap()
        .description("Persist the current document")
        .unwrap()
        .checked(checked.read())
        .unwrap();
        CommandFixtureState {
            command,
            _enabled: enabled,
            _checked: checked,
        }
    }

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let resolved = state.command.resolve_state(ui).unwrap();
        self.resolved.replace(Some(resolved));
        self.invoked
            .replace(Some(state.command.invoke(resolved, ChangeSource::Keyboard)));
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
fn public_command_model_resolves_controlled_state_and_builds_one_fresh_action() {
    let resolved = Rc::new(RefCell::new(None));
    let invoked = Rc::new(RefCell::new(None));
    let _runtime = ViewRuntime::from_component(CommandFixture {
        resolved: resolved.clone(),
        invoked: invoked.clone(),
    })
    .unwrap();

    assert_eq!(
        *resolved.borrow(),
        Some(ResolvedCommandState::new(true, Some(CheckState::Checked)))
    );
    assert_eq!(
        *invoked.borrow(),
        Some(CommandInvocation::Invoked {
            action: SaveRequested {
                source: ChangeSource::Keyboard,
            },
            source: ChangeSource::Keyboard,
            checked: Some(CheckState::Checked),
        })
    );
}
