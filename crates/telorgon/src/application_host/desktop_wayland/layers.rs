use super::*;

pub(super) struct Layer {
    pub(super) runtime: ComposedAppRuntime,
    renderer: SoftwareRenderer,
    scene: SoftwareScene,
    surface: SoftwareSurface,
    content_version: u64,
    retained_pixels: Arc<[u8]>,
}

impl Layer {
    pub(super) fn new(
        driver: CompositionDriver,
        extent: SizeI,
        assets: AssetBundle,
    ) -> AppResult<Self> {
        let renderer = SoftwareRenderer;
        let scene = renderer.create_scene().map_err(app_error)?;
        let mut runtime = ComposedAppRuntime::from_composition_driver(driver, extent)?;
        let mut media = AssetMediaCache::new(assets).map_err(app_error)?;
        for resource in media.preload_render_resources().map_err(app_error)? {
            runtime.set_image_resource(resource)?;
        }
        Ok(Self {
            runtime,
            renderer,
            scene,
            surface: SoftwareSurface::default(),
            content_version: 0,
            retained_pixels: Arc::from([]),
        })
    }

    pub(super) fn render(&mut self, extent: SizeI, now: u64, force: bool) -> AppResult<&[u8]> {
        let extent_changed = self.runtime.extent()
            != (SizeF {
                width: extent.width as f32,
                height: extent.height as f32,
            });
        if extent_changed {
            self.runtime.resize(extent)?;
        }
        self.runtime
            .prepare_frame(MonotonicInstant::from_nanos(now), force)?;
        let mut changed = extent_changed || force;
        while let Some(delta) = self.runtime.pop_scene_delta() {
            changed = true;
            self.renderer
                .apply_scene_delta(&mut self.scene, &delta)
                .map_err(app_error)?;
        }
        if !changed && self.content_version != 0 {
            return Ok(self.surface.pixels_rgba8());
        }
        let target = SoftwareTarget::new(RenderTargetInfo::full(extent));
        let clear = self.scene.background();
        let mut frame = self.surface.begin_frame();
        self.renderer
            .render(
                &mut self.scene,
                &mut frame,
                &target,
                &RenderRequest {
                    force,
                    load: TargetLoad::Clear(clear),
                    store: TargetStore::Store,
                    region: None,
                },
            )
            .map_err(app_error)?;
        self.content_version = self.content_version.wrapping_add(1).max(1);
        self.retained_pixels = Arc::from(self.surface.pixels_rgba8());
        Ok(self.surface.pixels_rgba8())
    }

    pub(super) fn content_version(&self) -> u64 {
        self.content_version
    }

    pub(super) fn pixels(&self) -> Arc<[u8]> {
        Arc::clone(&self.retained_pixels)
    }

    pub(super) fn pointer_motion(&mut self, position: PointF, now: MonotonicInstant) -> bool {
        self.runtime
            .queue_input(crate::input::InputEvent::mouse_moved(position));
        self.runtime.flush_input(now).frame_needed_after
    }

    pub(super) fn pointer_button(&mut self, pressed: bool, now: MonotonicInstant) -> bool {
        self.runtime
            .queue_input(crate::input::InputEvent::mouse_button(
                crate::input::PointerButton::PRIMARY,
                if pressed {
                    crate::input::ButtonState::Pressed
                } else {
                    crate::input::ButtonState::Released
                },
            ));
        self.runtime.flush_input(now).frame_needed_after
    }

    fn has_pending_runtime_turn(&self, now: MonotonicInstant) -> bool {
        self.runtime.has_pending_runtime_turn(now)
            || (self.runtime.needs_frame() && !self.runtime.animation_active())
    }

    fn next_deadline(&self) -> Option<MonotonicInstant> {
        self.runtime.next_deadline()
    }

    fn animation_active(&self) -> bool {
        self.runtime.animation_active()
    }
}

pub(super) fn route_frame_pointer_motion(
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    position: PointF,
    session_locked: bool,
    now: MonotonicInstant,
) -> bool {
    let mut repaint = false;
    for (surface, frame) in frames {
        let local = windows
            .get(surface)
            .filter(|window| !window.minimized && !session_locked)
            .map_or(
                PointF {
                    x: -1_000_000.0,
                    y: -1_000_000.0,
                },
                |window| PointF {
                    x: position.x - window.position.x as f32,
                    y: position.y - window.position.y as f32,
                },
            );
        repaint |= frame.layer.pointer_motion(local, now);
    }
    repaint
}

pub(super) fn route_frame_pointer_button(
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    pressed: bool,
    now: MonotonicInstant,
) -> bool {
    frames.values_mut().fold(false, |repaint, frame| {
        repaint | frame.layer.pointer_button(pressed, now)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_window_frames(
    factory: Option<&WindowFrameFactory>,
    frames: &mut BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    wayland: &NativeCompositor<'_>,
    config: &LinuxDesktopConfig,
    assets: AssetBundle,
    fallback_icon: &crate::AppIconProfile,
    wake: &EventNotifier,
    now: u64,
) -> AppResult<()> {
    let Some(factory) = factory else {
        frames.clear();
        for window in windows.values_mut() {
            window.chrome_outer = None;
            window.chrome_content_offset = None;
            window.chrome = None;
        }
        return Ok(());
    };

    frames.retain(|surface, _| {
        windows.get(surface).is_some_and(|window| {
            window.role == SurfaceRole::XdgToplevel && window_is_decorated(window)
        })
    });

    let active = wayland
        .core()
        .seats
        .get(&1)
        .and_then(|seat| seat.keyboard_focus)
        .map(|focus| focus.surface);
    let surfaces = windows
        .iter()
        .filter(|(_, window)| {
            window.role == SurfaceRole::XdgToplevel && window_is_decorated(window)
        })
        .map(|(surface, _)| *surface)
        .collect::<Vec<_>>();
    let mut updates = Vec::with_capacity(surfaces.len());

    for surface in surfaces {
        let window = windows
            .get(&surface)
            .expect("frame candidates came from live windows");
        let metadata = wayland.toplevel_metadata(surface);
        let title = metadata.map_or_else(String::new, |metadata| {
            if metadata.title.is_empty() {
                metadata.application_id.clone()
            } else {
                metadata.title.clone()
            }
        });
        let state = if window.fullscreen {
            WindowChromeState::Fullscreen
        } else if window.maximized {
            WindowChromeState::Maximized
        } else {
            WindowChromeState::Normal
        };
        let protocol_icon = wayland.toplevel_icon(surface);
        let icon_name = protocol_icon
            .and_then(|icon| icon.name.clone())
            .or_else(|| fallback_icon.name().map(str::to_owned));
        let icon_image = protocol_icon.and_then(|icon| {
            icon.images
                .iter()
                .min_by_key(|image| {
                    let logical = image.image.descriptor.size.width / image.scale.max(1);
                    logical.abs_diff(32)
                })
                .map(|image| (icon.revision, image.clone()))
        });
        let icon_image_id = icon_image
            .as_ref()
            .map(|(revision, _)| toplevel_icon_image_id(surface, *revision));
        let mut model = WindowChromeModel::new(u64::from(surface.get()), title)
            .state(state)
            .active(active == Some(surface));
        if let Some(name) = icon_name {
            model = model.app_icon_name(name);
        }
        if let Some(image) = icon_image_id {
            model = model.app_icon_image(image);
        } else if let Some(icon) = fallback_icon.preferred(32) {
            model = model.app_icon(icon);
        }
        let fallback_outer = legacy_window_outer(window, config);
        let previous_outer = frames
            .get(&surface)
            .map_or(fallback_outer, |frame| frame.outer);
        let rebuild = frames
            .get(&surface)
            .is_none_or(|frame| frame.model != model);
        if rebuild {
            let mut driver = factory.compose(model.clone());
            driver.set_wake({
                let wake = wake.clone();
                move || wake.notify()
            });
            let mut layer = Layer::new(driver, previous_outer, assets)?;
            if let Some((revision, icon)) = &icon_image {
                let mut resource =
                    shm_image_resource(icon.buffer, (*revision).max(1), icon.image.clone())
                        .map_err(app_error)?;
                resource.image = icon_image_id.expect("image source produced an image id");
                resource.content_version = (*revision).max(1);
                layer.runtime.set_image_resource(resource)?;
            }
            frames.insert(
                surface,
                WindowFrameLayer {
                    model: model.clone(),
                    layer,
                    snapshot: None,
                    outer: previous_outer,
                    pixels: Arc::from([]),
                    content_version: 0,
                },
            );
        }

        let frame = frames
            .get_mut(&surface)
            .expect("window frame was created above");
        frame.model = model;
        frame.layer.render(frame.outer, now, rebuild)?;
        let mut snapshot =
            WindowChromeSnapshot::derive(frame.layer.runtime.ui(), frame.layer.runtime.layout())
                .map_err(app_error)?;
        let content_width = snapshot.content.bounds.width.round().max(1.0) as i32;
        let content_height = snapshot.content.bounds.height.round().max(1.0) as i32;
        let corrected = SizeI {
            width: frame
                .outer
                .width
                .saturating_add(window.requested_size.width.saturating_sub(content_width))
                .max(1),
            height: frame
                .outer
                .height
                .saturating_add(window.requested_size.height.saturating_sub(content_height))
                .max(1),
        };
        if corrected != frame.outer {
            frame.outer = corrected;
            frame.layer.render(frame.outer, now, true)?;
            snapshot = WindowChromeSnapshot::derive(
                frame.layer.runtime.ui(),
                frame.layer.runtime.layout(),
            )
            .map_err(app_error)?;
        }
        frame.snapshot = Some(snapshot.clone());
        frame.layer.render(frame.outer, now, false)?;
        frame.pixels = frame.layer.pixels();
        frame.content_version = frame.layer.content_version();
        updates.push((
            surface,
            frame.outer,
            PointI {
                x: snapshot.content.bounds.x.round() as i32,
                y: snapshot.content.bounds.y.round() as i32,
            },
            snapshot,
        ));
    }

    for (surface, outer, content_offset, snapshot) in updates {
        if let Some(window) = windows.get_mut(&surface) {
            window.chrome_outer = Some(outer);
            window.chrome_content_offset = Some(content_offset);
            window.chrome = Some(snapshot);
        }
    }
    Ok(())
}

fn toplevel_icon_image_id(surface: WaylandSurfaceId, revision: u64) -> ImageId {
    let folded = revision as u32 ^ (revision >> 32) as u32;
    ImageId(0x6000_0000_u32 ^ surface.get().rotate_left(11) ^ folded.rotate_left(3))
}

pub(super) struct WidgetLayer {
    pub(super) anchor: ShellWidgetAnchor,
    pub(super) width: ShellWidgetExtent,
    pub(super) height: ShellWidgetExtent,
    pub(super) reserved_space: i32,
    pub(super) layer: Layer,
}

pub(super) struct WindowFrameLayer {
    pub(super) model: WindowChromeModel,
    pub(super) layer: Layer,
    pub(super) snapshot: Option<WindowChromeSnapshot>,
    pub(super) outer: SizeI,
    pub(super) pixels: Arc<[u8]>,
    pub(super) content_version: u64,
}

pub(super) fn desktop_runtime_schedule(
    background: &Layer,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    pointer: Option<&Layer>,
    icons: &[(String, Layer)],
    widgets: &[WidgetLayer],
    now: MonotonicInstant,
) -> (bool, Option<MonotonicInstant>, bool) {
    let layers = std::iter::once(background)
        .chain(frames.values().map(|frame| &frame.layer))
        .chain(pointer)
        .chain(icons.iter().map(|(_, layer)| layer))
        .chain(widgets.iter().map(|widget| &widget.layer));
    let mut immediate = false;
    let mut deadline = None::<MonotonicInstant>;
    let mut animation = false;
    for layer in layers {
        immediate |= layer.has_pending_runtime_turn(now);
        animation |= layer.animation_active();
        if let Some(candidate) = layer.next_deadline() {
            deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
        }
    }
    (immediate, deadline, animation)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_desktop_layers(
    session_locked: bool,
    extent: SizeI,
    now: u64,
    first_modeset: bool,
    background: &mut Layer,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    widgets: &mut [WidgetLayer],
    icons: &mut [(String, Layer)],
    drag_icon: Option<WaylandSurfaceId>,
    drag_position: PointF,
    cursor: Option<&RenderedCursor>,
    pointer_position: PointF,
    config: &LinuxDesktopConfig,
) -> AppResult<Vec<DesktopLayer>> {
    let mut layers = Vec::new();
    if !session_locked {
        background.render(extent, now, first_modeset)?;
        let pixels = background.pixels();
        layers.push(DesktopLayer {
            key: DesktopLayerKey::Background,
            content_version: background.content_version(),
            pixels,
            extent,
            position: PointI::default(),
            clip: None,
            alpha_mode: ImageAlphaMode::Opaque,
            damage: None,
        });
    }

    let placements = windows
        .iter()
        .map(|(surface, window)| {
            let position = window
                .parent
                .and_then(|parent| windows.get(&parent))
                .map_or(window.position, |parent| PointI {
                    x: parent.position.x + window.offset.x,
                    y: parent.position.y + window.offset.y,
                });
            (*surface, position)
        })
        .collect::<BTreeMap<_, _>>();
    for surface in stacking_order {
        let Some(window) = windows.get(surface) else {
            continue;
        };
        if matches!(window.role, SurfaceRole::Cursor | SurfaceRole::DragIcon)
            || window.minimized
            || (window.role == SurfaceRole::SessionLock) != session_locked
        {
            continue;
        }
        let position = placements.get(surface).copied().unwrap_or(window.position);
        let outer = window
            .chrome_outer
            .unwrap_or_else(|| legacy_window_outer(window, config));
        if window_is_decorated(window)
            && let Some(frame) = frames.get(surface)
        {
            layers.push(DesktopLayer {
                key: DesktopLayerKey::Frame(surface.get()),
                content_version: frame.content_version,
                pixels: Arc::clone(&frame.pixels),
                extent: frame.outer,
                position,
                clip: None,
                alpha_mode: ImageAlphaMode::Straight,
                damage: None,
            });
        }
        if window_is_decorated(window)
            && window.chrome.is_none()
            && window.role == SurfaceRole::XdgToplevel
        {
            let icon_extent = SizeI {
                width: config.titlebar_height.clamp(1, 24),
                height: config.titlebar_height.clamp(1, 24),
            };
            for (index, name) in ["window.close", "window.maximize", "window.minimize"]
                .into_iter()
                .enumerate()
            {
                let Some((_, icon)) = icons.iter_mut().find(|(candidate, _)| candidate == name)
                else {
                    continue;
                };
                icon.render(icon_extent, now, false)?;
                let pixels = icon.pixels();
                layers.push(DesktopLayer {
                    key: DesktopLayerKey::LegacyControl(surface.get(), index as u8),
                    content_version: icon.content_version(),
                    pixels,
                    extent: icon_extent,
                    position: PointI {
                        x: position.x + outer.width
                            - config.window_border
                            - (index as i32 + 1) * (icon_extent.width + 4),
                        y: position.y
                            + config.window_border
                            + (config.titlebar_height - icon_extent.height) / 2,
                    },
                    clip: None,
                    alpha_mode: ImageAlphaMode::Premultiplied,
                    damage: None,
                });
            }
        }
        let content_offset = window_content_offset(window, config);
        let preview_position = PointI {
            x: position.x + content_offset.x,
            y: position.y + content_offset.y,
        };
        let mut content_position = preview_position;
        if let Some(anchor) = window.resize_anchor {
            let offset = anchor.committed_buffer_offset(window.requested_size, window.size);
            content_position.x = content_position.x.saturating_add(offset.x);
            content_position.y = content_position.y.saturating_add(offset.y);
        }
        let preview_clip = (window.role == SurfaceRole::XdgToplevel).then_some(RectI {
            x: preview_position.x,
            y: preview_position.y,
            width: window.requested_size.width,
            height: window.requested_size.height,
        });
        layers.push(DesktopLayer {
            key: DesktopLayerKey::Surface(surface.get()),
            content_version: window.revision,
            pixels: Arc::clone(&window.rgba),
            extent: window.size,
            position: content_position,
            clip: preview_clip,
            alpha_mode: window.alpha_mode,
            damage: window.damage,
        });
    }

    if !session_locked {
        for (index, widget) in widgets.iter_mut().enumerate() {
            let widget_extent = resolved_widget_extent(extent, widget.width, widget.height);
            let position = widget_position(extent, widget_extent, widget.anchor);
            widget.layer.render(widget_extent, now, false)?;
            let pixels = widget.layer.pixels();
            layers.push(DesktopLayer {
                key: DesktopLayerKey::Widget(index as u32),
                content_version: widget.layer.content_version(),
                pixels,
                extent: widget_extent,
                position,
                clip: None,
                alpha_mode: ImageAlphaMode::Straight,
                damage: None,
            });
        }
        if let Some((surface, icon)) =
            drag_icon.and_then(|surface| windows.get(&surface).map(|icon| (surface, icon)))
        {
            layers.push(DesktopLayer {
                key: DesktopLayerKey::DragIcon(surface.get()),
                content_version: icon.revision,
                pixels: Arc::clone(&icon.rgba),
                extent: icon.size,
                position: PointI {
                    x: drag_position.x.round() as i32,
                    y: drag_position.y.round() as i32,
                },
                clip: None,
                alpha_mode: icon.alpha_mode,
                damage: icon.damage,
            });
        }
    }
    if let Some(cursor) = cursor {
        layers.push(DesktopLayer {
            key: DesktopLayerKey::Cursor,
            content_version: cursor_image_signature(cursor),
            pixels: Arc::from(cursor.rgba.as_slice()),
            extent: cursor.size,
            position: PointI {
                x: pointer_position.x.round() as i32 - cursor.hotspot.x,
                y: pointer_position.y.round() as i32 - cursor.hotspot.y,
            },
            clip: None,
            alpha_mode: if cursor.premultiplied {
                ImageAlphaMode::Premultiplied
            } else {
                ImageAlphaMode::Straight
            },
            damage: None,
        });
    }
    Ok(layers)
}
