//! Axis-specific scrollbar projection over a controller-owned metrics snapshot.

use std::fmt;

use crate::core::{ColorRgba8, PointF, Transform2D};
use crate::runtime::{RuntimeError, RuntimeResult, Ui};
use crate::ui::{
    Background, Border, BoxStyle, ControlHandle, CornerRadii, Flow, LayoutStyle, Property,
    SemanticAction, SemanticActions, SemanticName, SemanticNode, SemanticRole, SemanticState,
    SemanticValue, SizeRule, SizeRule2D, UiNodeId,
};

use crate::application_components::{
    DensityMetrics, ScrollController, ScrollControllerCommand, ScrollInputSource,
};

use super::{ScrollMetrics, ScrollViewAxis};

/// One scrollbar request translated into an unapplied controller command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollBarCommand {
    LineBackward,
    LineForward,
    PageBackward,
    PageForward,
    ToStart,
    ToEnd,
    SetOffset(f32),
}

/// Axis projection of one immutable controller metrics snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarModel {
    metrics: ScrollMetrics,
    axis: ScrollViewAxis,
}

impl ScrollBarModel {
    pub fn from_controller(controller: &ScrollController, axis: ScrollViewAxis) -> Self {
        Self {
            metrics: controller.metrics(),
            axis,
        }
    }

    pub const fn metrics(self) -> ScrollMetrics {
        self.metrics
    }

    pub const fn axis(self) -> ScrollViewAxis {
        self.axis
    }

    pub fn offset(self) -> f32 {
        self.axis_value(self.metrics.offset)
    }

    pub fn maximum_offset(self) -> f32 {
        self.axis_value(self.metrics.max_offset())
    }

    pub fn viewport_extent(self) -> f32 {
        match self.axis {
            ScrollViewAxis::Horizontal => self.metrics.viewport.width,
            ScrollViewAxis::Vertical => self.metrics.viewport.height,
        }
    }

    pub fn content_extent(self) -> f32 {
        match self.axis {
            ScrollViewAxis::Horizontal => self.metrics.content.width,
            ScrollViewAxis::Vertical => self.metrics.content.height,
        }
    }

    pub fn can_scroll_backward(self) -> bool {
        match self.axis {
            ScrollViewAxis::Horizontal => self.metrics.can_scroll_left(),
            ScrollViewAxis::Vertical => self.metrics.can_scroll_up(),
        }
    }

    pub fn can_scroll_forward(self) -> bool {
        match self.axis {
            ScrollViewAxis::Horizontal => self.metrics.can_scroll_right(),
            ScrollViewAxis::Vertical => self.metrics.can_scroll_down(),
        }
    }

    pub fn is_scrollable(self) -> bool {
        self.maximum_offset() > 0.0
    }

    /// Fraction of the track occupied by visible content before applying a minimum thumb extent.
    pub fn thumb_fraction(self) -> f32 {
        let content = self.content_extent();
        if content <= 0.0 {
            1.0
        } else {
            (self.viewport_extent() / content).clamp(0.0, 1.0)
        }
    }

    /// Fraction of available thumb travel occupied by the current offset.
    pub fn position_fraction(self) -> f32 {
        let maximum = self.maximum_offset();
        if maximum <= 0.0 {
            0.0
        } else {
            (self.offset() / maximum).clamp(0.0, 1.0)
        }
    }

    fn axis_value(self, point: PointF) -> f32 {
        match self.axis {
            ScrollViewAxis::Horizontal => point.x,
            ScrollViewAxis::Vertical => point.y,
        }
    }

    fn point_with_offset(self, offset: f32) -> PointF {
        match self.axis {
            ScrollViewAxis::Horizontal => PointF {
                x: offset,
                y: self.metrics.offset.y,
            },
            ScrollViewAxis::Vertical => PointF {
                x: self.metrics.offset.x,
                y: offset,
            },
        }
    }

    fn delta(self, distance: f32) -> PointF {
        match self.axis {
            ScrollViewAxis::Horizontal => PointF {
                x: distance,
                y: 0.0,
            },
            ScrollViewAxis::Vertical => PointF {
                x: 0.0,
                y: distance,
            },
        }
    }
}

/// Validated visual track used to project snapshot geometry and absolute thumb movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarTrackGeometry {
    start: f32,
    length: f32,
    minimum_thumb_extent: f32,
}

impl ScrollBarTrackGeometry {
    pub fn new(start: f32, length: f32, minimum_thumb_extent: f32) -> Result<Self, ScrollBarError> {
        if !start.is_finite()
            || !length.is_finite()
            || length <= 0.0
            || !minimum_thumb_extent.is_finite()
            || minimum_thumb_extent <= 0.0
            || minimum_thumb_extent > length
        {
            return Err(ScrollBarError::InvalidTrackGeometry);
        }
        Ok(Self {
            start,
            length,
            minimum_thumb_extent,
        })
    }

    pub const fn start(self) -> f32 {
        self.start
    }

    pub const fn length(self) -> f32 {
        self.length
    }

    pub const fn minimum_thumb_extent(self) -> f32 {
        self.minimum_thumb_extent
    }

    pub fn project(self, model: ScrollBarModel) -> ScrollBarThumbGeometry {
        let extent = (self.length * model.thumb_fraction())
            .max(self.minimum_thumb_extent)
            .min(self.length);
        let travel = self.length - extent;
        ScrollBarThumbGeometry {
            origin: self.start + travel * model.position_fraction(),
            extent,
            travel,
        }
    }

    fn offset_for_thumb_origin(
        self,
        model: ScrollBarModel,
        thumb_origin: f32,
    ) -> Result<f32, ScrollBarError> {
        if !thumb_origin.is_finite() {
            return Err(ScrollBarError::InvalidThumbOrigin);
        }
        let thumb = self.project(model);
        if thumb.travel <= 0.0 {
            return Ok(model.offset());
        }
        let fraction = ((thumb_origin - self.start) / thumb.travel).clamp(0.0, 1.0);
        Ok(model.maximum_offset() * fraction)
    }
}

/// Resolved thumb origin, extent, and available travel along a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarThumbGeometry {
    origin: f32,
    extent: f32,
    travel: f32,
}

impl ScrollBarThumbGeometry {
    pub const fn origin(self) -> f32 {
        self.origin
    }

    pub const fn extent(self) -> f32 {
        self.extent
    }

    pub const fn travel(self) -> f32 {
        self.travel
    }
}

/// Stateless request behavior over one [`ScrollBarModel`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarBehavior {
    model: ScrollBarModel,
    line_extent: f32,
    enabled: bool,
}

impl ScrollBarBehavior {
    pub fn from_controller(
        controller: &ScrollController,
        axis: ScrollViewAxis,
        line_extent: f32,
        enabled: bool,
    ) -> Result<Self, ScrollBarError> {
        validate_line_extent(line_extent)?;
        Ok(Self {
            model: ScrollBarModel::from_controller(controller, axis),
            line_extent,
            enabled,
        })
    }

    pub const fn model(self) -> ScrollBarModel {
        self.model
    }

    pub const fn line_extent(self) -> f32 {
        self.line_extent
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }

    pub fn request(
        self,
        command: ScrollBarCommand,
        source: ScrollInputSource,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        if let ScrollBarCommand::SetOffset(offset) = command
            && !offset.is_finite()
        {
            return Err(ScrollBarError::InvalidOffset);
        }
        if !self.enabled {
            return Ok(None);
        }
        let current = self.model.offset();
        let controller_command = match command {
            ScrollBarCommand::LineBackward if self.model.can_scroll_backward() => {
                Some(self.scroll_by(-self.line_extent, source))
            }
            ScrollBarCommand::LineForward if self.model.can_scroll_forward() => {
                Some(self.scroll_by(self.line_extent, source))
            }
            ScrollBarCommand::PageBackward
                if self.model.can_scroll_backward() && self.model.viewport_extent() > 0.0 =>
            {
                Some(self.scroll_by(-self.model.viewport_extent(), source))
            }
            ScrollBarCommand::PageForward
                if self.model.can_scroll_forward() && self.model.viewport_extent() > 0.0 =>
            {
                Some(self.scroll_by(self.model.viewport_extent(), source))
            }
            ScrollBarCommand::ToStart if self.model.can_scroll_backward() => {
                Some(self.scroll_to(0.0, source))
            }
            ScrollBarCommand::ToEnd if self.model.can_scroll_forward() => {
                Some(self.scroll_to(self.model.maximum_offset(), source))
            }
            ScrollBarCommand::SetOffset(offset) if offset != current => {
                Some(self.scroll_to(offset, source))
            }
            ScrollBarCommand::LineBackward
            | ScrollBarCommand::LineForward
            | ScrollBarCommand::PageBackward
            | ScrollBarCommand::PageForward
            | ScrollBarCommand::ToStart
            | ScrollBarCommand::ToEnd
            | ScrollBarCommand::SetOffset(_) => None,
        };
        Ok(controller_command)
    }

    pub fn semantic_request(
        self,
        action: SemanticAction,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        let command = match action {
            SemanticAction::Increment => ScrollBarCommand::LineForward,
            SemanticAction::Decrement => ScrollBarCommand::LineBackward,
            SemanticAction::ScrollForward => ScrollBarCommand::PageForward,
            SemanticAction::ScrollBackward => ScrollBarCommand::PageBackward,
            _ => return Ok(None),
        };
        self.request(command, ScrollInputSource::Semantic)
    }

    pub fn semantic_set_value(
        self,
        offset: f64,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        let maximum = f64::from(self.model.maximum_offset());
        if !offset.is_finite() {
            return Err(ScrollBarError::InvalidOffset);
        }
        if offset < 0.0 || offset > maximum {
            return Err(ScrollBarError::OffsetOutOfRange);
        }
        let offset = offset as f32;
        if !offset.is_finite() {
            return Err(ScrollBarError::InvalidOffset);
        }
        self.request(
            ScrollBarCommand::SetOffset(offset),
            ScrollInputSource::Semantic,
        )
    }

    /// Maps a caller-owned thumb origin to an absolute pointer-sourced controller request.
    pub fn drag_to_offset(
        self,
        thumb_origin: f32,
        track: ScrollBarTrackGeometry,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        let offset = track.offset_for_thumb_origin(self.model, thumb_origin)?;
        self.request(
            ScrollBarCommand::SetOffset(offset),
            ScrollInputSource::Pointer,
        )
    }

    pub fn semantic_actions(self) -> SemanticActions {
        if !self.enabled || !self.model.is_scrollable() {
            return SemanticActions::NONE;
        }
        let mut actions = SemanticActions::FOCUS | SemanticActions::SET_VALUE;
        if self.model.can_scroll_forward() {
            actions |= SemanticActions::INCREMENT;
        }
        if self.model.can_scroll_backward() {
            actions |= SemanticActions::DECREMENT;
        }
        actions
    }

    fn scroll_by(self, distance: f32, source: ScrollInputSource) -> ScrollControllerCommand {
        ScrollControllerCommand::ScrollBy {
            delta: self.model.delta(distance),
            source,
        }
    }

    fn scroll_to(self, offset: f32, source: ScrollInputSource) -> ScrollControllerCommand {
        ScrollControllerCommand::ScrollTo {
            offset: self.model.point_with_offset(offset),
            source,
        }
    }
}

/// Caller-supplied scrollbar visuals plus logical track geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarStyle {
    pub container: BoxStyle,
    pub track: BoxStyle,
    pub thumb: BoxStyle,
    pub track_extent: f32,
    pub track_thickness: f32,
    pub minimum_thumb_extent: f32,
}

impl Default for ScrollBarStyle {
    fn default() -> Self {
        Self {
            container: BoxStyle {
                decoration: crate::ui::BoxDecoration {
                    background: Background::None,
                    ..crate::ui::BoxDecoration::default()
                },
                ..BoxStyle::default()
            },
            track: BoxStyle {
                decoration: crate::ui::BoxDecoration {
                    background: Background::Color(ColorRgba8::rgba(74, 84, 103, 190)),
                    corner_radii: CornerRadii::all(4.0),
                    ..crate::ui::BoxDecoration::default()
                },
                ..BoxStyle::default()
            },
            thumb: BoxStyle {
                decoration: crate::ui::BoxDecoration {
                    background: Background::Color(ColorRgba8::rgba(205, 212, 226, 255)),
                    border: Border::all(1.0, ColorRgba8::rgba(99, 112, 139, 255)),
                    corner_radii: CornerRadii::all(4.0),
                    ..crate::ui::BoxDecoration::default()
                },
                ..BoxStyle::default()
            },
            track_extent: 160.0,
            track_thickness: 8.0,
            minimum_thumb_extent: 24.0,
        }
    }
}

/// Mounted scrollbar over one immutable controller snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollBar {
    label: String,
    snapshot: ScrollMetrics,
    axis: ScrollViewAxis,
    line_extent: f32,
    enabled: bool,
    density: DensityMetrics,
    style: ScrollBarStyle,
}

impl ScrollBar {
    pub fn new(
        label: impl Into<String>,
        controller: &ScrollController,
    ) -> Result<Self, ScrollBarError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(ScrollBarError::MissingAccessibleName);
        }
        Ok(Self {
            label,
            snapshot: controller.metrics(),
            axis: ScrollViewAxis::Vertical,
            line_extent: 40.0,
            enabled: true,
            density: DensityMetrics::baseline(
                crate::application_components::DensityClass::Standard,
            ),
            style: ScrollBarStyle::default(),
        })
    }

    pub const fn axis(mut self, axis: ScrollViewAxis) -> Self {
        self.axis = axis;
        self
    }

    pub fn line_extent(mut self, line_extent: f32) -> Result<Self, ScrollBarError> {
        validate_line_extent(line_extent)?;
        self.line_extent = line_extent;
        Ok(self)
    }

    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub const fn density(mut self, density: DensityMetrics) -> Self {
        self.density = density;
        self
    }

    pub const fn style(mut self, style: ScrollBarStyle) -> Self {
        self.style = style;
        self
    }

    pub const fn snapshot(&self) -> ScrollMetrics {
        self.snapshot
    }

    pub fn behavior(&self) -> Result<ScrollBarBehavior, ScrollBarError> {
        validate_line_extent(self.line_extent)?;
        Ok(ScrollBarBehavior {
            model: ScrollBarModel {
                metrics: self.snapshot,
                axis: self.axis,
            },
            line_extent: self.line_extent,
            enabled: self.enabled,
        })
    }

    pub fn mount<Action: 'static>(
        &self,
        ui: &mut Ui<'_, '_, Action>,
        host: UiNodeId,
    ) -> RuntimeResult<ScrollBarRef> {
        let behavior = self
            .behavior()
            .map_err(|error| RuntimeError::new(format!("invalid scrollbar behavior: {error}")))?;
        let resolved = resolve_style(self.style, self.axis, self.density, behavior.model())
            .map_err(|error| RuntimeError::new(format!("invalid scrollbar style: {error}")))?;
        let mut track = None;
        let mut thumb = None;
        let control = ui
            .foundation()
            .action_node_under(
                host,
                resolved.container,
                self.enabled,
                behavior.model().is_scrollable(),
                |writer| {
                    track = Some(writer.container(
                        resolved.track,
                        LayoutStyle {
                            flow: Flow::Overlay,
                            ..LayoutStyle::default()
                        },
                        |writer| {
                            thumb = Some(writer.container(
                                resolved.thumb,
                                LayoutStyle::default(),
                                |_| {},
                            ));
                        },
                    ));
                },
            )
            .ok_or_else(|| RuntimeError::new("application scrollbar host is stale"))?;
        let track = track.expect("scrollbar mounts its track");
        let thumb = thumb.expect("scrollbar mounts its thumb");

        let name = ui.foundation().intern(&self.label);
        let value_text = ui.foundation().intern(format!(
            "{} of {} logical pixels",
            behavior.model().offset(),
            behavior.model().maximum_offset()
        ));
        ui.foundation()
            .semantic_node(
                control.node,
                SemanticNode {
                    role: SemanticRole::ScrollBar,
                    name: SemanticName::Text(name),
                    state: SemanticState {
                        disabled: !self.enabled,
                        focusable: self.enabled && behavior.model().is_scrollable(),
                        ..SemanticState::default()
                    },
                    value: SemanticValue::Number {
                        current: f64::from(behavior.model().offset()),
                        minimum: 0.0,
                        maximum: f64::from(behavior.model().maximum_offset()),
                        step: Some(f64::from(self.line_extent)),
                        value_text: Some(value_text),
                    },
                    actions: behavior.semantic_actions(),
                    ..SemanticNode::default()
                },
            )
            .map_err(semantic_runtime_error)?;
        if !self.enabled {
            ui.foundation().disabled(control.node, true);
        }

        Ok(ScrollBarRef {
            control,
            track,
            thumb,
            behavior,
            track_geometry: resolved.track_geometry,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct ResolvedScrollBarStyle {
    container: BoxStyle,
    track: BoxStyle,
    thumb: BoxStyle,
    track_geometry: ScrollBarTrackGeometry,
}

fn resolve_style(
    style: ScrollBarStyle,
    axis: ScrollViewAxis,
    density: DensityMetrics,
    model: ScrollBarModel,
) -> Result<ResolvedScrollBarStyle, ScrollBarError> {
    if !style.track_thickness.is_finite() || style.track_thickness <= 0.0 {
        return Err(ScrollBarError::InvalidTrackGeometry);
    }
    let track_geometry =
        ScrollBarTrackGeometry::new(0.0, style.track_extent, style.minimum_thumb_extent)?;
    let thumb_geometry = track_geometry.project(model);
    let minimum = density.effective_minimum();
    let mut container = style.container;
    container.min_size = SizeRule2D {
        width: SizeRule::Px(minimum.width()),
        height: SizeRule::Px(minimum.height()),
    };
    let mut track = style.track;
    let mut thumb = style.thumb;
    match axis {
        ScrollViewAxis::Horizontal => {
            track.width = SizeRule::Px(style.track_extent);
            track.height = SizeRule::Px(style.track_thickness);
            thumb.width = SizeRule::Px(thumb_geometry.extent());
            thumb.height = SizeRule::Px(style.track_thickness);
            thumb.transform = Transform2D {
                translation: PointF {
                    x: thumb_geometry.origin(),
                    y: thumb.transform.translation.y,
                },
                ..thumb.transform
            };
        }
        ScrollViewAxis::Vertical => {
            track.width = SizeRule::Px(style.track_thickness);
            track.height = SizeRule::Px(style.track_extent);
            thumb.width = SizeRule::Px(style.track_thickness);
            thumb.height = SizeRule::Px(thumb_geometry.extent());
            thumb.transform = Transform2D {
                translation: PointF {
                    x: thumb.transform.translation.x,
                    y: thumb_geometry.origin(),
                },
                ..thumb.transform
            };
        }
    }
    Ok(ResolvedScrollBarStyle {
        container,
        track,
        thumb,
        track_geometry,
    })
}

fn validate_line_extent(line_extent: f32) -> Result<(), ScrollBarError> {
    if !line_extent.is_finite() || line_extent <= 0.0 {
        Err(ScrollBarError::InvalidLineExtent)
    } else {
        Ok(())
    }
}

fn semantic_runtime_error(error: crate::ui::SemanticError) -> RuntimeError {
    RuntimeError::new(format!("invalid scrollbar semantics: {error:?}"))
}

/// Stable mounted scrollbar, track, and thumb identities plus snapshot request behavior.
#[derive(Clone, Copy, Debug)]
pub struct ScrollBarRef {
    control: ControlHandle,
    track: UiNodeId,
    thumb: UiNodeId,
    behavior: ScrollBarBehavior,
    track_geometry: ScrollBarTrackGeometry,
}

impl ScrollBarRef {
    pub const fn node(self) -> UiNodeId {
        self.control.node
    }

    pub const fn track_node(self) -> UiNodeId {
        self.track
    }

    pub const fn thumb_node(self) -> UiNodeId {
        self.thumb
    }

    pub const fn style(self) -> Property<BoxStyle> {
        self.control.style
    }

    pub const fn snapshot(self) -> ScrollMetrics {
        self.behavior.model().metrics()
    }

    pub const fn track_geometry(self) -> ScrollBarTrackGeometry {
        self.track_geometry
    }

    pub fn request(
        self,
        command: ScrollBarCommand,
        source: ScrollInputSource,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        self.behavior.request(command, source)
    }

    pub fn semantic_request(
        self,
        action: SemanticAction,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        self.behavior.semantic_request(action)
    }

    pub fn semantic_set_value(
        self,
        offset: f64,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        self.behavior.semantic_set_value(offset)
    }

    pub fn drag_to_offset(
        self,
        thumb_origin: f32,
    ) -> Result<Option<ScrollControllerCommand>, ScrollBarError> {
        self.behavior
            .drag_to_offset(thumb_origin, self.track_geometry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarError {
    MissingAccessibleName,
    InvalidLineExtent,
    InvalidTrackGeometry,
    InvalidThumbOrigin,
    InvalidOffset,
    OffsetOutOfRange,
}

impl fmt::Display for ScrollBarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid scrollbar: {self:?}")
    }
}

impl std::error::Error for ScrollBarError {}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::core::SizeF;
    use crate::runtime::{Component, CreateContext, UpdateContext, ViewRuntime};
    use crate::ui::{NodeKind, SemanticAction, UiRoot};

    use super::*;
    use crate::application_components::{DensityClass, ScrollChangeSource};

    fn size(width: f32, height: f32) -> SizeF {
        SizeF { width, height }
    }

    fn vertical_controller(offset: f32) -> ScrollController {
        let mut controller = ScrollController::new(size(100.0, 100.0), size(100.0, 500.0)).unwrap();
        controller
            .route(ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: offset },
                source: ScrollInputSource::Programmatic,
            })
            .unwrap();
        controller
    }

    #[test]
    fn line_page_and_bound_commands_preserve_boundary_handoff_and_source() {
        let mut controller = vertical_controller(390.0);
        let behavior =
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 16.0, true)
                .unwrap();
        let command = behavior
            .request(ScrollBarCommand::LineForward, ScrollInputSource::Keyboard)
            .unwrap()
            .unwrap();
        assert_eq!(
            command,
            ScrollControllerCommand::ScrollBy {
                delta: PointF { x: 0.0, y: 16.0 },
                source: ScrollInputSource::Keyboard,
            }
        );
        assert_eq!(controller.metrics().offset.y, 390.0);
        let update = controller.route(command).unwrap().update();
        assert_eq!(update.after.offset.y, 400.0);
        assert_eq!(update.consumed_delta.y, 10.0);
        assert_eq!(update.unconsumed_delta.y, 6.0);
        assert_eq!(
            update.source,
            ScrollChangeSource::Input(ScrollInputSource::Keyboard)
        );

        let behavior =
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 16.0, true)
                .unwrap();
        assert_eq!(
            behavior
                .request(ScrollBarCommand::LineForward, ScrollInputSource::Keyboard)
                .unwrap(),
            None
        );
        assert_eq!(
            behavior
                .request(ScrollBarCommand::ToEnd, ScrollInputSource::Keyboard)
                .unwrap(),
            None
        );
        assert_eq!(
            behavior
                .request(ScrollBarCommand::PageBackward, ScrollInputSource::Keyboard)
                .unwrap(),
            Some(ScrollControllerCommand::ScrollBy {
                delta: PointF { x: 0.0, y: -100.0 },
                source: ScrollInputSource::Keyboard,
            })
        );
        assert_eq!(
            behavior
                .request(ScrollBarCommand::ToStart, ScrollInputSource::Keyboard)
                .unwrap(),
            Some(ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: 0.0 },
                source: ScrollInputSource::Keyboard,
            })
        );
    }

    #[test]
    fn model_and_track_project_thumb_extent_position_and_pointer_offset() {
        let controller = vertical_controller(100.0);
        let behavior =
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 20.0, true)
                .unwrap();
        let model = behavior.model();
        assert_eq!(model.offset(), 100.0);
        assert_eq!(model.maximum_offset(), 400.0);
        assert_eq!(model.thumb_fraction(), 0.2);
        assert_eq!(model.position_fraction(), 0.25);

        let track = ScrollBarTrackGeometry::new(10.0, 200.0, 24.0).unwrap();
        assert_eq!(
            track.project(model),
            ScrollBarThumbGeometry {
                origin: 50.0,
                extent: 40.0,
                travel: 160.0,
            }
        );
        assert_eq!(behavior.drag_to_offset(50.0, track).unwrap(), None);
        let pointer_command = behavior.drag_to_offset(90.0, track).unwrap().unwrap();
        assert_eq!(
            pointer_command,
            ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: 200.0 },
                source: ScrollInputSource::Pointer,
            }
        );
        let mut routed = controller.clone();
        let update = routed.route(pointer_command).unwrap().update();
        assert_eq!(update.after.offset.y, 200.0);
        assert_eq!(
            update.source,
            ScrollChangeSource::Input(ScrollInputSource::Pointer)
        );
        assert_eq!(
            behavior.drag_to_offset(1_000.0, track).unwrap(),
            Some(ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: 400.0 },
                source: ScrollInputSource::Pointer,
            })
        );
    }

    #[test]
    fn semantic_actions_and_values_are_boundary_aware_and_nonmutating() {
        let controller = vertical_controller(0.0);
        let behavior =
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 25.0, true)
                .unwrap();
        let actions = behavior.semantic_actions();
        assert!(actions.contains(SemanticAction::Focus));
        assert!(actions.contains(SemanticAction::Increment));
        assert!(actions.contains(SemanticAction::SetValue));
        assert!(!actions.contains(SemanticAction::Decrement));
        assert_eq!(
            behavior
                .semantic_request(SemanticAction::Increment)
                .unwrap(),
            Some(ScrollControllerCommand::ScrollBy {
                delta: PointF { x: 0.0, y: 25.0 },
                source: ScrollInputSource::Semantic,
            })
        );
        assert_eq!(
            behavior.semantic_set_value(200.0).unwrap(),
            Some(ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: 200.0 },
                source: ScrollInputSource::Semantic,
            })
        );
        assert_eq!(
            behavior.semantic_set_value(401.0),
            Err(ScrollBarError::OffsetOutOfRange)
        );
        assert_eq!(controller.metrics().offset.y, 0.0);

        let disabled =
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 25.0, false)
                .unwrap();
        assert!(disabled.semantic_actions().is_empty());
        assert_eq!(
            disabled
                .request(ScrollBarCommand::LineForward, ScrollInputSource::Keyboard)
                .unwrap(),
            None
        );
    }

    #[test]
    fn construction_and_geometry_reject_invalid_public_inputs() {
        let controller = vertical_controller(0.0);
        assert_eq!(
            ScrollBar::new(" ", &controller),
            Err(ScrollBarError::MissingAccessibleName)
        );
        assert_eq!(
            ScrollBarBehavior::from_controller(&controller, ScrollViewAxis::Vertical, 0.0, true),
            Err(ScrollBarError::InvalidLineExtent)
        );
        assert_eq!(
            ScrollBarTrackGeometry::new(0.0, 20.0, 21.0),
            Err(ScrollBarError::InvalidTrackGeometry)
        );
    }

    struct Fixture {
        controller: ScrollController,
        reference: Rc<RefCell<Option<ScrollBarRef>>>,
    }

    impl Component for Fixture {
        type State = ();
        type Action = ();

        fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

        fn mount(&self, _: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
            let root = ui
                .foundation()
                .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
            let style = ScrollBarStyle {
                track_extent: 200.0,
                track_thickness: 10.0,
                minimum_thumb_extent: 20.0,
                thumb: BoxStyle {
                    transform: Transform2D {
                        translation: PointF { x: 3.0, y: 0.0 },
                        ..Transform2D::default()
                    },
                    ..ScrollBarStyle::default().thumb
                },
                ..ScrollBarStyle::default()
            };
            let reference = ScrollBar::new("Document scroll position", &self.controller)
                .unwrap()
                .line_extent(25.0)
                .unwrap()
                .density(DensityMetrics::baseline(DensityClass::Touch))
                .style(style)
                .mount(ui, root.0)
                .unwrap();
            *self.reference.borrow_mut() = Some(reference);
            root
        }

        fn action(&self, _: &mut Self::State, _: Self::Action, _: &mut UpdateContext<'_, Self>) {}
    }

    #[test]
    fn mounted_scrollbar_has_stable_track_thumb_density_and_range_semantics() {
        let reference = Rc::new(RefCell::new(None));
        let runtime = ViewRuntime::from_component(Fixture {
            controller: vertical_controller(100.0),
            reference: reference.clone(),
        })
        .unwrap();
        let reference = reference.borrow().expect("scrollbar reference");
        let root = reference.node();
        let track = reference.track_node();
        let thumb = reference.thumb_node();
        assert_eq!(runtime.ui().kinds.get(root), Some(&NodeKind::Button));
        assert_eq!(runtime.ui().nodes.core(track).unwrap().parent, Some(root));
        assert_eq!(runtime.ui().nodes.core(thumb).unwrap().parent, Some(track));
        assert_eq!(
            runtime.ui().box_styles.get(root).unwrap().min_size,
            SizeRule2D {
                width: SizeRule::Px(44.0),
                height: SizeRule::Px(44.0),
            }
        );
        assert_eq!(
            runtime.ui().box_styles.get(track).unwrap().width,
            SizeRule::Px(10.0)
        );
        assert_eq!(
            runtime.ui().box_styles.get(track).unwrap().height,
            SizeRule::Px(200.0)
        );
        let thumb_style = runtime.ui().box_styles.get(thumb).unwrap();
        assert_eq!(thumb_style.width, SizeRule::Px(10.0));
        assert_eq!(thumb_style.height, SizeRule::Px(40.0));
        assert_eq!(
            thumb_style.transform.translation,
            PointF { x: 3.0, y: 40.0 }
        );

        let interaction = runtime.ui().interactions.get(root).unwrap();
        assert!(interaction.enabled);
        assert!(interaction.focusable);
        let semantic = runtime.ui().semantics.get(root).unwrap();
        assert_eq!(semantic.role, SemanticRole::ScrollBar);
        assert!(semantic.actions.contains(SemanticAction::Increment));
        assert!(semantic.actions.contains(SemanticAction::Decrement));
        assert!(semantic.actions.contains(SemanticAction::SetValue));
        assert_eq!(
            semantic.value,
            SemanticValue::Number {
                current: 100.0,
                minimum: 0.0,
                maximum: 400.0,
                step: Some(25.0),
                value_text: match semantic.value {
                    SemanticValue::Number { value_text, .. } => value_text,
                    _ => None,
                },
            }
        );
        assert_eq!(
            reference.drag_to_offset(80.0).unwrap(),
            Some(ScrollControllerCommand::ScrollTo {
                offset: PointF { x: 0.0, y: 200.0 },
                source: ScrollInputSource::Pointer,
            })
        );
    }
}
