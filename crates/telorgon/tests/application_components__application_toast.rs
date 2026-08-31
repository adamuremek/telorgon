use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use telorgon::application_components::{
    ApplicationOverlayController, ResolvedToastCorner, Toast, ToastAnnouncementPolicy,
    ToastAnnouncementPriority, ToastCoalescingIntent, ToastCoalescingKey, ToastCorner, ToastExtent,
    ToastLifetime, ToastRedactionIntent,
};
use telorgon::application_primitives::EnvironmentValues;
use telorgon::core::{EdgeInsets, SizeF};
use telorgon::input::WritingDirection;
use telorgon::runtime::{
    Component, CreateContext, MonotonicInstant, Ui, UpdateContext, ViewRuntime,
};
use telorgon::ui::{
    BoxStyle, LayoutStyle, OutsidePressPolicy, OverlayFocusRequest, OverlayInitialFocus,
    OverlayModality, SemanticRole, UiRoot,
};

struct ToastFixture {
    controller: Rc<RefCell<ApplicationOverlayController>>,
}

impl Component for ToastFixture {
    type State = ();
    type Action = ();

    fn create(&self, _context: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
        let root = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        self.controller.borrow_mut().mount(ui, root.0).unwrap();
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
fn public_toast_path_returns_safe_expiry_announcement_and_nonfocusable_overlay() {
    let controller = Rc::new(RefCell::new(ApplicationOverlayController::new()));
    let runtime = ViewRuntime::from_component(ToastFixture {
        controller: controller.clone(),
    })
    .unwrap();
    let key = ToastCoalescingKey::from_raw(21).unwrap();
    let toast = Toast::new(
        "Connection restored",
        ToastCorner::BlockEndInlineEnd,
        ToastExtent::new(
            SizeF {
                width: 140.0,
                height: 56.0,
            },
            SizeF {
                width: 90.0,
                height: 36.0,
            },
        ),
        ToastAnnouncementPolicy::new(ToastAnnouncementPriority::Assertive)
            .coalescing(ToastCoalescingIntent::ReplaceMatching(key))
            .redaction(ToastRedactionIntent::Diagnostics),
        ToastLifetime::expiring(Duration::from_secs(4)).unwrap(),
    )
    .unwrap();
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

    let opened = toast
        .open(
            MonotonicInstant::from_nanos(50),
            &mut controller.borrow_mut(),
            runtime.ui(),
            &environment,
        )
        .unwrap();
    assert_eq!(opened.corner, ResolvedToastCorner::BottomLeft);
    assert_eq!(opened.placement.placement.rect.x, 32.0);
    assert_eq!(opened.announcement.role, SemanticRole::Alert);
    assert_eq!(
        opened.announcement.coalescing,
        ToastCoalescingIntent::ReplaceMatching(key)
    );
    assert_eq!(
        opened.dismissal.expiry.unwrap().at.as_nanos(),
        4_000_000_050
    );
    assert_eq!(opened.focus_request(), OverlayFocusRequest::None);
    let controller = controller.borrow();
    let entry = controller.entry(opened.id()).unwrap();
    assert_eq!(entry.modality, OverlayModality::NonModal);
    assert_eq!(entry.focus.initial, OverlayInitialFocus::None);
    assert_eq!(entry.dismissal.outside_press, OutsidePressPolicy::Ignore);
    assert!(!controller.state().background_is_inert);
}
