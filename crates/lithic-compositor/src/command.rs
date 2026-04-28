use lithic_core::RectI;

use crate::chrome::WindowChrome;
use crate::id::SurfaceId;
use crate::surface::{DesktopSurface, LayerSurface, SurfaceContent};
use lithic_theme::WindowSurfaceTheme;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateWindowSurface {
    pub id: SurfaceId,
    pub geometry: RectI,
    pub z_order: i32,
    pub title: String,
    pub app_id: String,
    pub content: Option<SurfaceContent>,
    pub chrome: WindowChrome,
    pub surface_theme: Option<WindowSurfaceTheme>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateLayerSurface {
    pub id: SurfaceId,
    pub geometry: RectI,
    pub z_order: i32,
    pub content: Option<SurfaceContent>,
    pub layer: LayerSurface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDesktopSurface {
    pub id: SurfaceId,
    pub geometry: RectI,
    pub z_order: i32,
    pub content: Option<SurfaceContent>,
    pub desktop: DesktopSurface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceCommand {
    CreateWindow(CreateWindowSurface),
    CreateLayer(CreateLayerSurface),
    CreateDesktop(CreateDesktopSurface),
    DestroySurface {
        id: SurfaceId,
    },
    SetGeometry {
        id: SurfaceId,
        geometry: RectI,
    },
    SetContent {
        id: SurfaceId,
        content: Option<SurfaceContent>,
    },
    SetFocus {
        id: Option<SurfaceId>,
    },
    Raise {
        id: SurfaceId,
    },
    Lower {
        id: SurfaceId,
    },
    SetZOrder {
        id: SurfaceId,
        z_order: i32,
    },
    SetChrome {
        id: SurfaceId,
        chrome: WindowChrome,
    },
    SetWindowSurfaceTheme {
        id: SurfaceId,
        surface_theme: Option<WindowSurfaceTheme>,
    },
    SetWindowMetadata {
        id: SurfaceId,
        title: String,
        app_id: String,
    },
    SetVisible {
        id: SurfaceId,
        visible: bool,
    },
    SetOpacity {
        id: SurfaceId,
        opacity: u8,
    },
}
