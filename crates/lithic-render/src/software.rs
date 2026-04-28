use std::collections::BTreeMap;

use crate::core::{ColorRgba8, RectI, SizeI};
use crate::{
    CornerRadii, RenderBlit, RenderFrame, RenderGraph, RenderMaterial, RenderMaterialKind,
    RenderOp, RenderRect, RenderResult, RenderTargetId, RenderText, RenderedFrame, Renderer,
};

#[derive(Clone, Debug, Default)]
pub struct SoftwareRenderer {
    targets: BTreeMap<RenderTargetId, SizeI>,
}

impl SoftwareRenderer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Renderer for SoftwareRenderer {
    fn register_target(&mut self, target_id: RenderTargetId, extent: SizeI) -> RenderResult<()> {
        self.targets.insert(target_id, extent);
        Ok(())
    }

    fn registered_extent(&self, target_id: RenderTargetId) -> Option<SizeI> {
        self.targets.get(&target_id).copied()
    }

    fn render(&mut self, frame: &RenderFrame, _graph: &RenderGraph) -> RenderResult<RenderedFrame> {
        Ok(render_frame_software(frame))
    }
}

pub fn render_frame_software(frame: &RenderFrame) -> RenderedFrame {
    let width = frame.extent.width.max(1) as usize;
    let height = frame.extent.height.max(1) as usize;
    let mut pixels = vec![0; width * height * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[
            frame.background.r,
            frame.background.g,
            frame.background.b,
            frame.background.a,
        ]);
    }
    for op in &frame.ops {
        match op {
            RenderOp::Rect(rect) => render_rect(&mut pixels, width, height, rect),
            RenderOp::Blit(blit) => render_blit(&mut pixels, width, height, blit),
            RenderOp::Text(text) => render_text(&mut pixels, width, height, text),
            RenderOp::Material(material) => render_material(&mut pixels, width, height, material),
        }
    }
    RenderedFrame {
        output_id: frame.output_id,
        extent: frame.extent,
        pixels_rgba8: pixels,
    }
}

fn render_rect(pixels: &mut [u8], width: usize, height: usize, rect: &RenderRect) {
    fill_rect(pixels, width, height, rect.rect, rect.color, rect.corner_radii_px);
}

fn render_blit(pixels: &mut [u8], width: usize, height: usize, blit: &RenderBlit) {
    let dst = RectI {
        x: blit.dst_x,
        y: blit.dst_y,
        width: blit.width,
        height: blit.height,
    };
    let Some(clipped) = clip_rect(dst, width as i32, height as i32) else {
        return;
    };
    let src_width = blit.src_width.max(1);
    let src_height = blit.height.max(1);
    for y in clipped.y..clipped.y + clipped.height {
        for x in clipped.x..clipped.x + clipped.width {
            let local_x = x - dst.x;
            let local_y = y - dst.y;
            if !inside_rounded_rect(local_x, local_y, dst.width, dst.height, blit.corner_radii_px) {
                continue;
            }
            let src_x = blit.src_x + local_x * src_width / blit.width.max(1);
            let src_y = blit.src_y + local_y * src_height / blit.height.max(1);
            let src_index = ((src_y * blit.src_width + src_x) * 4) as usize;
            if src_index + 3 >= blit.pixels_rgba8.len() {
                continue;
            }
            blend_pixel(
                pixels,
                width,
                x,
                y,
                ColorRgba8::rgba(
                    blit.pixels_rgba8[src_index],
                    blit.pixels_rgba8[src_index + 1],
                    blit.pixels_rgba8[src_index + 2],
                    blit.pixels_rgba8[src_index + 3],
                ),
            );
        }
    }
}

fn render_text(pixels: &mut [u8], width: usize, height: usize, text: &RenderText) {
    let scale = (text.font_size_px / 7).max(1);
    let glyph_width = 5 * scale;
    let glyph_gap = scale;
    let mut x = text.rect.x;
    let y = text.rect.y + (text.rect.height - 7 * scale) / 2;
    let max_x = text.rect.x + text.rect.width;
    for ch in text.text.chars() {
        if x + glyph_width > max_x {
            break;
        }
        if ch != ' ' {
            draw_glyph(pixels, width, height, x, y, scale, text.color, ch);
        }
        x += glyph_width + glyph_gap;
    }
}

fn draw_glyph(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    scale: i32,
    color: ColorRgba8,
    ch: char,
) {
    let pattern = glyph_pattern(ch);
    for (row_index, row) in pattern.iter().enumerate() {
        for column in 0..5i32 {
            if *row & (1u8 << (4 - column)) == 0 {
                continue;
            }
            fill_rect(
                pixels,
                width,
                height,
                RectI {
                    x: x + column * scale,
                    y: y + row_index as i32 * scale,
                    width: scale,
                    height: scale,
                },
                color,
                CornerRadii::zero(),
            );
        }
    }
}

fn glyph_pattern(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [15, 16, 16, 16, 16, 16, 15],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [15, 16, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '-' => [0, 0, 0, 30, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 0, 12, 12],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        _ => [31, 17, 2, 4, 4, 0, 4],
    }
}

fn render_material(pixels: &mut [u8], width: usize, height: usize, material: &RenderMaterial) {
    match material.kind {
        RenderMaterialKind::Shadow {
            color,
            radius_px,
            strength,
        } => render_shadow(
            pixels,
            width,
            height,
            material.rect,
            material.corner_radii_px,
            color,
            radius_px,
            strength,
        ),
        RenderMaterialKind::Glass {
            tint_color,
            opacity,
            ..
        } => fill_rect(
            pixels,
            width,
            height,
            material.rect,
            tint_color.with_alpha_scale(opacity as f32 / 255.0),
            material.corner_radii_px,
        ),
        RenderMaterialKind::Tint { color, opacity } => fill_rect(
            pixels,
            width,
            height,
            material.rect,
            color.with_alpha_scale(opacity as f32 / 255.0),
            material.corner_radii_px,
        ),
        RenderMaterialKind::BackdropBlur { .. } => {}
    }
}

fn render_shadow(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    rect: RectI,
    radii: CornerRadii,
    color: ColorRgba8,
    radius_px: i32,
    strength: u8,
) {
    let Some(clipped) = clip_rect(rect, width as i32, height as i32) else {
        return;
    };
    let radius = radius_px.max(1) as f32;
    for y in clipped.y..clipped.y + clipped.height {
        for x in clipped.x..clipped.x + clipped.width {
            let local_x = x - rect.x;
            let local_y = y - rect.y;
            if !inside_rounded_rect(local_x, local_y, rect.width, rect.height, radii) {
                continue;
            }
            let edge = local_x
                .min(local_y)
                .min(rect.width - local_x - 1)
                .min(rect.height - local_y - 1)
                .max(0) as f32;
            let alpha_scale = (edge / radius).clamp(0.0, 1.0) * (strength as f32 / 255.0);
            blend_pixel(pixels, width, x, y, color.with_alpha_scale(alpha_scale));
        }
    }
}

fn fill_rect(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    rect: RectI,
    color: ColorRgba8,
    radii: CornerRadii,
) {
    let Some(clipped) = clip_rect(rect, width as i32, height as i32) else {
        return;
    };
    for y in clipped.y..clipped.y + clipped.height {
        for x in clipped.x..clipped.x + clipped.width {
            let local_x = x - rect.x;
            let local_y = y - rect.y;
            if inside_rounded_rect(local_x, local_y, rect.width, rect.height, radii) {
                blend_pixel(pixels, width, x, y, color);
            }
        }
    }
}

fn clip_rect(rect: RectI, width: i32, height: i32) -> Option<RectI> {
    let x0 = rect.x.max(0).min(width);
    let y0 = rect.y.max(0).min(height);
    let x1 = rect.right().max(0).min(width);
    let y1 = rect.bottom().max(0).min(height);
    (x1 > x0 && y1 > y0).then_some(RectI {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

fn inside_rounded_rect(x: i32, y: i32, width: i32, height: i32, radii: CornerRadii) -> bool {
    let radius = radii
        .top_left
        .max(radii.top_right)
        .max(radii.bottom_right)
        .max(radii.bottom_left);
    if radius <= 0 {
        return true;
    }
    let r = radius.min(width / 2).min(height / 2);
    let corner = if x < r && y < r {
        Some((r, r, radii.top_left))
    } else if x >= width - r && y < r {
        Some((width - r - 1, r, radii.top_right))
    } else if x >= width - r && y >= height - r {
        Some((width - r - 1, height - r - 1, radii.bottom_right))
    } else if x < r && y >= height - r {
        Some((r, height - r - 1, radii.bottom_left))
    } else {
        None
    };
    if let Some((cx, cy, cr)) = corner {
        if cr <= 0 {
            return false;
        }
        let dx = x - cx;
        let dy = y - cy;
        dx * dx + dy * dy <= cr * cr
    } else {
        true
    }
}

fn blend_pixel(pixels: &mut [u8], width: usize, x: i32, y: i32, src: ColorRgba8) {
    if src.a == 0 || x < 0 || y < 0 {
        return;
    }
    let index = (y as usize * width + x as usize) * 4;
    if index + 3 >= pixels.len() {
        return;
    }
    let src_a = src.a as u32;
    let dst_a = pixels[index + 3] as u32;
    let out_a = src_a + (dst_a * (255 - src_a) + 127) / 255;
    if out_a == 0 {
        pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let src_c = match channel {
            0 => src.r,
            1 => src.g,
            _ => src.b,
        } as u32;
        let dst_c = pixels[index + channel] as u32;
        let premul = src_c * src_a + (dst_c * dst_a * (255 - src_a) + 127) / 255;
        pixels[index + channel] = ((premul + out_a / 2) / out_a).min(255) as u8;
    }
    pixels[index + 3] = out_a.min(255) as u8;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::{ColorRgba8, RectI, SizeI};
    use crate::{RenderFrame, RenderOp, RenderRect, RenderTargetId};

    use super::render_frame_software;

    #[test]
    fn software_renderer_draws_rect_over_background() {
        let frame = RenderFrame {
            output_id: RenderTargetId::new(7),
            extent: SizeI {
                width: 4,
                height: 4,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            damage_rects: Arc::from([]),
            ops: vec![RenderOp::Rect(RenderRect {
                rect: RectI {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 2,
                },
                color: ColorRgba8::rgba(255, 0, 0, 255),
                corner_radii_px: crate::CornerRadii::zero(),
            })],
        };

        let rendered = render_frame_software(&frame);
        assert_eq!(rendered.output_id, RenderTargetId::new(7));
        assert_eq!(&rendered.pixels_rgba8[(5 * 4)..(6 * 4)], &[255, 0, 0, 255]);
        assert_eq!(&rendered.pixels_rgba8[0..4], &[0, 0, 0, 255]);
    }
}
