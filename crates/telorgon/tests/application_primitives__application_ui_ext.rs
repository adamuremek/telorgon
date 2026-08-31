use std::cell::Cell;
use std::rc::Rc;

use telorgon::application_primitives::prelude::{
    ApplicationRegion, ApplicationRegionRef, ApplicationRoot, ApplicationRootRef, ApplicationUiExt,
};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticRole, UiRoot};

struct Fixture {
    root: Rc<Cell<Option<ApplicationRootRef>>>,
    region: Rc<Cell<Option<ApplicationRegionRef>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ui
            .mount_application_root(host.0, &ApplicationRoot::new("Extended workspace").unwrap())
            .unwrap();
        let region = ui
            .mount_application_region(
                root.content_node(),
                &ApplicationRegion::content("Extended content").unwrap(),
            )
            .unwrap();
        self.root.set(Some(root));
        self.region.set(Some(region));
        host
    }

    fn action(&self, _: &mut Self::State, _: Self::Action, _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn public_extension_methods_delegate_to_the_same_stable_primitive_owners() {
    let root = Rc::new(Cell::new(None));
    let region = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(Fixture {
        root: root.clone(),
        region: region.clone(),
    })
    .unwrap();
    let root = root.get().unwrap();
    let region = region.get().unwrap();

    assert_ne!(root.node(), root.content_node());
    assert_ne!(root.content_node(), region.node());
    assert_eq!(
        runtime.ui().semantics.get(root.node()).unwrap().role,
        SemanticRole::Application
    );
    assert_eq!(
        runtime.ui().semantics.get(region.node()).unwrap().role,
        SemanticRole::Main
    );
}
