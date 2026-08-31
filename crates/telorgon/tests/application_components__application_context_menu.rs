use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ApplicationOverlayController, CommandSpec, ContextMenu, ContextMenuDismissal,
    ContextMenuOpenRequest, ResolvedCommandState,
};
use telorgon::core::PointF;
use telorgon::input::{ChangeSource, CompositeItem};
use telorgon::runtime::{Component, CreateContext, State, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, OverlayFocusRequest, UiNodeId, UiRoot};

#[derive(Debug, PartialEq, Eq)]
struct NonCloneAction(ChangeSource);

struct Fixture {
    overlays: Rc<RefCell<ApplicationOverlayController>>,
    anchor: Rc<Cell<Option<UiNodeId>>>,
    command: Rc<RefCell<Option<CommandSpec<u32, NonCloneAction>>>>,
}

struct FixtureState {
    _enabled: State<bool>,
}

impl Component for Fixture {
    type State = FixtureState;
    type Action = ();

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        self.command.replace(Some(
            CommandSpec::new(
                4,
                "Inspect",
                enabled.read(),
                ActionFactory::new(context.component(), NonCloneAction),
            )
            .unwrap(),
        ));
        FixtureState { _enabled: enabled }
    }

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
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
fn public_context_menu_reuses_shared_owners_and_returns_unapplied_effects() {
    let overlays = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let anchor = Rc::new(Cell::new(None));
    let command = Rc::new(RefCell::new(None));
    let runtime = ViewRuntime::from_component(Fixture {
        overlays: overlays.clone(),
        anchor: anchor.clone(),
        command: command.clone(),
    })
    .unwrap();
    let node_count = runtime.ui().nodes.alive().len();
    let mut context = ContextMenu::new();
    let opened = context
        .open(
            &mut overlays.borrow_mut(),
            runtime.ui(),
            ContextMenuOpenRequest::pointer(
                PointF { x: 14.0, y: 20.0 },
                [CompositeItem {
                    key: 4,
                    enabled: true,
                }],
            ),
        )
        .unwrap();
    assert_eq!(opened.source, ChangeSource::Pointer);
    assert_eq!(opened.focus_request(), OverlayFocusRequest::None);
    assert_eq!(runtime.ui().nodes.alive().len(), node_count);

    let intent = context
        .activate(
            &mut overlays.borrow_mut(),
            command.borrow().as_ref().unwrap(),
            ResolvedCommandState::new(true, None),
            ChangeSource::Keyboard,
        )
        .unwrap();
    assert_eq!(intent.source(), ChangeSource::Keyboard);
    assert_eq!(intent.into_action(), NonCloneAction(ChangeSource::Keyboard));
    assert_eq!(overlays.borrow().state().entry_count, 0);

    context
        .open(
            &mut overlays.borrow_mut(),
            runtime.ui(),
            ContextMenuOpenRequest::keyboard(
                anchor.get().unwrap(),
                [CompositeItem {
                    key: 4,
                    enabled: true,
                }],
            ),
        )
        .unwrap();
    let close = context
        .dismiss(
            &mut overlays.borrow_mut(),
            ContextMenuDismissal::OutsidePress,
        )
        .unwrap();
    assert!(close.consume_input);
}
