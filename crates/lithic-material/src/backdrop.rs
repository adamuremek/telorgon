use crate::foundation::{ColorRgba8, RectI, SizeI};
use crate::render::{RenderMaterial, RenderMaterialPass};

pub fn execute_material_passes(
    frame_rgba8: &mut [u8],
    frame_extent: SizeI,
    material: &RenderMaterial,
) {
    let target_rect = clip_rect(material.rect, frame_extent);
    if target_rect.width <= 0 || target_rect.height <= 0 {
        return;
    }

    let mut working_rect = target_rect;
    let mut working_pixels = Vec::new();
    let mut should_composite = false;

    for pass in &material.passes {
        match *pass {
            RenderMaterialPass::BackdropCapture { source_rect } => {
                working_rect = clip_rect(source_rect, frame_extent);
                if working_rect.width <= 0 || working_rect.height <= 0 {
                    working_pixels.clear();
                    should_composite = false;
                    continue;
                }
                working_pixels = capture_region(frame_rgba8, frame_extent.width, working_rect);
                should_composite = true;
            }
            RenderMaterialPass::Blur { radius_px, passes } => {
                if !working_pixels.is_empty() {
                    blur_region(
                        &mut working_pixels,
                        working_rect.width,
                        working_rect.height,
                        radius_px,
                        passes,
                    );
                }
            }
            RenderMaterialPass::Tint { color, opacity } => {
                if working_pixels.is_empty() {
                    working_pixels = capture_region(frame_rgba8, frame_extent.width, working_rect);
                    should_composite = true;
                }
                tint_region(&mut working_pixels, color, opacity);
            }
            RenderMaterialPass::Shadow {
                color,
                radius_px,
                strength,
            } => draw_shadow(
                frame_rgba8,
                frame_extent.width,
                frame_extent.height,
                target_rect,
                color,
                radius_px,
                strength,
            ),
        }
    }

    if should_composite && !working_pixels.is_empty() {
        composite_region(
            frame_rgba8,
            frame_extent.width,
            frame_extent.height,
            working_rect,
            &working_pixels,
        );
    }
}

fn capture_region(frame_rgba8: &[u8], frame_width: i32, rect: RectI) -> Vec<u8> {
    let mut region = vec![0; (rect.width * rect.height * 4) as usize];

    for row in 0..rect.height {
        let src_offset = (((rect.y + row) * frame_width + rect.x) * 4) as usize;
        let dst_offset = (row * rect.width * 4) as usize;
        let len = (rect.width * 4) as usize;
        region[dst_offset..dst_offset + len]
            .copy_from_slice(&frame_rgba8[src_offset..src_offset + len]);
    }

    region
}

fn composite_region(
    frame_rgba8: &mut [u8],
    frame_width: i32,
    frame_height: i32,
    rect: RectI,
    region_rgba8: &[u8],
) {
    if rect.x < 0
        || rect.y < 0
        || rect.x + rect.width > frame_width
        || rect.y + rect.height > frame_height
    {
        return;
    }

    for row in 0..rect.height {
        for column in 0..rect.width {
            let dst_offset = (((rect.y + row) * frame_width + rect.x + column) * 4) as usize;
            let src_offset = ((row * rect.width + column) * 4) as usize;
            blend_source_over(
                &mut frame_rgba8[dst_offset..dst_offset + 4],
                &region_rgba8[src_offset..src_offset + 4],
            );
        }
    }
}

fn blur_region(region_rgba8: &mut [u8], width: i32, height: i32, radius_px: i32, passes: u8) {
    if radius_px <= 0 || passes == 0 || width <= 0 || height <= 0 {
        return;
    }

    let radius = radius_px.max(1).min(24);
    let mut source = region_rgba8.to_vec();
    let mut dest = vec![0; region_rgba8.len()];

    for _ in 0..passes {
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0u32; 4];
                let mut count = 0u32;

                for sample_y in (y - radius).max(0)..=(y + radius).min(height - 1) {
                    for sample_x in (x - radius).max(0)..=(x + radius).min(width - 1) {
                        let offset = ((sample_y * width + sample_x) * 4) as usize;
                        sum[0] += source[offset] as u32;
                        sum[1] += source[offset + 1] as u32;
                        sum[2] += source[offset + 2] as u32;
                        sum[3] += source[offset + 3] as u32;
                        count += 1;
                    }
                }

                let dst_offset = ((y * width + x) * 4) as usize;
                dest[dst_offset] = (sum[0] / count) as u8;
                dest[dst_offset + 1] = (sum[1] / count) as u8;
                dest[dst_offset + 2] = (sum[2] / count) as u8;
                dest[dst_offset + 3] = (sum[3] / count) as u8;
            }
        }

        source.copy_from_slice(&dest);
    }

    region_rgba8.copy_from_slice(&source);
}

fn tint_region(region_rgba8: &mut [u8], color: ColorRgba8, opacity: u8) {
    for pixel in region_rgba8.chunks_exact_mut(4) {
        let base = ColorRgba8::rgba(pixel[0], pixel[1], pixel[2], pixel[3]);
        let tinted = mix_color(base, color, opacity);
        pixel.copy_from_slice(&[tinted.r, tinted.g, tinted.b, tinted.a]);
    }
}

fn draw_shadow(
    frame_rgba8: &mut [u8],
    frame_width: i32,
    frame_height: i32,
    rect: RectI,
    color: ColorRgba8,
    radius_px: i32,
    strength: u8,
) {
    if radius_px <= 0 || strength == 0 || color.a == 0 {
        return;
    }

    let spread = radius_px.max(1);
    let expanded = RectI {
        x: (rect.x - spread).max(0),
        y: (rect.y - spread).max(0),
        width: (rect.width + spread * 2).min(frame_width),
        height: (rect.height + spread * 2).min(frame_height),
    };

    for y in expanded.y..(expanded.y + expanded.height).min(frame_height) {
        for x in expanded.x..(expanded.x + expanded.width).min(frame_width) {
            let dx = if x < rect.x {
                rect.x - x
            } else if x >= rect.x + rect.width {
                x - (rect.x + rect.width - 1)
            } else {
                0
            };
            let dy = if y < rect.y {
                rect.y - y
            } else if y >= rect.y + rect.height {
                y - (rect.y + rect.height - 1)
            } else {
                0
            };
            let distance = dx.max(dy);
            if distance > spread {
                continue;
            }

            let falloff = 1.0 - distance as f32 / spread as f32;
            let alpha = (color.a as f32 * (strength as f32 / 255.0) * falloff)
                .round()
                .clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }

            let offset = ((y * frame_width + x) * 4) as usize;
            let shadow = [color.r, color.g, color.b, alpha];
            blend_source_over(&mut frame_rgba8[offset..offset + 4], &shadow);
        }
    }
}

fn mix_color(base: ColorRgba8, tint: ColorRgba8, opacity: u8) -> ColorRgba8 {
    let t = opacity as f32 / 255.0;
    ColorRgba8::rgba(
        ((base.r as f32 * (1.0 - t)) + (tint.r as f32 * t)).round() as u8,
        ((base.g as f32 * (1.0 - t)) + (tint.g as f32 * t)).round() as u8,
        ((base.b as f32 * (1.0 - t)) + (tint.b as f32 * t)).round() as u8,
        base.a.max(tint.a),
    )
}

fn blend_source_over(dst: &mut [u8], src: &[u8]) {
    let src_alpha = src[3] as u32;
    if src_alpha == 0 {
        return;
    }
    if src_alpha == 255 {
        dst.copy_from_slice(src);
        return;
    }

    let inverse_alpha = 255 - src_alpha;
    dst[0] = (src[0] as u32 + (dst[0] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[1] = (src[1] as u32 + (dst[1] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[2] = (src[2] as u32 + (dst[2] as u32 * inverse_alpha) / 255).min(255) as u8;
    dst[3] = (src_alpha + (dst[3] as u32 * inverse_alpha) / 255).min(255) as u8;
}

fn clip_rect(rect: RectI, extent: SizeI) -> RectI {
    let x0 = rect.x.max(0);
    let y0 = rect.y.max(0);
    let x1 = (rect.x + rect.width).min(extent.width);
    let y1 = (rect.y + rect.height).min(extent.height);

    RectI {
        x: x0,
        y: y0,
        width: (x1 - x0).max(0),
        height: (y1 - y0).max(0),
    }
}
