use std::sync::Arc;

use lithic_core::{ColorRgba8, RectI, SizeI};
use lithic_render::RenderDmabuf;
use lithic_theme::WindowSurfaceTheme;

use crate::chrome::WindowChrome;
use crate::id::SurfaceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub id: SurfaceId,
    pub geometry: RectI,
    pub z_order: i32,
    pub visible: bool,
    pub opacity: u8,
    pub content: Option<SurfaceContent>,
    pub kind: SurfaceKind,
}

impl Surface {
    pub fn new(id: SurfaceId, geometry: RectI, z_order: i32, kind: SurfaceKind) -> Self {
        Self {
            id,
            geometry,
            z_order,
            visible: true,
            opacity: 255,
            content: None,
            kind,
        }
    }

    pub fn with_content(mut self, content: Option<SurfaceContent>) -> Self {
        self.content = content;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Window(WindowSurface),
    Layer(LayerSurface),
    Desktop(DesktopSurface),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSurface {
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub chrome: WindowChrome,
    pub surface_theme: Option<WindowSurfaceTheme>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayerSurfaceRole {
    Taskbar,
    Dock,
    Panel,
    Overlay,
    Background,
    Custom,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayerAnchor {
    Top,
    Bottom,
    Left,
    Right,
    Fill,
    Floating,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerSurface {
    pub role: LayerSurfaceRole,
    pub anchor: LayerAnchor,
    pub exclusive_zone_px: Option<i32>,
    pub accepts_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSurface {
    pub background_color: ColorRgba8,
    pub accepts_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceContent {
    pub texture_key: u64,
    pub size: SizeI,
    pub pixels_rgba8: Arc<[u8]>,
    pub dmabuf: Option<Arc<RenderDmabuf>>,
    pub content_version: u64,
    pub damage_rects: Arc<[RectI]>,
    pub is_opaque: bool,
}

impl SurfaceContent {
    pub fn from_rgba8(
        texture_key: u64,
        size: SizeI,
        pixels_rgba8: impl Into<Arc<[u8]>>,
        content_version: u64,
    ) -> Self {
        Self {
            texture_key,
            size,
            pixels_rgba8: pixels_rgba8.into(),
            dmabuf: None,
            content_version,
            damage_rects: Vec::<RectI>::new().into(),
            is_opaque: false,
        }
    }
}
