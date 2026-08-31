use std::cell::Cell;
use std::rc::Rc;

use telorgon::application_primitives::prelude::{
    ApplicationRoot, ApplicationRootError, ApplicationRootRef, ApplicationRootStyle,
};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticRelationshipKind, SemanticRole, UiRoot};

struct Fixture {
    reference: Rc<Cell<Option<ApplicationRootRef>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let reference = ApplicationRoot::new("Public workspace")
            .unwrap()
            .style(ApplicationRootStyle {
                container: BoxStyle {
                    opacity: 0.8,
                    ..BoxStyle::default()
                },
                ..ApplicationRootStyle::default()
            })
            .mount(ui, host.0)
            .unwrap();
        self.reference.set(Some(reference));
        host
    }

    fn action(&self, _: &mut Self::State, _: Self::Action, _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn public_application_root_exposes_one_named_owned_content_scope() {
    assert_eq!(
        ApplicationRoot::new(""),
        Err(ApplicationRootError::MissingAccessibleName)
    );
    let reference = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(Fixture {
        reference: reference.clone(),
    })
    .unwrap();
    let reference = reference.get().unwrap();
    let semantic = runtime.ui().semantics.get(reference.node()).unwrap();

    assert_eq!(semantic.role, SemanticRole::Application);
    assert_eq!(semantic.relationships.len(), 1);
    assert_eq!(
        semantic.relationships[0].kind,
        SemanticRelationshipKind::Owns
    );
    assert_eq!(semantic.relationships[0].target, reference.content_node());
    assert_eq!(
        runtime
            .ui()
            .box_styles
            .get(reference.node())
            .unwrap()
            .opacity,
        0.8
    );
}
