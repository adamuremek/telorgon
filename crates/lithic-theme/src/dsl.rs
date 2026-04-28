use crate::foundation::{ColorRgba8, PointI};

use super::{
    CursorTheme, OutputTheme, ThemeFrame, ThemeNode, ThemeOutputId, ThemeViewId,
    WindowControlButton, WindowControlHoverEffect, WindowControlKind, WindowTheme,
};

/// Declarative theme constructors intended to read like a nested UI tree.
///
/// Example:
/// ```rust
/// use lithic_theme::foundation::ColorRgba8;
/// use lithic_theme::dsl::*;
///
/// let chrome = stack([
///     shadow(ColorRgba8::rgba(0, 0, 0, 0x80), 32, point(0, 12), 120),
///     border(ColorRgba8::rgba(0xff, 0xff, 0xff, 0xaa), 2, 18),
///     clip(18, rounded_rect(ColorRgba8::rgba(0x24, 0x33, 0x41, 0xda), 18)),
///     title_bar(ColorRgba8::rgba(0x21, 0x2d, 0x39, 0xe0), 34),
///     backdrop_blur(18, 3),
///     glass_material(ColorRgba8::rgba(0xbd, 0xd1, 0xe5, 0xff), 128),
/// ]);
/// ```

pub fn point(x: i32, y: i32) -> PointI {
    PointI { x, y }
}

pub fn stack<I, T>(children: I) -> ThemeNode
where
    I: IntoIterator<Item = T>,
    T: Into<ThemeNode>,
{
    ThemeNode::Stack {
        children: children.into_iter().map(Into::into).collect(),
    }
}

pub fn surface_content(fill_color: ColorRgba8) -> ThemeNode {
    ThemeNode::SurfaceContent { fill_color }
}

pub fn rounded_rect(fill_color: ColorRgba8, radius_px: i32) -> ThemeNode {
    ThemeNode::RoundedRect {
        fill_color,
        radius_px,
    }
}

pub fn border(color: ColorRgba8, thickness_px: i32, radius_px: i32) -> ThemeNode {
    ThemeNode::Border {
        color,
        thickness_px,
        radius_px,
    }
}

pub fn top_row<I, T>(color: ColorRgba8, height_px: i32, children: I) -> ThemeNode
where
    I: IntoIterator<Item = T>,
    T: Into<ThemeNode>,
{
    ThemeNode::TopRow {
        color,
        height_px,
        children: children.into_iter().map(Into::into).collect(),
    }
}

pub fn title_bar(color: ColorRgba8, height_px: i32) -> ThemeNode {
    ThemeNode::TitleBar { color, height_px }
}

pub fn title_text(text: impl Into<String>, color: ColorRgba8) -> ThemeNode {
    ThemeNode::TitleText {
        text: text.into(),
        color,
    }
}

pub fn button_row(accent_color: ColorRgba8, button_count: u8) -> ThemeNode {
    ThemeNode::ButtonRow {
        accent_color,
        button_count,
    }
}

pub fn window_controls<I>(buttons: I) -> ThemeNode
where
    I: IntoIterator<Item = WindowControlButton>,
{
    ThemeNode::WindowControls {
        buttons: buttons.into_iter().collect(),
        button_size_px: 12,
        spacing_px: 8,
        margin_px: 10,
    }
}

pub fn window_controls_with<I>(
    buttons: I,
    button_size_px: i32,
    spacing_px: i32,
    margin_px: i32,
) -> ThemeNode
where
    I: IntoIterator<Item = WindowControlButton>,
{
    ThemeNode::WindowControls {
        buttons: buttons.into_iter().collect(),
        button_size_px,
        spacing_px,
        margin_px,
    }
}

pub fn window_button(kind: WindowControlKind, color: ColorRgba8) -> WindowControlButton {
    WindowControlButton {
        kind,
        color,
        on_hover: None,
    }
}

pub fn button_hover(background_color: ColorRgba8) -> WindowControlHoverEffect {
    WindowControlHoverEffect { background_color }
}

pub fn window_button_with_hover(
    kind: WindowControlKind,
    color: ColorRgba8,
    hover_background_color: ColorRgba8,
) -> WindowControlButton {
    WindowControlButton {
        kind,
        color,
        on_hover: Some(button_hover(hover_background_color)),
    }
}

pub fn expand_shrink_button(color: ColorRgba8) -> WindowControlButton {
    window_button(WindowControlKind::ToggleExpand, color)
}

pub fn expand_shrink_button_with_hover(
    color: ColorRgba8,
    hover_background_color: ColorRgba8,
) -> WindowControlButton {
    window_button_with_hover(
        WindowControlKind::ToggleExpand,
        color,
        hover_background_color,
    )
}

pub fn close_button(color: ColorRgba8) -> WindowControlButton {
    window_button(WindowControlKind::Close, color)
}

pub fn close_button_with_hover(
    color: ColorRgba8,
    hover_background_color: ColorRgba8,
) -> WindowControlButton {
    window_button_with_hover(WindowControlKind::Close, color, hover_background_color)
}

pub fn backdrop_blur(radius_px: i32, passes: u8) -> ThemeNode {
    ThemeNode::BackdropBlur { radius_px, passes }
}

pub fn glass_material(tint_color: ColorRgba8, opacity: u8) -> ThemeNode {
    ThemeNode::GlassMaterial {
        tint_color,
        opacity,
    }
}

pub fn shadow(color: ColorRgba8, radius_px: i32, offset: PointI, strength: u8) -> ThemeNode {
    ThemeNode::Shadow {
        color,
        radius_px,
        offset,
        strength,
    }
}

pub fn transform(offset: PointI, child: impl Into<ThemeNode>) -> ThemeNode {
    ThemeNode::Transform {
        offset,
        child: Box::new(child.into()),
    }
}

pub fn opacity(alpha: u8, child: impl Into<ThemeNode>) -> ThemeNode {
    ThemeNode::Opacity {
        alpha,
        child: Box::new(child.into()),
    }
}

pub fn clip(radius_px: i32, child: impl Into<ThemeNode>) -> ThemeNode {
    ThemeNode::Clip {
        radius_px,
        child: Box::new(child.into()),
    }
}

pub fn output_theme(
    output_id: ThemeOutputId,
    background_color: ColorRgba8,
    overlay_nodes: impl IntoIterator<Item = ThemeNode>,
) -> OutputTheme {
    OutputTheme {
        output_id,
        background_color,
        overlay_nodes: overlay_nodes.into_iter().collect(),
        cursor: None,
    }
}

pub fn output_theme_with_cursor(
    output_id: ThemeOutputId,
    background_color: ColorRgba8,
    overlay_nodes: impl IntoIterator<Item = ThemeNode>,
    cursor: CursorTheme,
) -> OutputTheme {
    OutputTheme {
        output_id,
        background_color,
        overlay_nodes: overlay_nodes.into_iter().collect(),
        cursor: Some(cursor),
    }
}

pub fn window_theme(
    view_id: ThemeViewId,
    chrome_nodes: impl IntoIterator<Item = ThemeNode>,
) -> WindowTheme {
    WindowTheme {
        view_id,
        chrome_nodes: chrome_nodes.into_iter().collect(),
    }
}

pub fn frame(output: OutputTheme, windows: impl IntoIterator<Item = WindowTheme>) -> ThemeFrame {
    ThemeFrame {
        output,
        windows: windows.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::foundation::ColorRgba8;

    use super::{
        backdrop_blur, border, clip, close_button_with_hover, expand_shrink_button_with_hover,
        glass_material, point, rounded_rect, shadow, stack, surface_content, title_text, top_row,
        window_controls,
    };
    use crate::{ThemeNode, WindowControlKind};

    #[test]
    fn stack_supports_array_children() {
        let tree = stack([
            shadow(ColorRgba8::rgba(1, 2, 3, 4), 18, point(0, 12), 90),
            border(ColorRgba8::rgba(5, 6, 7, 8), 2, 16),
            clip(16, rounded_rect(ColorRgba8::rgba(9, 10, 11, 12), 16)),
            top_row(
                ColorRgba8::rgba(13, 14, 15, 16),
                32,
                [
                    title_text("Notes", ColorRgba8::rgba(17, 18, 19, 20)),
                    window_controls([
                        expand_shrink_button_with_hover(
                            ColorRgba8::rgba(21, 22, 23, 24),
                            ColorRgba8::rgba(24, 25, 26, 27),
                        ),
                        close_button_with_hover(
                            ColorRgba8::rgba(25, 26, 27, 28),
                            ColorRgba8::rgba(28, 29, 30, 31),
                        ),
                    ]),
                ],
            ),
            backdrop_blur(14, 2),
            glass_material(ColorRgba8::rgba(29, 30, 31, 32), 100),
            surface_content(ColorRgba8::rgba(33, 34, 35, 36)),
        ]);

        match tree {
            ThemeNode::Stack { children } => {
                assert_eq!(children.len(), 7);
                assert!(matches!(children[0], ThemeNode::Shadow { .. }));
                assert!(matches!(children[1], ThemeNode::Border { .. }));
                assert!(matches!(children[2], ThemeNode::Clip { .. }));
                let ThemeNode::TopRow { children, .. } = &children[3] else {
                    panic!("expected top row");
                };
                assert_eq!(children.len(), 2);
                let ThemeNode::WindowControls { buttons, .. } = &children[1] else {
                    panic!("expected window controls");
                };
                assert_eq!(buttons.len(), 2);
                assert_eq!(buttons[0].kind, WindowControlKind::ToggleExpand);
                assert_eq!(buttons[1].kind, WindowControlKind::Close);
                assert_eq!(
                    buttons[0]
                        .on_hover
                        .as_ref()
                        .expect("hover style")
                        .background_color,
                    ColorRgba8::rgba(24, 25, 26, 27)
                );
            }
            other => panic!("expected stack, got {other:?}"),
        }
    }
}
