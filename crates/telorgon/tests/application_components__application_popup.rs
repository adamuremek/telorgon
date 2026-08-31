use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telorgon::application_components::{ApplicationOverlayController, Popup, PopupAnchor};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{RectF, SizeF};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::ui::{BoxStyle, LayoutStyle, OverlayModality, UiNodeId, UiRoot};

struct PopupFixture {
    controller: Rc<RefCell<ApplicationOverlayController>>,
    anchor: Rc<Cell<Option<UiNodeId>>>,
}

impl Component for PopupFixture {
    type State = ();
    type Action = ();

    fn create(&self, _context: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        self.controller.borrow_mut().mount(ui, root.0).unwrap();
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
fn public_popup_path_opens_a_placed_nonmodal_generation() {
    let controller = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let anchor = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(PopupFixture {
        controller: controller.clone(),
        anchor: anchor.clone(),
    })
    .unwrap();
    let environment = EnvironmentValues {
        available_size: SizeF {
            width: 240.0,
            height: 160.0,
        },
        ..EnvironmentValues::default()
    };
    let popup = Popup::new(
        PopupAnchor::node(
            anchor.get().unwrap(),
            RectF {
                x: 80.0,
                y: 40.0,
                width: 40.0,
                height: 20.0,
            },
        ),
        SizeF {
            width: 100.0,
            height: 60.0,
        },
    );

    let opened = popup
        .open(&mut controller.borrow_mut(), runtime.ui(), &environment)
        .unwrap();
    let controller = controller.borrow();
    assert_eq!(controller.state().entry_count, 1);
    assert_eq!(controller.state().top, Some(opened.id()));
    assert_eq!(
        controller.entry(opened.id()).unwrap().modality,
        OverlayModality::NonModal
    );
    assert_eq!(
        opened.placement.placement.rect,
        RectF {
            x: 80.0,
            y: 60.0,
            width: 100.0,
            height: 60.0,
        }
    );
}
