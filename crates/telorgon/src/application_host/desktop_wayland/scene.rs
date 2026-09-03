use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::core::{ColorRgba8, PointI, RectF, RectI, SizeF, SizeI};
use crate::render::{
    BatchKey, BlendMode, BoxInstance, ClipId, DrawItem, ImageAlphaMode, ImageColorEncoding,
    ImageId, ImageInstance, ImagePixelFormat, ImageResource, ImageResourceUpdate, PipelineKind,
    PrimitiveKind, RangePatch, RenderScene, RenderSceneDelta, SpatialId,
};
use crate::scene::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]
pub(super) enum DesktopLayerKey {
    Background,
    Frame(u32),
    Surface(u32),
    LegacyControl(u32, u8),
    LegacyControlSource(u8),
    Widget(u32),
    DragIcon(u32),
    Cursor,
    ComposedPointerSource,
    ComposedIconSource(usize),
}

/// Identifies retained scene content independently from a particular desktop placement.
///
/// A single icon scene, for example, can be placed in more than one window without duplicating
/// its backend resources. Placement keys remain unique so movement and stacking damage are still
/// tracked correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]
pub(super) enum DesktopSceneKey {
    Background,
    Frame(u32),
    Surface(u32),
    LegacyControl(u8),
    Widget(u32),
    DragIcon(u32),
    CursorImage,
    ComposedPointer,
    ComposedIcon(usize),
}

#[derive(Clone)]
pub(super) struct DesktopImageRegion {
    pub rect: RectI,
    pub row_bytes: usize,
    pub pixels: Arc<[u8]>,
}

#[derive(Clone)]
pub(super) enum DesktopImageUpdate {
    Unchanged,
    Full(Arc<[u8]>),
    Regions(Vec<DesktopImageRegion>),
}

pub(super) enum DesktopLayerContent {
    Retained {
        scene: DesktopSceneKey,
        deltas: Vec<RenderSceneDelta>,
    },
    Image {
        scene: DesktopSceneKey,
        content_version: u64,
        update: DesktopImageUpdate,
        alpha_mode: ImageAlphaMode,
        pixel_format: ImagePixelFormat,
    },
}

pub(super) struct DesktopLayer {
    pub key: DesktopLayerKey,
    pub content: DesktopLayerContent,
    pub extent: SizeI,
    pub position: PointI,
    /// A compositor-space clip used to constrain stale committed buffers during live resize.
    pub clip: Option<RectI>,
    /// Invisible placements retain their backend scene but contribute no output geometry.
    pub visible: bool,
}

impl DesktopLayer {
    pub(super) fn retained(
        key: DesktopLayerKey,
        scene: DesktopSceneKey,
        deltas: Vec<RenderSceneDelta>,
        extent: SizeI,
        position: PointI,
        visible: bool,
    ) -> Self {
        Self {
            key,
            content: DesktopLayerContent::Retained { scene, deltas },
            extent,
            position,
            clip: None,
            visible,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn image(
        key: DesktopLayerKey,
        scene: DesktopSceneKey,
        content_version: u64,
        update: DesktopImageUpdate,
        extent: SizeI,
        position: PointI,
        clip: Option<RectI>,
        alpha_mode: ImageAlphaMode,
        pixel_format: ImagePixelFormat,
        visible: bool,
    ) -> Self {
        Self {
            key,
            content: DesktopLayerContent::Image {
                scene,
                content_version,
                update,
                alpha_mode,
                pixel_format,
            },
            extent,
            position,
            clip,
            visible,
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]
pub(super) struct DesktopSceneUpdate {
    pub key: DesktopSceneKey,
    pub deltas: Vec<RenderSceneDelta>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DesktopPlacement {
    pub key: DesktopLayerKey,
    pub scene: DesktopSceneKey,
    pub target: RectI,
    pub clip: Option<RectI>,
}

#[derive(Clone, Debug)]
#[cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]
pub(super) struct DesktopFrame {
    pub extent: SizeI,
    pub live_scenes: BTreeSet<DesktopSceneKey>,
    pub updates: Vec<DesktopSceneUpdate>,
    pub placements: Vec<DesktopPlacement>,
    /// `None` means the complete output; `Some` is a retained-output damage rectangle.
    pub damage: Option<RectI>,
}

struct ImageScene {
    source: RenderScene,
    source_version: u64,
    content_version: u64,
    extent: SizeI,
    alpha_mode: ImageAlphaMode,
    pixel_format: ImagePixelFormat,
}

impl ImageScene {
    fn new() -> Self {
        let mut source = RenderScene::default();
        source.background = ColorRgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        source.extent = SizeF {
            width: 1.0,
            height: 1.0,
        };
        Self {
            source,
            source_version: 0,
            content_version: 0,
            extent: SizeI::default(),
            alpha_mode: ImageAlphaMode::Straight,
            pixel_format: ImagePixelFormat::Rgba8,
        }
    }

    fn synchronize(
        &mut self,
        source_version: u64,
        update: &DesktopImageUpdate,
        extent: SizeI,
        alpha_mode: ImageAlphaMode,
        pixel_format: ImagePixelFormat,
    ) -> Option<RenderSceneDelta> {
        let metadata_changed = self.extent != extent
            || self.alpha_mode != alpha_mode
            || self.pixel_format != pixel_format;
        let source_changed = self.source_version != source_version;
        if self.content_version == 0 || metadata_changed {
            let DesktopImageUpdate::Full(pixels) = update else {
                return None;
            };
            self.content_version = self.content_version.wrapping_add(1).max(1);
            self.source.extent = size_f(extent);
            self.source.damage.full = true;
            self.source.damage.rects.clear();
            self.source
                .set_image_resource(ImageResource {
                    image: ImageId(1),
                    content_version: self.content_version,
                    extent,
                    color_encoding: ImageColorEncoding::Srgb,
                    alpha_mode,
                    pixel_format,
                    pixels: Arc::clone(pixels),
                })
                .expect("validated desktop image resource");
            self.extent = extent;
            self.alpha_mode = alpha_mode;
            self.pixel_format = pixel_format;
        } else if source_changed {
            match update {
                // A hidden client can publish a newer revision while its pixel update remains
                // queued in `ClientWindow`. Do not acknowledge that revision until the queued
                // pixels are actually handed to this retained image scene.
                DesktopImageUpdate::Unchanged => return None,
                DesktopImageUpdate::Full(pixels) => {
                    self.content_version = self.content_version.wrapping_add(1).max(1);
                    self.source
                        .set_image_resource(ImageResource {
                            image: ImageId(1),
                            content_version: self.content_version,
                            extent,
                            color_encoding: ImageColorEncoding::Srgb,
                            alpha_mode,
                            pixel_format,
                            pixels: Arc::clone(pixels),
                        })
                        .expect("validated desktop image resource");
                }
                DesktopImageUpdate::Regions(regions) => {
                    for region in regions {
                        self.content_version = self.content_version.wrapping_add(1).max(1);
                        self.source
                            .update_image_resource_region(ImageResourceUpdate {
                                image: ImageId(1),
                                content_version: self.content_version,
                                extent,
                                rect: region.rect,
                                row_bytes: region.row_bytes,
                                color_encoding: ImageColorEncoding::Srgb,
                                alpha_mode,
                                pixel_format,
                                pixels: Arc::clone(&region.pixels),
                            })
                            .expect("validated desktop image region");
                    }
                }
            }
        }
        self.source_version = source_version;
        if self.content_version == 0 {
            return None;
        }
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
        };
        self.source.images.upsert(
            NodeId::new(1, 1),
            ImageInstance {
                node: NodeId::new(1, 1),
                image: ImageId(1),
                tint: None,
                rect: bounds,
                view_bounds: bounds,
                content_version: self.content_version,
                opacity: 1.0,
                clip: ClipId(0),
                spatial: SpatialId(0),
            },
        );
        self.source.set_draw_order(vec![DrawItem {
            kind: PrimitiveKind::Image,
            index: 0,
            batch: BatchKey {
                pipeline: PipelineKind::Image,
                resource: 1,
                clip: ClipId(0),
                blend: if alpha_mode == ImageAlphaMode::Opaque {
                    BlendMode::Opaque
                } else {
                    BlendMode::Alpha
                },
                target: 0,
            },
        }]);
        self.source.take_delta()
    }
}

#[derive(Clone, Copy)]
struct PlacementState {
    scene: DesktopSceneKey,
    bounds: Option<RectI>,
    target: RectI,
    clip: Option<RectI>,
}

#[derive(Default)]
struct RetainedSceneAdapter {
    draw_order: Arc<[DrawItem]>,
}

impl RetainedSceneAdapter {
    fn adapt(&mut self, mut delta: RenderSceneDelta) -> RenderSceneDelta {
        if let Some(order) = &delta.draw_order {
            self.draw_order = Arc::clone(order);
        }
        let background_index = delta.box_len;
        let bounds = RectF {
            x: 0.0,
            y: 0.0,
            width: delta.extent.width,
            height: delta.extent.height,
        };
        delta.boxes.push(RangePatch {
            start: background_index,
            values: Arc::from([BoxInstance {
                node: NodeId::new(u32::MAX - 1, 1),
                rect: bounds,
                view_bounds: bounds,
                background: Some(delta.background),
                border: Default::default(),
                outline: Default::default(),
                corner_radii: Default::default(),
                shadows: Default::default(),
                opacity: 1.0,
                clip: ClipId(0),
                spatial: SpatialId(0),
            }]),
        });
        delta.box_len = delta.box_len.saturating_add(1);
        let mut order = Vec::with_capacity(self.draw_order.len() + 1);
        if delta.background.a != 0 {
            order.push(DrawItem {
                kind: PrimitiveKind::Box,
                index: background_index as u32,
                batch: BatchKey {
                    pipeline: PipelineKind::AnalyticBox,
                    resource: 0,
                    clip: ClipId(0),
                    blend: if delta.background.a == u8::MAX {
                        BlendMode::Opaque
                    } else {
                        BlendMode::Alpha
                    },
                    target: 0,
                },
            });
        }
        order.extend(self.draw_order.iter().copied());
        delta.draw_order = Some(order.into());
        delta
    }
}

/// Builds an ordered desktop frame without choosing or invoking a concrete renderer.
pub(super) struct DesktopComposition {
    extent: SizeI,
    image_scenes: BTreeMap<DesktopSceneKey, ImageScene>,
    retained_scenes: BTreeMap<DesktopSceneKey, RetainedSceneAdapter>,
    placements: BTreeMap<DesktopLayerKey, PlacementState>,
    order: Vec<DesktopLayerKey>,
}

impl DesktopComposition {
    pub(super) fn new(extent: SizeI) -> Self {
        Self {
            extent,
            image_scenes: BTreeMap::new(),
            retained_scenes: BTreeMap::new(),
            placements: BTreeMap::new(),
            order: Vec::new(),
        }
    }

    pub(super) fn synchronize(
        &mut self,
        extent: SizeI,
        layers: Vec<DesktopLayer>,
    ) -> Option<DesktopFrame> {
        let output = full_rect(extent);
        let extent_changed = self.extent != extent;
        self.extent = extent;
        let mut live_scenes = BTreeSet::new();
        let mut updates = BTreeMap::<DesktopSceneKey, Vec<RenderSceneDelta>>::new();
        let mut placements = Vec::new();
        let mut next_states = BTreeMap::new();
        let mut next_order = Vec::new();
        let mut damage = None;

        for layer in layers.into_iter().filter(valid_layer) {
            let scene = match layer.content {
                DesktopLayerContent::Retained { scene, deltas } => {
                    if !deltas.is_empty() {
                        let adapter = self.retained_scenes.entry(scene).or_default();
                        updates
                            .entry(scene)
                            .or_default()
                            .extend(deltas.into_iter().map(|delta| adapter.adapt(delta)));
                    }
                    scene
                }
                DesktopLayerContent::Image {
                    scene,
                    content_version,
                    update,
                    alpha_mode,
                    pixel_format,
                } => {
                    let source = self
                        .image_scenes
                        .entry(scene)
                        .or_insert_with(ImageScene::new);
                    if let Some(delta) = source.synchronize(
                        content_version,
                        &update,
                        layer.extent,
                        alpha_mode,
                        pixel_format,
                    ) {
                        updates.entry(scene).or_default().push(delta);
                    }
                    scene
                }
            };
            live_scenes.insert(scene);
            let target = RectI {
                x: layer.position.x,
                y: layer.position.y,
                width: layer.extent.width,
                height: layer.extent.height,
            };
            let bounds = layer
                .visible
                .then_some(target)
                .and_then(|target| {
                    layer
                        .clip
                        .map_or(Some(target), |clip| intersect(target, clip))
                })
                .and_then(|target| intersect(target, output));
            let state = PlacementState {
                scene,
                bounds,
                target,
                clip: layer.clip,
            };
            if self.placements.get(&layer.key).is_none_or(|previous| {
                previous.scene != state.scene
                    || previous.bounds != state.bounds
                    || previous.target != state.target
                    || previous.clip != state.clip
            }) {
                if let Some(previous) = self
                    .placements
                    .get(&layer.key)
                    .and_then(|state| state.bounds)
                {
                    add_damage(&mut damage, previous, output);
                }
                if let Some(bounds) = bounds {
                    add_damage(&mut damage, bounds, output);
                }
            }
            next_states.insert(layer.key, state);
            if bounds.is_some() {
                next_order.push(layer.key);
                placements.push(DesktopPlacement {
                    key: layer.key,
                    scene,
                    target,
                    clip: layer.clip,
                });
            }
        }

        for (key, previous) in &self.placements {
            if !next_states.contains_key(key)
                && let Some(bounds) = previous.bounds
            {
                add_damage(&mut damage, bounds, output);
            }
        }

        let order_changed = self.order != next_order;
        if order_changed || extent_changed {
            // Reordering can change every overlap in the stack. It is infrequent and correctness
            // is clearer than attempting a fragile pairwise overlap reconstruction here.
            damage = Some(output);
        }

        for (scene, deltas) in &updates {
            for delta in deltas {
                for placement in placements
                    .iter()
                    .filter(|placement| placement.scene == *scene)
                {
                    if delta.damage.full {
                        if let Some(bounds) = placement_bounds(*placement, output) {
                            add_damage(&mut damage, bounds, output);
                        }
                    } else {
                        for rect in &delta.damage.rects {
                            if let Some(mapped) = map_scene_rect(*rect, delta.extent, *placement)
                                .and_then(|rect| intersect(rect, output))
                            {
                                add_damage(&mut damage, mapped, output);
                            }
                        }
                    }
                }
            }
        }

        self.image_scenes.retain(|key, _| live_scenes.contains(key));
        self.retained_scenes
            .retain(|key, _| live_scenes.contains(key));
        self.placements = next_states;
        self.order = next_order;

        let has_updates = !updates.is_empty();
        if !has_updates && damage.is_none() {
            return None;
        }
        // Resource-only changes should not get stranded without a backend turn. If a producer did
        // not attach explicit damage, repaint every visible placement that consumes that scene.
        if damage.is_none() && has_updates {
            for placement in &placements {
                if updates.contains_key(&placement.scene)
                    && let Some(bounds) = placement_bounds(*placement, output)
                {
                    add_damage(&mut damage, bounds, output);
                }
            }
        }
        let damage = damage.map(|damage| intersect(damage, output).unwrap_or(output));
        Some(DesktopFrame {
            extent,
            live_scenes,
            updates: updates
                .into_iter()
                .map(|(key, deltas)| DesktopSceneUpdate { key, deltas })
                .collect(),
            placements,
            damage: if damage == Some(output) { None } else { damage },
        })
    }
}

fn valid_layer(layer: &DesktopLayer) -> bool {
    if layer.extent.width <= 0 || layer.extent.height <= 0 {
        return false;
    }
    match &layer.content {
        DesktopLayerContent::Retained { .. } => true,
        DesktopLayerContent::Image { update, .. } => match update {
            DesktopImageUpdate::Unchanged => true,
            DesktopImageUpdate::Full(pixels) => {
                pixels.len() >= layer.extent.width as usize * layer.extent.height as usize * 4
            }
            DesktopImageUpdate::Regions(regions) => {
                !regions.is_empty()
                    && regions.iter().all(|region| {
                        let rect = region.rect;
                        rect.x >= 0
                            && rect.y >= 0
                            && rect.width > 0
                            && rect.height > 0
                            && rect.right() <= layer.extent.width
                            && rect.bottom() <= layer.extent.height
                            && region.row_bytes >= rect.width as usize * 4
                            && region.pixels.len()
                                >= region.row_bytes.saturating_mul(rect.height as usize)
                    })
            }
        },
    }
}

fn map_scene_rect(rect: RectF, source_extent: SizeF, placement: DesktopPlacement) -> Option<RectI> {
    let width = source_extent.width.max(1.0);
    let height = source_extent.height.max(1.0);
    let scale_x = placement.target.width as f32 / width;
    let scale_y = placement.target.height as f32 / height;
    let left = (placement.target.x as f32 + rect.x * scale_x).floor() as i32;
    let top = (placement.target.y as f32 + rect.y * scale_y).floor() as i32;
    let right = (placement.target.x as f32 + rect.right() * scale_x).ceil() as i32;
    let bottom = (placement.target.y as f32 + rect.bottom() * scale_y).ceil() as i32;
    let mapped = RectI {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    };
    placement
        .clip
        .map_or(Some(mapped), |clip| intersect(mapped, clip))
}

fn placement_bounds(placement: DesktopPlacement, output: RectI) -> Option<RectI> {
    placement
        .clip
        .map_or(Some(placement.target), |clip| {
            intersect(placement.target, clip)
        })
        .and_then(|rect| intersect(rect, output))
}

fn add_damage(damage: &mut Option<RectI>, rect: RectI, output: RectI) {
    let Some(rect) = intersect(rect, output) else {
        return;
    };
    *damage = Some(damage.map_or(rect, |current| union(current, rect)));
}

fn size_f(size: SizeI) -> SizeF {
    SizeF {
        width: size.width as f32,
        height: size.height as f32,
    }
}

fn full_rect(size: SizeI) -> RectI {
    RectI {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
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

    fn image_layer(position: PointI, update: DesktopImageUpdate) -> DesktopLayer {
        DesktopLayer::image(
            DesktopLayerKey::Surface(9),
            DesktopSceneKey::Surface(9),
            1,
            update,
            SizeI {
                width: 100,
                height: 80,
            },
            position,
            None,
            ImageAlphaMode::Opaque,
            ImagePixelFormat::Rgba8,
            true,
        )
    }

    #[test]
    fn movement_damages_old_and_new_bounds_without_rebuilding_content() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let mut composition = DesktopComposition::new(extent);
        let _ = composition.synchronize(
            extent,
            vec![image_layer(
                PointI { x: 10, y: 20 },
                DesktopImageUpdate::Full(vec![255; 100 * 80 * 4].into()),
            )],
        );
        let frame = composition
            .synchronize(
                extent,
                vec![image_layer(
                    PointI { x: 40, y: 20 },
                    DesktopImageUpdate::Unchanged,
                )],
            )
            .unwrap();
        assert_eq!(
            frame.damage,
            Some(RectI {
                x: 10,
                y: 20,
                width: 130,
                height: 80,
            })
        );
        assert!(frame.updates.is_empty());
    }

    #[test]
    fn disjoint_image_updates_remain_disjoint_in_the_scene_delta() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let position = PointI { x: 20, y: 30 };
        let mut composition = DesktopComposition::new(extent);
        let _ = composition.synchronize(
            extent,
            vec![image_layer(
                position,
                DesktopImageUpdate::Full(vec![255; 100 * 80 * 4].into()),
            )],
        );
        let rects = [
            RectI {
                x: 4,
                y: 5,
                width: 8,
                height: 6,
            },
            RectI {
                x: 82,
                y: 67,
                width: 7,
                height: 5,
            },
        ];
        let mut layer = image_layer(
            position,
            DesktopImageUpdate::Regions(
                rects
                    .into_iter()
                    .map(|rect| DesktopImageRegion {
                        rect,
                        row_bytes: rect.width as usize * 4,
                        pixels: vec![128; rect.width as usize * rect.height as usize * 4].into(),
                    })
                    .collect(),
            ),
        );
        if let DesktopLayerContent::Image {
            content_version, ..
        } = &mut layer.content
        {
            *content_version = 2;
        }
        let frame = composition.synchronize(extent, vec![layer]).unwrap();
        let delta = &frame.updates[0].deltas[0];
        let writes = delta
            .image_resources
            .iter()
            .filter_map(|update| match update {
                crate::render::ImageResourceDelta::Write(update) => Some(update.rect),
                crate::render::ImageResourceDelta::Remove(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(writes, rects);
    }

    #[test]
    fn one_retained_scene_can_be_placed_more_than_once() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let mut composition = DesktopComposition::new(extent);
        let layers = [10, 50]
            .into_iter()
            .enumerate()
            .map(|(index, x)| {
                DesktopLayer::retained(
                    DesktopLayerKey::LegacyControl(index as u32, 0),
                    DesktopSceneKey::LegacyControl(0),
                    Vec::new(),
                    SizeI {
                        width: 24,
                        height: 24,
                    },
                    PointI { x, y: 10 },
                    true,
                )
            })
            .collect();
        let frame = composition.synchronize(extent, layers).unwrap();
        assert_eq!(frame.live_scenes.len(), 1);
        assert_eq!(frame.placements.len(), 2);
        assert_eq!(frame.placements[0].scene, frame.placements[1].scene);
    }

    #[test]
    fn hidden_image_revision_is_not_consumed_before_its_pixels_arrive() {
        let extent = SizeI {
            width: 800,
            height: 600,
        };
        let position = PointI { x: 20, y: 30 };
        let mut composition = DesktopComposition::new(extent);
        let _ = composition.synchronize(
            extent,
            vec![image_layer(
                position,
                DesktopImageUpdate::Full(vec![255; 100 * 80 * 4].into()),
            )],
        );

        let mut hidden = image_layer(position, DesktopImageUpdate::Unchanged);
        hidden.visible = false;
        if let DesktopLayerContent::Image {
            content_version, ..
        } = &mut hidden.content
        {
            *content_version = 2;
        }
        let _ = composition.synchronize(extent, vec![hidden]);

        let rect = RectI {
            x: 4,
            y: 5,
            width: 8,
            height: 6,
        };
        let mut visible = image_layer(
            position,
            DesktopImageUpdate::Regions(vec![DesktopImageRegion {
                rect,
                row_bytes: rect.width as usize * 4,
                pixels: vec![128; rect.width as usize * rect.height as usize * 4].into(),
            }]),
        );
        if let DesktopLayerContent::Image {
            content_version, ..
        } = &mut visible.content
        {
            *content_version = 2;
        }
        let frame = composition.synchronize(extent, vec![visible]).unwrap();
        assert_eq!(frame.updates.len(), 1);
        assert_eq!(frame.updates[0].deltas[0].image_resources.len(), 1);
    }
}
