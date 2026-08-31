use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ChangeSource, CheckState, CommandSpec, DensityClass, DensityMetrics, Menu,
    MenuItem, MenuItemKind, MenuLevelState, MenuRef, MenuRouteRequest,
};
use telorgon::runtime::{Component, CreateContext, State, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{
    BoxStyle, LayoutStyle, OverlayId, SemanticAction, SemanticCheckState, SemanticRelationshipKind,
    SemanticRole, SizeRule, SizeRule2D, UiRoot,
};

#[derive(Debug, PartialEq, Eq)]
struct NonCloneAction {
    command: u32,
    source: ChangeSource,
}

struct MenuFixture {
    menu: Rc<RefCell<Option<MenuRef<u32, NonCloneAction>>>>,
    routes: Rc<RefCell<Vec<MenuRouteRequest<u32>>>>,
}

struct MenuFixtureState {
    menu: Menu<u32, NonCloneAction>,
    _enabled: State<bool>,
    _disabled: State<bool>,
    _checked: State<CheckState>,
}

impl Component for MenuFixture {
    type State = MenuFixtureState;
    type Action = MenuRouteRequest<u32>;

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        let disabled = context.state(false);
        let checked = context.state(CheckState::Mixed);
        let owner = context.component();
        let make = |command, label, available| {
            CommandSpec::new(
                command,
                label,
                available,
                ActionFactory::new(owner, move |source| NonCloneAction { command, source }),
            )
            .unwrap()
        };
        let save = make(1, "Save", enabled.read());
        let unavailable = make(2, "Unavailable", disabled.read())
            .checked(checked.read())
            .unwrap();
        let export = make(3, "Export", enabled.read());
        MenuFixtureState {
            menu: Menu::new(
                "Document menu",
                [
                    MenuItem::command(save),
                    MenuItem::command(unavailable),
                    MenuItem::submenu(export),
                ],
            )
            .unwrap()
            .density(DensityMetrics::baseline(DensityClass::Touch)),
            _enabled: enabled,
            _disabled: disabled,
            _checked: checked,
        }
    }

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let menu = state
            .menu
            .mount(
                ui,
                root.0,
                MenuLevelState {
                    overlay: OverlayId::from_raw(1, 1).unwrap(),
                    parent: None,
                    active_command: Some(1),
                },
                |request| request,
            )
            .unwrap();
        self.menu.replace(Some(menu));
        root
    }

    fn action(
        &self,
        _state: &mut Self::State,
        action: Self::Action,
        _context: &mut UpdateContext<'_, Self>,
    ) {
        self.routes.borrow_mut().push(action);
    }
}

#[test]
fn public_menu_mounts_one_focus_entry_and_preserves_command_and_submenu_sources() {
    let menu = Rc::new(RefCell::new(None));
    let routes = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(MenuFixture {
        menu: menu.clone(),
        routes: routes.clone(),
    })
    .unwrap();
    let (menu_node, items) = {
        let menu = menu.borrow();
        let menu = menu.as_ref().unwrap();
        (menu.node(), menu.items().to_vec())
    };

    let root_semantics = runtime.ui().semantics.get(menu_node).unwrap();
    assert_eq!(root_semantics.role, SemanticRole::Menu);
    assert_eq!(root_semantics.relationships.len(), 4);
    assert_eq!(
        root_semantics.relationships.last().unwrap().kind,
        SemanticRelationshipKind::ActiveDescendant
    );
    assert!(runtime.ui().interactions.get(menu_node).unwrap().focusable);
    assert!(items.iter().all(|item| {
        !runtime
            .ui()
            .interactions
            .get(item.node())
            .is_some_and(|interaction| interaction.focusable)
    }));
    assert_eq!(items[2].kind(), MenuItemKind::Submenu);
    assert_eq!(
        runtime
            .ui()
            .box_styles
            .get(items[0].node())
            .unwrap()
            .min_size,
        SizeRule2D {
            width: SizeRule::Px(44.0),
            height: SizeRule::Px(44.0),
        }
    );
    let disabled = runtime.ui().semantics.get(items[1].node()).unwrap();
    assert!(disabled.state.disabled);
    assert_eq!(disabled.state.checked, Some(SemanticCheckState::Mixed));
    assert!(disabled.effective_actions().is_empty());
    assert!(!runtime.dispatch_activation(items[1].node(), ChangeSource::Pointer));
    assert!(runtime.dispatch_activation(items[0].node(), ChangeSource::Accessibility));
    assert!(runtime.dispatch_activation(items[2].node(), ChangeSource::Keyboard));

    assert_eq!(
        &*routes.borrow(),
        &[
            MenuRouteRequest::Activate {
                command: 1,
                source: ChangeSource::Accessibility,
                dismissal: telorgon::application_components::MenuActivationDismissal::Chain,
            },
            MenuRouteRequest::Submenu {
                parent: OverlayId::from_raw(1, 1).unwrap(),
                command: 3,
                source: ChangeSource::Keyboard,
            },
        ]
    );
    assert!(
        runtime
            .ui()
            .semantics
            .get(items[0].node())
            .unwrap()
            .actions
            .contains(SemanticAction::Activate)
    );
    assert!(
        runtime
            .ui()
            .semantics
            .get(items[2].node())
            .unwrap()
            .actions
            .contains(SemanticAction::Expand)
    );
}
