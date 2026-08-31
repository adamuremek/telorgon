use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_components::{
    ActionFactory, ChangeSource, CommandSpec, DensityClass, DensityMetrics, Toolbar,
    ToolbarInvocation, ToolbarRef,
};
use telorgon::runtime::{Component, CreateContext, State, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticRole, SizeRule, SizeRule2D, UiRoot};

#[derive(Debug, PartialEq, Eq)]
struct SaveAction(ChangeSource);

struct ToolbarFixture {
    toolbar: Rc<RefCell<Option<ToolbarRef<u32, SaveAction>>>>,
    actions: Rc<RefCell<Vec<SaveAction>>>,
}

struct ToolbarFixtureState {
    toolbar: Toolbar<u32, SaveAction>,
    _enabled: State<bool>,
}

impl Component for ToolbarFixture {
    type State = ToolbarFixtureState;
    type Action = ToolbarInvocation<u32, SaveAction>;

    fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
        let enabled = context.state(true);
        let command = CommandSpec::new(
            1,
            "Save",
            enabled.read(),
            ActionFactory::new(context.component(), SaveAction),
        )
        .unwrap();
        ToolbarFixtureState {
            toolbar: Toolbar::new("Document actions", [command])
                .unwrap()
                .density(DensityMetrics::baseline(DensityClass::Touch)),
            _enabled: enabled,
        }
    }

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let toolbar = state
            .toolbar
            .mount(ui, root.0, |invocation| invocation)
            .unwrap();
        self.toolbar.replace(Some(toolbar));
        root
    }

    fn action(
        &self,
        _state: &mut Self::State,
        action: Self::Action,
        _context: &mut UpdateContext<'_, Self>,
    ) {
        self.actions.borrow_mut().push(action.into_action());
    }
}

#[test]
fn public_toolbar_mounts_one_focus_stop_and_routes_a_fresh_typed_action() {
    let toolbar = Rc::new(RefCell::new(None));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = ViewRuntime::from_component(ToolbarFixture {
        toolbar: toolbar.clone(),
        actions: actions.clone(),
    })
    .unwrap();
    let (toolbar_node, item_node) = {
        let toolbar = toolbar.borrow();
        let toolbar = toolbar.as_ref().unwrap();
        (toolbar.node(), toolbar.items()[0].node())
    };

    assert_eq!(
        runtime.ui().semantics.get(toolbar_node).unwrap().role,
        SemanticRole::Toolbar
    );
    assert!(
        runtime
            .ui()
            .interactions
            .get(toolbar_node)
            .unwrap()
            .focusable
    );
    assert_eq!(
        runtime.ui().box_styles.get(item_node).unwrap().min_size,
        SizeRule2D {
            width: SizeRule::Px(44.0),
            height: SizeRule::Px(44.0),
        }
    );
    assert!(runtime.dispatch_activation(item_node, ChangeSource::Keyboard));
    assert_eq!(&*actions.borrow(), &[SaveAction(ChangeSource::Keyboard)]);
}
