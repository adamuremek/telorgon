use std::cell::Cell;
use std::num::{NonZeroU16, NonZeroU32};
use std::rc::Rc;

use telorgon::core::PointF;
use telorgon::platform::{
    AdmittedRequest, CursorAdmissionError, CursorAnimationFrame, CursorAppearance,
    CursorAppearanceAdmission, CursorAppearanceApplied, CursorAppearanceRequest, CursorCapability,
    CursorCapabilityQuery, CursorConstraintAdmission, CursorConstraintKind, CursorConstraintLease,
    CursorConstraintLeaseHandle, CursorConstraintLeaseId, CursorConstraintLeaseStatus,
    CursorConstraintRequest, CursorImageError, CursorLimits, CursorOperations,
    CursorPositionAdmission, CursorPositionApplied, CursorPositionError, CursorPositionRequest,
    CursorSelection, CursorSelectionKind, CursorService, CursorServiceKey, CustomCursor,
    CustomCursorAnimation, CustomCursorImage, DisplayProperties, ExecutionRequirement,
    PermissionState, PhysicalExtent, RequestId, RequestOutcome, ScaleFactor, ServiceLookup,
    ServiceRegistry, StandardCursor, Support, UnavailableReason, UserGestureRequirement, ViewId,
    ViewMetrics, ViewState,
};

#[derive(Debug)]
struct FixtureLease {
    id: CursorConstraintLeaseId,
    view: ViewId,
    kind: CursorConstraintKind,
    active: Rc<Cell<bool>>,
}

impl CursorConstraintLease for FixtureLease {
    fn id(&self) -> CursorConstraintLeaseId {
        self.id
    }

    fn view(&self) -> ViewId {
        self.view
    }

    fn kind(&self) -> CursorConstraintKind {
        self.kind
    }

    fn status(&self) -> CursorConstraintLeaseStatus {
        CursorConstraintLeaseStatus::Active
    }
}

impl Drop for FixtureLease {
    fn drop(&mut self) {
        self.active.set(false);
    }
}

struct FixtureCursorService {
    view: ViewId,
    metrics_revision: telorgon::platform::MetricsRevision,
    next_request: Cell<u64>,
    constraint_active: Rc<Cell<bool>>,
}

impl FixtureCursorService {
    fn admit<T>(&self) -> AdmittedRequest<T> {
        let next = self.next_request.get() + 1;
        self.next_request.set(next);
        AdmittedRequest::new(RequestId::from_raw(next).unwrap())
    }
}

impl CursorService for FixtureCursorService {
    fn capability(&self, query: CursorCapabilityQuery) -> Support<CursorCapability> {
        if query.view() != self.view {
            return Support::Unavailable(UnavailableReason::UnavailableInScope);
        }
        Support::Available(CursorCapability::new(
            CursorOperations::new(true, true, true, true, true, true, true),
            CursorLimits::default(),
            PermissionState::NotRequired,
            ExecutionRequirement::HostEventLoop,
            UserGestureRequirement::RecentRequired,
        ))
    }

    fn set_appearance(&self, request: CursorAppearanceRequest) -> CursorAppearanceAdmission {
        if request.view() != self.view {
            return Err(CursorAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        if !CursorLimits::default().admits(request.appearance().selection()) {
            return Err(CursorAdmissionError::CustomCursorExceedsLimits);
        }
        Ok(self.admit())
    }

    fn set_position(&self, request: CursorPositionRequest) -> CursorPositionAdmission {
        if request.view() != self.view {
            return Err(CursorAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        if request.metrics_revision() != self.metrics_revision {
            return Err(CursorAdmissionError::StaleMetrics {
                view: request.view(),
                expected: self.metrics_revision,
                observed: request.metrics_revision(),
            });
        }
        Ok(self.admit())
    }

    fn acquire_constraint(&self, request: CursorConstraintRequest) -> CursorConstraintAdmission {
        if request.view() != self.view {
            return Err(CursorAdmissionError::ViewUnavailable {
                view: request.view(),
            });
        }
        if self.constraint_active.replace(true) {
            return Err(CursorAdmissionError::ConstraintAlreadyActive {
                view: request.view(),
            });
        }
        Ok(self.admit())
    }
}

fn view_state(view: ViewId) -> ViewState {
    ViewState::with_metrics(
        view,
        ViewMetrics::new(
            PhysicalExtent::new(400, 240),
            ScaleFactor::new(2.0).unwrap(),
            DisplayProperties::default(),
        )
        .unwrap(),
    )
}

fn image(value: u8) -> CustomCursorImage {
    CustomCursorImage::new(vec![value; 4 * 4 * 4], 4, 4, 1, 2).unwrap()
}

#[test]
fn custom_images_and_animation_are_bounded_geometry_checked_and_debug_redacted() {
    let secret_pixels = vec![17_u8; 4 * 4 * 4];
    let custom = CustomCursorImage::new(secret_pixels.clone(), 4, 4, 1, 2).unwrap();
    assert_eq!(custom.rgba8_srgb_straight(), secret_pixels);
    assert!(!format!("{custom:?}").contains(&format!("{:?}", secret_pixels)));
    assert_eq!(
        CustomCursorImage::new(vec![0; 7], 2, 1, 0, 0),
        Err(CursorImageError::ByteLengthMismatch)
    );
    assert_eq!(
        CustomCursorImage::new(vec![0; 16], 2, 2, 2, 0),
        Err(CursorImageError::HotspotOutOfBounds)
    );

    let animation = CustomCursorAnimation::new(vec![
        CursorAnimationFrame::new(image(1), NonZeroU32::new(20).unwrap()).unwrap(),
        CursorAnimationFrame::new(image(2), NonZeroU32::new(30).unwrap()).unwrap(),
    ])
    .unwrap();
    assert_eq!(animation.frames().len(), 2);
    assert_eq!(animation.cycle_duration_ms(), 50);
    assert_eq!(animation.total_bytes(), 128);

    let constrained = CursorLimits::new(
        NonZeroU16::new(3).unwrap(),
        NonZeroU16::new(3).unwrap(),
        NonZeroU16::new(1).unwrap(),
        NonZeroU32::new(64).unwrap(),
    )
    .unwrap();
    assert!(!constrained.admits(&CursorSelection::Custom(CustomCursor::Animated(animation))));
}

#[test]
fn logical_position_cites_exact_metrics_and_rejects_sentinels_or_outside_points() {
    let view = ViewId::from_raw(4, 7).unwrap();
    let snapshot = view_state(view).snapshot();
    let request = CursorPositionRequest::new(&snapshot, PointF { x: 199.0, y: 119.0 }).unwrap();
    assert_eq!(request.view(), view);
    assert_eq!(request.metrics_revision(), snapshot.metrics().revision());
    assert_eq!(
        request.coordinate_space(),
        telorgon::platform::CoordinateSpace::ViewLogical
    );
    assert_eq!(
        CursorPositionRequest::new(&snapshot, PointF { x: -1.0, y: -1.0 }),
        Err(CursorPositionError::OutsideView {
            logical_extent: telorgon::core::SizeF {
                width: 200.0,
                height: 120.0
            }
        })
    );
    assert_eq!(
        CursorPositionRequest::new(
            &snapshot,
            PointF {
                x: f32::NAN,
                y: 0.0
            }
        ),
        Err(CursorPositionError::NonFinitePosition)
    );
}

#[test]
fn public_service_admits_typed_requests_and_constraint_drop_releases_the_effect() {
    let view = ViewId::from_raw(8, 3).unwrap();
    let snapshot = view_state(view).snapshot();
    let active = Rc::new(Cell::new(false));
    let concrete = Rc::new(FixtureCursorService {
        view,
        metrics_revision: snapshot.metrics().revision(),
        next_request: Cell::new(40),
        constraint_active: Rc::clone(&active),
    });
    let service: Rc<dyn CursorService> = concrete;
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<CursorServiceKey>(service)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<CursorServiceKey>() else {
        panic!("registered cursor service must be available")
    };
    let capability = service
        .capability(CursorCapabilityQuery::new(view))
        .into_available()
        .unwrap();
    assert!(capability.operations().supports_custom_animation());
    assert!(capability.operations().supports_lock());

    let appearance = CursorAppearanceRequest::new(
        view,
        CursorAppearance::new(CursorSelection::Standard(StandardCursor::Pointer), true),
    );
    let applied = CursorAppearanceApplied::from_request(&appearance);
    let appearance_completion = service
        .set_appearance(appearance)
        .unwrap()
        .complete(RequestOutcome::Applied(applied));
    assert_eq!(
        appearance_completion.request_id(),
        RequestId::from_raw(41).unwrap()
    );
    assert_eq!(
        appearance_completion
            .outcome()
            .applied()
            .unwrap()
            .selection_kind(),
        CursorSelectionKind::Standard
    );

    let position = CursorPositionRequest::new(&snapshot, PointF { x: 10.0, y: 20.0 }).unwrap();
    let positioned = CursorPositionApplied::from_request(position);
    let position_completion = service
        .set_position(position)
        .unwrap()
        .complete(RequestOutcome::Applied(positioned));
    assert_eq!(
        position_completion.outcome().applied().unwrap().position(),
        PointF { x: 10.0, y: 20.0 }
    );

    let constraint = CursorConstraintRequest::new(view, CursorConstraintKind::Locked);
    let token = service.acquire_constraint(constraint).unwrap();
    assert!(active.get());
    assert!(matches!(
        service.acquire_constraint(constraint),
        Err(CursorAdmissionError::ConstraintAlreadyActive { .. })
    ));
    let lease: CursorConstraintLeaseHandle = Box::new(FixtureLease {
        id: CursorConstraintLeaseId::from_raw(2, 1).unwrap(),
        view,
        kind: CursorConstraintKind::Locked,
        active: Rc::clone(&active),
    });
    let completion = token.complete(RequestOutcome::Applied(lease));
    let retained = completion.outcome().applied().unwrap();
    assert_eq!(retained.view(), view);
    assert_eq!(retained.kind(), CursorConstraintKind::Locked);
    assert_eq!(retained.status(), CursorConstraintLeaseStatus::Active);
    drop(completion);
    assert!(!active.get());
}
