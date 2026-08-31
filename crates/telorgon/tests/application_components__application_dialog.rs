use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_components::{
    ApplicationOverlayController, Dialog, DialogBarrierPolicy, DialogInitialFocus,
};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{RectF, SizeF};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{
    BoxStyle, LayoutStyle, OverlayFocusContainment, OverlayFocusRequest, OverlayInitialFocus,
    OverlayModality, UiNodeId, UiRoot,
};

struct DialogFixture {
    controller: Rc<RefCell<ApplicationOverlayController>>,
    opener: Rc<Cell<Option<UiNodeId>>>,
}

impl Component for DialogFixture {
    type State = ();
    type Action = ();

    fn create(&self, _context: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        self.controller.borrow_mut().mount(ui, root.0).unwrap();
        self.opener.set(Some(root.0));
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
fn public_dialog_path_opens_a_safe_contained_modal_generation() {
    let controller = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let opener = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(DialogFixture {
        controller: controller.clone(),
        opener: opener.clone(),
    })
    .unwrap();
    let environment = EnvironmentValues {
        available_size: SizeF {
            width: 320.0,
            height: 220.0,
        },
        ..EnvironmentValues::default()
    };
    let dialog = Dialog::new(
        "Public confirmation",
        opener.get().unwrap(),
        RectF {
            x: 110.0,
            y: 40.0,
            width: 100.0,
            height: 20.0,
        },
        SizeF {
            width: 160.0,
            height: 120.0,
        },
        DialogInitialFocus::FirstFocusable,
    )
    .unwrap();

    let opened = dialog
        .open(&mut controller.borrow_mut(), runtime.ui(), &environment)
        .unwrap();
    assert_eq!(
        opened.barrier.policy,
        DialogBarrierPolicy::BlockOutsidePress
    );
    assert_eq!(
        opened.focus_request(),
        OverlayFocusRequest::Initial(OverlayInitialFocus::FirstFocusable)
    );
    let controller = controller.borrow();
    let entry = controller.entry(opened.id()).unwrap();
    assert_eq!(entry.modality, OverlayModality::Modal);
    assert_eq!(entry.focus.containment, OverlayFocusContainment::Contain);
    assert!(controller.state().background_is_inert);
    assert_eq!(controller.state().active_modal, Some(opened.id()));
}
