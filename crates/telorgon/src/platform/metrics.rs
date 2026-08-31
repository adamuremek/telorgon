//! Validated, revisioned view geometry and display facts.

use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;

use crate::core::{EdgeInsets, RectF, SizeF};

/// Maximum number of separately described IME/system/host avoidance regions in one publication.
pub const MAX_AVOID_REGIONS: usize = 32;

/// Coordinate space explicitly attached to an inset or rectangular metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoordinateSpace {
    ViewLogical,
    ViewPhysical,
    DisplayLogical,
    DisplayPhysical,
}

/// Unsigned physical-pixel extent. Either dimension may be zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalExtent {
    width: u32,
    height: u32,
}

impl PhysicalExtent {
    pub const ZERO: Self = Self::new(0, 0);

    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn is_renderable(self) -> bool {
        self.width != 0 && self.height != 0
    }
}

/// Validated uniform conversion factor from view-logical units to view-physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f32);

impl ScaleFactor {
    pub fn new(value: f32) -> Result<Self, ViewMetricsError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(ViewMetricsError::InvalidScaleFactor { value });
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for ScaleFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Explicit view-logical to view-physical transform.
///
/// View orientation is kept separately in [`DisplayProperties`]. A view-local origin remains the
/// same origin across these two spaces, so the current neutral transform is validated uniform
/// scale rather than an adapter-owned desktop position or render-area translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalToPhysicalTransform {
    scale_factor: ScaleFactor,
}

impl LogicalToPhysicalTransform {
    const fn new(scale_factor: ScaleFactor) -> Self {
        Self { scale_factor }
    }

    pub const fn source_space(self) -> CoordinateSpace {
        CoordinateSpace::ViewLogical
    }

    pub const fn destination_space(self) -> CoordinateSpace {
        CoordinateSpace::ViewPhysical
    }

    pub const fn scale_factor(self) -> ScaleFactor {
        self.scale_factor
    }
}

/// Display rotation without mirroring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayOrientation {
    #[default]
    Upright,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

/// Output transform including the mirrored variants used by native display protocols.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayTransform {
    #[default]
    Identity,
    Rotate90,
    Rotate180,
    Rotate270,
    Mirror,
    MirrorRotate90,
    MirrorRotate180,
    MirrorRotate270,
}

impl DisplayTransform {
    pub const fn orientation(self) -> DisplayOrientation {
        match self {
            Self::Identity | Self::Mirror => DisplayOrientation::Upright,
            Self::Rotate90 | Self::MirrorRotate90 => DisplayOrientation::Clockwise90,
            Self::Rotate180 | Self::MirrorRotate180 => DisplayOrientation::Clockwise180,
            Self::Rotate270 | Self::MirrorRotate270 => DisplayOrientation::Clockwise270,
        }
    }

    pub const fn is_mirrored(self) -> bool {
        matches!(
            self,
            Self::Mirror | Self::MirrorRotate90 | Self::MirrorRotate180 | Self::MirrorRotate270
        )
    }
}

/// Display color encoding reported for the current view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayColorSpace {
    #[default]
    Unknown,
    Srgb,
    DisplayP3,
    Rec2020,
}

/// Host-reported high-dynamic-range availability and current use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HdrState {
    #[default]
    Unknown,
    Unsupported,
    Supported,
    Active,
}

/// Renderer-relevant display facts copied into one metrics publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DisplayProperties {
    transform: DisplayTransform,
    color_space: DisplayColorSpace,
    hdr: HdrState,
}

impl DisplayProperties {
    pub const fn new(
        transform: DisplayTransform,
        color_space: DisplayColorSpace,
        hdr: HdrState,
    ) -> Self {
        Self {
            transform,
            color_space,
            hdr,
        }
    }

    pub const fn transform(self) -> DisplayTransform {
        self.transform
    }

    pub const fn orientation(self) -> DisplayOrientation {
        self.transform.orientation()
    }

    pub const fn color_space(self) -> DisplayColorSpace {
        self.color_space
    }

    pub const fn hdr(self) -> HdrState {
        self.hdr
    }
}

/// Validated edge insets with an explicit source coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricInsets {
    space: CoordinateSpace,
    value: EdgeInsets,
}

impl MetricInsets {
    pub fn new(space: CoordinateSpace, value: EdgeInsets) -> Result<Self, ViewMetricsError> {
        if !matches!(
            space,
            CoordinateSpace::ViewLogical | CoordinateSpace::ViewPhysical
        ) {
            return Err(ViewMetricsError::InvalidInsetSpace { space });
        }
        let values = [value.top, value.right, value.bottom, value.left];
        if values
            .into_iter()
            .any(|component| !component.is_finite() || component < 0.0)
        {
            return Err(ViewMetricsError::InvalidInsets { value });
        }
        Ok(Self { space, value })
    }

    const fn view_logical_zero() -> Self {
        Self {
            space: CoordinateSpace::ViewLogical,
            value: EdgeInsets::ZERO,
        }
    }

    pub const fn space(self) -> CoordinateSpace {
        self.space
    }

    pub const fn value(self) -> EdgeInsets {
        self.value
    }
}

/// Semantic source of a region that view content should avoid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvoidRegionKind {
    Ime,
    SystemUi,
    DisplayCutout,
    HostReserved,
}

/// Validated nonempty avoidance rectangle with an explicit source coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AvoidRegion {
    kind: AvoidRegionKind,
    space: CoordinateSpace,
    bounds: RectF,
}

impl AvoidRegion {
    pub fn new(
        kind: AvoidRegionKind,
        space: CoordinateSpace,
        bounds: RectF,
    ) -> Result<Self, ViewMetricsError> {
        let values = [bounds.x, bounds.y, bounds.width, bounds.height];
        if values.into_iter().any(|component| !component.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return Err(ViewMetricsError::InvalidAvoidRegion { bounds });
        }
        Ok(Self {
            kind,
            space,
            bounds,
        })
    }

    pub const fn kind(self) -> AvoidRegionKind {
        self.kind
    }

    pub const fn space(self) -> CoordinateSpace {
        self.space
    }

    pub const fn bounds(self) -> RectF {
        self.bounds
    }
}

/// Validated immutable metrics values before publication revision is attached.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewMetrics {
    physical_extent: PhysicalExtent,
    logical_extent: SizeF,
    scale_factor: ScaleFactor,
    logical_to_physical: LogicalToPhysicalTransform,
    display: DisplayProperties,
    safe_drawing_insets: MetricInsets,
    safe_gesture_insets: MetricInsets,
    avoid_regions: Arc<[AvoidRegion]>,
}

impl ViewMetrics {
    pub fn new(
        physical_extent: PhysicalExtent,
        scale_factor: ScaleFactor,
        display: DisplayProperties,
    ) -> Result<Self, ViewMetricsError> {
        let scale = scale_factor.get();
        let logical_extent = SizeF {
            width: physical_extent.width as f32 / scale,
            height: physical_extent.height as f32 / scale,
        };
        if !logical_extent.width.is_finite() || !logical_extent.height.is_finite() {
            return Err(ViewMetricsError::NonFiniteLogicalExtent {
                physical_extent,
                scale_factor,
            });
        }

        Ok(Self {
            physical_extent,
            logical_extent,
            scale_factor,
            logical_to_physical: LogicalToPhysicalTransform::new(scale_factor),
            display,
            safe_drawing_insets: MetricInsets::view_logical_zero(),
            safe_gesture_insets: MetricInsets::view_logical_zero(),
            avoid_regions: Arc::from([]),
        })
    }

    pub fn with_safe_drawing_insets(
        mut self,
        insets: MetricInsets,
    ) -> Result<Self, ViewMetricsError> {
        self.validate_insets(InsetKind::SafeDrawing, insets)?;
        self.safe_drawing_insets = insets;
        Ok(self)
    }

    pub fn with_safe_gesture_insets(
        mut self,
        insets: MetricInsets,
    ) -> Result<Self, ViewMetricsError> {
        self.validate_insets(InsetKind::SafeGesture, insets)?;
        self.safe_gesture_insets = insets;
        Ok(self)
    }

    pub fn with_avoid_regions(
        mut self,
        regions: Vec<AvoidRegion>,
    ) -> Result<Self, ViewMetricsError> {
        if regions.len() > MAX_AVOID_REGIONS {
            return Err(ViewMetricsError::TooManyAvoidRegions {
                count: regions.len(),
                maximum: MAX_AVOID_REGIONS,
            });
        }
        self.avoid_regions = Arc::from(regions);
        Ok(self)
    }

    pub const fn physical_extent(&self) -> PhysicalExtent {
        self.physical_extent
    }

    pub const fn logical_extent(&self) -> SizeF {
        self.logical_extent
    }

    pub const fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }

    pub const fn logical_to_physical(&self) -> LogicalToPhysicalTransform {
        self.logical_to_physical
    }

    pub const fn display(&self) -> DisplayProperties {
        self.display
    }

    pub const fn safe_drawing_insets(&self) -> MetricInsets {
        self.safe_drawing_insets
    }

    pub const fn safe_gesture_insets(&self) -> MetricInsets {
        self.safe_gesture_insets
    }

    pub fn avoid_regions(&self) -> &[AvoidRegion] {
        &self.avoid_regions
    }

    pub const fn is_renderable(&self) -> bool {
        self.physical_extent.is_renderable()
    }

    fn validate_insets(
        &self,
        kind: InsetKind,
        insets: MetricInsets,
    ) -> Result<(), ViewMetricsError> {
        let extent = match insets.space {
            CoordinateSpace::ViewLogical => self.logical_extent,
            CoordinateSpace::ViewPhysical => SizeF {
                width: self.physical_extent.width as f32,
                height: self.physical_extent.height as f32,
            },
            CoordinateSpace::DisplayLogical | CoordinateSpace::DisplayPhysical => {
                return Err(ViewMetricsError::InvalidInsetSpace {
                    space: insets.space,
                });
            }
        };
        if insets.value.horizontal() > extent.width || insets.value.vertical() > extent.height {
            return Err(ViewMetricsError::InsetsExceedExtent {
                kind,
                space: insets.space,
            });
        }
        Ok(())
    }
}

impl Default for ViewMetrics {
    fn default() -> Self {
        Self {
            physical_extent: PhysicalExtent::ZERO,
            logical_extent: SizeF {
                width: 0.0,
                height: 0.0,
            },
            scale_factor: ScaleFactor::default(),
            logical_to_physical: LogicalToPhysicalTransform::new(ScaleFactor::default()),
            display: DisplayProperties::default(),
            safe_drawing_insets: MetricInsets::view_logical_zero(),
            safe_gesture_insets: MetricInsets::view_logical_zero(),
            avoid_regions: Arc::from([]),
        }
    }
}

/// Inset field cited by a validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InsetKind {
    SafeDrawing,
    SafeGesture,
}

/// Monotonic revision of one view's complete metrics publication.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MetricsRevision(NonZeroU64);

impl MetricsRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    pub const fn new(revision: NonZeroU64) -> Self {
        Self(revision)
    }

    pub const fn from_raw(revision: u64) -> Option<Self> {
        match NonZeroU64::new(revision) {
            Some(revision) => Some(Self(revision)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(next) => Self::from_raw(next),
            None => None,
        }
    }
}

impl fmt::Display for MetricsRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Immutable retained metrics publication.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewMetricsSnapshot {
    revision: MetricsRevision,
    metrics: Arc<ViewMetrics>,
}

impl ViewMetricsSnapshot {
    pub const fn revision(&self) -> MetricsRevision {
        self.revision
    }

    pub fn metrics(&self) -> &ViewMetrics {
        &self.metrics
    }

    pub fn is_renderable(&self) -> bool {
        self.metrics.is_renderable()
    }
}

/// Before/after result of one accepted or redundant metrics observation.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewMetricsUpdate {
    previous: ViewMetricsSnapshot,
    current: ViewMetricsSnapshot,
}

impl ViewMetricsUpdate {
    pub const fn previous(&self) -> &ViewMetricsSnapshot {
        &self.previous
    }

    pub const fn current(&self) -> &ViewMetricsSnapshot {
        &self.current
    }

    pub const fn is_changed(&self) -> bool {
        self.previous.revision.get() != self.current.revision.get()
    }
}

/// Validation or revision failure that leaves the current publication unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewMetricsError {
    InvalidScaleFactor {
        value: f32,
    },
    NonFiniteLogicalExtent {
        physical_extent: PhysicalExtent,
        scale_factor: ScaleFactor,
    },
    InvalidInsetSpace {
        space: CoordinateSpace,
    },
    InvalidInsets {
        value: EdgeInsets,
    },
    InsetsExceedExtent {
        kind: InsetKind,
        space: CoordinateSpace,
    },
    InvalidAvoidRegion {
        bounds: RectF,
    },
    TooManyAvoidRegions {
        count: usize,
        maximum: usize,
    },
    RevisionExhausted {
        revision: MetricsRevision,
    },
}

impl fmt::Display for ViewMetricsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScaleFactor { value } => {
                write!(
                    formatter,
                    "view scale factor must be finite and positive, got {value}"
                )
            }
            Self::NonFiniteLogicalExtent { .. } => {
                formatter.write_str("physical extent and scale produce a non-finite logical extent")
            }
            Self::InvalidInsetSpace { space } => {
                write!(
                    formatter,
                    "view edge insets cannot use {space:?} coordinates"
                )
            }
            Self::InvalidInsets { .. } => {
                formatter.write_str("view edge insets must be finite and nonnegative")
            }
            Self::InsetsExceedExtent { kind, space } => write!(
                formatter,
                "{kind:?} insets exceed the current {space:?} view extent"
            ),
            Self::InvalidAvoidRegion { .. } => {
                formatter.write_str("avoid region must have finite coordinates and positive extent")
            }
            Self::TooManyAvoidRegions { count, maximum } => write!(
                formatter,
                "view metrics contain {count} avoid regions, exceeding the limit of {maximum}"
            ),
            Self::RevisionExhausted { revision } => {
                write!(formatter, "view metrics revision {revision} cannot advance")
            }
        }
    }
}

impl Error for ViewMetricsError {}

/// Single-view owner of retained, atomically revisioned metrics.
#[derive(Debug)]
pub struct ViewMetricsState {
    revision: MetricsRevision,
    current: Arc<ViewMetrics>,
}

impl ViewMetricsState {
    pub fn new(initial: ViewMetrics) -> Self {
        Self {
            revision: MetricsRevision::INITIAL,
            current: Arc::new(initial),
        }
    }

    pub const fn revision(&self) -> MetricsRevision {
        self.revision
    }

    pub fn current(&self) -> &ViewMetrics {
        &self.current
    }

    pub fn snapshot(&self) -> ViewMetricsSnapshot {
        ViewMetricsSnapshot {
            revision: self.revision,
            metrics: Arc::clone(&self.current),
        }
    }

    pub fn update(&mut self, next: ViewMetrics) -> Result<ViewMetricsUpdate, ViewMetricsError> {
        let (changed, revision) = self.validate_update(&next)?;
        Ok(self.publish_validated(next, changed, revision))
    }

    pub(crate) fn validate_update(
        &self,
        next: &ViewMetrics,
    ) -> Result<(bool, MetricsRevision), ViewMetricsError> {
        if self.current.as_ref() == next {
            return Ok((false, self.revision));
        }
        let revision = self
            .revision
            .checked_next()
            .ok_or(ViewMetricsError::RevisionExhausted {
                revision: self.revision,
            })?;
        Ok((true, revision))
    }

    pub(crate) fn publish_validated(
        &mut self,
        next: ViewMetrics,
        changed: bool,
        revision: MetricsRevision,
    ) -> ViewMetricsUpdate {
        let previous = self.snapshot();
        if changed {
            self.current = Arc::new(next);
            self.revision = revision;
        }
        ViewMetricsUpdate {
            previous,
            current: self.snapshot(),
        }
    }
}

impl Default for ViewMetricsState {
    fn default() -> Self {
        Self::new(ViewMetrics::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinary_metrics() -> ViewMetrics {
        ViewMetrics::new(
            PhysicalExtent::new(1200, 800),
            ScaleFactor::new(2.0).unwrap(),
            DisplayProperties::new(
                DisplayTransform::Rotate90,
                DisplayColorSpace::DisplayP3,
                HdrState::Supported,
            ),
        )
        .unwrap()
    }

    fn ime_region() -> AvoidRegion {
        AvoidRegion::new(
            AvoidRegionKind::Ime,
            CoordinateSpace::ViewLogical,
            RectF {
                x: 0.0,
                y: 300.0,
                width: 600.0,
                height: 100.0,
            },
        )
        .unwrap()
    }

    fn assert_snapshot<T: Clone + PartialEq + Send + Sync + 'static>() {}

    #[test]
    fn scale_derives_coherent_logical_extent_and_named_transform_spaces() {
        let metrics = ordinary_metrics();
        assert_eq!(metrics.physical_extent(), PhysicalExtent::new(1200, 800));
        assert_eq!(
            metrics.logical_extent(),
            SizeF {
                width: 600.0,
                height: 400.0,
            }
        );
        assert_eq!(
            metrics.logical_to_physical().source_space(),
            CoordinateSpace::ViewLogical
        );
        assert_eq!(
            metrics.logical_to_physical().destination_space(),
            CoordinateSpace::ViewPhysical
        );
        assert_eq!(metrics.logical_to_physical().scale_factor().get(), 2.0);
        assert!(metrics.is_renderable());

        let fractional = ViewMetrics::new(
            PhysicalExtent::new(1001, 501),
            ScaleFactor::new(1.25).unwrap(),
            DisplayProperties::default(),
        )
        .unwrap()
        .logical_extent();
        assert!((fractional.width - 800.8).abs() < 0.001);
        assert!((fractional.height - 400.8).abs() < 0.001);
    }

    #[test]
    fn zero_extent_is_preserved_and_never_clamped_to_a_renderable_size() {
        for extent in [
            PhysicalExtent::new(0, 800),
            PhysicalExtent::new(1200, 0),
            PhysicalExtent::ZERO,
        ] {
            let metrics = ViewMetrics::new(
                extent,
                ScaleFactor::new(2.0).unwrap(),
                DisplayProperties::default(),
            )
            .unwrap();
            assert_eq!(metrics.physical_extent(), extent);
            assert!(!metrics.is_renderable());
        }
    }

    #[test]
    fn invalid_scale_and_derived_overflow_are_rejected() {
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(matches!(
                ScaleFactor::new(value),
                Err(ViewMetricsError::InvalidScaleFactor { .. })
            ));
        }
        let tiny = ScaleFactor::new(f32::MIN_POSITIVE).unwrap();
        assert!(matches!(
            ViewMetrics::new(
                PhysicalExtent::new(u32::MAX, 1),
                tiny,
                DisplayProperties::default()
            ),
            Err(ViewMetricsError::NonFiniteLogicalExtent { .. })
        ));
    }

    #[test]
    fn safe_insets_are_finite_nonnegative_view_values_that_fit_the_extent() {
        let drawing = MetricInsets::new(
            CoordinateSpace::ViewLogical,
            EdgeInsets {
                top: 10.0,
                right: 20.0,
                bottom: 30.0,
                left: 20.0,
            },
        )
        .unwrap();
        let gesture =
            MetricInsets::new(CoordinateSpace::ViewPhysical, EdgeInsets::all(16.0)).unwrap();
        let metrics = ordinary_metrics()
            .with_safe_drawing_insets(drawing)
            .unwrap()
            .with_safe_gesture_insets(gesture)
            .unwrap();
        assert_eq!(metrics.safe_drawing_insets(), drawing);
        assert_eq!(metrics.safe_gesture_insets(), gesture);

        assert!(matches!(
            MetricInsets::new(CoordinateSpace::DisplayPhysical, EdgeInsets::ZERO),
            Err(ViewMetricsError::InvalidInsetSpace { .. })
        ));
        assert!(matches!(
            MetricInsets::new(CoordinateSpace::ViewLogical, EdgeInsets::all(-1.0)),
            Err(ViewMetricsError::InvalidInsets { .. })
        ));
        let too_large = MetricInsets::new(
            CoordinateSpace::ViewLogical,
            EdgeInsets {
                top: 0.0,
                right: 301.0,
                bottom: 0.0,
                left: 300.0,
            },
        )
        .unwrap();
        assert!(matches!(
            ordinary_metrics().with_safe_drawing_insets(too_large),
            Err(ViewMetricsError::InsetsExceedExtent { .. })
        ));
    }

    #[test]
    fn avoid_regions_are_typed_bounded_and_coordinate_space_explicit() {
        let region = ime_region();
        let metrics = ordinary_metrics().with_avoid_regions(vec![region]).unwrap();
        assert_eq!(metrics.avoid_regions(), &[region]);
        assert_eq!(region.kind(), AvoidRegionKind::Ime);
        assert_eq!(region.space(), CoordinateSpace::ViewLogical);

        assert!(matches!(
            AvoidRegion::new(
                AvoidRegionKind::SystemUi,
                CoordinateSpace::DisplayPhysical,
                RectF::ZERO
            ),
            Err(ViewMetricsError::InvalidAvoidRegion { .. })
        ));
        assert!(matches!(
            ordinary_metrics().with_avoid_regions(vec![region; MAX_AVOID_REGIONS + 1]),
            Err(ViewMetricsError::TooManyAvoidRegions { .. })
        ));
    }

    #[test]
    fn display_properties_preserve_transform_color_and_hdr_without_backend_types() {
        let display = ordinary_metrics().display();
        assert_eq!(display.transform(), DisplayTransform::Rotate90);
        assert_eq!(display.orientation(), DisplayOrientation::Clockwise90);
        assert!(!display.transform().is_mirrored());
        assert_eq!(display.color_space(), DisplayColorSpace::DisplayP3);
        assert_eq!(display.hdr(), HdrState::Supported);
    }

    #[test]
    fn state_reuses_equal_publications_and_rejects_exhaustion_atomically() {
        let initial = ordinary_metrics();
        let mut state = ViewMetricsState::new(initial.clone());
        let redundant = state.update(initial).unwrap();
        assert!(!redundant.is_changed());
        assert_eq!(redundant.previous(), redundant.current());

        let changed = state
            .update(
                ViewMetrics::new(
                    PhysicalExtent::new(600, 400),
                    ScaleFactor::new(1.0).unwrap(),
                    DisplayProperties::default(),
                )
                .unwrap(),
            )
            .unwrap();
        assert!(changed.is_changed());
        assert_eq!(changed.current().revision().get(), 2);
        assert_eq!(changed.current().metrics().physical_extent().width(), 600);

        state.revision = MetricsRevision::from_raw(u64::MAX).unwrap();
        let before = state.snapshot();
        assert_eq!(
            state.update(ViewMetrics::default()),
            Err(ViewMetricsError::RevisionExhausted {
                revision: MetricsRevision::from_raw(u64::MAX).unwrap(),
            })
        );
        assert_eq!(state.snapshot(), before);
        assert_snapshot::<ViewMetricsSnapshot>();
        assert_snapshot::<ViewMetricsUpdate>();
    }
}
