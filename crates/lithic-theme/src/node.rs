use crate::foundation::{ColorRgba8, PointI};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WindowControlKind {
    ToggleExpand,
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowControlHoverEffect {
    pub background_color: ColorRgba8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowControlButton {
    pub kind: WindowControlKind,
    pub color: ColorRgba8,
    pub on_hover: Option<WindowControlHoverEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeNode {
    Stack {
        children: Vec<ThemeNode>,
    },
    SurfaceContent {
        fill_color: ColorRgba8,
    },
    RoundedRect {
        fill_color: ColorRgba8,
        radius_px: i32,
    },
    Border {
        color: ColorRgba8,
        thickness_px: i32,
        radius_px: i32,
    },
    TopRow {
        color: ColorRgba8,
        height_px: i32,
        children: Vec<ThemeNode>,
    },
    TitleBar {
        color: ColorRgba8,
        height_px: i32,
    },
    TitleText {
        text: String,
        color: ColorRgba8,
    },
    ButtonRow {
        accent_color: ColorRgba8,
        button_count: u8,
    },
    WindowControls {
        buttons: Vec<WindowControlButton>,
        button_size_px: i32,
        spacing_px: i32,
        margin_px: i32,
    },
    BackdropBlur {
        radius_px: i32,
        passes: u8,
    },
    GlassMaterial {
        tint_color: ColorRgba8,
        opacity: u8,
    },
    Shadow {
        color: ColorRgba8,
        radius_px: i32,
        offset: PointI,
        strength: u8,
    },
    Transform {
        offset: PointI,
        child: Box<ThemeNode>,
    },
    Opacity {
        alpha: u8,
        child: Box<ThemeNode>,
    },
    Clip {
        radius_px: i32,
        child: Box<ThemeNode>,
    },
}
