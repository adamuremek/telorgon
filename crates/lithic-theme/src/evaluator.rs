use crate::dsl::{
    backdrop_blur, border, close_button_with_hover, expand_shrink_button_with_hover, frame,
    glass_material, output_theme_with_cursor, shadow, stack, surface_content, title_text,
    top_row, window_controls_with, window_theme,
};
use crate::foundation::ColorRgba8;
use crate::{
    CursorTheme, ThemeFrame, ThemeImage, ThemeInput, ThemeNode, ThemeRecipe, ThemeWindowStyle,
    WindowModel,
};

pub(crate) fn evaluate_recipe_theme(recipe: &ThemeRecipe, input: &ThemeInput) -> ThemeFrame {
    let windows = input
        .windows
        .iter()
        .map(|window| {
            let style = if window.focused {
                &recipe.focused
            } else {
                &recipe.unfocused
            };
            window_theme(window.id, window_nodes(recipe, style, window))
        })
        .collect::<Vec<_>>();

    frame(
        output_theme_with_cursor(
            input.output.id,
            recipe.output_background,
            [],
            CursorTheme {
                hotspot: recipe.cursor.hotspot,
                image: ThemeImage {
                    size: recipe.cursor.image.size,
                    pixels_rgba8: recipe.cursor.image.pixels_rgba8.clone(),
                },
            },
        ),
        windows,
    )
}

fn window_nodes(
    recipe: &ThemeRecipe,
    style: &ThemeWindowStyle,
    window: &WindowModel,
) -> Vec<ThemeNode> {
    let mut titlebar_children = Vec::new();
    if style.show_title_text {
        titlebar_children.push(title_text(window_title(window), style.title_text_color));
    }
    if style.show_window_controls {
        titlebar_children.push(window_controls_with(
            [
                expand_shrink_button_with_hover(style.expand_color, style.expand_hover_color),
                close_button_with_hover(style.close_color, style.close_hover_color),
            ],
            14,
            10,
            0,
        ));
    }

    let mut children = vec![
        shadow(
            style.shadow_color,
            style.shadow_radius_px,
            style.shadow_offset,
            style.shadow_strength,
        ),
        border(style.border_color, style.border_px, style.radius_px),
        top_row(style.titlebar_color, style.titlebar_px, titlebar_children),
    ];

    if style.use_glass {
        children.push(backdrop_blur(
            style.backdrop_blur_radius_px,
            style.backdrop_blur_passes,
        ));
        children.push(glass_material(style.glass_tint_color, style.glass_opacity));
    } else {
        children.push(surface_content(content_color(recipe, window)));
    }

    vec![stack(children)]
}

fn content_color(recipe: &ThemeRecipe, window: &WindowModel) -> ColorRgba8 {
    let mut hash = window.id.get();
    for byte in window
        .title
        .bytes()
        .chain(window.app_id.bytes())
        .chain(window.content_extent.width.to_le_bytes())
        .chain(window.content_extent.height.to_le_bytes())
    {
        hash = hash.wrapping_mul(16777619).wrapping_add(byte as u64);
    }
    recipe.content_palette[(hash as usize) % recipe.content_palette.len()]
}

fn window_title(window: &WindowModel) -> String {
    if window.title.is_empty() {
        window.app_id.clone()
    } else {
        window.title.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::foundation::{ColorRgba8, PointI, RectI, SizeI};
    use crate::{
        OutputModel, ThemeCursorAsset, ThemeImageAsset, ThemeInput, ThemeOutputId, ThemeRecipe,
        ThemeViewId, ThemeWindowStyle, WindowModel,
    };

    use super::evaluate_recipe_theme;

    #[test]
    fn recipe_evaluator_does_not_inject_title_or_controls_when_disabled() {
        let recipe = recipe(false, false);
        let frame = evaluate_recipe_theme(&recipe, &input());
        let window = frame.window_theme(ThemeViewId::new(1)).unwrap();
        let flattened = flatten(&window.chrome_nodes);

        assert!(!flattened.iter().any(|node| matches!(node, crate::ThemeNode::TitleText { .. })));
        assert!(
            !flattened
                .iter()
                .any(|node| matches!(node, crate::ThemeNode::WindowControls { .. }))
        );
    }

    #[test]
    fn recipe_evaluator_emits_declared_title_and_controls() {
        let recipe = recipe(true, true);
        let frame = evaluate_recipe_theme(&recipe, &input());
        let window = frame.window_theme(ThemeViewId::new(1)).unwrap();
        let flattened = flatten(&window.chrome_nodes);

        assert!(flattened.iter().any(|node| matches!(node, crate::ThemeNode::TitleText { .. })));
        assert!(
            flattened
                .iter()
                .any(|node| matches!(node, crate::ThemeNode::WindowControls { .. }))
        );
    }

    fn flatten(nodes: &[crate::ThemeNode]) -> Vec<&crate::ThemeNode> {
        let mut out = Vec::new();
        for node in nodes {
            out.push(node);
            match node {
                crate::ThemeNode::Stack { children } | crate::ThemeNode::TopRow { children, .. } => {
                    out.extend(flatten(children));
                }
                crate::ThemeNode::Transform { child, .. }
                | crate::ThemeNode::Opacity { child, .. }
                | crate::ThemeNode::Clip { child, .. } => out.extend(flatten(std::slice::from_ref(child))),
                _ => {}
            }
        }
        out
    }

    fn input() -> ThemeInput {
        ThemeInput {
            output: OutputModel {
                id: ThemeOutputId::new(1),
                name: "test".to_string(),
                logical_size: SizeI {
                    width: 800,
                    height: 600,
                },
                scale: 1,
                keyboard_focused_window: Some(ThemeViewId::new(1)),
                pointer_focused_window: Some(ThemeViewId::new(1)),
            },
            windows: vec![WindowModel {
                id: ThemeViewId::new(1),
                title: "Declared".to_string(),
                app_id: "test".to_string(),
                mapped: true,
                focused: true,
                geometry: Some(RectI {
                    x: 0,
                    y: 0,
                    width: 320,
                    height: 200,
                }),
                content_extent: SizeI {
                    width: 320,
                    height: 200,
                },
            }],
        }
    }

    fn recipe(show_title_text: bool, show_window_controls: bool) -> ThemeRecipe {
        ThemeRecipe {
            output_background: ColorRgba8::rgba(1, 2, 3, 255),
            focused: style(show_title_text, show_window_controls),
            unfocused: style(show_title_text, show_window_controls),
            content_palette: vec![ColorRgba8::rgba(10, 11, 12, 255)],
            cursor: ThemeCursorAsset {
                hotspot: PointI { x: 0, y: 0 },
                image: ThemeImageAsset {
                    size: SizeI {
                        width: 1,
                        height: 1,
                    },
                    pixels_rgba8: Arc::from([255, 255, 255, 255]),
                },
            },
        }
    }

    fn style(show_title_text: bool, show_window_controls: bool) -> ThemeWindowStyle {
        ThemeWindowStyle {
            border_px: 1,
            titlebar_px: 24,
            radius_px: 4,
            titlebar_color: ColorRgba8::rgba(20, 21, 22, 255),
            border_color: ColorRgba8::rgba(30, 31, 32, 255),
            show_title_text,
            title_text_color: ColorRgba8::rgba(40, 41, 42, 255),
            shadow_color: ColorRgba8::rgba(0, 0, 0, 80),
            shadow_radius_px: 8,
            shadow_offset: PointI { x: 0, y: 2 },
            shadow_strength: 64,
            glass_tint_color: ColorRgba8::rgba(50, 51, 52, 255),
            glass_opacity: 0,
            backdrop_blur_radius_px: 0,
            backdrop_blur_passes: 0,
            use_glass: false,
            show_window_controls,
            expand_color: ColorRgba8::rgba(60, 61, 62, 255),
            expand_hover_color: ColorRgba8::rgba(70, 71, 72, 255),
            close_color: ColorRgba8::rgba(80, 81, 82, 255),
            close_hover_color: ColorRgba8::rgba(90, 91, 92, 255),
        }
    }
}
