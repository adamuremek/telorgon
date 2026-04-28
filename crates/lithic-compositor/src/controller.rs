use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use lithic_core::{ColorRgba8, RectI, SizeI};
use lithic_render::{
    CornerRadii, RenderBlit, RenderFrame, RenderMaterial, RenderMaterialKind, RenderMaterialPass,
    RenderOp, RenderRect, RenderTargetId, RenderText,
};
use lithic_theme::{
    ButtonPaint, ButtonShape, ChromeButton, ChromeButtonGroup, CompositorRequest, FrameElement,
    FrameRegion, FrameRegionRole, FrameSlot, IconRef, RowLayout, SurfacePaint, SurfaceRequest,
    TextElement, TextValue, ThemeAssetStore, ThemeImageAsset, ThemePackage, WindowSurfaceTheme,
};
use lithic_ui::{Action, ButtonRow, ControlGroup, Widget};

use crate::chrome::{ChromeMaterial, WindowChrome};
use crate::command::{
    CreateDesktopSurface, CreateLayerSurface, CreateWindowSurface, SurfaceCommand,
};
use crate::id::SurfaceId;
use crate::surface::{DesktopSurface, Surface, SurfaceContent, SurfaceKind, WindowSurface};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceError {
    message: String,
}

impl SurfaceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for SurfaceError {}

pub type SurfaceResult<T> = Result<T, SurfaceError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickInput {
    pub output_id: RenderTargetId,
    pub extent: SizeI,
    pub background: ColorRgba8,
    pub frame_time_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutput {
    pub render_frame: RenderFrame,
    pub hit_regions: Vec<HitRegion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitRegionKind {
    Surface,
    Content,
    Chrome,
    TitleBar,
    Action { index: usize, action: Action },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub surface_id: SurfaceId,
    pub kind: HitRegionKind,
    pub rect: RectI,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoveredAction {
    pub surface_id: SurfaceId,
    pub index: usize,
    pub action_name: String,
}

#[derive(Default)]
pub struct SurfaceController {
    surfaces: BTreeMap<SurfaceId, Surface>,
    theme_assets: BTreeMap<String, ThemeImageAsset>,
    next_surface_id: u64,
    hovered_action: Option<HoveredAction>,
}

impl SurfaceController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate_surface_id(&mut self) -> SurfaceId {
        self.next_surface_id = self.next_surface_id.saturating_add(1);
        while self
            .surfaces
            .contains_key(&SurfaceId::new(self.next_surface_id))
        {
            self.next_surface_id = self.next_surface_id.saturating_add(1);
        }
        SurfaceId::new(self.next_surface_id)
    }

    pub fn submit(&mut self, command: SurfaceCommand) -> SurfaceResult<()> {
        self.apply(command)
    }

    pub fn apply(&mut self, command: SurfaceCommand) -> SurfaceResult<()> {
        match command {
            SurfaceCommand::CreateWindow(command) => self.create_window(command),
            SurfaceCommand::CreateLayer(command) => self.create_layer(command),
            SurfaceCommand::CreateDesktop(command) => self.create_desktop(command),
            SurfaceCommand::DestroySurface { id } => self
                .surfaces
                .remove(&id)
                .map(|_| ())
                .ok_or_else(|| missing_surface(id)),
            SurfaceCommand::SetGeometry { id, geometry } => {
                self.surface_mut(id)?.geometry = geometry;
                Ok(())
            }
            SurfaceCommand::SetContent { id, content } => {
                self.surface_mut(id)?.content = content;
                Ok(())
            }
            SurfaceCommand::SetFocus { id } => self.set_focus(id),
            SurfaceCommand::Raise { id } => {
                let z_order = self.max_z_order().saturating_add(1);
                self.surface_mut(id)?.z_order = z_order;
                Ok(())
            }
            SurfaceCommand::Lower { id } => {
                let z_order = self.min_z_order().saturating_sub(1);
                self.surface_mut(id)?.z_order = z_order;
                Ok(())
            }
            SurfaceCommand::SetZOrder { id, z_order } => {
                self.surface_mut(id)?.z_order = z_order;
                Ok(())
            }
            SurfaceCommand::SetChrome { id, chrome } => {
                let surface = self.surface_mut(id)?;
                let SurfaceKind::Window(window) = &mut surface.kind else {
                    return Err(SurfaceError::new(format!(
                        "{id} is not a window surface and cannot receive window chrome"
                    )));
                };
                window.chrome = chrome;
                Ok(())
            }
            SurfaceCommand::SetWindowSurfaceTheme { id, surface_theme } => {
                let surface = self.surface_mut(id)?;
                let SurfaceKind::Window(window) = &mut surface.kind else {
                    return Err(SurfaceError::new(format!(
                        "{id} is not a window surface and cannot receive a window surface theme"
                    )));
                };
                window.surface_theme = surface_theme;
                Ok(())
            }
            SurfaceCommand::SetWindowMetadata { id, title, app_id } => {
                let surface = self.surface_mut(id)?;
                let SurfaceKind::Window(window) = &mut surface.kind else {
                    return Err(SurfaceError::new(format!(
                        "{id} is not a window surface and cannot receive window metadata"
                    )));
                };
                window.title = title;
                window.app_id = app_id;
                Ok(())
            }
            SurfaceCommand::SetVisible { id, visible } => {
                self.surface_mut(id)?.visible = visible;
                Ok(())
            }
            SurfaceCommand::SetOpacity { id, opacity } => {
                self.surface_mut(id)?.opacity = opacity;
                Ok(())
            }
        }
    }

    pub fn tick(&self, input: TickInput) -> TickOutput {
        let mut ops = Vec::new();
        let mut hit_regions = Vec::new();

        for surface in self.ordered_surfaces() {
            if !surface.visible {
                continue;
            }

            match &surface.kind {
                SurfaceKind::Window(window) => {
                    self.push_window(surface, window, &mut ops, &mut hit_regions);
                }
                SurfaceKind::Layer(layer) => {
                    self.push_plain_surface(
                        surface,
                        layer.accepts_input,
                        &mut ops,
                        &mut hit_regions,
                    );
                }
                SurfaceKind::Desktop(desktop) => {
                    self.push_desktop(surface, desktop, &mut ops, &mut hit_regions);
                }
            }
        }

        TickOutput {
            render_frame: RenderFrame {
                output_id: input.output_id,
                extent: input.extent,
                background: input.background,
                damage_rects: Vec::<RectI>::new().into(),
                ops,
            },
            hit_regions,
        }
    }

    pub fn surface(&self, id: SurfaceId) -> Option<&Surface> {
        self.surfaces.get(&id)
    }

    pub fn load_theme_package(&mut self, package: &ThemePackage) {
        self.theme_assets = package
            .assets
            .image_assets()
            .map(|(id, image)| (id.to_string(), image.clone()))
            .collect();
    }

    pub fn set_hovered_action(
        &mut self,
        surface_id: SurfaceId,
        index: usize,
        action_name: impl Into<String>,
    ) -> bool {
        let next = Some(HoveredAction {
            surface_id,
            index,
            action_name: action_name.into(),
        });
        if self.hovered_action == next {
            return false;
        }
        self.hovered_action = next;
        true
    }

    pub fn clear_hovered_action(&mut self) -> bool {
        if self.hovered_action.is_none() {
            return false;
        }
        self.hovered_action = None;
        true
    }

    pub fn surfaces(&self) -> impl Iterator<Item = &Surface> {
        self.surfaces.values()
    }

    pub fn ordered_surfaces(&self) -> Vec<&Surface> {
        let mut surfaces: Vec<_> = self.surfaces.values().collect();
        surfaces.sort_by_key(|surface| (surface.z_order, surface.id));
        surfaces
    }

    fn create_window(&mut self, command: CreateWindowSurface) -> SurfaceResult<()> {
        self.insert_surface(
            Surface::new(
                command.id,
                command.geometry,
                command.z_order,
                SurfaceKind::Window(WindowSurface {
                    title: command.title,
                    app_id: command.app_id,
                    focused: false,
                    chrome: command.chrome,
                    surface_theme: command.surface_theme,
                }),
            )
            .with_content(command.content),
        )
    }

    fn create_layer(&mut self, command: CreateLayerSurface) -> SurfaceResult<()> {
        self.insert_surface(
            Surface::new(
                command.id,
                command.geometry,
                command.z_order,
                SurfaceKind::Layer(command.layer),
            )
            .with_content(command.content),
        )
    }

    fn create_desktop(&mut self, command: CreateDesktopSurface) -> SurfaceResult<()> {
        self.insert_surface(
            Surface::new(
                command.id,
                command.geometry,
                command.z_order,
                SurfaceKind::Desktop(command.desktop),
            )
            .with_content(command.content),
        )
    }

    fn insert_surface(&mut self, surface: Surface) -> SurfaceResult<()> {
        if self.surfaces.contains_key(&surface.id) {
            return Err(SurfaceError::new(format!("{} already exists", surface.id)));
        }
        self.next_surface_id = self.next_surface_id.max(surface.id.get());
        self.surfaces.insert(surface.id, surface);
        Ok(())
    }

    fn set_focus(&mut self, id: Option<SurfaceId>) -> SurfaceResult<()> {
        if let Some(id) = id {
            let Some(surface) = self.surfaces.get(&id) else {
                return Err(missing_surface(id));
            };
            if !matches!(surface.kind, SurfaceKind::Window(_)) {
                return Err(SurfaceError::new(format!("{id} is not a window surface")));
            }
        }

        for surface in self.surfaces.values_mut() {
            if let SurfaceKind::Window(window) = &mut surface.kind {
                window.focused = Some(surface.id) == id;
            }
        }
        Ok(())
    }

    fn surface_mut(&mut self, id: SurfaceId) -> SurfaceResult<&mut Surface> {
        self.surfaces
            .get_mut(&id)
            .ok_or_else(|| missing_surface(id))
    }

    fn max_z_order(&self) -> i32 {
        self.surfaces
            .values()
            .map(|surface| surface.z_order)
            .max()
            .unwrap_or(0)
    }

    fn min_z_order(&self) -> i32 {
        self.surfaces
            .values()
            .map(|surface| surface.z_order)
            .min()
            .unwrap_or(0)
    }

    fn push_window(
        &self,
        surface: &Surface,
        window: &WindowSurface,
        ops: &mut Vec<RenderOp>,
        hit_regions: &mut Vec<HitRegion>,
    ) {
        if let Some(surface_theme) = &window.surface_theme {
            self.push_explicit_window(surface, window, surface_theme, ops, hit_regions);
            return;
        }

        let chrome = focused_chrome(&window.chrome, window.focused);
        let content_rect = surface.geometry;
        let frame_rect = chrome.frame_rect(content_rect);
        let titlebar_rect = chrome.titlebar_rect(content_rect);
        let opacity = opacity_scale(surface.opacity);

        if let Some(shadow) = chrome.shadow {
            let inset = shadow.radius_px.max(0) / 2;
            let color = shadow.color.with_alpha_scale(opacity);
            ops.push(RenderOp::Material(RenderMaterial {
                rect: RectI {
                    x: frame_rect.x + shadow.offset.x - inset,
                    y: frame_rect.y + shadow.offset.y - inset,
                    width: frame_rect.width + inset * 2,
                    height: frame_rect.height + inset * 2,
                },
                corner_radii_px: chrome.corner_radii_px,
                shader_name: "shadow.spv".to_string(),
                shader_spirv_words: None,
                kind: RenderMaterialKind::Shadow {
                    color,
                    radius_px: shadow.radius_px,
                    strength: shadow.strength,
                },
                passes: vec![RenderMaterialPass::Shadow {
                    color,
                    radius_px: shadow.radius_px,
                    strength: shadow.strength,
                }],
            }));
        }

        push_chrome_material(ops, frame_rect, &chrome, opacity);

        push_rect(
            ops,
            frame_rect,
            chrome.border_color.with_alpha_scale(opacity),
            chrome.corner_radii_px,
        );
        push_rect(
            ops,
            titlebar_rect,
            chrome.titlebar_color.with_alpha_scale(opacity),
            CornerRadii::top(chrome.corner_radii_px.top_left),
        );
        push_titlebar_widgets(
            surface.id,
            ops,
            hit_regions,
            titlebar_rect,
            chrome.corner_radii_px.top_right,
            &chrome.titlebar_widgets,
            &window.title,
            self.hovered_action.as_ref(),
            opacity,
        );

        if surface
            .content
            .as_ref()
            .is_none_or(|content| !content.is_opaque)
        {
            push_rect(
                ops,
                content_rect,
                chrome.content_background.with_alpha_scale(opacity),
                CornerRadii::bottom(chrome.corner_radii_px.bottom_left),
            );
        }
        push_content(ops, content_rect, surface.content.as_ref(), surface.opacity);

        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Surface,
            rect: frame_rect,
        });
        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Chrome,
            rect: frame_rect,
        });
        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::TitleBar,
            rect: titlebar_rect,
        });
        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Content,
            rect: content_rect,
        });
    }

    fn push_explicit_window(
        &self,
        surface: &Surface,
        window: &WindowSurface,
        theme: &WindowSurfaceTheme,
        ops: &mut Vec<RenderOp>,
        hit_regions: &mut Vec<HitRegion>,
    ) {
        let opacity = opacity_scale(surface.opacity);
        let border = theme.theme.border.width_px.max(0);
        let header_height = explicit_header_height(theme);
        let content_rect = surface.geometry;
        let frame_rect = RectI {
            x: content_rect.x.saturating_sub(border),
            y: content_rect.y.saturating_sub(border).saturating_sub(header_height),
            width: content_rect.width.saturating_add(border * 2),
            height: content_rect
                .height
                .saturating_add(header_height)
                .saturating_add(border * 2),
        };
        let radii = CornerRadii::all(theme.theme.radius_px);

        let shadow = &theme.theme.shadow;
        if shadow.color.a != 0 && shadow.blur_px > 0 {
            let inset = shadow.blur_px.max(0) / 2;
            let color = shadow.color.with_alpha_scale(opacity);
            ops.push(RenderOp::Material(RenderMaterial {
                rect: RectI {
                    x: frame_rect.x + shadow.offset.x - inset,
                    y: frame_rect.y + shadow.offset.y - inset,
                    width: frame_rect.width + inset * 2,
                    height: frame_rect.height + inset * 2,
                },
                corner_radii_px: radii,
                shader_name: "shadow.spv".to_string(),
                shader_spirv_words: None,
                kind: RenderMaterialKind::Shadow {
                    color,
                    radius_px: shadow.blur_px,
                    strength: shadow.strength,
                },
                passes: vec![RenderMaterialPass::Shadow {
                    color,
                    radius_px: shadow.blur_px,
                    strength: shadow.strength,
                }],
            }));
        }

        push_rect(
            ops,
            frame_rect,
            theme.theme.border.color.with_alpha_scale(opacity),
            radii,
        );

        let inner_rect = RectI {
            x: frame_rect.x + border,
            y: frame_rect.y + border,
            width: frame_rect.width.saturating_sub(border * 2),
            height: frame_rect.height.saturating_sub(border * 2),
        };
        push_rect(
            ops,
            inner_rect,
            paint_color(&theme.theme.background).with_alpha_scale(opacity),
            radii,
        );

        let mut content_region_painted = false;
        let mut header_index = 0;
        for region in &theme.frame.regions {
            match region.role {
                FrameRegionRole::Header => {
                    let rect = RectI {
                        x: content_rect.x,
                        y: content_rect.y.saturating_sub(header_height)
                            + header_index * region.height_px.unwrap_or(0),
                        width: content_rect.width,
                        height: region.height_px.unwrap_or(header_height).max(0),
                    };
                    header_index += 1;
                    self.push_frame_region(
                        surface.id,
                        window,
                        region,
                        rect,
                        CornerRadii::top(theme.theme.radius_px),
                        ops,
                        hit_regions,
                        opacity,
                    );
                }
                FrameRegionRole::Content => {
                    content_region_painted = true;
                    push_rect(
                        ops,
                        content_rect,
                        paint_color(&region.paint).with_alpha_scale(opacity),
                        CornerRadii::bottom(theme.theme.radius_px),
                    );
                    push_frame_elements(
                        surface.id,
                        window,
                        &region.children,
                        content_rect,
                        &self.theme_assets,
                        ops,
                        hit_regions,
                        self.hovered_action.as_ref(),
                        opacity,
                    );
                }
                FrameRegionRole::Custom => {}
            }
        }

        if !content_region_painted {
            push_rect(
                ops,
                content_rect,
                paint_color(&theme.theme.background).with_alpha_scale(opacity),
                CornerRadii::bottom(theme.theme.radius_px),
            );
        }

        push_content(ops, content_rect, surface.content.as_ref(), surface.opacity);

        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Surface,
            rect: frame_rect,
        });
        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Chrome,
            rect: frame_rect,
        });
        if header_height > 0 {
            hit_regions.push(HitRegion {
                surface_id: surface.id,
                kind: HitRegionKind::TitleBar,
                rect: RectI {
                    x: content_rect.x,
                    y: content_rect.y - header_height,
                    width: content_rect.width,
                    height: header_height,
                },
            });
        }
        hit_regions.push(HitRegion {
            surface_id: surface.id,
            kind: HitRegionKind::Content,
            rect: content_rect,
        });
    }

    fn push_frame_region(
        &self,
        surface_id: SurfaceId,
        window: &WindowSurface,
        region: &FrameRegion,
        rect: RectI,
        corner_radii: CornerRadii,
        ops: &mut Vec<RenderOp>,
        hit_regions: &mut Vec<HitRegion>,
        opacity: f32,
    ) {
        push_rect(
            ops,
            rect,
            paint_color(&region.paint).with_alpha_scale(opacity),
            corner_radii,
        );
        if let Some(layout) = &region.layout {
            push_row_layout(
                surface_id,
                window,
                layout,
                rect,
                &self.theme_assets,
                ops,
                hit_regions,
                self.hovered_action.as_ref(),
                opacity,
            );
        } else {
            push_frame_elements(
                surface_id,
                window,
                &region.children,
                rect,
                &self.theme_assets,
                ops,
                hit_regions,
                self.hovered_action.as_ref(),
                opacity,
            );
        }
    }

    fn push_plain_surface(
        &self,
        surface: &Surface,
        accepts_input: bool,
        ops: &mut Vec<RenderOp>,
        hit_regions: &mut Vec<HitRegion>,
    ) {
        push_content(
            ops,
            surface.geometry,
            surface.content.as_ref(),
            surface.opacity,
        );
        if accepts_input {
            hit_regions.push(HitRegion {
                surface_id: surface.id,
                kind: HitRegionKind::Surface,
                rect: surface.geometry,
            });
        }
    }

    fn push_desktop(
        &self,
        surface: &Surface,
        desktop: &DesktopSurface,
        ops: &mut Vec<RenderOp>,
        hit_regions: &mut Vec<HitRegion>,
    ) {
        push_rect(
            ops,
            surface.geometry,
            desktop
                .background_color
                .with_alpha_scale(opacity_scale(surface.opacity)),
            CornerRadii::zero(),
        );
        push_content(
            ops,
            surface.geometry,
            surface.content.as_ref(),
            surface.opacity,
        );
        if desktop.accepts_input {
            hit_regions.push(HitRegion {
                surface_id: surface.id,
                kind: HitRegionKind::Surface,
                rect: surface.geometry,
            });
        }
    }
}

fn focused_chrome(chrome: &WindowChrome, focused: bool) -> WindowChrome {
    let mut chrome = chrome.clone();
    if focused {
        chrome.border_color = chrome.border_color.with_alpha_scale(1.0);
    } else {
        chrome.border_color = chrome.border_color.with_alpha_scale(0.65);
        chrome.titlebar_color = chrome.titlebar_color.with_alpha_scale(0.82);
    }
    chrome
}

fn push_rect(ops: &mut Vec<RenderOp>, rect: RectI, color: ColorRgba8, corner_radii: CornerRadii) {
    if rect.width <= 0 || rect.height <= 0 || color.a == 0 {
        return;
    }
    ops.push(RenderOp::Rect(RenderRect {
        rect,
        color,
        corner_radii_px: corner_radii.sanitize(),
    }));
}

fn push_chrome_material(
    ops: &mut Vec<RenderOp>,
    frame_rect: RectI,
    chrome: &WindowChrome,
    opacity: f32,
) {
    let (shader_name, kind) = match chrome.material {
        ChromeMaterial::Solid => return,
        ChromeMaterial::BackdropBlur { radius_px, passes } => (
            "blur.spv",
            RenderMaterialKind::BackdropBlur { radius_px, passes },
        ),
        ChromeMaterial::Glass {
            tint_color,
            opacity: material_opacity,
            blur_radius_px,
            passes,
        } => (
            "glass.spv",
            RenderMaterialKind::Glass {
                tint_color: tint_color.with_alpha_scale(opacity),
                opacity: ((material_opacity as f32) * opacity).round().clamp(0.0, 255.0) as u8,
                blur_radius_px,
                passes,
            },
        ),
    };

    ops.push(RenderOp::Material(RenderMaterial {
        rect: frame_rect,
        corner_radii_px: chrome.corner_radii_px,
        shader_name: shader_name.to_string(),
        shader_spirv_words: None,
        passes: Vec::new(),
        kind,
    }));
}

fn push_titlebar_widgets(
    surface_id: SurfaceId,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    titlebar_rect: RectI,
    titlebar_radius_px: i32,
    widgets: &[Widget],
    fallback_title: &str,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    if titlebar_rect.width <= 0 || titlebar_rect.height <= 0 {
        return;
    }

    let mut flattened = Vec::new();
    collect_titlebar_widgets(widgets, &mut flattened);

    for row in flattened.iter().filter_map(|widget| match widget {
        Widget::ButtonRow(row) => Some(row),
        _ => None,
    }) {
        push_button_row(ops, titlebar_rect, row, opacity);
    }

    push_title_text(
        ops,
        titlebar_rect,
        &flattened,
        fallback_title,
        opacity,
        title_reserved_left(titlebar_rect, &flattened),
        title_reserved_right(titlebar_rect, titlebar_radius_px, &flattened),
    );

    for controls in flattened.iter().filter_map(|widget| match widget {
        Widget::ControlGroup(controls) => Some(controls),
        _ => None,
    }) {
        push_window_controls(
            surface_id,
            ops,
            hit_regions,
            titlebar_rect,
            titlebar_radius_px,
            controls,
            hovered_action,
            opacity,
        );
    }
}

fn push_title_text(
    ops: &mut Vec<RenderOp>,
    titlebar_rect: RectI,
    widgets: &[&Widget],
    fallback_title: &str,
    opacity: f32,
    reserved_left: i32,
    reserved_right: i32,
) {
    let title = widgets.iter().find_map(|widget| match widget {
        Widget::Text(title) => Some(title),
        _ => None,
    });
    let text = title
        .map(|title| title.text.as_str())
        .filter(|text| !text.is_empty())
        .unwrap_or(fallback_title);
    if text.is_empty() {
        return;
    }

    let color = title
        .map(|title| title.color)
        .unwrap_or_else(|| ColorRgba8::rgba(0xf6, 0xf9, 0xfc, 0xe8))
        .with_alpha_scale(opacity);
    let x = reserved_left.max(titlebar_rect.x.saturating_add(12));
    let max_right = reserved_right.min(titlebar_rect.right().saturating_sub(12));
    let max_width = max_right.saturating_sub(x).max(0);
    if max_width <= 0 {
        return;
    }

    ops.push(RenderOp::Text(RenderText {
        rect: RectI {
            x,
            y: titlebar_rect.y,
            width: max_width,
            height: titlebar_rect.height,
        },
        text: text.to_string(),
        color,
        font_size_px: (titlebar_rect.height / 2).clamp(7, 14),
    }));
}

fn explicit_header_height(theme: &WindowSurfaceTheme) -> i32 {
    theme
        .frame
        .regions
        .iter()
        .filter(|region| region.role == FrameRegionRole::Header)
        .map(|region| region.height_px.unwrap_or(0).max(0))
        .sum()
}

fn paint_color(paint: &SurfacePaint) -> ColorRgba8 {
    match paint {
        SurfacePaint::Transparent => ColorRgba8::rgba(0, 0, 0, 0),
        SurfacePaint::Color(color) => *color,
    }
}

fn button_paint_color(paint: ButtonPaint) -> ColorRgba8 {
    match paint {
        ButtonPaint::Transparent => ColorRgba8::rgba(0, 0, 0, 0),
        ButtonPaint::Fill(color) => color,
    }
}

fn push_row_layout(
    surface_id: SurfaceId,
    window: &WindowSurface,
    layout: &RowLayout,
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    let inner = RectI {
        x: rect.x + layout.padding.left,
        y: rect.y + layout.padding.top,
        width: rect
            .width
            .saturating_sub(layout.padding.left + layout.padding.right)
            .max(0),
        height: rect
            .height
            .saturating_sub(layout.padding.top + layout.padding.bottom)
            .max(0),
    };
    let fixed_width: i32 = layout.children.iter().map(measure_frame_element_width).sum();
    let expanded_count = layout
        .children
        .iter()
        .filter(|element| matches!(element, FrameElement::Slot(slot) if slot.expanded))
        .count() as i32;
    let expanded_width = if expanded_count > 0 {
        inner.width.saturating_sub(fixed_width).max(0) / expanded_count
    } else {
        0
    };

    let mut x = inner.x;
    for element in &layout.children {
        let width = match element {
            FrameElement::Slot(slot) if slot.expanded => expanded_width,
            _ => measure_frame_element_width(element),
        };
        let child_rect = RectI {
            x,
            y: inner.y,
            width,
            height: inner.height,
        };
        push_frame_element(
            surface_id,
            window,
            element,
            child_rect,
            assets,
            ops,
            hit_regions,
            hovered_action,
            opacity,
        );
        x = x.saturating_add(width);
    }
}

fn measure_frame_element_width(element: &FrameElement) -> i32 {
    match element {
        FrameElement::Slot(slot) => {
            if slot.expanded {
                0
            } else {
                measure_frame_element_width(slot.child.as_ref())
            }
        }
        FrameElement::ButtonGroup(group) => measure_button_group_width(group),
        FrameElement::Text(_) => 120,
        FrameElement::AppContent => 0,
    }
}

fn measure_button_group_width(group: &ChromeButtonGroup) -> i32 {
    let buttons = group.buttons.len() as i32;
    if buttons == 0 {
        return 0;
    }
    group
        .buttons
        .iter()
        .map(|button| button.size.width.max(0))
        .sum::<i32>()
        .saturating_add(group.spacing_px.max(0).saturating_mul(buttons.saturating_sub(1)))
}

fn push_frame_elements(
    surface_id: SurfaceId,
    window: &WindowSurface,
    elements: &[FrameElement],
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    for element in elements {
        push_frame_element(
            surface_id,
            window,
            element,
            rect,
            assets,
            ops,
            hit_regions,
            hovered_action,
            opacity,
        );
    }
}

fn push_frame_element(
    surface_id: SurfaceId,
    window: &WindowSurface,
    element: &FrameElement,
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    match element {
        FrameElement::Slot(slot) => {
            push_frame_slot(
                surface_id,
                window,
                slot,
                rect,
                assets,
                ops,
                hit_regions,
                hovered_action,
                opacity,
            );
        }
        FrameElement::ButtonGroup(group) => {
            push_chrome_button_group(
                surface_id,
                group,
                rect,
                assets,
                ops,
                hit_regions,
                hovered_action,
                opacity,
            );
        }
        FrameElement::Text(text) => push_explicit_text(window, text, rect, ops, opacity),
        FrameElement::AppContent => {}
    }
}

fn push_frame_slot(
    surface_id: SurfaceId,
    window: &WindowSurface,
    slot: &FrameSlot,
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    push_frame_element(
        surface_id,
        window,
        slot.child.as_ref(),
        rect,
        assets,
        ops,
        hit_regions,
        hovered_action,
        opacity,
    );
}

fn push_explicit_text(
    window: &WindowSurface,
    text: &TextElement,
    rect: RectI,
    ops: &mut Vec<RenderOp>,
    opacity: f32,
) {
    let value = match &text.value {
        TextValue::Literal(value) => value.clone(),
        TextValue::WindowTitle => window.title.clone(),
    };
    if value.is_empty() || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    ops.push(RenderOp::Text(RenderText {
        rect,
        text: value,
        color: text.style.color.with_alpha_scale(opacity),
        font_size_px: text.style.font_size_px,
    }));
}

fn push_chrome_button_group(
    surface_id: SurfaceId,
    group: &ChromeButtonGroup,
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    let mut x = rect.x;
    for (index, button) in group.buttons.iter().enumerate() {
        let y = rect.y + (rect.height - button.size.height) / 2;
        let button_rect = RectI {
            x,
            y,
            width: button.size.width,
            height: button.size.height,
        };
        push_chrome_button(
            surface_id,
            index,
            button,
            button_rect,
            assets,
            ops,
            hit_regions,
            hovered_action,
            opacity,
        );
        x = x.saturating_add(button.size.width + group.spacing_px.max(0));
    }
}

fn push_chrome_button(
    surface_id: SurfaceId,
    index: usize,
    button: &ChromeButton,
    rect: RectI,
    assets: &BTreeMap<String, ThemeImageAsset>,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    let radius = match button.shape {
        ButtonShape::Circle => rect.width.min(rect.height) / 2,
        ButtonShape::RoundedRect => button.radius_px,
    };
    let action = action_for_request(&button.request);
    let paint = if action
        .as_ref()
        .is_some_and(|action| action_is_hovered(hovered_action, surface_id, index, &action.name))
    {
        button.hover_paint.unwrap_or(button.paint)
    } else {
        button.paint
    };
    let paint_color = button_paint_color(paint);
    push_rect(
        ops,
        rect,
        paint_color.with_alpha_scale(opacity),
        CornerRadii::all(radius),
    );
    if let Some(icon) = &button.icon {
        push_icon_ref(
            ops,
            icon,
            rect,
            opacity,
            assets,
        );
    }
    if let Some(action) = action {
        hit_regions.push(HitRegion {
            surface_id,
            kind: HitRegionKind::Action { index, action },
            rect,
        });
    }
}

fn action_for_request(request: &SurfaceRequest) -> Option<Action> {
    match request {
        SurfaceRequest::Compositor(CompositorRequest::Close) => Some(Action::new("window.close")),
        SurfaceRequest::Compositor(CompositorRequest::Minimize) => {
            Some(Action::new("window.minimize"))
        }
        SurfaceRequest::Compositor(CompositorRequest::ToggleExpanded) => {
            Some(Action::new("window.toggle_expand"))
        }
        SurfaceRequest::App(name) => Some(Action::new(format!("app.{name}"))),
        SurfaceRequest::None => None,
    }
}

fn action_is_hovered(
    hovered_action: Option<&HoveredAction>,
    surface_id: SurfaceId,
    index: usize,
    action_name: &str,
) -> bool {
    hovered_action.is_some_and(|hovered| {
        hovered.surface_id == surface_id
            && hovered.index == index
            && hovered.action_name == action_name
    })
}

fn push_icon_ref(
    ops: &mut Vec<RenderOp>,
    icon: &IconRef,
    rect: RectI,
    opacity: f32,
    assets: &BTreeMap<String, ThemeImageAsset>,
) {
    match icon {
        IconRef::Asset(asset) => push_asset_icon(ops, asset.path.as_str(), rect, opacity, assets),
    }
}

fn push_asset_icon(
    ops: &mut Vec<RenderOp>,
    asset_path: &str,
    rect: RectI,
    opacity: f32,
    assets: &BTreeMap<String, ThemeImageAsset>,
) {
    let Some(asset) = assets.get(asset_path) else {
        return;
    };
    if asset.size.width <= 0 || asset.size.height <= 0 || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let max_size = rect.width.min(rect.height).max(1);
    let scale_width = max_size;
    let scale_height = asset.size.height.saturating_mul(max_size) / asset.size.width.max(1);
    let (width, height) = if scale_height <= rect.height {
        (scale_width, scale_height.max(1))
    } else {
        let height = rect.height.max(1);
        (
            asset.size.width.saturating_mul(height) / asset.size.height.max(1),
            height,
        )
    };
    let dst_x = rect.x + (rect.width - width) / 2;
    let dst_y = rect.y + (rect.height - height) / 2;
    let pixels_rgba8 = if opacity >= 0.999 {
        asset.pixels_rgba8.clone()
    } else {
        scale_pixels_alpha(&asset.pixels_rgba8, opacity).into()
    };
    ops.push(RenderOp::Blit(RenderBlit {
        texture_key: ThemeAssetStore::stable_texture_key(asset_path),
        dst_x,
        dst_y,
        width,
        height,
        src_x: 0,
        src_y: 0,
        src_width: asset.size.width,
        pixels_rgba8,
        dmabuf: None,
        content_version: ThemeAssetStore::stable_texture_key(asset_path),
        damage_rects: Vec::<RectI>::new().into(),
        corner_radii_px: CornerRadii::zero(),
    }));
}

fn scale_pixels_alpha(pixels: &[u8], opacity: f32) -> Vec<u8> {
    let mut scaled = pixels.to_vec();
    for pixel in scaled.chunks_exact_mut(4) {
        pixel[3] = ((pixel[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
    }
    scaled
}

fn collect_titlebar_widgets<'a>(widgets: &'a [Widget], output: &mut Vec<&'a Widget>) {
    for widget in widgets {
        output.push(widget);
        match widget {
            Widget::Stack(stack) => collect_titlebar_widgets(&stack.children, output),
            Widget::VStack(stack) => collect_titlebar_widgets(&stack.children, output),
            Widget::HStack(stack) => collect_titlebar_widgets(&stack.children, output),
            Widget::Align(align) => {
                collect_titlebar_widgets(std::slice::from_ref(align.child.as_ref()), output);
            }
            Widget::Padding(padding) => {
                collect_titlebar_widgets(std::slice::from_ref(padding.child.as_ref()), output);
            }
            _ => {}
        }
    }
}

fn push_button_row(ops: &mut Vec<RenderOp>, titlebar_rect: RectI, row: &ButtonRow, opacity: f32) {
    let count = row.button_count as i32;
    if count <= 0 {
        return;
    }

    let size = titlebar_button_row_size(titlebar_rect);
    let spacing = 6;
    let mut x = titlebar_rect.x.saturating_add(10);
    let y = titlebar_rect.y + (titlebar_rect.height - size) / 2;
    for index in 0..count {
        let alpha_scale = 1.0 - (index as f32 * 0.14).min(0.36);
        push_rect(
            ops,
            RectI {
                x,
                y,
                width: size,
                height: size,
            },
            row.accent_color
                .with_alpha_scale(opacity)
                .with_alpha_scale(alpha_scale),
            CornerRadii::all(size / 2),
        );
        x = x.saturating_add(size + spacing);
    }
}

fn push_window_controls(
    surface_id: SurfaceId,
    ops: &mut Vec<RenderOp>,
    hit_regions: &mut Vec<HitRegion>,
    titlebar_rect: RectI,
    titlebar_radius_px: i32,
    controls: &ControlGroup,
    hovered_action: Option<&HoveredAction>,
    opacity: f32,
) {
    for button in layout_window_controls(titlebar_rect, titlebar_radius_px, controls) {
        let color = if button.action.as_ref().is_some_and(|action| {
            action_is_hovered(hovered_action, surface_id, button.index, &action.name)
        }) {
            button.hover_color.unwrap_or(button.color)
        } else {
            button.color
        };
        let fill = color.with_alpha_scale(opacity);
        push_rect(ops, button.rect, fill, button.corner_radii_px);
        if let Some(action) = button.action {
            hit_regions.push(HitRegion {
                surface_id,
                kind: HitRegionKind::Action {
                    index: button.index,
                    action,
                },
                rect: button.rect,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowControlLayout {
    index: usize,
    action: Option<Action>,
    color: ColorRgba8,
    hover_color: Option<ColorRgba8>,
    rect: RectI,
    corner_radii_px: CornerRadii,
}

fn layout_window_controls(
    titlebar_rect: RectI,
    titlebar_radius_px: i32,
    controls: &ControlGroup,
) -> Vec<WindowControlLayout> {
    let buttons: Vec<_> = controls
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| match child {
            Widget::IconButton(button) => Some((index, button)),
            _ => None,
        })
        .collect();

    if titlebar_rect.width <= 0 || titlebar_rect.height <= 0 || buttons.is_empty() {
        return Vec::new();
    }

    let spacing = controls.spacing_px.max(0);
    let margin = controls.margin_px.max(0);
    let button_height = titlebar_rect.height.max(12);
    let button_width = (controls.button_size_px.max(12) * 3).max(38);
    let total_width = button_width
        .saturating_mul(buttons.len() as i32)
        .saturating_add(spacing.saturating_mul(buttons.len().saturating_sub(1) as i32));
    let mut x = titlebar_rect
        .x
        .saturating_add(titlebar_rect.width)
        .saturating_sub(margin)
        .saturating_sub(total_width);
    let titlebar_left = titlebar_rect.x;
    let titlebar_right = titlebar_rect.right();

    buttons
        .into_iter()
        .map(|(index, button)| {
            let rect = RectI {
                x,
                y: titlebar_rect.y,
                width: button_width,
                height: button_height,
            };
            x = x.saturating_add(button_width + spacing);
            WindowControlLayout {
                index,
                action: button.action.clone(),
                color: button.color,
                hover_color: button.hover_background_color,
                rect,
                corner_radii_px: CornerRadii {
                    top_left: if rect.x <= titlebar_left {
                        titlebar_radius_px
                    } else {
                        0
                    },
                    top_right: if rect.right() >= titlebar_right {
                        titlebar_radius_px
                    } else {
                        0
                    },
                    bottom_right: 0,
                    bottom_left: 0,
                },
            }
        })
        .collect()
}

fn title_reserved_left(titlebar_rect: RectI, widgets: &[&Widget]) -> i32 {
    let mut reserved = titlebar_rect.x.saturating_add(12);
    for row in widgets.iter().filter_map(|widget| match widget {
        Widget::ButtonRow(row) => Some(row),
        _ => None,
    }) {
        let count = row.button_count as i32;
        if count <= 0 {
            continue;
        }
        let size = titlebar_button_row_size(titlebar_rect);
        let spacing: i32 = 6;
        let width = size
            .saturating_mul(count)
            .saturating_add(spacing.saturating_mul(count.saturating_sub(1)));
        reserved = reserved.max(
            titlebar_rect
                .x
                .saturating_add(10)
                .saturating_add(width)
                .saturating_add(10),
        );
    }
    reserved
}

fn title_reserved_right(titlebar_rect: RectI, titlebar_radius_px: i32, widgets: &[&Widget]) -> i32 {
    widgets
        .iter()
        .filter_map(|widget| match widget {
            Widget::ControlGroup(controls) => {
                layout_window_controls(titlebar_rect, titlebar_radius_px, controls)
                    .first()
                    .map(|button| button.rect.x.saturating_sub(8))
            }
            _ => None,
        })
        .min()
        .unwrap_or_else(|| titlebar_rect.right().saturating_sub(12))
}

fn titlebar_button_row_size(titlebar_rect: RectI) -> i32 {
    (titlebar_rect.height - 14).clamp(8, 16)
}

fn push_content(
    ops: &mut Vec<RenderOp>,
    dst_rect: RectI,
    content: Option<&SurfaceContent>,
    opacity: u8,
) {
    let Some(content) = content else {
        return;
    };
    let width = dst_rect.width.min(content.size.width).max(0);
    let height = dst_rect.height.min(content.size.height).max(0);
    if width == 0 || height == 0 || opacity == 0 {
        return;
    }

    ops.push(RenderOp::Blit(RenderBlit {
        texture_key: content.texture_key,
        dst_x: dst_rect.x,
        dst_y: dst_rect.y,
        width,
        height,
        src_x: 0,
        src_y: 0,
        src_width: content.size.width,
        pixels_rgba8: content.pixels_rgba8.clone(),
        dmabuf: content.dmabuf.clone(),
        content_version: content.content_version,
        damage_rects: content.damage_rects.clone(),
        corner_radii_px: CornerRadii::zero(),
    }));
}

fn opacity_scale(opacity: u8) -> f32 {
    opacity as f32 / 255.0
}

fn missing_surface(id: SurfaceId) -> SurfaceError {
    SurfaceError::new(format!("{id} does not exist"))
}

#[cfg(test)]
mod tests {
    use lithic_core::{ColorRgba8, RectI, SizeI};
    use lithic_render::{RenderOp, RenderTargetId};
    use lithic_theme::surface::{
        app_content, chrome_button, chrome_button_group, frame_region, frame_slot, row_layout,
        surface_frame, surface_theme, window_surface, AssetRef, ButtonPaint, CompositorRequest,
        EdgeInsetsI, FrameElement, IconRef, SurfacePaint, SurfaceRequest, TextValue,
    };
    use lithic_theme::{TextElement, TextElementStyle, ThemeImageAsset};
    use lithic_ui::{Action, Icon, control_group, icon_button, text};

    use crate::chrome::{ChromeMaterial, WINDOW_ACTION_CLOSE, WINDOW_ACTION_TOGGLE_EXPAND};
    use crate::{
        CreateDesktopSurface, CreateLayerSurface, CreateWindowSurface, DesktopSurface,
        HitRegionKind, LayerAnchor, LayerSurface, LayerSurfaceRole, SurfaceCommand, SurfaceContent,
        SurfaceController, SurfaceId, WindowChrome,
    };

    #[test]
    fn commands_create_order_focus_and_destroy_surfaces() {
        let mut controller = SurfaceController::new();
        let desktop_id = SurfaceId::new(1);
        let window_id = SurfaceId::new(2);
        let layer_id = SurfaceId::new(3);

        controller
            .submit(SurfaceCommand::CreateDesktop(CreateDesktopSurface {
                id: desktop_id,
                geometry: rect(0, 0, 800, 600),
                z_order: 0,
                content: None,
                desktop: DesktopSurface {
                    background_color: ColorRgba8::rgba(1, 2, 3, 255),
                    accepts_input: true,
                },
            }))
            .unwrap();
        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: window_id,
                geometry: rect(20, 50, 320, 180),
                z_order: 10,
                title: "Notes".to_string(),
                app_id: "notes".to_string(),
                content: Some(content(20, 320, 180)),
                chrome: WindowChrome::default(),
                surface_theme: None,
            }))
            .unwrap();
        controller
            .submit(SurfaceCommand::CreateLayer(CreateLayerSurface {
                id: layer_id,
                geometry: rect(0, 560, 800, 40),
                z_order: 50,
                content: Some(content(30, 800, 40)),
                layer: LayerSurface {
                    role: LayerSurfaceRole::Taskbar,
                    anchor: LayerAnchor::Bottom,
                    exclusive_zone_px: Some(40),
                    accepts_input: true,
                },
            }))
            .unwrap();

        controller
            .submit(SurfaceCommand::SetFocus {
                id: Some(window_id),
            })
            .unwrap();
        controller
            .submit(SurfaceCommand::Raise { id: window_id })
            .unwrap();

        let ordered: Vec<_> = controller
            .ordered_surfaces()
            .into_iter()
            .map(|surface| surface.id)
            .collect();
        assert_eq!(ordered, vec![desktop_id, layer_id, window_id]);

        controller
            .submit(SurfaceCommand::DestroySurface { id: layer_id })
            .unwrap();
        assert!(controller.surface(layer_id).is_none());
    }

    #[test]
    fn tick_outputs_render_frame_and_hit_regions() {
        let mut controller = SurfaceController::new();
        let window_id = SurfaceId::new(7);
        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: window_id,
                geometry: rect(20, 50, 100, 80),
                z_order: 1,
                title: "Terminal".to_string(),
                app_id: "terminal".to_string(),
                content: Some(content(1, 100, 80)),
                chrome: WindowChrome::default(),
                surface_theme: None,
            }))
            .unwrap();

        let tick = controller.tick(crate::controller::TickInput {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 200,
                height: 160,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            frame_time_ns: 0,
        });

        assert_eq!(tick.render_frame.output_id, RenderTargetId::new(1));
        assert!(
            tick.render_frame
                .ops
                .iter()
                .any(|op| matches!(op, RenderOp::Blit(_)))
        );
        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Material(material) if material.shader_name == "shadow.spv"
        )));
        assert!(
            tick.hit_regions
                .iter()
                .any(|region| region.surface_id == window_id)
        );
    }

    #[test]
    fn tick_outputs_titlebar_widgets_and_action_hit_regions() {
        let mut controller = SurfaceController::new();
        let window_id = SurfaceId::new(8);
        let close_color = ColorRgba8::rgba(0xee, 0x55, 0x66, 0xff);
        let title_color = ColorRgba8::rgba(0x22, 0x33, 0x44, 0xff);

        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: window_id,
                geometry: rect(20, 50, 160, 80),
                z_order: 1,
                title: "Fallback Title".to_string(),
                app_id: "terminal".to_string(),
                content: None,
                chrome: WindowChrome::default().with_titlebar_widgets([
                    text("Terminal", title_color),
                    control_group(
                        [
                            icon_button(
                                Icon::ToggleExpand,
                                ColorRgba8::rgba(0x68, 0x99, 0xff, 0xff),
                                Some(Action::new(WINDOW_ACTION_TOGGLE_EXPAND)),
                            ),
                            icon_button(
                                Icon::Close,
                                close_color,
                                Some(Action::new(WINDOW_ACTION_CLOSE)),
                            ),
                        ],
                        12,
                        8,
                        10,
                    ),
                ]),
                surface_theme: None,
            }))
            .unwrap();

        let tick = controller.tick(crate::controller::TickInput {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 240,
                height: 180,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            frame_time_ns: 0,
        });

        assert!(
            tick.render_frame.ops.iter().any(|op| matches!(
                op,
                RenderOp::Text(text) if text.text == "Terminal" && text.color == title_color
            )),
            "expected title text render op"
        );
        assert!(
            tick.render_frame.ops.iter().any(|op| matches!(
                op,
                RenderOp::Rect(rect) if rect.color == close_color
            )),
            "expected close button rect"
        );
        assert!(tick.hit_regions.iter().any(|region| matches!(
            &region.kind,
            HitRegionKind::Action {
                index: 1,
                action,
            } if action.name == WINDOW_ACTION_CLOSE
        )));
    }

    #[test]
    fn tick_outputs_window_glass_material_from_chrome() {
        let mut controller = SurfaceController::new();
        let window_id = SurfaceId::new(9);
        let tint = ColorRgba8::rgba(0x70, 0x90, 0xa8, 0xff);

        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: window_id,
                geometry: rect(20, 50, 160, 80),
                z_order: 1,
                title: "Glass".to_string(),
                app_id: "demo".to_string(),
                content: None,
                chrome: WindowChrome {
                    material: ChromeMaterial::Glass {
                        tint_color: tint,
                        opacity: 120,
                        blur_radius_px: 8,
                        passes: 2,
                    },
                    ..WindowChrome::default()
                },
                surface_theme: None,
            }))
            .unwrap();

        let tick = controller.tick(crate::controller::TickInput {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 240,
                height: 180,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            frame_time_ns: 0,
        });

        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Material(material)
                if material.shader_name == "glass.spv"
                    && matches!(
                        material.kind,
                        lithic_render::RenderMaterialKind::Glass {
                            tint_color,
                            opacity: 120,
                            blur_radius_px: 8,
                            passes: 2,
                        } if tint_color == tint
                    )
        )));
    }

    #[test]
    fn explicit_surface_theme_renders_declared_buttons_without_implicit_icons() {
        let mut controller = SurfaceController::new();
        let window_id = SurfaceId::new(10);
        let close_color = ColorRgba8::rgba(0xff, 0x5f, 0x57, 0xff);
        let expand_color = ColorRgba8::rgba(0x28, 0xc8, 0x40, 0xff);
        let title_color = ColorRgba8::rgba(0x20, 0x21, 0x24, 0xff);

        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: window_id,
                geometry: rect(30, 80, 240, 140),
                z_order: 1,
                title: "Project Editor".to_string(),
                app_id: "editor".to_string(),
                content: None,
                chrome: WindowChrome::default(),
                surface_theme: Some(
                    window_surface("editor")
                        .theme(
                            surface_theme()
                                .background(SurfacePaint::color(ColorRgba8::rgba(
                                    0xf7, 0xf8, 0xfa, 0xff,
                                )))
                                .border(crate::theme::BorderPaint::new(
                                    ColorRgba8::rgba(0, 0, 0, 0x22),
                                    1,
                                ))
                                .radius(12),
                        )
                        .frame(surface_frame([
                            frame_region("header")
                                .header()
                                .height(48)
                                .paint(SurfacePaint::color(ColorRgba8::rgba(
                                    0xed, 0xef, 0xf3, 0xff,
                                )))
                                .layout(
                                    row_layout([
                                        FrameElement::from(frame_slot(
                                            "left_buttons",
                                            chrome_button_group([
                                                chrome_button(
                                                    "close",
                                                    SurfaceRequest::Compositor(
                                                        CompositorRequest::Close,
                                                    ),
                                                )
                                                .circle(14)
                                                .paint(ButtonPaint::fill(close_color))
                                                .icon(None),
                                                chrome_button(
                                                    "expand",
                                                    SurfaceRequest::Compositor(
                                                        CompositorRequest::ToggleExpanded,
                                                    ),
                                                )
                                                .circle(14)
                                                .paint(ButtonPaint::fill(expand_color))
                                                .icon(None),
                                            ])
                                            .spacing(8),
                                        )),
                                        FrameElement::from(
                                            frame_slot(
                                                "title",
                                                TextElement::new(TextValue::WindowTitle).style(
                                                    TextElementStyle {
                                                        font_size_px: 13,
                                                        weight: crate::theme::FontWeight::Medium,
                                                        color: title_color,
                                                    },
                                                ),
                                            )
                                            .expanded(),
                                        ),
                                        FrameElement::from(frame_slot(
                                            "right_buttons",
                                            chrome_button_group([chrome_button(
                                                "search",
                                                SurfaceRequest::App("open_search".to_string()),
                                            )
                                            .rounded_rect(SizeI { width: 34, height: 30 }, 6)
                                            .paint(ButtonPaint::transparent())
                                            .icon(None)])
                                            .spacing(4),
                                        )),
                                    ])
                                    .padding(EdgeInsetsI::symmetric(10, 0))
                                    .align_center(),
                                ),
                            frame_region("content").content().child(app_content()),
                        ])),
                ),
            }))
            .unwrap();

        let tick = controller.tick(crate::controller::TickInput {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 320,
                height: 260,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            frame_time_ns: 0,
        });

        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Rect(rect) if rect.color == close_color
        )));
        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Rect(rect) if rect.color == expand_color
        )));
        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Text(text) if text.text == "Project Editor" && text.color == title_color
        )));
        assert!(tick.hit_regions.iter().any(|region| matches!(
            &region.kind,
            HitRegionKind::Action { action, .. } if action.name == "window.close"
        )));
        assert!(tick.hit_regions.iter().any(|region| matches!(
            &region.kind,
            HitRegionKind::Action { action, .. } if action.name == "app.open_search"
        )));
    }

    #[test]
    fn explicit_surface_theme_renders_asset_icon_as_blit() {
        let mut controller = SurfaceController::new();
        controller.theme_assets.insert(
            "icons/close.rgba".to_string(),
            ThemeImageAsset {
                size: SizeI {
                    width: 2,
                    height: 2,
                },
                pixels_rgba8: vec![
                    0xff, 0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0x00, 0xff, 0xff,
                    0xff, 0xff, 0xff, 0xff,
                ]
                .into(),
            },
        );

        controller
            .submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
                id: SurfaceId::new(11),
                geometry: rect(20, 60, 160, 90),
                z_order: 1,
                title: "Assets".to_string(),
                app_id: "editor".to_string(),
                content: None,
                chrome: WindowChrome::default(),
                surface_theme: Some(
                    window_surface("editor").frame(surface_frame([frame_region("header")
                        .header()
                        .height(32)
                        .layout(row_layout([FrameElement::from(frame_slot(
                            "buttons",
                            chrome_button_group([chrome_button(
                                "close",
                                SurfaceRequest::Compositor(CompositorRequest::Close),
                            )
                            .rounded_rect(SizeI { width: 32, height: 32 }, 0)
                            .paint(ButtonPaint::transparent())
                            .icon(Some(IconRef::Asset(AssetRef::new("icons/close.rgba"))))]),
                        ))]))])),
                ),
            }))
            .unwrap();

        let tick = controller.tick(crate::controller::TickInput {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 240,
                height: 180,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            frame_time_ns: 0,
        });

        assert!(tick.render_frame.ops.iter().any(|op| matches!(
            op,
            RenderOp::Blit(blit)
                if blit.texture_key == lithic_theme::ThemeAssetStore::stable_texture_key("icons/close.rgba")
                    && blit.src_width == 2
                    && blit.pixels_rgba8.len() == 16
        )));
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> RectI {
        RectI {
            x,
            y,
            width,
            height,
        }
    }

    fn content(texture_key: u64, width: i32, height: i32) -> SurfaceContent {
        let mut pixels = Vec::new();
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xff]);
        }
        SurfaceContent::from_rgba8(texture_key, SizeI { width, height }, pixels, 1)
    }
}
