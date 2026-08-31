use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ApplicationOverlayController, CommandSpec, MenuActivationDismissal,
    MenuController, MenuOpenRequest, ResolvedCommandState,
};
use telorgon::input::{ChangeSource, CompositeItem};
use telorgon::runtime::{Component, CreateContext, State, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, OverlayAnchor, UiNodeId, UiRoot};

#[derive(Debug, PartialEq, Eq)]
struct NonCloneAction(ChangeSource);

struct Fixture {
    overlays: Rc<RefCell<ApplicationOverlayController>>,
    anchor: Rc<RefCell<Option<UiNodeId>>>,
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
                7,
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
        self.anchor.replace(Some(root.0));
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
fn public_menu_controller_coordinates_mounted_overlay_without_mounting_rows() {
    let overlays = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let anchor = Rc::new(RefCell::new(None));
    let command = Rc::new(RefCell::new(None));
    let runtime = ViewRuntime::from_component(Fixture {
        overlays: overlays.clone(),
        anchor: anchor.clone(),
        command: command.clone(),
    })
    .unwrap();
    let node_count = runtime.ui().nodes.alive().len();
    let mut menus = MenuController::new();
    let opened = menus
        .open(
            &mut overlays.borrow_mut(),
            runtime.ui(),
            MenuOpenRequest::root(
                OverlayAnchor::Node(anchor.borrow().unwrap()),
                [CompositeItem {
                    key: 7,
                    enabled: true,
                }],
            ),
        )
        .unwrap();

    assert_eq!(runtime.ui().nodes.alive().len(), node_count);
    assert_eq!(menus.active_overlay(), Some(opened.overlay));
    let intent = menus
        .activate(
            &mut overlays.borrow_mut(),
            command.borrow().as_ref().unwrap(),
            ResolvedCommandState::new(true, None),
            ChangeSource::Keyboard,
            MenuActivationDismissal::Chain,
        )
        .unwrap();
    assert_eq!(intent.source(), ChangeSource::Keyboard);
    assert_eq!(intent.into_action(), NonCloneAction(ChangeSource::Keyboard));
    assert_eq!(overlays.borrow().state().entry_count, 0);
}
