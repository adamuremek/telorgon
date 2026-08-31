use std::cell::Cell;
use std::rc::Rc;

use telorgon::core::SizeF;
use telorgon::platform::capability::{
    ExecutionRequirement, PermissionState, Support, UserGestureRequirement,
};
use telorgon::platform::request::{AdmittedRequest, RequestAdmission, RequestOutcome};
use telorgon::platform::services::window::{
    WindowAdmissionError, WindowAttentionApplied, WindowAttentionRequest, WindowCapability,
    WindowCapabilityLimits, WindowCapabilityQuery, WindowCloseApplied, WindowCloseIntent,
    WindowCloseRequest, WindowOperation, WindowRequestAdmission, WindowService, WindowServiceKey,
    WindowSizeConstraints, WindowSizeConstraintsApplied, WindowSizeConstraintsRequest,
    WindowStateApplied, WindowStateIntent, WindowStateRequest, WindowTitle, WindowTitleApplied,
    WindowTitleRequest,
};
use telorgon::platform::services::{ServiceLookup, ServiceRegistry};
use telorgon::platform::{RequestId, ViewId};

struct FixtureWindowService {
    view: ViewId,
    next_request: Cell<u64>,
}

impl FixtureWindowService {
    fn admit<T>(&self) -> RequestAdmission<T, WindowAdmissionError> {
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        Ok(AdmittedRequest::new(RequestId::from_raw(next).unwrap()))
    }

    fn validate<T>(&self, view: ViewId) -> WindowRequestAdmission<T> {
        if view == self.view {
            self.admit()
        } else {
            Err(WindowAdmissionError::ViewUnavailable { view })
        }
    }
}

impl WindowService for FixtureWindowService {
    fn capability(&self, query: WindowCapabilityQuery) -> Support<WindowCapability> {
        if query.view() != self.view {
            return Support::Unavailable(
                telorgon::platform::capability::UnavailableReason::UnavailableInScope,
            );
        }
        Support::Available(WindowCapability::new(
            query.operation(),
            WindowCapabilityLimits::unspecified(),
            PermissionState::NotRequired,
            ExecutionRequirement::RuntimeOwner,
            UserGestureRequirement::NotRequired,
        ))
    }

    fn set_title(&self, request: WindowTitleRequest) -> WindowRequestAdmission<WindowTitleApplied> {
        self.validate(request.view())
    }

    fn set_state(&self, request: WindowStateRequest) -> WindowRequestAdmission<WindowStateApplied> {
        self.validate(request.view())
    }

    fn set_size_constraints(
        &self,
        request: WindowSizeConstraintsRequest,
    ) -> WindowRequestAdmission<WindowSizeConstraintsApplied> {
        self.validate(request.view())
    }

    fn request_attention(
        &self,
        request: WindowAttentionRequest,
    ) -> WindowRequestAdmission<WindowAttentionApplied> {
        self.validate(request.view())
    }

    fn request_close(
        &self,
        request: WindowCloseRequest,
    ) -> WindowRequestAdmission<WindowCloseApplied> {
        self.validate(request.view())
    }
}

#[test]
fn public_window_service_path_is_per_view_typed_bounded_and_observation_neutral() {
    let view = ViewId::from_raw(12, 4).unwrap();
    let service: Rc<dyn WindowService> = Rc::new(FixtureWindowService {
        view,
        next_request: Cell::new(100),
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<WindowServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<WindowServiceKey>() else {
        panic!("registered window service must remain available");
    };

    let capability = service
        .capability(WindowCapabilityQuery::new(view, WindowOperation::SetTitle))
        .into_available()
        .unwrap();
    assert_eq!(capability.operations(), &WindowOperation::SetTitle);
    assert_eq!(capability.execution(), ExecutionRequirement::RuntimeOwner);

    let title = WindowTitle::new("confidential-quarterly-plan").unwrap();
    let title_request = WindowTitleRequest::new(view, title);
    assert!(!format!("{title_request:?}").contains("confidential"));
    let title_applied = WindowTitleApplied::from_request(&title_request);
    let title_completion = service
        .set_title(title_request)
        .unwrap()
        .complete(RequestOutcome::Applied(title_applied));
    assert_eq!(title_completion.request_id().get(), 101);
    assert_eq!(title_completion.outcome().applied().unwrap().view(), view);

    let constraints = WindowSizeConstraints::new(
        Some(SizeF {
            width: 320.0,
            height: 200.0,
        }),
        Some(SizeF {
            width: 1920.0,
            height: 1080.0,
        }),
    )
    .unwrap();
    let constraints_request = WindowSizeConstraintsRequest::new(view, constraints);
    let constraints_applied = WindowSizeConstraintsApplied::from_request(constraints_request);
    assert!(
        service
            .set_size_constraints(constraints_request)
            .unwrap()
            .complete(RequestOutcome::Applied(constraints_applied))
            .outcome()
            .is_applied()
    );

    let state_request = WindowStateRequest::new(view, WindowStateIntent::Maximized);
    let state_applied = WindowStateApplied::from_request(state_request);
    assert_eq!(state_applied.intent(), WindowStateIntent::Maximized);
    assert!(
        service
            .set_state(state_request)
            .unwrap()
            .complete(RequestOutcome::Applied(state_applied))
            .outcome()
            .is_applied()
    );

    let close_request = WindowCloseRequest::new(view, WindowCloseIntent::ApplicationRequested);
    let close_applied = WindowCloseApplied::from_request(close_request);
    let close_completion = service
        .request_close(close_request)
        .unwrap()
        .complete(RequestOutcome::Applied(close_applied));
    assert_eq!(
        close_completion.outcome().applied().unwrap().intent(),
        WindowCloseIntent::ApplicationRequested
    );

    let stale_view = ViewId::from_raw(view.slot(), view.generation() + 1).unwrap();
    assert_eq!(
        service.set_state(WindowStateRequest::new(
            stale_view,
            WindowStateIntent::Normal,
        )),
        Err(WindowAdmissionError::ViewUnavailable { view: stale_view })
    );
}
