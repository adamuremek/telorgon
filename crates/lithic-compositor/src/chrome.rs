use lithic_core::{ColorRgba8, PointI, RectI};
use lithic_render::CornerRadii;
use lithic_theme::{ThemeNode, WindowControlButton, WindowControlKind};
use lithic_ui::{Action, ButtonRow, ControlGroup, Icon, IconButton, Text, Widget};

pub const WINDOW_ACTION_CLOSE: &str = "window.close";
pub const WINDOW_ACTION_TOGGLE_EXPAND: &str = "window.toggle_expand";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowChrome {
    pub border_width_px: i32,
    pub border_color: ColorRgba8,
    pub corner_radii_px: CornerRadii,
    pub titlebar_height_px: i32,
    pub titlebar_color: ColorRgba8,
    pub content_background: ColorRgba8,
    pub material: ChromeMaterial,
    pub shadow: Option<WindowShadow>,
    pub titlebar_widgets: Vec<Widget>,
}

impl Default for WindowChrome {
    fn default() -> Self {
        Self {
            border_width_px: 2,
            border_color: ColorRgba8::rgba(0xd5, 0xdf, 0xec, 0xaa),
            corner_radii_px: CornerRadii::all(12),
            titlebar_height_px: 32,
            titlebar_color: ColorRgba8::rgba(0x1c, 0x25, 0x30, 0xe8),
            content_background: ColorRgba8::rgba(0x18, 0x1d, 0x24, 0xff),
            material: ChromeMaterial::Solid,
            shadow: Some(WindowShadow::default()),
            titlebar_widgets: Vec::new(),
        }
    }
}

impl WindowChrome {
    pub fn with_titlebar_widgets(mut self, widgets: impl IntoIterator<Item = Widget>) -> Self {
        self.titlebar_widgets = widgets.into_iter().collect();
        self
    }

    pub fn with_titlebar_elements(self, widgets: impl IntoIterator<Item = Widget>) -> Self {
        self.with_titlebar_widgets(widgets)
    }

    pub fn with_controls(mut self, controls_tree: impl IntoIterator<Item = ThemeNode>) -> Self {
        self.titlebar_widgets.clear();
        for node in controls_tree {
            self.apply_titlebar_node(&node);
        }
        self
    }

    pub fn frame_rect(&self, content_rect: RectI) -> RectI {
        let border = self.border_width_px.max(0);
        let titlebar = self.titlebar_height_px.max(0);
        RectI {
            x: content_rect.x.saturating_sub(border),
            y: content_rect
                .y
                .saturating_sub(border)
                .saturating_sub(titlebar),
            width: content_rect.width.saturating_add(border * 2),
            height: content_rect
                .height
                .saturating_add(titlebar)
                .saturating_add(border * 2),
        }
    }

    pub fn titlebar_rect(&self, content_rect: RectI) -> RectI {
        let titlebar = self.titlebar_height_px.max(0);
        RectI {
            x: content_rect.x,
            y: content_rect.y.saturating_sub(titlebar),
            width: content_rect.width,
            height: titlebar,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChromeMaterial {
    Solid,
    BackdropBlur {
        radius_px: i32,
        passes: u8,
    },
    Glass {
        tint_color: ColorRgba8,
        opacity: u8,
        blur_radius_px: i32,
        passes: u8,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WindowShadow {
    pub color: ColorRgba8,
    pub radius_px: i32,
    pub offset: PointI,
    pub strength: u8,
}

impl Default for WindowShadow {
    fn default() -> Self {
        Self {
            color: ColorRgba8::rgba(0x00, 0x00, 0x00, 0x70),
            radius_px: 24,
            offset: PointI { x: 0, y: 10 },
            strength: 96,
        }
    }
}

impl WindowChrome {
    pub fn from_theme_nodes<'a>(nodes: impl IntoIterator<Item = &'a ThemeNode>) -> Self {
        let mut chrome = Self::default();
        for node in nodes {
            chrome.apply_theme_node(node);
        }
        chrome
    }

    pub fn apply_theme_node(&mut self, node: &ThemeNode) {
        match node {
            ThemeNode::Stack { children } => {
                for child in children {
                    self.apply_theme_node(child);
                }
            }
            ThemeNode::SurfaceContent { fill_color } => {
                self.content_background = *fill_color;
            }
            ThemeNode::RoundedRect {
                fill_color,
                radius_px,
            } => {
                self.content_background = *fill_color;
                self.corner_radii_px = CornerRadii::all(*radius_px);
            }
            ThemeNode::Border {
                color,
                thickness_px,
                radius_px,
            } => {
                self.border_color = *color;
                self.border_width_px = *thickness_px;
                self.corner_radii_px = CornerRadii::all(*radius_px);
            }
            ThemeNode::TopRow {
                color,
                height_px,
                children,
            } => {
                self.titlebar_color = *color;
                self.titlebar_height_px = *height_px;
                for child in children {
                    self.apply_theme_node(child);
                }
            }
            ThemeNode::TitleBar { color, height_px } => {
                self.titlebar_color = *color;
                self.titlebar_height_px = *height_px;
            }
            ThemeNode::TitleText { .. }
            | ThemeNode::ButtonRow { .. }
            | ThemeNode::WindowControls { .. } => {
                self.apply_titlebar_node(node);
            }
            ThemeNode::BackdropBlur { radius_px, passes } => {
                self.material = ChromeMaterial::BackdropBlur {
                    radius_px: *radius_px,
                    passes: *passes,
                };
            }
            ThemeNode::GlassMaterial {
                tint_color,
                opacity,
            } => {
                let (blur_radius_px, passes) = match self.material {
                    ChromeMaterial::BackdropBlur { radius_px, passes } => (radius_px, passes),
                    _ => (0, 0),
                };
                self.material = ChromeMaterial::Glass {
                    tint_color: *tint_color,
                    opacity: *opacity,
                    blur_radius_px,
                    passes,
                };
            }
            ThemeNode::Shadow {
                color,
                radius_px,
                offset,
                strength,
            } => {
                self.shadow = Some(WindowShadow {
                    color: *color,
                    radius_px: *radius_px,
                    offset: *offset,
                    strength: *strength,
                });
            }
            ThemeNode::Transform { offset, child } => {
                self.apply_theme_node(child);
                if let Some(shadow) = &mut self.shadow {
                    shadow.offset = PointI {
                        x: shadow.offset.x.saturating_add(offset.x),
                        y: shadow.offset.y.saturating_add(offset.y),
                    };
                }
            }
            ThemeNode::Opacity { child, .. } => {
                self.apply_theme_node(child);
            }
            ThemeNode::Clip { radius_px, child } => {
                self.corner_radii_px = CornerRadii::all(*radius_px);
                self.apply_theme_node(child);
            }
        }
    }

    fn apply_titlebar_node(&mut self, node: &ThemeNode) {
        match node {
            ThemeNode::Stack { children } | ThemeNode::TopRow { children, .. } => {
                for child in children {
                    self.apply_titlebar_node(child);
                }
            }
            ThemeNode::TitleText { text, color } => {
                self.titlebar_widgets.push(Widget::Text(Text {
                    text: text.clone(),
                    color: *color,
                }));
            }
            ThemeNode::ButtonRow {
                accent_color,
                button_count,
            } => {
                self.titlebar_widgets.push(Widget::ButtonRow(ButtonRow {
                    accent_color: *accent_color,
                    button_count: *button_count,
                }));
            }
            ThemeNode::WindowControls {
                buttons,
                button_size_px,
                spacing_px,
                margin_px,
            } => {
                self.titlebar_widgets
                    .push(Widget::ControlGroup(ControlGroup {
                        children: buttons.iter().map(window_control_widget).collect(),
                        button_size_px: *button_size_px,
                        spacing_px: *spacing_px,
                        margin_px: *margin_px,
                    }));
            }
            ThemeNode::Transform { child, .. }
            | ThemeNode::Opacity { child, .. }
            | ThemeNode::Clip { child, .. } => {
                self.apply_titlebar_node(child);
            }
            _ => {}
        }
    }
}

fn window_control_widget(button: &WindowControlButton) -> Widget {
    Widget::IconButton(IconButton {
        icon: match button.kind {
            WindowControlKind::ToggleExpand => Icon::ToggleExpand,
            WindowControlKind::Close => Icon::Close,
        },
        color: button.color,
        hover_background_color: button.on_hover.as_ref().map(|hover| hover.background_color),
        action: Some(Action::new(match button.kind {
            WindowControlKind::ToggleExpand => WINDOW_ACTION_TOGGLE_EXPAND,
            WindowControlKind::Close => WINDOW_ACTION_CLOSE,
        })),
    })
}

#[cfg(test)]
mod tests {
    use lithic_core::ColorRgba8;
    use lithic_theme::dsl::{
        backdrop_blur, border, close_button, glass_material, point, shadow, stack, title_text,
        top_row, window_controls,
    };
    use lithic_ui::Widget;

    use super::{ChromeMaterial, WINDOW_ACTION_CLOSE, WindowChrome};

    #[test]
    fn chrome_can_be_built_from_declarative_theme_nodes() {
        let nodes = [stack([
            shadow(ColorRgba8::rgba(1, 2, 3, 4), 18, point(0, 12), 90),
            border(ColorRgba8::rgba(5, 6, 7, 8), 2, 16),
            top_row(
                ColorRgba8::rgba(9, 10, 11, 12),
                30,
                [
                    title_text("Demo", ColorRgba8::rgba(13, 14, 15, 16)),
                    window_controls([close_button(ColorRgba8::rgba(17, 18, 19, 20))]),
                ],
            ),
            backdrop_blur(12, 2),
            glass_material(ColorRgba8::rgba(21, 22, 23, 24), 100),
        ])];

        let chrome = WindowChrome::from_theme_nodes(nodes.iter());

        assert_eq!(chrome.border_width_px, 2);
        assert_eq!(chrome.corner_radii_px.top_left, 16);
        assert_eq!(chrome.titlebar_height_px, 30);
        assert_eq!(chrome.shadow.unwrap().radius_px, 18);
        assert_eq!(chrome.titlebar_widgets.len(), 2);
        assert!(matches!(&chrome.titlebar_widgets[0], Widget::Text(_)));
        let Widget::ControlGroup(controls) = &chrome.titlebar_widgets[1] else {
            panic!("expected control group");
        };
        assert_eq!(
            controls.children.iter().find_map(|child| match child {
                Widget::IconButton(button) =>
                    button.action.as_ref().map(|action| action.name.as_str()),
                _ => None,
            }),
            Some(WINDOW_ACTION_CLOSE)
        );
        assert_eq!(
            chrome.material,
            ChromeMaterial::Glass {
                tint_color: ColorRgba8::rgba(21, 22, 23, 24),
                opacity: 100,
                blur_radius_px: 12,
                passes: 2,
            }
        );
    }
}
