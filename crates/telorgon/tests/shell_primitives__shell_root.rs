use std::cell::Cell;
use std::rc::Rc;

use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    OutputId, ShellCapabilities, ShellCapabilityGrant, ShellGrantToken, ShellLayerKind,
};
use telorgon::shell_primitives::prelude::{ShellRoot, ShellRootRef};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticRole, UiRoot};

struct Fixture(Rc<Cell<Option<ShellRootRef>>>);

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &(), ui: &mut Ui<'_, '_, ()>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let grant = ShellCapabilityGrant::from_host(
            ShellGrantToken::from_raw(1).unwrap(),
            OutputId::from_raw(2).unwrap(),
            ShellCapabilities::PANEL_LAYER,
        );
        self.0.set(Some(
            ShellRoot::new("Public shell", grant)
                .unwrap()
                .mount(ui, host.0)
                .unwrap(),
        ));
        host
    }

    fn action(&self, _: &mut (), _: (), _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn public_shell_root_is_named_and_narrows_output_authority() {
    let reference = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(Fixture(reference.clone())).unwrap();
    let reference = reference.get().unwrap();

    assert_eq!(reference.output().get(), 2);
    assert_eq!(
        runtime.ui().semantics.get(reference.node()).unwrap().role,
        SemanticRole::Region
    );
    assert!(reference.authorize_layer(ShellLayerKind::Panel).is_ok());
    assert!(reference.authorize_layer(ShellLayerKind::Lock).is_err());
}
