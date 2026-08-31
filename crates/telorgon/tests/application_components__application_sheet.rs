use std::cell::RefCell;
use std::rc::Rc;

use telorgon::application_components::{
    ApplicationOverlayController, ResolvedSheetEdge, Sheet, SheetBarrierPolicy, SheetEdge,
    SheetExtent, SheetInitialFocus, SheetMode,
};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{EdgeInsets, SizeF};
use telorgon::input::WritingDirection;
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{
    BoxStyle, LayoutStyle, OutsidePressPolicy, OverlayFocusContainment, OverlayFocusRequest,
    OverlayInitialFocus, OverlayModality, UiNodeId, UiRoot,
};

struct SheetFixture {
    controller: Rc<RefCell<ApplicationOverlayController>>,
    nodes: Rc<RefCell<Vec<UiNodeId>>>,
}

impl Component for SheetFixture {
    type State = ();
    type Action = ();

    fn create(&self, _context: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let nodes = self.nodes.clone();
        let root =
            ui.foundation()
                .root(BoxStyle::default(), LayoutStyle::default(), move |writer| {
                    nodes.borrow_mut().push(writer.container(
                        BoxStyle::default(),
                        LayoutStyle::default(),
                        |_| {},
                    ));
                });
        self.controller.borrow_mut().mount(ui, root.0).unwrap();
        self.nodes.borrow_mut().insert(0, root.0);
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
fn public_sheet_path_opens_a_safe_rtl_modal_edge_generation() {
    let controller = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let nodes = Rc::new(RefCell::new(Vec::new()));
    let runtime = ViewRuntime::from_component(SheetFixture {
        controller: controller.clone(),
        nodes: nodes.clone(),
    })
    .unwrap();
    let opener = nodes.borrow()[0];
    let focus_target = nodes.borrow()[1];
    let environment = EnvironmentValues {
        available_size: SizeF {
            width: 360.0,
            height: 240.0,
        },
        safe_area: EdgeInsets {
            top: 8.0,
            right: 16.0,
            bottom: 24.0,
            left: 32.0,
        },
        writing_direction: WritingDirection::RightToLeft,
        ..EnvironmentValues::default()
    };
    let sheet = Sheet::new(
        "Public account sheet",
        opener,
        SheetEdge::InlineStart,
        SheetExtent::new(
            SizeF {
                width: 140.0,
                height: 180.0,
            },
            SizeF {
                width: 100.0,
                height: 120.0,
            },
        ),
        SheetMode::Modal {
            initial_focus: SheetInitialFocus::Explicit(focus_target),
            barrier: SheetBarrierPolicy::DismissAndConsume,
        },
    )
    .unwrap();

    let opened = sheet
        .open(&mut controller.borrow_mut(), runtime.ui(), &environment)
        .unwrap();
    assert_eq!(opened.edge, ResolvedSheetEdge::Right);
    assert_eq!(opened.placement.placement.rect.right(), 344.0);
    assert_eq!(
        opened.focus_request(),
        OverlayFocusRequest::Initial(OverlayInitialFocus::Explicit(focus_target))
    );
    let controller = controller.borrow();
    let entry = controller.entry(opened.id()).unwrap();
    assert_eq!(entry.modality, OverlayModality::Modal);
    assert_eq!(entry.focus.containment, OverlayFocusContainment::Contain);
    assert_eq!(
        entry.dismissal.outside_press,
        OutsidePressPolicy::DismissAndConsume
    );
    assert!(controller.state().background_is_inert);
}
