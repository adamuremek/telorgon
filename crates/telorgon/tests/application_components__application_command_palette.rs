use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ApplicationOverlayController, CommandPalette, CommandPaletteMatchKind,
    CommandSpec,
};
use telorgon::input::{ChangeSource, CompositeNavigationCommand, WritingDirection};
use telorgon::runtime::{Component, CreateContext, State, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, OverlayAnchor, UiNodeId, UiRoot};

#[derive(Debug, PartialEq, Eq)]
struct NonCloneAction {
    command: u32,
    source: ChangeSource,
}

struct Fixture {
    palette: Rc<RefCell<Option<CommandPalette<u32, NonCloneAction>>>>,
    overlays: Rc<RefCell<ApplicationOverlayController>>,
    anchor: Rc<Cell<Option<UiNodeId>>>,
}

struct FixtureState {
    _enabled: State<bool>,
    _disabled: State<bool>,
}

impl Component for Fixture {
    type State = FixtureState;
    type Action = ();

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        let disabled = context.state(false);
        let owner = context.component();
        let command = |id, label, availability| {
            CommandSpec::new(
                id,
                label,
                availability,
                ActionFactory::new(owner, move |source| NonCloneAction {
                    command: id,
                    source,
                }),
            )
            .unwrap()
        };
        self.palette.replace(Some(
            CommandPalette::new(
                "Document commands",
                [
                    command(1, "Open File", enabled.read()),
                    command(2, "Open Folder", disabled.read()),
                    command(3, "Reopen Closed File", enabled.read()),
                ],
            )
            .unwrap(),
        ));
        FixtureState {
            _enabled: enabled,
            _disabled: disabled,
        }
    }

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        self.palette
            .borrow_mut()
            .as_mut()
            .unwrap()
            .refresh(ui)
            .unwrap();
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        self.overlays.borrow_mut().mount(ui, root.0).unwrap();
        self.anchor.set(Some(root.0));
        root
    }

    fn action(
        &self,
        _state: &mut Self::State,
        _action: Self::Action,
        _context: &mut UpdateContext<'_, Self>,
    ) {
    }
}

#[test]
fn public_command_palette_ranks_navigates_and_closes_before_fresh_action_intent() {
    let palette = Rc::new(RefCell::new(None));
    let overlays = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let anchor = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(Fixture {
        palette: palette.clone(),
        overlays: overlays.clone(),
        anchor: anchor.clone(),
    })
    .unwrap();
    let node_count = runtime.ui().nodes.alive().len();
    let mut borrowed = palette.borrow_mut();
    let palette = borrowed.as_mut().unwrap();

    palette.set_query("open").unwrap();
    assert_eq!(
        palette
            .results()
            .iter()
            .map(|result| (result.command, result.enabled, result.rank.kind))
            .collect::<Vec<_>>(),
        vec![
            (1, true, CommandPaletteMatchKind::Prefix),
            (2, false, CommandPaletteMatchKind::Prefix),
            (3, true, CommandPaletteMatchKind::Substring),
        ]
    );
    palette
        .navigate(
            CompositeNavigationCommand::Down,
            WritingDirection::LeftToRight,
        )
        .unwrap();
    assert_eq!(palette.highlighted_command(), Some(3));

    let opened = palette
        .open(
            &mut overlays.borrow_mut(),
            runtime.ui(),
            OverlayAnchor::Node(anchor.get().unwrap()),
        )
        .unwrap();
    assert_eq!(runtime.ui().nodes.alive().len(), node_count);
    assert!(overlays.borrow().state().background_is_inert);

    let intent = palette
        .activate(&mut overlays.borrow_mut(), ChangeSource::Accessibility)
        .unwrap();
    assert_eq!(intent.command(), &3);
    assert_eq!(intent.source(), ChangeSource::Accessibility);
    assert_eq!(intent.close_effect().dismissed[0].id, opened.overlay);
    assert_eq!(overlays.borrow().state().entry_count, 0);
    assert_eq!(
        intent.into_action(),
        NonCloneAction {
            command: 3,
            source: ChangeSource::Accessibility,
        }
    );
}
