use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::core::{ColorRgba8, PointI, RectF, RectI, SizeF, SizeI};
use crate::render::{
    BatchKey, BlendMode, ClipId, DrawItem, ImageAlphaMode, ImageColorEncoding, ImageId,
    ImageInstance, ImagePixelFormat, ImageResource, ImageResourceUpdate, PipelineKind,
    PrimitiveKind, RenderClip, RenderScene, RenderSceneDelta, SpatialId,
};
use crate::scene::NodeId;
use crate::ui::CornerRadii;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]
pub(super) enum DesktopLayerKey {
    Background,
    Frame(u32),
    Surface(u32),
    LegacyControl(u32, u8),
    Widget(u32),
    DragIcon(u32),
    Cursor,
}

#[derive(Clone)]
pub(super) enum DesktopImageUpdate {
    Unchanged,
    Full(Arc<[u8]>),
    Region {
        rect: RectI,
        row_bytes: usize,
        pixels: Arc<[u8]>,
    },
}

#[derive(Clone)]
pub(super) struct DesktopLayer {
    pub key: DesktopLayerKey,
    pub content_version: u64,
    pub update: DesktopImageUpdate,
    pub extent: SizeI,
    pub position: PointI,
    /// A compositor-space clip. This is how a stale committed client buffer is constrained to
    /// the current interactive-resize preview without stretching it.
    pub clip: Option<RectI>,
    pub alpha_mode: ImageAlphaMode,
    pub pixel_format: ImagePixelFormat,
    /// Invisible layers keep their retained resource while contributing no draw or hit geometry.
    pub visible: bool,
}

#[derive(Clone, Copy)]
struct SceneEntry {
    node: NodeId,
    image: ImageId,
    clip: ClipId,
    bounds: RectF,
    source_version: u64,
    content_version: u64,
    extent: SizeI,
    alpha_mode: ImageAlphaMode,
    pixel_format: ImagePixelFormat,
}

pub(super) struct DesktopScene {
    source: RenderScene,
    entries: BTreeMap<DesktopLayerKey, SceneEntry>,
    next_node: u32,
    next_image: u32,
}

impl DesktopScene {
    pub fn new(extent: SizeI) -> Self {
        let mut source = RenderScene::default();
        source.extent = size_f(extent);
        source.background = ColorRgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        Self {
            source,
            entries: BTreeMap::new(),
            next_node: 1,
            next_image: 0x7000_0000,
        }
    }

    pub fn synchronize(
        &mut self,
        extent: SizeI,
        layers: Vec<DesktopLayer>,
    ) -> Option<RenderSceneDelta> {
        let next_extent = size_f(extent);
        if self.source.extent != next_extent {
            self.source.extent = next_extent;
            self.source.damage.full = true;
            self.source.damage.rects.clear();
        }

        let live = layers
            .iter()
            .map(|layer| layer.key)
            .collect::<BTreeSet<_>>();
        let removed = self
            .entries
            .keys()
            .filter(|key| !live.contains(key))
            .copied()
            .collect::<Vec<_>>();
        for key in removed {
            if let Some(entry) = self.entries.remove(&key) {
                self.source.damage.add(entry.bounds, self.source.extent);
                self.source.images.remove(entry.node);
                self.source.clips.remove(entry.node);
                self.source.remove_image_resource(entry.image);
            }
        }

        let mut order = Vec::with_capacity(layers.len());
        for layer in layers {
            if !valid_layer(&layer) {
                continue;
            }
            let mut entry = match self.entries.get(&layer.key).copied() {
                Some(entry) => entry,
                None => {
                    let node = NodeId::new(self.next_node, 1);
                    let entry = SceneEntry {
                        node,
                        image: ImageId(self.next_image),
                        clip: ClipId(self.next_node),
                        bounds: RectF::ZERO,
                        source_version: 0,
                        content_version: 0,
                        extent: SizeI::default(),
                        alpha_mode: ImageAlphaMode::Straight,
                        pixel_format: ImagePixelFormat::Rgba8,
                    };
                    self.next_node = self.next_node.wrapping_add(1).max(1);
                    self.next_image = self.next_image.wrapping_add(1).max(0x7000_0000);
                    entry
                }
            };

            let source_changed = entry.source_version != layer.content_version;
            let metadata_changed = entry.extent != layer.extent
                || entry.alpha_mode != layer.alpha_mode
                || entry.pixel_format != layer.pixel_format;
            if entry.content_version == 0 || metadata_changed {
                let DesktopImageUpdate::Full(pixels) = &layer.update else {
                    continue;
                };
                let content_version = entry.content_version.wrapping_add(1).max(1);
                self.source
                    .set_image_resource(ImageResource {
                        image: entry.image,
                        content_version,
                        extent: layer.extent,
                        color_encoding: ImageColorEncoding::Srgb,
                        alpha_mode: layer.alpha_mode,
                        pixel_format: layer.pixel_format,
                        pixels: Arc::clone(pixels),
                    })
                    .expect("validated desktop image resource");
                entry.content_version = content_version;
                entry.source_version = layer.content_version;
                entry.extent = layer.extent;
                entry.alpha_mode = layer.alpha_mode;
                entry.pixel_format = layer.pixel_format;
            } else if source_changed {
                match &layer.update {
                    DesktopImageUpdate::Unchanged => {}
                    DesktopImageUpdate::Full(pixels) => {
                        let content_version = entry.content_version.wrapping_add(1).max(1);
                        self.source
                            .set_image_resource(ImageResource {
                                image: entry.image,
                                content_version,
                                extent: layer.extent,
                                color_encoding: ImageColorEncoding::Srgb,
                                alpha_mode: layer.alpha_mode,
                                pixel_format: layer.pixel_format,
                                pixels: Arc::clone(pixels),
                            })
                            .expect("validated desktop image resource");
                        entry.content_version = content_version;
                    }
                    DesktopImageUpdate::Region {
                        rect,
                        row_bytes,
                        pixels,
                    } => {
                        let Some(rect) = intersect(
                            *rect,
                            RectI {
                                x: 0,
                                y: 0,
                                width: layer.extent.width,
                                height: layer.extent.height,
                            },
                        ) else {
                            continue;
                        };
                        let content_version = entry.content_version.wrapping_add(1).max(1);
                        self.source
                            .update_image_resource_region(ImageResourceUpdate {
                                image: entry.image,
                                content_version,
                                extent: layer.extent,
                                rect,
                                row_bytes: *row_bytes,
                                color_encoding: ImageColorEncoding::Srgb,
                                alpha_mode: layer.alpha_mode,
                                pixel_format: layer.pixel_format,
                                pixels: Arc::clone(pixels),
                            })
                            .expect("validated desktop image damage");
                        entry.content_version = content_version;
                    }
                }
            }
            entry.source_version = layer.content_version;
            let content_version = entry.content_version.max(1);

            if !layer.visible {
                if let Some(previous) = self.source.images.get(entry.node).copied() {
                    self.source
                        .damage
                        .add(previous.view_bounds, self.source.extent);
                }
                self.source.images.remove(entry.node);
                self.source.clips.remove(entry.node);
                entry.bounds = RectF::ZERO;
                self.entries.insert(layer.key, entry);
                continue;
            }

            let rect = RectF {
                x: layer.position.x as f32,
                y: layer.position.y as f32,
                width: layer.extent.width as f32,
                height: layer.extent.height as f32,
            };
            let clip = layer.clip.map(rect_f);
            let clipped = match clip {
                Some(clip) => rect.intersection(clip),
                None => Some(rect),
            };
            let bounds = clipped
                .and_then(|rect| {
                    rect.intersection(RectF {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                    })
                })
                .unwrap_or(RectF::ZERO);
            let instance = ImageInstance {
                node: entry.node,
                image: entry.image,
                tint: None,
                rect,
                view_bounds: bounds,
                content_version,
                opacity: 1.0,
                clip: clip.map_or(ClipId(0), |_| entry.clip),
                spatial: SpatialId(0),
            };
            let previous = self.source.images.get(entry.node).copied();
            let geometry_changed = previous.is_none_or(|previous| {
                previous.image != instance.image
                    || previous.rect != instance.rect
                    || previous.view_bounds != instance.view_bounds
                    || previous.clip != instance.clip
                    || previous.spatial != instance.spatial
                    || previous.opacity != instance.opacity
                    || previous.tint != instance.tint
            });
            if self.source.images.upsert(entry.node, instance) && geometry_changed {
                if let Some(previous) = previous {
                    self.source
                        .damage
                        .add(previous.view_bounds, self.source.extent);
                }
                self.source.damage.add(bounds, self.source.extent);
            }
            if let Some(clip_rect) = clip {
                let previous = self.source.clips.get(entry.node).copied();
                let next = RenderClip {
                    id: entry.clip,
                    rect: clip_rect,
                    corner_radii: CornerRadii::default(),
                };
                if self.source.clips.upsert(entry.node, next) && previous != Some(next) {
                    self.source.damage.add(bounds, self.source.extent);
                }
            } else {
                self.source.clips.remove(entry.node);
            }
            entry.bounds = bounds;
            self.entries.insert(layer.key, entry);
            order.push(layer.key);
        }

        self.source.set_draw_order(
            order
                .into_iter()
                .filter_map(|key| {
                    let entry = self.entries.get(&key)?;
                    let index = self.source.images.index(entry.node)?;
                    Some(DrawItem {
                        kind: PrimitiveKind::Image,
                        index: index as u32,
                        batch: BatchKey {
                            pipeline: PipelineKind::Image,
                            resource: entry.image.0,
                            clip: self
                                .source
                                .images
                                .get(entry.node)
                                .map_or(ClipId(0), |instance| instance.clip),
                            blend: if entry.alpha_mode == ImageAlphaMode::Opaque {
                                BlendMode::Opaque
                            } else {
                                BlendMode::Alpha
                            },
                            target: 0,
                        },
                    })
                })
                .collect(),
        );
        self.source.take_delta()
    }
}

pub(super) fn delta_damage(delta: &RenderSceneDelta, extent: SizeI) -> Option<RectI> {
    if delta.damage.full {
        return None;
    }
    let mut damage = None;
    for rect in &delta.damage.rects {
        let rect = rect.to_i32();
        damage = Some(match damage {
            Some(current) => union(current, rect),
            None => rect,
        });
    }
    damage.and_then(|damage| {
        intersect(
            damage,
            RectI {
                x: 0,
                y: 0,
                width: extent.width,
                height: extent.height,
            },
        )
    })
}

fn valid_layer(layer: &DesktopLayer) -> bool {
    if layer.extent.width <= 0 || layer.extent.height <= 0 {
        return false;
    }
    match &layer.update {
        DesktopImageUpdate::Unchanged => true,
        DesktopImageUpdate::Full(pixels) => {
            pixels.len() >= layer.extent.width as usize * layer.extent.height as usize * 4
        }
        DesktopImageUpdate::Region {
            rect,
            row_bytes,
            pixels,
        } => {
            rect.x >= 0
                && rect.y >= 0
                && rect.width > 0
                && rect.height > 0
                && rect.right() <= layer.extent.width
                && rect.bottom() <= layer.extent.height
                && *row_bytes >= rect.width as usize * 4
                && pixels.len() >= row_bytes.saturating_mul(rect.height as usize)
        }
    }
}

fn size_f(size: SizeI) -> SizeF {
    SizeF {
        width: size.width as f32,
        height: size.height as f32,
    }
}

fn rect_f(rect: RectI) -> RectF {
    RectF {
        x: rect.x as f32,
        y: rect.y as f32,
        width: rect.width as f32,
        height: rect.height as f32,
    }
}

fn union(left: RectI, right: RectI) -> RectI {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = left.right().max(right.right());
    let bottom = left.bottom().max(right.bottom());
    RectI {
        x,
        y,
        width: right_edge.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

fn intersect(left: RectI, right: RectI) -> Option<RectI> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = left.right().min(right.right());
    let bottom = left.bottom().min(right.bottom());
    (right_edge > x && bottom > y).then_some(RectI {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{RenderBackend, RenderRequest, RenderTargetInfo, TargetLoad, TargetStore};
    use crate::renderer_software::{SoftwareRenderer, SoftwareSurface, SoftwareTarget};

    fn layer(position: PointI, clip: Option<RectI>) -> DesktopLayer {
        DesktopLayer {
            key: DesktopLayerKey::Surface(9),
            content_version: 1,
            update: DesktopImageUpdate::Full(vec![255; 100 * 80 * 4].into()),
            extent: SizeI {
                width: 100,
                height: 80,
            },
            position,
            clip,
            alpha_mode: ImageAlphaMode::Opaque,
            pixel_format: ImagePixelFormat::Rgba8,
            visible: true,
        }
    }

    #[test]
    fn movement_damages_old_and_new_bounds_without_forcing_full_output() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let mut scene = DesktopScene::new(extent);
        let _ = scene.synchronize(extent, vec![layer(PointI { x: 10, y: 20 }, None)]);
        let delta = scene
            .synchronize(extent, vec![layer(PointI { x: 40, y: 20 }, None)])
            .unwrap();
        assert!(!delta.damage.full);
        let damage = delta_damage(&delta, extent).unwrap();
        assert!(damage.x <= 10);
        assert!(damage.right() >= 140);
    }

    #[test]
    fn stale_buffer_is_clipped_to_resize_preview() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let mut scene = DesktopScene::new(extent);
        let clip = RectI {
            x: 20,
            y: 30,
            width: 64,
            height: 48,
        };
        let _ = scene.synchronize(extent, vec![layer(PointI { x: 20, y: 30 }, Some(clip))]);
        let entry = scene.entries[&DesktopLayerKey::Surface(9)];
        assert_eq!(
            scene.source.images.get(entry.node).unwrap().view_bounds,
            rect_f(clip)
        );
    }

    #[test]
    fn a_fully_clipped_layer_has_no_visible_bounds() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let mut scene = DesktopScene::new(extent);
        let _ = scene.synchronize(
            extent,
            vec![layer(
                PointI { x: 20, y: 30 },
                Some(RectI {
                    x: 400,
                    y: 400,
                    width: 20,
                    height: 20,
                }),
            )],
        );
        let entry = scene.entries[&DesktopLayerKey::Surface(9)];
        assert_eq!(
            scene.source.images.get(entry.node).unwrap().view_bounds,
            RectF::ZERO
        );
    }

    #[test]
    fn client_resource_damage_remains_regional() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let position = PointI { x: 20, y: 30 };
        let mut scene = DesktopScene::new(extent);
        let _ = scene.synchronize(extent, vec![layer(position, None)]);
        let mut updated = layer(position, None);
        updated.content_version = 2;
        let rect = RectI {
            x: 4,
            y: 5,
            width: 8,
            height: 6,
        };
        updated.update = DesktopImageUpdate::Region {
            rect,
            row_bytes: rect.width as usize * 4,
            pixels: vec![128; rect.width as usize * rect.height as usize * 4].into(),
        };
        let delta = scene.synchronize(extent, vec![updated]).unwrap();
        assert!(!delta.damage.full);
        let damage = delta_damage(&delta, extent).unwrap();
        assert!(damage.x >= 22 && damage.x <= 24);
        assert!(damage.y >= 33 && damage.y <= 35);
        assert!(damage.width <= 12);
        assert!(damage.height <= 10);
    }

    #[test]
    fn hiding_and_restoring_a_layer_retains_its_image_resource() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let position = PointI { x: 20, y: 30 };
        let mut scene = DesktopScene::new(extent);
        let _ = scene.synchronize(extent, vec![layer(position, None)]);
        let entry = scene.entries[&DesktopLayerKey::Surface(9)];

        let mut hidden = layer(position, None);
        hidden.update = DesktopImageUpdate::Unchanged;
        hidden.visible = false;
        let hidden_delta = scene.synchronize(extent, vec![hidden]).unwrap();
        assert!(hidden_delta.image_resources.is_empty());
        assert!(scene.source.images.get(entry.node).is_none());

        let mut restored = layer(position, None);
        restored.update = DesktopImageUpdate::Unchanged;
        let restored_delta = scene.synchronize(extent, vec![restored]).unwrap();
        assert!(restored_delta.image_resources.is_empty());
        assert_eq!(
            scene.entries[&DesktopLayerKey::Surface(9)].image,
            entry.image
        );
        assert!(scene.source.images.get(entry.node).is_some());
    }

    #[test]
    fn retained_desktop_layers_are_composited_by_the_software_backend() {
        let extent = SizeI {
            width: 4,
            height: 4,
        };
        let solid = |key, extent: SizeI, rgba: [u8; 4], position| DesktopLayer {
            key,
            content_version: 1,
            update: DesktopImageUpdate::Full(
                (0..extent.width * extent.height)
                    .flat_map(|_| rgba)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            extent,
            position,
            clip: None,
            alpha_mode: ImageAlphaMode::Opaque,
            pixel_format: ImagePixelFormat::Rgba8,
            visible: true,
        };
        let mut desktop = DesktopScene::new(extent);
        let delta = desktop
            .synchronize(
                extent,
                vec![
                    solid(
                        DesktopLayerKey::Background,
                        extent,
                        [255, 0, 0, 255],
                        PointI::default(),
                    ),
                    solid(
                        DesktopLayerKey::Surface(1),
                        SizeI {
                            width: 2,
                            height: 2,
                        },
                        [0, 255, 0, 255],
                        PointI { x: 1, y: 1 },
                    ),
                ],
            )
            .unwrap();
        let renderer = SoftwareRenderer;
        let mut scene = renderer.create_scene().unwrap();
        renderer.apply_scene_delta(&mut scene, &delta).unwrap();
        let mut surface = SoftwareSurface::default();
        let target = SoftwareTarget::new(RenderTargetInfo::full(extent));
        let mut frame = surface.begin_frame();
        renderer
            .render(
                &mut scene,
                &mut frame,
                &target,
                &RenderRequest {
                    force: true,
                    load: TargetLoad::Clear(ColorRgba8::default()),
                    store: TargetStore::Store,
                    region: None,
                },
            )
            .unwrap();
        let pixel = |x: usize, y: usize| {
            let index = (y * extent.width as usize + x) * 4;
            <[u8; 4]>::try_from(&surface.pixels_rgba8()[index..index + 4]).unwrap()
        };
        assert_eq!(pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(1, 1), [0, 255, 0, 255]);
        assert_eq!(pixel(3, 3), [255, 0, 0, 255]);
    }
}
