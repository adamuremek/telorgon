use std::sync::Arc;

use crate::foundation::{ColorRgba8, PointI, RectI, SizeI};

use super::{ThemeNode, ThemeOutputId, ThemeViewId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeInput {
    pub output: OutputModel,
    pub windows: Vec<WindowModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputModel {
    pub id: ThemeOutputId,
    pub name: String,
    pub logical_size: SizeI,
    pub scale: i32,
    pub keyboard_focused_window: Option<ThemeViewId>,
    pub pointer_focused_window: Option<ThemeViewId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowModel {
    pub id: ThemeViewId,
    pub title: String,
    pub app_id: String,
    pub mapped: bool,
    pub focused: bool,
    pub geometry: Option<RectI>,
    pub content_extent: SizeI,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFrame {
    pub output: OutputTheme,
    pub windows: Vec<WindowTheme>,
}

impl ThemeFrame {
    pub fn window_theme(&self, view_id: ThemeViewId) -> Option<&WindowTheme> {
        self.windows
            .iter()
            .find(|window_theme| window_theme.view_id == view_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputTheme {
    pub output_id: ThemeOutputId,
    pub background_color: ColorRgba8,
    pub overlay_nodes: Vec<ThemeNode>,
    pub cursor: Option<CursorTheme>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowTheme {
    pub view_id: ThemeViewId,
    pub chrome_nodes: Vec<ThemeNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorTheme {
    pub hotspot: PointI,
    pub image: ThemeImage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeImage {
    pub size: SizeI,
    pub pixels_rgba8: Arc<[u8]>,
}
