//! CPU framebuffer tests; no window, device, event loop, or presenting surface is opened.

use std::collections::BTreeMap;

#[cfg(not(target_os = "linux"))]
use super::desktop_wayland_scene_tests::*;
#[cfg(target_os = "linux")]
use super::scene::*;
use crate::core::{ColorRgba8, PointI, RectI, SizeF, SizeI};
use crate::render::{ImageAlphaMode, ImagePixelFormat, RenderBackend, RenderScene};
use crate::renderer_software::{
    SoftwareCompositeLayer, SoftwareRenderer, SoftwareScene, SoftwareSurface,
};

const EXTENT: SizeI = SizeI {
    width: 32,
    height: 24,
};
const CONTENT: RectI = RectI {
    x: 6,
    y: 8,
    width: 20,
    height: 12,
};

struct Raster {
    composition: DesktopComposition,
    scenes: BTreeMap<DesktopSceneKey, SoftwareScene>,
    surface: SoftwareSurface,
}

impl Raster {
    fn new() -> Self {
        Self {
            composition: DesktopComposition::new(EXTENT),
            scenes: BTreeMap::new(),
            surface: SoftwareSurface::default(),
        }
    }

    fn draw(&mut self, layers: Vec<DesktopLayer>) -> DesktopFrame {
        let frame = self.composition.synchronize(EXTENT, layers).unwrap();
        for update in &frame.updates {
            let scene = self
                .scenes
                .entry(update.key)
                .or_insert_with(|| SoftwareRenderer.create_scene().unwrap());
            for delta in &update.deltas {
                SoftwareRenderer.apply_scene_delta(scene, delta).unwrap();
            }
        }
        self.scenes.retain(|key, _| frame.live_scenes.contains(key));
        let layers = frame
            .placements
            .iter()
            .map(|placement| SoftwareCompositeLayer {
                scene: &self.scenes[&placement.scene],
                target: placement.target,
                clip: placement.clip,
            })
            .collect::<Vec<_>>();
        SoftwareRenderer
            .render_composite(
                &mut self.surface,
                &layers,
                EXTENT,
                frame.damage,
                ColorRgba8::rgba(0, 255, 0, 255),
            )
            .unwrap();
        frame
    }

    fn pixel(&self, x: usize, y: usize) -> &[u8] {
        let index = (y * EXTENT.width as usize + x) * 4;
        &self.surface.pixels_rgba8()[index..index + 4]
    }
}

fn frame_layers(color: ColorRgba8) -> Vec<DesktopLayer> {
    let mut scene = RenderScene::default();
    scene.extent = SizeF {
        width: 28.0,
        height: 22.0,
    };
    scene.background = color;
    DesktopLayer::retained_frame(
        9,
        vec![scene.take_delta().unwrap()],
        SizeI {
            width: 28,
            height: 22,
        },
        PointI { x: 2, y: 1 },
        true,
        Some(CONTENT),
    )
}

fn client(alpha_mode: ImageAlphaMode, pixel: [u8; 4], visible: bool) -> DesktopLayer {
    DesktopLayer::image(
        DesktopLayerKey::Surface(9),
        DesktopSceneKey::Surface(9),
        1,
        DesktopImageUpdate::Full(
            pixel
                .repeat((CONTENT.width * CONTENT.height) as usize)
                .into(),
        ),
        SizeI {
            width: CONTENT.width,
            height: CONTENT.height,
        },
        CONTENT,
        Some(CONTENT),
        alpha_mode,
        ImagePixelFormat::Rgba8,
        visible,
    )
}

#[test]
fn client_alpha_sees_lower_layers_without_a_frame_backing() {
    for (mode, pixel, expected) in [
        (
            ImageAlphaMode::Premultiplied,
            [0, 0, 0, 0],
            [0, 255, 0, 255],
        ),
        (
            ImageAlphaMode::Premultiplied,
            [0, 0, 0, 128],
            [0, 187, 0, 255],
        ),
        (
            ImageAlphaMode::Straight,
            [0, 0, 255, 128],
            [0, 187, 188, 255],
        ),
        (ImageAlphaMode::Opaque, [0, 0, 255, 0], [0, 0, 255, 255]),
    ] {
        let mut raster = Raster::new();
        let mut layers = frame_layers(ColorRgba8::rgba(255, 0, 0, 255));
        layers.push(DesktopLayer::solid(
            DesktopLayerKey::ContentBackground(9),
            DesktopSceneKey::ContentBackground(9),
            ColorRgba8::rgba(0, 0, 0, 0),
            CONTENT,
        ));
        layers.push(client(mode, pixel, true));
        raster.draw(layers);
        assert_eq!(raster.pixel(12, 12), expected, "{mode:?}");
        assert_eq!(
            raster.pixel(12, 3),
            [255, 0, 0, 255],
            "title bar is unchanged"
        );
    }
}

#[test]
fn transparent_preview_reveals_desktop_not_retained_client_or_normal_backing() {
    for alpha in [0, 128, 255] {
        let mut raster = Raster::new();
        let mut layers = frame_layers(ColorRgba8::rgba(255, 0, 0, 255));
        layers.push(DesktopLayer::solid(
            DesktopLayerKey::ContentBackground(9),
            DesktopSceneKey::ContentBackground(9),
            ColorRgba8::rgba(0, 0, 0, 255),
            CONTENT,
        ));
        layers.push(client(ImageAlphaMode::Opaque, [0, 0, 255, 255], true));
        raster.draw(layers);
        assert_eq!(raster.pixel(12, 12), [0, 0, 255, 255]);

        let mut layers = DesktopLayer::retained_frame(
            9,
            Vec::new(),
            SizeI {
                width: 28,
                height: 22,
            },
            PointI { x: 2, y: 1 },
            true,
            Some(CONTENT),
        );
        layers.push(DesktopLayer::solid(
            DesktopLayerKey::ResizeVeil(9),
            DesktopSceneKey::ResizeVeil(9),
            ColorRgba8::rgba(255, 0, 0, alpha),
            CONTENT,
        ));
        let mut hidden = client(ImageAlphaMode::Opaque, [0, 0, 255, 255], false);
        if let DesktopLayerContent::Image { update, .. } = &mut hidden.content {
            *update = DesktopImageUpdate::Unchanged;
        }
        layers.push(hidden);
        let frame = raster.draw(layers);
        // Source-over is evaluated in linear light, then encoded back to sRGB bytes.
        let expected = match alpha {
            0 => [0, 255, 0, 255],
            128 => [188, 187, 0, 255],
            255 => [255, 0, 0, 255],
            _ => unreachable!(),
        };
        assert_eq!(raster.pixel(12, 12), expected);
        assert_eq!(raster.pixel(12, 3), [255, 0, 0, 255]);
        assert!(frame.surface_revisions.is_empty());
        assert!(
            frame
                .updates
                .iter()
                .flat_map(|update| &update.deltas)
                .all(|delta| delta.image_resources.is_empty())
        );
    }
}

#[test]
fn translucent_frame_pieces_blend_exactly_once() {
    let mut raster = Raster::new();
    let frame = raster.draw(frame_layers(ColorRgba8::rgba(255, 0, 0, 128)));
    assert_eq!(
        frame.updates.len(),
        1,
        "one scene shared across four scissors"
    );
    for y in 0..EXTENT.height as usize {
        for x in 0..EXTENT.width as usize {
            let outside = !(2..30).contains(&x) || !(1..23).contains(&y);
            let content = (6..26).contains(&x) && (8..20).contains(&y);
            let expected = if outside || content {
                [0, 255, 0, 255]
            } else {
                [188, 187, 0, 255]
            };
            assert_eq!(raster.pixel(x, y), expected, "pixel {x},{y}");
        }
    }
}

#[test]
fn rounded_backing_and_resize_geometry_render_at_native_extent() {
    let mut raster = Raster::new();
    let backing = |rect, radius, alpha| {
        DesktopLayer::rounded_solid(
            DesktopLayerKey::ContentBackground(9),
            DesktopSceneKey::ContentBackground(9),
            ColorRgba8::rgba(255, 0, 0, alpha),
            rect,
            radius,
        )
    };
    raster.draw(vec![backing(CONTENT, 5.0, 255)]);
    assert_eq!(raster.pixel(6, 8), [0, 255, 0, 255]);
    assert_eq!(raster.pixel(12, 12), [255, 0, 0, 255]);
    raster.draw(vec![backing(CONTENT, 0.0, 128)]);
    assert_eq!(raster.pixel(6, 8), [188, 187, 0, 255]);
    raster.draw(vec![backing(CONTENT, 5.0, 128)]);
    assert_eq!(raster.pixel(6, 8), [0, 255, 0, 255]);
    assert_eq!(raster.pixel(12, 12), [188, 187, 0, 255]);
    let smaller = RectI {
        width: 12,
        height: 8,
        ..CONTENT
    };
    raster.draw(vec![backing(smaller, 0.0, 255)]);
    assert_eq!(raster.pixel(12, 12), [255, 0, 0, 255]);
    assert_eq!(
        raster.pixel(24, 18),
        [0, 255, 0, 255],
        "old coverage is repainted"
    );
}
