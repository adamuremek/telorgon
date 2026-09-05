use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::application_host::WindowFrameTemplate;
use crate::assets::{IconAsset, ImageSource};
use crate::compose::{
    Alignment, Component, ComponentFields, Dimension, Element, Insets, View, button, column, image,
    row, spacer, stack, text, window_content_slot, window_frame,
};
use crate::core::ColorRgba8;
use crate::theme::{
    CompiledComponentStyle, CompiledSlotStyle, CompiledStateStyle, InteractionState, TransitionSpec,
};
use crate::ui::{
    Background, BoxDecoration, ComponentStyleId, InteractionFlags, Shadow, SizeRule, SizeRule2D,
    StylePropertyPatch, StyleSlotId, ThemeDomainId,
};
use crate::window_chrome::{
    WindowAction, WindowChromeModel, WindowChromeState, WindowContentStyle, WindowEdgeMask,
    WindowResizeEdge,
};

use super::WindowChromeViewExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromePalette {
    pub frame_background: ColorRgba8,
    pub frame_border: ColorRgba8,
    pub frame_border_width: f32,
    pub title_color: ColorRgba8,
    pub title_weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromeStateStyle {
    pub title_bar_visible: bool,
    /// Outer frame radius. Client content is automatically clipped to the inner border curve.
    pub frame_radius: f32,
    pub shadow: Option<Shadow>,
    pub resize_regions: bool,
    /// Minimum resize grab thickness, including the visible border. Extra width extends outward.
    pub resize_edge: f32,
    /// Extra tolerance outside the resize border; the easy frame never expands it inward.
    pub resize_hit_slop: Insets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowTitleBarStyle {
    pub height: f32,
    pub padding: Insets,
    pub gap: f32,
    pub title_size: f32,
    pub app_icon_region_size: f32,
    pub app_icon_size: f32,
    pub show_client_icon: bool,
    pub fallback_app_icon: Option<IconAsset>,
    pub app_icon_opens_system_menu: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowControlVisual {
    pub decoration: BoxDecoration,
    pub icon_tint: ColorRgba8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowControlButtonStyle {
    /// Uses the title bar as its sizing parent; fill shares remaining width with the spacer.
    pub width: Dimension,
    /// Fill uses the title bar height remaining after vertical padding.
    pub height: Dimension,
    pub icon_size: f32,
    pub resting: WindowControlVisual,
    pub hovered: Option<WindowControlVisual>,
    pub pressed: Option<WindowControlVisual>,
    pub focused: Option<WindowControlVisual>,
    pub disabled: Option<WindowControlVisual>,
    pub transition: Option<TransitionSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowControlDesign {
    pub icon: IconAsset,
    pub style: WindowControlButtonStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowControlsDesign {
    pub minimize: WindowControlDesign,
    pub maximize: WindowControlDesign,
    pub restore: WindowControlDesign,
    pub close: WindowControlDesign,
    pub gap: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromeDesign {
    pub active: WindowChromePalette,
    pub inactive: WindowChromePalette,
    pub normal: WindowChromeStateStyle,
    pub maximized: WindowChromeStateStyle,
    pub tiled: WindowChromeStateStyle,
    pub fullscreen: WindowChromeStateStyle,
    pub title_bar: WindowTitleBarStyle,
    pub controls: WindowControlsDesign,
    /// Backing beneath the application's pixels. Set alpha to zero to let client transparency
    /// reveal lower desktop layers. Opaque client buffers remain opaque.
    pub content_background: ColorRgba8,
    /// Resize placeholder RGBA, independent of the normal content backing. `None` inherits
    /// `LinuxDesktopConfig::resize_preview_color`; alpha zero gives a frame-only preview.
    pub resize_preview_color: Option<ColorRgba8>,
}

impl WindowChromeDesign {
    pub fn validate(self) -> Result<Self, WindowChromeDesignError> {
        for palette in [self.active, self.inactive] {
            finite_nonnegative(palette.frame_border_width)
                .ok_or(WindowChromeDesignError::InvalidFrameBorderWidth)?;
            if !(1..=1000).contains(&palette.title_weight) {
                return Err(WindowChromeDesignError::InvalidTitleWeight);
            }
        }
        for state in [self.normal, self.maximized, self.tiled, self.fullscreen] {
            finite_nonnegative(state.frame_radius)
                .ok_or(WindowChromeDesignError::InvalidFrameRadius)?;
            finite_nonnegative(state.resize_edge)
                .ok_or(WindowChromeDesignError::InvalidResizeEdge)?;
            validate_nonnegative_insets(state.resize_hit_slop)
                .ok_or(WindowChromeDesignError::InvalidResizeHitSlop)?;
            state
                .shadow
                .map_or(Ok(()), validate_shadow)
                .map_err(|_| WindowChromeDesignError::InvalidShadow)?;
        }
        for value in [
            self.title_bar.height,
            self.title_bar.gap,
            self.title_bar.title_size,
            self.title_bar.app_icon_region_size,
            self.title_bar.app_icon_size,
            self.controls.gap,
        ] {
            finite_nonnegative(value).ok_or(WindowChromeDesignError::InvalidTitleBarMetric)?;
        }
        if self.title_bar.height == 0.0 || self.title_bar.title_size == 0.0 {
            return Err(WindowChromeDesignError::InvalidTitleBarMetric);
        }
        for control in [
            self.controls.minimize,
            self.controls.maximize,
            self.controls.restore,
            self.controls.close,
        ] {
            for dimension in [control.style.width, control.style.height] {
                let valid = match dimension {
                    Dimension::Shrink => true,
                    Dimension::Pixels(value) | Dimension::Fill(value) => {
                        value.is_finite() && value > 0.0
                    }
                    Dimension::Percent(value) => value.is_finite() && value > 0.0 && value <= 1.0,
                };
                if !valid {
                    return Err(WindowChromeDesignError::InvalidControlMetric);
                }
            }
            if !control.style.icon_size.is_finite() || control.style.icon_size <= 0.0 {
                return Err(WindowChromeDesignError::InvalidControlMetric);
            }
        }
        Ok(self)
    }

    fn palette(self, active: bool) -> WindowChromePalette {
        if active { self.active } else { self.inactive }
    }

    fn state(self, state: WindowChromeState) -> WindowChromeStateStyle {
        match state {
            WindowChromeState::Normal => self.normal,
            WindowChromeState::Maximized => self.maximized,
            WindowChromeState::Fullscreen => self.fullscreen,
            WindowChromeState::Tiled => self.tiled,
        }
    }
}

fn finite_nonnegative(value: f32) -> Option<f32> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn validate_shadow(shadow: Shadow) -> Result<(), ()> {
    if !shadow.offset.x.is_finite()
        || !shadow.offset.y.is_finite()
        || finite_nonnegative(shadow.blur).is_none()
        || finite_nonnegative(shadow.spread).is_none()
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_nonnegative_insets(insets: Insets) -> Option<Insets> {
    let insets = insets.0;
    [insets.top, insets.right, insets.bottom, insets.left]
        .into_iter()
        .all(|value| finite_nonnegative(value).is_some())
        .then_some(Insets(insets))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WindowChromeDesignError {
    #[error("window chrome frame border width must be finite and nonnegative")]
    InvalidFrameBorderWidth,
    #[error("window chrome title weight must be between 1 and 1000")]
    InvalidTitleWeight,
    #[error("window chrome frame radius must be finite and nonnegative")]
    InvalidFrameRadius,
    #[error("window chrome resize edge must be finite and nonnegative")]
    InvalidResizeEdge,
    #[error("window chrome resize hit slop must be finite and nonnegative")]
    InvalidResizeHitSlop,
    #[error("window chrome shadow metrics must be finite and nonnegative")]
    InvalidShadow,
    #[error("window chrome title-bar metrics must be finite and positive where required")]
    InvalidTitleBarMetric,
    #[error("window chrome control metrics must be finite and positive")]
    InvalidControlMetric,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EasyWindowFrame {
    design: WindowChromeDesign,
}

impl EasyWindowFrame {
    pub const fn new(design: WindowChromeDesign) -> Self {
        Self { design }
    }

    pub const fn design(self) -> WindowChromeDesign {
        self.design
    }
}

pub const fn easy_window_frame(design: WindowChromeDesign) -> EasyWindowFrame {
    EasyWindowFrame::new(design)
}

#[doc(hidden)]
pub struct EasyWindowFrameComponent {
    model: WindowChromeModel,
    design: WindowChromeDesign,
}

impl ComponentFields for EasyWindowFrameComponent {
    type InputSnapshot = (WindowChromeModel, WindowChromeDesign);

    fn update_inputs(&mut self, incoming: Self) -> bool {
        if self.model == incoming.model && self.design == incoming.design {
            false
        } else {
            *self = incoming;
            true
        }
    }

    fn capture_inputs(&self) -> Self::InputSnapshot {
        (self.model.clone(), self.design)
    }

    fn restore_inputs(&mut self, snapshot: Self::InputSnapshot) -> bool {
        let changed = self.model != snapshot.0 || self.design != snapshot.1;
        self.model = snapshot.0;
        self.design = snapshot.1;
        changed
    }
}

impl Component for EasyWindowFrameComponent {
    fn view(&self) -> impl View {
        let design = self.design;
        let palette = design.palette(self.model.active);
        let state = design.state(self.model.state);
        let inner_radius = (state.frame_radius - palette.frame_border_width).max(0.0);
        let mut frame_decoration = BoxDecoration::new()
            .background(Background::Color(palette.frame_background))
            .uniform_border(palette.frame_border_width, palette.frame_border)
            .corner_radius(state.frame_radius);
        if let Some(shadow) = state.shadow {
            frame_decoration = frame_decoration.shadow(shadow);
        }

        let title_bar = build_title_bar(&self.model, design, palette, state);
        let resize = build_resize_regions(&self.model, state, palette.frame_border_width);

        window_frame()
            .decoration(frame_decoration)
            .overflow(crate::ui::Overflow::Clip)
            .children(title_bar)
            .children(resize)
            .content_slot(
                window_content_slot()
                    // The root border already reserves its width on every side.
                    .margin(Insets::new(
                        if state.title_bar_visible {
                            design.title_bar.height
                        } else {
                            0.0
                        },
                        0.0,
                        0.0,
                        0.0,
                    ))
                    .overflow(crate::ui::Overflow::Clip)
                    .decoration(
                        BoxDecoration::new()
                            .background(Background::Color(design.content_background))
                            .corner_radii(crate::ui::CornerRadii {
                                top_left: if state.title_bar_visible {
                                    0.0
                                } else {
                                    inner_radius
                                },
                                top_right: if state.title_bar_visible {
                                    0.0
                                } else {
                                    inner_radius
                                },
                                bottom_right: inner_radius,
                                bottom_left: inner_radius,
                            }),
                    ),
            )
    }
}

impl WindowFrameTemplate for EasyWindowFrame {
    type Component = EasyWindowFrameComponent;

    fn compose(&self, model: WindowChromeModel) -> Self::Component {
        EasyWindowFrameComponent {
            model,
            design: self.design,
        }
    }

    fn content_style(&self, _model: &WindowChromeModel) -> Option<WindowContentStyle> {
        Some(WindowContentStyle {
            background: self.design.content_background,
            // The full-window inner border clip owns rounding; no second aperture is needed.
            corner_radius: 0.0,
            resize_preview_color: self.design.resize_preview_color,
        })
    }
}

fn build_title_bar(
    model: &WindowChromeModel,
    design: WindowChromeDesign,
    palette: WindowChromePalette,
    state: WindowChromeStateStyle,
) -> Option<Element> {
    if !state.title_bar_visible {
        return None;
    }

    let title = text(&model.title)
        .size(design.title_bar.title_size)
        .weight(palette.title_weight)
        .color(palette.title_color)
        .window_title();
    let icon = window_icon(model, design.title_bar);
    let controls = window_controls(model, design.controls);
    let mut children = Vec::new();
    if let Some(icon) = icon {
        children.push(icon);
        children.push(spacer().width(design.title_bar.gap).into_element());
    }
    children.push(title.into_element());
    children.push(spacer().width(design.title_bar.gap).into_element());
    children.push(spacer().into_element());
    if !controls.is_empty() {
        children.push(spacer().width(design.title_bar.gap).into_element());
    }
    children.extend(controls);
    // Keep controls in the bar's row so every Dimension resolves against the same parent.
    let title_bar = row()
        .height(design.title_bar.height)
        .padding(design.title_bar.padding)
        .align_items(Alignment::Center)
        .children(children);
    Some(if model.capabilities.move_window {
        title_bar.window_drag_region()
    } else {
        title_bar.into_element()
    })
}

fn window_icon(model: &WindowChromeModel, style: WindowTitleBarStyle) -> Option<Element> {
    if !style.show_client_icon {
        return None;
    }
    let source = model
        .app_icon_image
        .map(ImageSource::from)
        .or_else(|| model.app_icon.map(ImageSource::from))
        .or_else(|| style.fallback_app_icon.map(ImageSource::from))?;
    let region = stack()
        .width(style.app_icon_region_size)
        .height(style.app_icon_region_size)
        .center_content()
        .child(
            image(source)
                .width(style.app_icon_size)
                .height(style.app_icon_size)
                .accessible_label("Application icon")
                .window_app_icon(),
        );
    Some(
        if style.app_icon_opens_system_menu && model.capabilities.system_menu {
            region.window_system_menu()
        } else {
            region.into_element()
        },
    )
}

fn window_controls(model: &WindowChromeModel, design: WindowControlsDesign) -> Vec<Element> {
    let mut controls = Vec::with_capacity(3);
    if model.capabilities.minimize {
        controls.push(control("Minimize", design.minimize, WindowAction::Minimize));
    }
    if model.capabilities.maximize {
        let (label, control_design) = if model.state == WindowChromeState::Maximized {
            ("Restore", design.restore)
        } else {
            ("Maximize", design.maximize)
        };
        controls.push(control(label, control_design, WindowAction::ToggleMaximize));
    }
    if model.capabilities.close {
        controls.push(control("Close", design.close, WindowAction::Close));
    }
    let mut children = Vec::with_capacity(5);
    for control in controls {
        if !children.is_empty() {
            children.push(spacer().width(design.gap).into_element());
        }
        children.push(control);
    }
    children
}

fn control(label: &'static str, design: WindowControlDesign, action: WindowAction) -> Element {
    button(label)
        .icon(design.icon)
        .icon_tint(design.style.resting.icon_tint)
        .icon_size(design.style.icon_size)
        .width(design.style.width)
        .height(design.style.height)
        .decoration(design.style.resting.decoration)
        .inline_style(compiled_control_style(design.style))
        .window_action(action)
}

fn compiled_control_style(style: WindowControlButtonStyle) -> Arc<CompiledComponentStyle> {
    let root_slot = StyleSlotId::named("root");
    let icon_slot = StyleSlotId::named("icon");
    let mut root = visual_root_patch(style.resting);
    root.width = Some(style.width.into());
    root.height = Some(style.height.into());
    // Chrome controls follow their authored dimensions, including bars shorter than 32px.
    root.min_size = Some(SizeRule2D {
        width: SizeRule::Px(0.0),
        height: SizeRule::Px(0.0),
    });
    let icon = visual_icon_patch(style.resting);

    let mut slots = BTreeMap::new();
    slots.insert(
        root_slot,
        CompiledSlotStyle {
            patch: root,
            font_family: None,
        },
    );
    slots.insert(
        icon_slot,
        CompiledSlotStyle {
            patch: icon,
            font_family: None,
        },
    );

    let mut states = BTreeMap::new();
    for (state, visual) in [
        (InteractionState::Hovered, style.hovered),
        (InteractionState::FocusVisible, style.focused),
        (InteractionState::Pressed, style.pressed),
        (InteractionState::Disabled, style.disabled),
    ] {
        let Some(visual) = visual else { continue };
        states.insert(
            state,
            CompiledStateStyle {
                slots: BTreeMap::from([
                    (
                        root_slot,
                        CompiledSlotStyle {
                            patch: visual_root_patch(visual),
                            font_family: None,
                        },
                    ),
                    (
                        icon_slot,
                        CompiledSlotStyle {
                            patch: visual_icon_patch(visual),
                            font_family: None,
                        },
                    ),
                ]),
                transition: None,
            },
        );
    }

    Arc::new(CompiledComponentStyle {
        id: ComponentStyleId::named(ThemeDomainId::SHELL, "window-control", "inline"),
        slots,
        variants: BTreeMap::new(),
        states,
        state_precedence: vec![
            InteractionState::Hovered,
            InteractionState::FocusVisible,
            InteractionState::Pressed,
            InteractionState::Disabled,
        ],
        relevant_states: InteractionFlags::from_bits(
            InteractionFlags::HOVERED.bits()
                | InteractionFlags::FOCUS_VISIBLE.bits()
                | InteractionFlags::PRESSED.bits()
                | InteractionFlags::DISABLED.bits(),
        ),
        transition: style.transition.unwrap_or_default(),
        controlled_slots: BTreeMap::from([(root_slot, root), (icon_slot, icon)]),
        controlled_font_families: BTreeSet::new(),
    })
}

fn visual_root_patch(visual: WindowControlVisual) -> StylePropertyPatch {
    StylePropertyPatch {
        background: Some(visual.decoration.background),
        border: Some(visual.decoration.border),
        outline: Some(visual.decoration.outline),
        corner_radii: Some(visual.decoration.corner_radii),
        shadows: Some(visual.decoration.shadows),
        ..StylePropertyPatch::default()
    }
}

fn visual_icon_patch(visual: WindowControlVisual) -> StylePropertyPatch {
    StylePropertyPatch {
        image_tint: Some(Some(visual.icon_tint)),
        ..StylePropertyPatch::default()
    }
}

fn build_resize_regions(
    model: &WindowChromeModel,
    style: WindowChromeStateStyle,
    frame_border_width: f32,
) -> Vec<Element> {
    if !model.capabilities.resize || !style.resize_regions {
        return Vec::new();
    }
    let edges = model
        .tiling
        .map_or(WindowEdgeMask::ALL, |tiling| tiling.resizable_edges);
    let extra = (style.resize_edge - frame_border_width).max(0.0);
    let slop = style.resize_hit_slop.0;
    let slop = Insets::new(
        slop.top + extra,
        slop.right + extra,
        slop.bottom + extra,
        slop.left + extra,
    );
    // The invisible boxes start at the root's inner edge. Outset restores the entire painted
    // border and adds tolerance outside it; the shared inner contour excludes app/title pixels.
    let region = |view: Element, edge| resize_region(view, edge, slop, frame_border_width);
    let corner = style
        .frame_radius
        .max(style.resize_edge.max(frame_border_width) * 2.0);
    let mut regions = Vec::with_capacity(8);
    if edges.contains(WindowEdgeMask::TOP) {
        regions.push(region(
            stack().height(0.0).into_element(),
            WindowResizeEdge::Top,
        ));
    }
    if edges.contains(WindowEdgeMask::RIGHT) {
        regions.push(
            row()
                .child(spacer())
                .child(region(
                    stack().width(0.0).into_element(),
                    WindowResizeEdge::Right,
                ))
                .into_element(),
        );
    }
    if edges.contains(WindowEdgeMask::BOTTOM) {
        regions.push(
            column()
                .child(spacer())
                .child(region(
                    stack().height(0.0).into_element(),
                    WindowResizeEdge::Bottom,
                ))
                .into_element(),
        );
    }
    if edges.contains(WindowEdgeMask::LEFT) {
        regions.push(region(
            stack().width(0.0).into_element(),
            WindowResizeEdge::Left,
        ));
    }
    if edges.contains(WindowEdgeMask::TOP | WindowEdgeMask::RIGHT) {
        regions.push(
            row()
                .child(spacer())
                .child(region(
                    stack().width(corner).height(corner).into_element(),
                    WindowResizeEdge::TopRight,
                ))
                .into_element(),
        );
    }
    if edges.contains(WindowEdgeMask::BOTTOM | WindowEdgeMask::RIGHT) {
        regions.push(
            column()
                .child(spacer())
                .child(row().height(corner).child(spacer()).child(region(
                    stack().width(corner).height(corner).into_element(),
                    WindowResizeEdge::BottomRight,
                )))
                .into_element(),
        );
    }
    if edges.contains(WindowEdgeMask::BOTTOM | WindowEdgeMask::LEFT) {
        regions.push(
            column()
                .child(spacer())
                .child(
                    row()
                        .height(corner)
                        .child(region(
                            stack().width(corner).height(corner).into_element(),
                            WindowResizeEdge::BottomLeft,
                        ))
                        .child(spacer()),
                )
                .into_element(),
        );
    }
    if edges.contains(WindowEdgeMask::TOP | WindowEdgeMask::LEFT) {
        regions.push(region(
            stack().width(corner).height(corner).into_element(),
            WindowResizeEdge::TopLeft,
        ));
    }
    regions
}

fn resize_region(
    region: impl View,
    edge: WindowResizeEdge,
    hit_slop: Insets,
    frame_border_width: f32,
) -> Element {
    region
        .window_resize(edge)
        .window_hit_slop(outward_resize_hit_slop(edge, hit_slop, frame_border_width))
        .with_window_chrome_border_hit(hit_slop.0)
}

fn outward_resize_hit_slop(
    edge: WindowResizeEdge,
    hit_slop: Insets,
    frame_border_width: f32,
) -> Insets {
    let hit_slop = hit_slop.0;
    match edge {
        WindowResizeEdge::Top => Insets::new(hit_slop.top + frame_border_width, 0.0, 0.0, 0.0),
        WindowResizeEdge::TopRight => Insets::new(
            hit_slop.top + frame_border_width,
            hit_slop.right + frame_border_width,
            0.0,
            0.0,
        ),
        WindowResizeEdge::Right => Insets::new(0.0, hit_slop.right + frame_border_width, 0.0, 0.0),
        WindowResizeEdge::BottomRight => Insets::new(
            0.0,
            hit_slop.right + frame_border_width,
            hit_slop.bottom + frame_border_width,
            0.0,
        ),
        WindowResizeEdge::Bottom => {
            Insets::new(0.0, 0.0, hit_slop.bottom + frame_border_width, 0.0)
        }
        WindowResizeEdge::BottomLeft => Insets::new(
            0.0,
            0.0,
            hit_slop.bottom + frame_border_width,
            hit_slop.left + frame_border_width,
        ),
        WindowResizeEdge::Left => Insets::new(0.0, 0.0, 0.0, hit_slop.left + frame_border_width),
        WindowResizeEdge::TopLeft => Insets::new(
            hit_slop.top + frame_border_width,
            0.0,
            0.0,
            hit_slop.left + frame_border_width,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetKey;
    use crate::theme::Easing;

    const fn icon(path: &'static str) -> IconAsset {
        IconAsset::new(AssetKey::new(path))
    }

    const VISUAL: WindowControlVisual = WindowControlVisual {
        decoration: BoxDecoration::new(),
        icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
    };
    const BUTTON: WindowControlButtonStyle = WindowControlButtonStyle {
        width: Dimension::Pixels(38.0),
        height: Dimension::Pixels(30.0),
        icon_size: 15.0,
        resting: VISUAL,
        hovered: None,
        pressed: None,
        focused: None,
        disabled: None,
        transition: None,
    };
    const STATE: WindowChromeStateStyle = WindowChromeStateStyle {
        title_bar_visible: true,
        frame_radius: 12.0,
        shadow: None,
        resize_regions: true,
        resize_edge: 6.0,
        resize_hit_slop: Insets::all(2.0),
    };
    const DESIGN: WindowChromeDesign = WindowChromeDesign {
        active: WindowChromePalette {
            frame_background: ColorRgba8::rgba(20, 24, 32, 255),
            frame_border: ColorRgba8::rgba(80, 90, 120, 255),
            frame_border_width: 1.0,
            title_color: ColorRgba8::rgba(255, 255, 255, 255),
            title_weight: 600,
        },
        inactive: WindowChromePalette {
            frame_background: ColorRgba8::rgba(30, 34, 42, 255),
            frame_border: ColorRgba8::rgba(60, 65, 80, 255),
            frame_border_width: 1.0,
            title_color: ColorRgba8::rgba(180, 180, 190, 255),
            title_weight: 400,
        },
        normal: STATE,
        maximized: WindowChromeStateStyle {
            frame_radius: 0.0,
            shadow: None,
            resize_regions: false,
            resize_edge: 0.0,
            resize_hit_slop: Insets::ZERO,
            ..STATE
        },
        tiled: STATE,
        fullscreen: WindowChromeStateStyle {
            title_bar_visible: false,
            frame_radius: 0.0,
            shadow: None,
            resize_regions: false,
            resize_edge: 0.0,
            resize_hit_slop: Insets::ZERO,
        },
        title_bar: WindowTitleBarStyle {
            height: 42.0,
            padding: Insets::symmetric(6.0, 8.0),
            gap: 6.0,
            title_size: 14.0,
            app_icon_region_size: 30.0,
            app_icon_size: 20.0,
            show_client_icon: true,
            fallback_app_icon: None,
            app_icon_opens_system_menu: true,
        },
        controls: WindowControlsDesign {
            minimize: WindowControlDesign {
                icon: icon("icons/minimize.svg"),
                style: BUTTON,
            },
            maximize: WindowControlDesign {
                icon: icon("icons/maximize.svg"),
                style: BUTTON,
            },
            restore: WindowControlDesign {
                icon: icon("icons/restore.svg"),
                style: BUTTON,
            },
            close: WindowControlDesign {
                icon: icon("icons/close.svg"),
                style: BUTTON,
            },
            gap: 6.0,
        },
        content_background: ColorRgba8::rgba(10, 12, 18, 255),
        resize_preview_color: None,
    };

    #[test]
    fn design_validation_accepts_finite_complete_chrome() {
        assert_eq!(DESIGN.validate(), Ok(DESIGN));
    }

    #[test]
    fn content_style_preserves_alpha_preview_inheritance_and_state_radius() {
        for alpha in [0, 128, 255] {
            for preview in [None, Some(ColorRgba8::rgba(30, 40, 50, alpha))] {
                let design = WindowChromeDesign {
                    content_background: ColorRgba8::rgba(0, 0, 0, alpha),
                    resize_preview_color: preview,
                    ..DESIGN
                };
                assert_eq!(design.validate(), Ok(design));
                for state in [
                    WindowChromeState::Normal,
                    WindowChromeState::Maximized,
                    WindowChromeState::Tiled,
                    WindowChromeState::Fullscreen,
                ] {
                    let style = easy_window_frame(design)
                        .content_style(&WindowChromeModel::new(7, "Editor").state(state))
                        .unwrap();
                    assert_eq!(style.background, design.content_background);
                    assert_eq!(style.resize_preview_color, preview);
                    assert_eq!(style.corner_radius, 0.0);
                }
            }
        }
    }

    #[test]
    fn template_composes_a_distinct_model_owned_component() {
        let model = WindowChromeModel::new(7, "Editor").active(true);
        let component = easy_window_frame(DESIGN).compose(model.clone());
        assert_eq!(component.model, model);
        assert_eq!(component.design, DESIGN);
    }

    #[test]
    fn control_design_resolves_interaction_visuals_without_a_theme_catalog_entry() {
        let hovered = WindowControlVisual {
            decoration: BoxDecoration::new()
                .background(Background::Color(ColorRgba8::rgba(40, 50, 60, 255))),
            icon_tint: ColorRgba8::rgba(10, 20, 30, 255),
        };
        let style = WindowControlButtonStyle {
            hovered: Some(hovered),
            transition: Some(TransitionSpec {
                duration_ms: 90,
                easing: Easing::EaseOut,
                repeat: false,
            }),
            ..BUTTON
        };
        let compiled = compiled_control_style(style);
        let root = compiled
            .resolve_slot(&[], InteractionFlags::HOVERED, StyleSlotId::named("root"))
            .unwrap();
        let icon = compiled
            .resolve_slot(&[], InteractionFlags::HOVERED, StyleSlotId::named("icon"))
            .unwrap();

        assert_eq!(root.patch.background, Some(hovered.decoration.background));
        assert_eq!(icon.patch.image_tint, Some(Some(hovered.icon_tint)));
        assert_eq!(root.transition.duration_ms, 90);
    }
    #[test]
    fn rounded_controls_keep_their_shape_during_hover_and_interrupted_transitions() {
        use crate::application_host::AppRuntimeCore;
        use crate::core::{MonotonicInstant, SizeI};
        use crate::ui::CornerRadii;

        let mut design = DESIGN;
        let radius = CornerRadii::all(7.0);
        for control in [
            &mut design.controls.minimize,
            &mut design.controls.maximize,
            &mut design.controls.restore,
            &mut design.controls.close,
        ] {
            control.style.resting.decoration = BoxDecoration::new()
                .background(Background::Color(ColorRgba8::rgba(30, 40, 50, 255)))
                .corner_radii(radius);
            control.style.hovered = Some(WindowControlVisual {
                decoration: control
                    .style
                    .resting
                    .decoration
                    .background(Background::Color(ColorRgba8::rgba(120, 140, 160, 255))),
                ..control.style.resting
            });
            control.style.transition = Some(TransitionSpec {
                duration_ms: 100,
                easing: Easing::Linear,
                repeat: false,
            });
        }
        let mut runtime = AppRuntimeCore::from_composed_with_extent(
            easy_window_frame(design).compose(WindowChromeModel::new(42, "Rounded")),
            SizeI {
                width: 640,
                height: 480,
            },
        )
        .unwrap();
        runtime.prepare_frame(MonotonicInstant::ZERO, true).unwrap();
        let nodes: Vec<_> = runtime
            .ui()
            .style_bindings()
            .iter()
            .filter(|binding| binding.local_style.is_some())
            .map(|binding| binding.state_root)
            .collect();
        assert_eq!(nodes.len(), 3);
        let mut saw_intermediate_color = false;
        for ms in 1..=350 {
            if let Some(hovered) = match ms {
                1 | 61 => Some(true),
                41 | 201 => Some(false),
                _ => None,
            } {
                for &node in &nodes {
                    runtime.ui_mut().route_interaction_flag(
                        node,
                        InteractionFlags::HOVERED,
                        hovered,
                    );
                }
            }
            runtime
                .prepare_frame(MonotonicInstant::from_nanos(ms * 1_000_000), true)
                .unwrap();
            for &node in &nodes {
                let decoration = runtime.ui().box_styles.get(node).unwrap().decoration;
                assert_eq!(decoration.corner_radii, radius, "radius changed at {ms}ms");
                if let Background::Color(color) = decoration.background {
                    saw_intermediate_color |= color.r > 30 && color.r < 120;
                }
            }
        }
        assert!(
            saw_intermediate_color,
            "the check must sample an active color transition"
        );
    }
}
