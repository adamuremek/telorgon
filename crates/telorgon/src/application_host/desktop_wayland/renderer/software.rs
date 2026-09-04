use std::collections::{BTreeMap, VecDeque};

use crate::core::{ColorRgba8, RectI};
use crate::render::RenderBackend;
use crate::renderer_software::{
    SoftwareCompositeLayer, SoftwareRenderer, SoftwareScene, SoftwareSurface,
};

use super::super::geometry::{accumulated_damage, full_rect};
use super::super::scene::{DesktopFrame, DesktopSceneKey};
use crate::application_host::{AppError, AppResult};

pub(in crate::application_host::desktop_wayland) struct SoftwareDesktopRenderer {
    renderer: SoftwareRenderer,
    scenes: BTreeMap<DesktopSceneKey, SoftwareScene>,
    surface: SoftwareSurface,
    content_version: u64,
    target_versions: Vec<u64>,
    damage_history: VecDeque<(u64, Option<RectI>)>,
}

impl SoftwareDesktopRenderer {
    pub(super) fn new(targets: usize) -> Self {
        Self {
            renderer: SoftwareRenderer,
            scenes: BTreeMap::new(),
            surface: SoftwareSurface::default(),
            content_version: 0,
            target_versions: vec![0; targets],
            damage_history: VecDeque::new(),
        }
    }

    pub(super) fn render(&mut self, target_index: usize, frame: DesktopFrame) -> AppResult<RectI> {
        self.content_version = self.content_version.wrapping_add(1).max(1);
        self.damage_history
            .push_back((self.content_version, frame.damage));
        while self.damage_history.len() > 64 {
            self.damage_history.pop_front();
        }
        for update in &frame.updates {
            let scene = match self.scenes.entry(update.key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                    self.renderer
                        .create_scene()
                        .map_err(|error| AppError::new(error.to_string()))?,
                ),
            };
            for delta in &update.deltas {
                self.renderer
                    .apply_scene_delta(scene, delta)
                    .map_err(|error| AppError::new(error.to_string()))?;
            }
        }
        self.scenes.retain(|key, _| frame.live_scenes.contains(key));
        let previous_target_version = *self
            .target_versions
            .get(target_index)
            .ok_or_else(|| AppError::new("software scanout target index is invalid"))?;
        let render_damage = accumulated_damage(
            previous_target_version,
            self.content_version,
            &self.damage_history,
            frame.extent,
        );
        let mut layers = Vec::with_capacity(frame.placements.len());
        for placement in &frame.placements {
            let scene = self.scenes.get(&placement.scene).ok_or_else(|| {
                AppError::new(format!(
                    "software desktop scene {:?} has no retained content",
                    placement.scene
                ))
            })?;
            layers.push(SoftwareCompositeLayer {
                scene,
                target: placement.target,
                clip: placement.clip,
                rounded_clips: placement.rounded_clips,
            });
        }
        self.renderer
            .render_composite(
                &mut self.surface,
                &layers,
                frame.extent,
                render_damage,
                ColorRgba8 {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            )
            .map_err(|error| AppError::new(error.to_string()))?;
        for scene in self.scenes.values_mut() {
            scene.discard_pending_damage();
        }
        Ok(render_damage.unwrap_or_else(|| full_rect(frame.extent)))
    }

    pub(super) fn pixels(&self) -> &[u8] {
        self.surface.pixels_rgba8()
    }

    pub(super) fn mark_copied(&mut self, target_index: usize) {
        self.target_versions[target_index] = self.content_version;
    }
}
