use std::num::NonZeroU16;
use std::rc::Rc;

use telorgon::core::{EdgeInsets, RectF};
use telorgon::platform::{
    AvoidRegion, AvoidRegionKind, CapabilityLimit, CoordinateSpace, DisplayAccuracy,
    DisplayAccuracyProfile, DisplayCapability, DisplayChange, DisplayChangeError,
    DisplayColorSpace, DisplayDescriptor, DisplayDescriptorError, DisplayId, DisplayLimitError,
    DisplayLimits, DisplayLogicalBounds, DisplayOperations, DisplayProperties, DisplayRevision,
    DisplayService, DisplayServiceKey, DisplaySnapshot, DisplaySnapshotError,
    DisplaySnapshotStatus, DisplayTransform, ExecutionRequirement, HdrState, MAX_DISPLAYS,
    MetricInsets, PermissionState, PhysicalExtent, ScaleFactor, ServiceLookup, ServiceRegistry,
    Support, ViewDisplayError, ViewDisplaySnapshot, ViewDisplayStatus, ViewId, ViewMetrics,
    ViewState,
};

fn display_descriptor(id: DisplayId, x: f32) -> DisplayDescriptor {
    DisplayDescriptor::new(
        id,
        DisplayLogicalBounds::new(RectF {
            x,
            y: -120.0,
            width: 1_920.0,
            height: 1_080.0,
        })
        .unwrap(),
        PhysicalExtent::new(3_840, 2_160),
        ScaleFactor::new(2.0).unwrap(),
        DisplayProperties::new(
            DisplayTransform::Identity,
            DisplayColorSpace::DisplayP3,
            HdrState::Active,
        ),
    )
    .unwrap()
}

#[test]
fn display_enumeration_is_bounded_generation_safe_and_revisioned() {
    let first = DisplayId::from_raw(1, 1).unwrap();
    let replacement = DisplayId::from_raw(1, 2).unwrap();
    let second = DisplayId::from_raw(2, 1).unwrap();
    assert_ne!(first, replacement);

    assert_eq!(
        DisplayLogicalBounds::new(RectF {
            x: f32::NAN,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        }),
        Err(DisplayDescriptorError::NonFiniteLogicalBounds)
    );
    assert_eq!(
        DisplayLogicalBounds::new(RectF::ZERO),
        Err(DisplayDescriptorError::EmptyLogicalBounds)
    );
    let logical = DisplayLogicalBounds::new(RectF {
        x: -1_920.0,
        y: 0.0,
        width: 1_920.0,
        height: 1_080.0,
    })
    .unwrap();
    assert_eq!(logical.coordinate_space(), CoordinateSpace::DisplayLogical);
    assert_eq!(logical.bounds().x, -1_920.0);
    assert_eq!(
        DisplayDescriptor::new(
            first,
            logical,
            PhysicalExtent::ZERO,
            ScaleFactor::default(),
            DisplayProperties::default(),
        ),
        Err(DisplayDescriptorError::EmptyPhysicalExtent)
    );

    let first_descriptor = display_descriptor(first, 0.0);
    let second_descriptor = display_descriptor(second, 1_920.0);
    let snapshot = DisplaySnapshot::new(
        DisplayRevision::INITIAL,
        vec![first_descriptor, second_descriptor],
        Some(first),
    )
    .unwrap();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot.primary_display(), Some(&first_descriptor));
    assert_eq!(snapshot.display(second), Some(&second_descriptor));

    assert_eq!(
        DisplaySnapshot::new(
            DisplayRevision::INITIAL,
            vec![first_descriptor, first_descriptor],
            Some(first),
        ),
        Err(DisplaySnapshotError::DuplicateDisplay { display: first })
    );
    assert_eq!(
        DisplaySnapshot::new(
            DisplayRevision::INITIAL,
            vec![first_descriptor],
            Some(second),
        ),
        Err(DisplaySnapshotError::PrimaryDisplayMissing { display: second })
    );
    assert!(
        DisplaySnapshot::new(DisplayRevision::INITIAL, Vec::new(), None)
            .unwrap()
            .is_empty()
    );

    let too_many = (1..=MAX_DISPLAYS + 1)
        .map(|slot| display_descriptor(DisplayId::from_raw(slot as u32, 1).unwrap(), slot as f32))
        .collect();
    assert_eq!(
        DisplaySnapshot::new(DisplayRevision::INITIAL, too_many, None),
        Err(DisplaySnapshotError::TooManyDisplays {
            supplied: MAX_DISPLAYS + 1,
            maximum: MAX_DISPLAYS,
        })
    );

    assert_eq!(DisplayRevision::from_raw(0), None);
    let next_revision = DisplayRevision::INITIAL.checked_next().unwrap();
    let changed = DisplaySnapshot::new(
        next_revision,
        vec![display_descriptor(replacement, 0.0), second_descriptor],
        Some(replacement),
    )
    .unwrap();
    let change = DisplayChange::new(Some(DisplayRevision::INITIAL), changed.clone()).unwrap();
    assert_eq!(change.previous(), Some(DisplayRevision::INITIAL));
    assert_eq!(change.current(), &changed);
    assert_eq!(
        DisplayChange::new(Some(next_revision), changed),
        Err(DisplayChangeError::RevisionDidNotAdvance {
            previous: next_revision,
            current: next_revision,
        })
    );
}

#[test]
fn view_association_reuses_the_exact_canonical_metrics_publication() {
    let display = DisplayId::from_raw(7, 3).unwrap();
    let properties = DisplayProperties::new(
        DisplayTransform::MirrorRotate90,
        DisplayColorSpace::Rec2020,
        HdrState::Supported,
    );
    let drawing = MetricInsets::new(CoordinateSpace::ViewLogical, EdgeInsets::all(12.0)).unwrap();
    let gesture = MetricInsets::new(CoordinateSpace::ViewPhysical, EdgeInsets::all(20.0)).unwrap();
    let ime = AvoidRegion::new(
        AvoidRegionKind::Ime,
        CoordinateSpace::ViewLogical,
        RectF {
            x: 0.0,
            y: 420.0,
            width: 800.0,
            height: 180.0,
        },
    )
    .unwrap();
    let metrics = ViewMetrics::new(
        PhysicalExtent::new(1_600, 1_200),
        ScaleFactor::new(2.0).unwrap(),
        properties,
    )
    .unwrap()
    .with_safe_drawing_insets(drawing)
    .unwrap()
    .with_safe_gesture_insets(gesture)
    .unwrap()
    .with_avoid_regions(vec![ime])
    .unwrap();
    let view = ViewId::from_raw(4, 2).unwrap();
    let view_snapshot = ViewState::with_metrics(view, metrics).snapshot();
    let displays = DisplaySnapshot::new(
        DisplayRevision::INITIAL,
        vec![display_descriptor(display, 0.0)],
        Some(display),
    )
    .unwrap();
    let association = ViewDisplaySnapshot::new(&view_snapshot, &displays, Some(display)).unwrap();

    assert_eq!(association.view(), view);
    assert_eq!(association.view_revision(), view_snapshot.revision());
    assert_eq!(association.display(), Some(display));
    assert_eq!(association.display_revision(), displays.revision());
    assert_eq!(
        association.metrics_revision(),
        view_snapshot.metrics().revision()
    );
    assert_eq!(association.metrics(), view_snapshot.metrics());
    assert_eq!(association.scale_factor().get(), 2.0);
    assert_eq!(association.display_properties(), properties);
    assert_eq!(association.safe_drawing_insets(), drawing);
    assert_eq!(association.safe_gesture_insets(), gesture);
    assert_eq!(association.avoid_regions(), &[ime]);
    let missing = DisplayId::from_raw(99, 1).unwrap();
    assert_eq!(
        ViewDisplaySnapshot::new(&view_snapshot, &displays, Some(missing)),
        Err(ViewDisplayError::DisplayMissing {
            display: missing,
            revision: DisplayRevision::INITIAL,
        })
    );
}

#[derive(Clone)]
struct FixtureDisplayService {
    capability: DisplayCapability,
    snapshot: DisplaySnapshot,
    display: DisplayId,
}

impl DisplayService for FixtureDisplayService {
    fn capability(&self) -> Support<DisplayCapability> {
        Support::Available(self.capability)
    }

    fn current_snapshot(&self) -> DisplaySnapshotStatus {
        DisplaySnapshotStatus::Current(self.snapshot.clone())
    }

    fn for_view(&self, view: &telorgon::platform::ViewSnapshot) -> ViewDisplayStatus {
        ViewDisplayStatus::Current(
            ViewDisplaySnapshot::new(view, &self.snapshot, Some(self.display)).unwrap(),
        )
    }
}

#[test]
fn capability_accuracy_and_object_safe_registry_queries_remain_explicit() {
    assert_eq!(
        DisplayLimits::new(CapabilityLimit::Bounded(
            NonZeroU16::new(MAX_DISPLAYS as u16 + 1).unwrap()
        )),
        Err(DisplayLimitError::DisplayLimitTooLarge)
    );
    let limits = DisplayLimits::new(CapabilityLimit::Bounded(NonZeroU16::new(8).unwrap())).unwrap();
    let accuracy = DisplayAccuracyProfile::new(
        DisplayAccuracy::Exact,
        DisplayAccuracy::Exact,
        DisplayAccuracy::Exact,
        DisplayAccuracy::Estimated,
        DisplayAccuracy::Unknown,
        DisplayAccuracy::Exact,
        DisplayAccuracy::Estimated,
    );
    let capability = DisplayCapability::new(
        DisplayOperations::new(true, true, true),
        limits,
        accuracy,
        PermissionState::NotRequired,
        ExecutionRequirement::HostEventLoop,
    );
    assert!(capability.operations().supports_snapshot());
    assert!(capability.operations().supports_change_notifications());
    assert!(capability.operations().supports_view_association());
    assert_eq!(
        capability.limits().maximum_displays().into_bound(),
        NonZeroU16::new(8)
    );
    assert_eq!(
        capability.accuracy().logical_bounds(),
        DisplayAccuracy::Exact
    );
    assert_eq!(
        capability.accuracy().color_space(),
        DisplayAccuracy::Estimated
    );
    assert_eq!(capability.accuracy().hdr(), DisplayAccuracy::Unknown);
    assert_eq!(capability.permission(), PermissionState::NotRequired);
    assert_eq!(capability.execution(), ExecutionRequirement::HostEventLoop);

    let display = DisplayId::from_raw(2, 5).unwrap();
    let snapshot = DisplaySnapshot::new(
        DisplayRevision::INITIAL,
        vec![display_descriptor(display, 0.0)],
        Some(display),
    )
    .unwrap();
    let handle: Rc<dyn DisplayService> = Rc::new(FixtureDisplayService {
        capability,
        snapshot,
        display,
    });
    let mut registry = ServiceRegistry::new();
    assert!(
        registry
            .register::<DisplayServiceKey>(handle)
            .is_registered()
    );
    let ServiceLookup::Available(service) = registry.lookup::<DisplayServiceKey>() else {
        panic!("registered display service must be available");
    };
    assert!(service.capability().is_available());
    assert_eq!(
        service.current_snapshot().current().unwrap().primary(),
        Some(display)
    );
    let view_snapshot = ViewState::new(ViewId::from_raw(9, 1).unwrap()).snapshot();
    let associated = service.for_view(&view_snapshot);
    assert_eq!(associated.current().unwrap().display(), Some(display));
}
