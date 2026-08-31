use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_primitives::prelude::{
    ApplicationRegion, ApplicationRegionError, ApplicationRegionKind, ApplicationRegionRef,
    ApplicationRoot,
};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, SemanticRole, UiRoot};

struct Fixture {
    references: Rc<RefCell<Vec<ApplicationRegionRef>>>,
}

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let root = ApplicationRoot::new("Public workspace")
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        for region in [
            ApplicationRegion::content("Document").unwrap(),
            ApplicationRegion::navigation("Sections").unwrap(),
            ApplicationRegion::status("Sync status").unwrap(),
        ] {
            self.references
                .borrow_mut()
                .push(region.mount(ui, root.content_node()).unwrap());
        }
        host
    }

    fn action(&self, _: &mut Self::State, _: Self::Action, _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn public_regions_publish_only_their_typed_landmarks() {
    assert_eq!(
        ApplicationRegion::navigation(" "),
        Err(ApplicationRegionError::MissingAccessibleName)
    );
    let references = Rc::new(RefCell::new(Vec::new()));
    let runtime = ViewRuntime::from_component(Fixture {
        references: references.clone(),
    })
    .unwrap();
    let references = references.borrow();
    for (reference, kind, role) in [
        (
            references[0],
            ApplicationRegionKind::Content,
            SemanticRole::Main,
        ),
        (
            references[1],
            ApplicationRegionKind::Navigation,
            SemanticRole::Navigation,
        ),
        (
            references[2],
            ApplicationRegionKind::Status,
            SemanticRole::Status,
        ),
    ] {
        assert_eq!(reference.kind(), kind);
        let semantic = runtime.ui().semantics.get(reference.node()).unwrap();
        assert_eq!(semantic.role, role);
        assert!(semantic.actions.is_empty());
    }
}
