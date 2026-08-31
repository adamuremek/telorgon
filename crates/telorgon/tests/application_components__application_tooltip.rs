use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use telorgon::application_components::{
    ApplicationOverlayController, Tooltip, TooltipAccessibleContribution, TooltipAnchor,
    TooltipExtent, TooltipTrigger, TooltipTriggerPolicy,
};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{RectF, SizeF};
use telorgon::runtime::{
    Component, CreateContext, MonotonicInstant, Ui, UpdateContext, ViewRuntime,
};
use telorgon::ui::{
    BoxStyle, LayoutStyle, OutsidePressPolicy, OverlayFocusRequest, OverlayInitialFocus,
    OverlayModality, SemanticRelationshipKind, SemanticRole, UiNodeId, UiRoot,
};

struct TooltipFixture {
    controller: Rc<RefCell<ApplicationOverlayController>>,
    anchor: Rc<Cell<Option<UiNodeId>>>,
}

impl Component for TooltipFixture {
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
fn public_tooltip_path_returns_deadline_description_and_nonfocusable_overlay() {
    let controller = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let anchor = Rc::new(Cell::new(None));
    let runtime = ViewRuntime::from_component(TooltipFixture {
        controller: controller.clone(),
        anchor: anchor.clone(),
    })
    .unwrap();
    let anchor = anchor.get().unwrap();
    let triggers = TooltipTriggerPolicy::hover_and_sustained_focus(
        Duration::from_millis(400),
        Duration::from_millis(600),
    )
    .unwrap();
    let deadline = triggers
        .deadline(
            TooltipTrigger::SustainedFocus,
            MonotonicInstant::from_nanos(10),
        )
        .unwrap()
        .unwrap();
    assert_eq!(deadline.at, MonotonicInstant::from_nanos(600_000_010));

    let tooltip = Tooltip::new(
        "Additional save-command guidance",
        TooltipAnchor::new(
            anchor,
            RectF {
                x: 100.0,
                y: 50.0,
                width: 40.0,
                height: 20.0,
            },
        ),
        TooltipExtent::new(
            SizeF {
                width: 100.0,
                height: 40.0,
            },
            SizeF {
                width: 60.0,
                height: 24.0,
            },
        ),
        triggers,
    )
    .unwrap();
    let environment = EnvironmentValues {
        available_size: SizeF {
            width: 320.0,
            height: 200.0,
        },
        text_scale: 1.5,
        ..EnvironmentValues::default()
    };
    let opened = tooltip
        .open(
            deadline.trigger,
            &mut controller.borrow_mut(),
            runtime.ui(),
            &environment,
        )
        .unwrap();

    assert_eq!(opened.focus_request(), OverlayFocusRequest::None);
    assert_eq!(opened.extent.text_scale, 1.5);
    assert_eq!(opened.semantics.role, SemanticRole::Tooltip);
    assert_eq!(
        opened.semantics.anchor_relationship,
        SemanticRelationshipKind::DescribedBy
    );
    assert_eq!(
        opened.semantics.contribution,
        TooltipAccessibleContribution::DescriptionOnly
    );
    let controller = controller.borrow();
    let entry = controller.entry(opened.id()).unwrap();
    assert_eq!(entry.modality, OverlayModality::NonModal);
    assert_eq!(entry.focus.initial, OverlayInitialFocus::None);
    assert_eq!(entry.dismissal.outside_press, OutsidePressPolicy::Ignore);
    assert!(!controller.state().background_is_inert);
}
