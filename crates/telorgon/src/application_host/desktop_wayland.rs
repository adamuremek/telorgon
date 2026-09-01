use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::compositor_render::{shm_image_resource, transform_surface_image};
use crate::compositor_wayland::{
    ButtonState as WaylandButtonState, ClientLimits, CompositorAction, CursorImage,
    NativeCompositor, OutputDescription, OutputMode, OutputState, OutputTransform,
    PointerConstraintKind, PointerConstraintState, ResizeEdge, SeatCapabilities,
    SeatState as WaylandSeatState, SurfaceRole, ToplevelState, WaylandSurfaceId,
};
use crate::core::{MonotonicInstant, PointF, PointI, RectI, SizeF, SizeI};
use crate::platform_linux::{
    KeyDirection, LibInputContext, LinuxInputEventKind, LinuxSeat, SeatState, XkbKeyboard,
};
use crate::presenter_vulkan_kms::{
    ConnectorStatus, DRM_FORMAT_MOD_LINEAR, DRM_FORMAT_XRGB8888, GbmDevice, KmsCrtcId, KmsDevice,
    KmsPropertyObject, KmsTopology, ScanoutFormat,
};
use crate::render::{
    BatchKey, BlendMode, DrawItem, ImageAlphaMode, ImageColorEncoding, ImageId, ImageInstance,
    ImageResource, PipelineKind, PrimitiveKind, RenderBackend, RenderRequest, RenderScene,
    RenderTargetInfo, TargetLoad, TargetStore,
};
use crate::renderer_software::{SoftwareRenderer, SoftwareScene, SoftwareSurface, SoftwareTarget};
use crate::renderer_vulkan::{
    DeviceSelection, VulkanConfig, VulkanDevice, VulkanDmaBufScanoutTarget, VulkanInstance,
    VulkanScene,
};
use crate::runtime::CompositionDriver;
use crate::scene::NodeId;
use crate::wayland_server::{Display, ProtocolCatalog, ProtocolSourcePaths};

use crate::application_host::{
    AppError, AppResult, ComposedAppRuntime, LinuxDesktopConfig, ReadyDesktopEnvironment, Renderer,
    ShellWidgetAnchor, ShellWidgetExtent,
};

struct Layer {
    runtime: ComposedAppRuntime,
    renderer: SoftwareRenderer,
    scene: SoftwareScene,
    surface: SoftwareSurface,
}

struct VulkanScanout {
    device: VulkanDevice,
    scene: VulkanScene,
    source: RenderScene,
    targets: Vec<VulkanDmaBufScanoutTarget>,
    content_version: u64,
}

impl VulkanScanout {
    fn new(
        buffers: &[crate::presenter_vulkan_kms::GbmBuffer<'_, '_>],
        extent: SizeI,
    ) -> AppResult<Self> {
        let frame_bytes = u64::try_from(extent.width)
            .ok()
            .and_then(|width| {
                u64::try_from(extent.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| AppError::new("Vulkan scanout extent overflows its upload budget"))?;
        let config = VulkanConfig {
            enable_validation: false,
            frames_in_flight: buffers.len().max(2),
            staging_budget_bytes: frame_bytes.max(4 * 1024 * 1024),
            ..VulkanConfig::default()
        };
        let instance = VulkanInstance::load(&config, &[]).map_err(app_error)?;
        let mut adapters = instance.adapters().map_err(app_error)?;
        adapters.sort_by_key(|adapter| std::cmp::Reverse(adapter.score));
        let mut failures = Vec::new();
        for adapter in adapters.into_iter().filter(|adapter| adapter.supported) {
            let selection = DeviceSelection {
                adapter_index: adapter.index,
            };
            let device =
                match VulkanDevice::create_owned(instance.clone(), &config, &selection, None) {
                    Ok(device) => device,
                    Err(error) => {
                        failures.push(format!("{}: {error}", adapter.name));
                        continue;
                    }
                };
            let targets = buffers
                .iter()
                .map(|buffer| {
                    let format = buffer.format();
                    let mut planes = buffer.export_planes().map_err(app_error)?;
                    if planes.len() != 1 {
                        return Err(AppError::new(
                            "Vulkan scanout currently requires one GBM DMA-BUF plane",
                        ));
                    }
                    let plane = planes.pop().expect("one plane checked");
                    unsafe {
                        VulkanDmaBufScanoutTarget::import(
                            &device,
                            plane.fd,
                            format.fourcc,
                            format.modifier,
                            buffer.size(),
                            u64::from(plane.offset),
                            plane.stride,
                        )
                    }
                    .map_err(app_error)
                })
                .collect::<AppResult<Vec<_>>>();
            let targets = match targets {
                Ok(targets) => targets,
                Err(error) => {
                    failures.push(format!("{}: {error}", adapter.name));
                    continue;
                }
            };
            let scene = device.create_scene().map_err(app_error)?;
            let mut source = RenderScene::default();
            source.extent = SizeF {
                width: extent.width as f32,
                height: extent.height as f32,
            };
            let node = NodeId::new(0, 1);
            let rectangle = crate::core::RectF {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
            };
            source.images.upsert(
                node,
                ImageInstance {
                    node,
                    image: ImageId(1),
                    rect: rectangle,
                    view_bounds: rectangle,
                    content_version: 1,
                    opacity: 1.0,
                    clip: crate::render::ClipId(0),
                    spatial: crate::render::SpatialId(0),
                },
            );
            source.set_draw_order(vec![DrawItem {
                kind: PrimitiveKind::Image,
                index: 0,
                batch: BatchKey {
                    pipeline: PipelineKind::Image,
                    resource: 1,
                    clip: crate::render::ClipId(0),
                    blend: BlendMode::Opaque,
                    target: 0,
                },
            }]);
            return Ok(Self {
                device,
                scene,
                source,
                targets,
                content_version: 0,
            });
        }
        Err(AppError::new(if failures.is_empty() {
            "no supported Vulkan adapter was found".to_owned()
        } else {
            format!(
                "no Vulkan adapter could import the KMS scanout buffers: {}",
                failures.join("; ")
            )
        }))
    }

    fn render(&mut self, target_index: usize, extent: SizeI, rgba: Vec<u8>) -> AppResult<()> {
        self.content_version = self.content_version.wrapping_add(1).max(1);
        self.source
            .set_image_resource(ImageResource {
                image: ImageId(1),
                content_version: self.content_version,
                extent,
                color_encoding: ImageColorEncoding::Srgb,
                alpha_mode: ImageAlphaMode::Opaque,
                pixels_rgba8: Arc::from(rgba),
            })
            .map_err(app_error)?;
        let delta = self
            .source
            .take_delta()
            .ok_or_else(|| AppError::new("Vulkan scanout scene produced no frame delta"))?;
        self.device
            .apply_scene_delta(&mut self.scene, &delta)
            .map_err(app_error)?;
        let target = self
            .targets
            .get_mut(target_index)
            .ok_or_else(|| AppError::new("Vulkan scanout target index is invalid"))?;
        let target = target.target();
        let mut frame = self.device.begin_owned_frame().map_err(app_error)?;
        {
            let mut context = frame.context_mut();
            self.device
                .render(
                    &mut self.scene,
                    &mut context,
                    &target,
                    &RenderRequest {
                        force: true,
                        load: TargetLoad::Clear(self.source.background),
                        store: TargetStore::Store,
                        region: None,
                    },
                )
                .map_err(app_error)?;
        }
        let mut receipt = frame
            .finish()
            .and_then(|frame| frame.submit())
            .map_err(app_error)?;
        receipt.wait(Duration::from_secs(2)).map_err(app_error)?;
        Ok(())
    }
}

impl Layer {
    fn new(driver: CompositionDriver, extent: SizeI) -> AppResult<Self> {
        let renderer = SoftwareRenderer;
        let scene = renderer.create_scene().map_err(app_error)?;
        Ok(Self {
            runtime: ComposedAppRuntime::from_composition_driver(driver, extent)?,
            renderer,
            scene,
            surface: SoftwareSurface::default(),
        })
    }

    fn render(&mut self, extent: SizeI, now: u64, force: bool) -> AppResult<&[u8]> {
        if self.runtime.extent()
            != (SizeF {
                width: extent.width as f32,
                height: extent.height as f32,
            })
        {
            self.runtime.resize(extent)?;
        }
        self.runtime
            .prepare_frame(MonotonicInstant::from_nanos(now), force)?;
        while let Some(delta) = self.runtime.pop_scene_delta() {
            self.renderer
                .apply_scene_delta(&mut self.scene, &delta)
                .map_err(app_error)?;
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
        Ok(self.surface.pixels_rgba8())
    }
}

struct WidgetLayer {
    anchor: ShellWidgetAnchor,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
    reserved_space: i32,
    layer: Layer,
}

struct ClientWindow {
    revision: u64,
    role: SurfaceRole,
    parent: Option<WaylandSurfaceId>,
    offset: PointI,
    server_decorated: bool,
    position: PointI,
    size: SizeI,
    requested_size: SizeI,
    restore_geometry: Option<(PointI, SizeI)>,
    maximized: bool,
    fullscreen: bool,
    minimized: bool,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
enum WindowInteraction {
    Move {
        surface: WaylandSurfaceId,
    },
    Resize {
        surface: WaylandSurfaceId,
        edge: ResizeEdge,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecorationHit {
    Titlebar,
    Resize(ResizeEdge),
    Close,
    Maximize,
    Minimize,
}

pub(crate) fn run(application: ReadyDesktopEnvironment) -> AppResult<()> {
    let (_name, compositor, widgets, renderer, config) = application.into_parts()?;
    let (policy, window_frame, pointer, icons) = compositor.into_runtime_parts();

    let seat = LinuxSeat::open().map_err(app_error)?;
    seat.dispatch(0).map_err(app_error)?;
    let input = LibInputContext::new(&seat, &config.seat_name).map_err(app_error)?;
    let drm_path = config
        .drm_device
        .to_str()
        .ok_or_else(|| AppError::new("DRM device path is not UTF-8"))?;
    let drm_seat_device = seat.open_device(drm_path).map_err(app_error)?;
    let kms =
        KmsDevice::new(drm_seat_device.try_clone_fd().map_err(app_error)?).map_err(app_error)?;
    let topology = KmsTopology::query(&kms).map_err(app_error)?;
    let connector = topology
        .connectors
        .iter()
        .find(|connector| {
            connector.status == ConnectorStatus::Connected && !connector.modes.is_empty()
        })
        .ok_or_else(|| AppError::new("no connected KMS output with a display mode was found"))?;
    let mode_index = connector
        .modes
        .iter()
        .position(|mode| mode.preferred())
        .unwrap_or(0);
    let mode = connector.modes[mode_index];
    let extent = mode.size();
    let (crtc_index, crtc_raw) = topology
        .crtcs
        .iter()
        .copied()
        .enumerate()
        .find(|(index, _)| {
            let mask = 1_u32.checked_shl(*index as u32).unwrap_or(0);
            connector.possible_crtcs_mask & mask != 0
                && topology.planes.iter().any(|plane| {
                    plane.possible_crtcs_mask & mask != 0
                        && plane.formats.contains(&DRM_FORMAT_XRGB8888)
                        && is_primary_plane(&kms, plane.id.get())
                })
        })
        .ok_or_else(|| AppError::new("no KMS CRTC has an XRGB8888 primary-plane candidate"))?;
    let crtc = KmsCrtcId::from_raw(crtc_raw)
        .ok_or_else(|| AppError::new("KMS returned CRTC identity zero"))?;
    let plane = topology
        .planes
        .iter()
        .find(|plane| {
            plane.possible_crtcs_mask & (1_u32.checked_shl(crtc_index as u32).unwrap_or(0)) != 0
                && plane.formats.contains(&DRM_FORMAT_XRGB8888)
                && is_primary_plane(&kms, plane.id.get())
        })
        .ok_or_else(|| AppError::new("no compatible KMS plane was found"))?;
    let connector_properties =
        KmsTopology::object_properties(&kms, connector.id.get(), KmsPropertyObject::Connector)
            .map_err(app_error)?;
    let crtc_properties = KmsTopology::object_properties(&kms, crtc.get(), KmsPropertyObject::Crtc)
        .map_err(app_error)?;
    let plane_properties =
        KmsTopology::object_properties(&kms, plane.id.get(), KmsPropertyObject::Plane)
            .map_err(app_error)?;
    let mode_blob = kms.create_mode_blob(&mode).map_err(app_error)?;
    let gbm = GbmDevice::new(kms.fd()).map_err(app_error)?;
    let scanout_format = ScanoutFormat {
        fourcc: DRM_FORMAT_XRGB8888,
        modifier: DRM_FORMAT_MOD_LINEAR,
    };
    let mut scanout_buffers = [
        gbm.allocate(extent, scanout_format, &[DRM_FORMAT_MOD_LINEAR])
            .map_err(app_error)?,
        gbm.allocate(extent, scanout_format, &[DRM_FORMAT_MOD_LINEAR])
            .map_err(app_error)?,
    ];
    let framebuffers = scanout_buffers
        .iter()
        .map(|buffer| kms.add_framebuffer(buffer).map_err(app_error))
        .collect::<AppResult<Vec<_>>>()?;
    let mut vulkan_scanout = match renderer {
        Renderer::Vulkan => Some(VulkanScanout::new(&scanout_buffers, extent)?),
        Renderer::Auto => VulkanScanout::new(&scanout_buffers, extent).ok(),
        Renderer::Software => None,
    };

    let display = Display::new().map_err(app_error)?;
    let catalog =
        ProtocolCatalog::load_desktop(&ProtocolSourcePaths::standard_linux()).map_err(app_error)?;
    let mut wayland =
        NativeCompositor::new(&display, catalog, ClientLimits::default()).map_err(app_error)?;
    wayland
        .add_output(&display, 1, output_state(connector, mode_index, &config)?)
        .map_err(app_error)?;
    wayland
        .add_seat(
            &display,
            1,
            WaylandSeatState::new(
                &config.seat_name,
                SeatCapabilities {
                    pointer: true,
                    keyboard: true,
                    touch: true,
                },
            ),
        )
        .map_err(app_error)?;
    let _socket = match config.socket_name.as_deref() {
        Some(name) => {
            display.add_socket(name).map_err(app_error)?;
            name.to_owned()
        }
        None => display.add_socket_auto().map_err(app_error)?,
    };

    let keyboard = XkbKeyboard::from_names(None, None, None, None, None).map_err(app_error)?;
    let keymap = keyboard.keymap_file().map_err(app_error)?;
    wayland
        .keyboard_keymap(1, keymap.fd(), keymap.size())
        .map_err(app_error)?;

    let mut policy = Layer::new(policy, extent)?;
    let mut frame = window_frame
        .map(|driver| Layer::new(driver, extent))
        .transpose()?;
    let mut pointer = pointer
        .map(|driver| Layer::new(driver, config.pointer_extent))
        .transpose()?;
    let mut icon_layers = icons
        .into_iter()
        .map(|(name, driver)| {
            Ok((
                name,
                Layer::new(
                    driver,
                    SizeI {
                        width: 24,
                        height: 24,
                    },
                )?,
            ))
        })
        .collect::<AppResult<Vec<_>>>()?;
    // Mount icon components immediately so semantic icon declarations are validated and retained.
    for (_, icon) in &mut icon_layers {
        let _ = icon.render(
            SizeI {
                width: 24,
                height: 24,
            },
            0,
            true,
        )?;
    }
    let mut widgets = widgets
        .into_iter()
        .map(|widget| {
            let (_, anchor, width, height, reserved_space, driver) = widget.into_runtime_parts();
            let widget_extent = resolved_widget_extent(extent, width, height);
            Ok(WidgetLayer {
                anchor,
                width,
                height,
                reserved_space: reserved_space.round().max(0.0) as i32,
                layer: Layer::new(driver, widget_extent)?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let work_area = shell_work_area(extent, &widgets);

    let start = Instant::now();
    let mut pointer_position = PointF {
        x: extent.width as f32 * 0.5,
        y: extent.height as f32 * 0.5,
    };
    let mut drag_position = pointer_position;
    let mut pointer_focus = None;
    let mut touch_targets = BTreeMap::<i32, WaylandSurfaceId>::new();
    let mut windows = BTreeMap::<WaylandSurfaceId, ClientWindow>::new();
    let mut stacking_order = Vec::<WaylandSurfaceId>::new();
    let mut session_locked = false;
    let mut pending_session_lock = None;
    let mut window_interaction = None;
    let mut next_window_offset = 0_i32;
    let mut scanout_index = 0_usize;
    let mut first_modeset = true;
    let mut repaint = true;
    let mut keyboard = keyboard;

    loop {
        display
            .event_loop()
            .dispatch(Some(Duration::from_millis(8)))
            .map_err(app_error)?;
        seat.dispatch(0).map_err(app_error)?;
        if seat.state() == SeatState::Enabled {
            input.dispatch().map_err(app_error)?;
            while let Some(event) = input.next_event() {
                let time_microseconds = event.time_microseconds;
                let time = time_microseconds / 1_000;
                let time = u32::try_from(time).unwrap_or(u32::MAX);
                match event.kind {
                    LinuxInputEventKind::PointerMotion {
                        delta,
                        unaccelerated,
                    } => {
                        let constraint =
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                None
                            } else {
                                wayland.pointer_constraint(1)
                            };
                        let locked = constraint.as_ref().is_some_and(|constraint| {
                            constraint.kind == PointerConstraintKind::Locked
                        });
                        if !locked {
                            let proposed = PointF {
                                x: (pointer_position.x + delta.x)
                                    .clamp(0.0, extent.width as f32 - 1.0),
                                y: (pointer_position.y + delta.y)
                                    .clamp(0.0, extent.height as f32 - 1.0),
                            };
                            pointer_position = constraint.as_ref().map_or(proposed, |constraint| {
                                constrain_pointer(
                                    &windows,
                                    pointer_position,
                                    proposed,
                                    constraint,
                                    &config,
                                )
                            });
                            if let Some(interaction) = window_interaction {
                                apply_window_interaction(
                                    &mut wayland,
                                    &mut windows,
                                    interaction,
                                    delta,
                                    extent,
                                    &config,
                                )?;
                            } else {
                                route_pointer_motion(
                                    &display,
                                    &mut wayland,
                                    &windows,
                                    &stacking_order,
                                    session_locked,
                                    &mut pointer_focus,
                                    pointer_position,
                                    time,
                                    &config,
                                )?;
                            }
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                drag_position = pointer_position;
                            }
                        }
                        if pointer_focus.is_some()
                            && window_interaction.is_none()
                            && (!wayland.drag_active(1) || wayland.drag_touch_slot(1).is_some())
                        {
                            let _ = wayland.relative_pointer_motion(
                                1,
                                time_microseconds,
                                delta,
                                unaccelerated,
                            );
                        }
                        repaint = true;
                    }
                    LinuxInputEventKind::PointerAbsolute { normalized } => {
                        let proposed = PointF {
                            x: normalized.x.clamp(0.0, 1.0) * (extent.width - 1) as f32,
                            y: normalized.y.clamp(0.0, 1.0) * (extent.height - 1) as f32,
                        };
                        let constraint =
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                None
                            } else {
                                wayland.pointer_constraint(1)
                            };
                        if !constraint.as_ref().is_some_and(|constraint| {
                            constraint.kind == PointerConstraintKind::Locked
                        }) {
                            let delta = PointF {
                                x: proposed.x - pointer_position.x,
                                y: proposed.y - pointer_position.y,
                            };
                            pointer_position = constraint.as_ref().map_or(proposed, |constraint| {
                                constrain_pointer(
                                    &windows,
                                    pointer_position,
                                    proposed,
                                    constraint,
                                    &config,
                                )
                            });
                            if let Some(interaction) = window_interaction {
                                apply_window_interaction(
                                    &mut wayland,
                                    &mut windows,
                                    interaction,
                                    delta,
                                    extent,
                                    &config,
                                )?;
                            } else {
                                route_pointer_motion(
                                    &display,
                                    &mut wayland,
                                    &windows,
                                    &stacking_order,
                                    session_locked,
                                    &mut pointer_focus,
                                    pointer_position,
                                    time,
                                    &config,
                                )?;
                            }
                            if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
                                drag_position = pointer_position;
                            }
                        }
                        repaint = true;
                    }
                    LinuxInputEventKind::PointerButton { button, pressed } => {
                        if pressed
                            && button == 0x110
                            && !session_locked
                            && let Some((surface, hit)) = hit_test_decoration(
                                &windows,
                                &stacking_order,
                                pointer_position,
                                &config,
                                &icon_layers,
                            )
                        {
                            focus_toplevel(&display, &mut wayland, &windows, Some(surface))?;
                            match hit {
                                DecorationHit::Titlebar => {
                                    window_interaction = Some(WindowInteraction::Move { surface });
                                }
                                DecorationHit::Resize(edge) => {
                                    window_interaction =
                                        Some(WindowInteraction::Resize { surface, edge });
                                }
                                DecorationHit::Close => {
                                    wayland.close_toplevel(surface).map_err(app_error)?;
                                }
                                DecorationHit::Maximize => {
                                    let maximized = windows
                                        .get(&surface)
                                        .is_some_and(|window| !window.maximized);
                                    set_window_maximized(
                                        &mut wayland,
                                        &mut windows,
                                        surface,
                                        maximized,
                                        work_area,
                                        &config,
                                    )?;
                                }
                                DecorationHit::Minimize => {
                                    if let Some(window) = windows.get_mut(&surface) {
                                        window.minimized = true;
                                        stacking_order.retain(|candidate| *candidate != surface);
                                    }
                                }
                            }
                            repaint = true;
                            continue;
                        }
                        if pointer_focus.is_some() {
                            let serial = display.next_serial();
                            wayland
                                .pointer_button(
                                    1,
                                    time,
                                    button,
                                    if pressed {
                                        WaylandButtonState::Pressed
                                    } else {
                                        WaylandButtonState::Released
                                    },
                                    serial,
                                )
                                .map_err(app_error)?;
                            if pressed {
                                focus_toplevel(&display, &mut wayland, &windows, pointer_focus)?;
                            }
                        }
                        if !pressed
                            && wayland.drag_active(1)
                            && wayland.drag_touch_slot(1).is_none()
                            && wayland
                                .core()
                                .seats
                                .get(&1)
                                .is_some_and(|seat| seat.pressed_buttons().is_empty())
                        {
                            wayland.drop_drag(1).map_err(app_error)?;
                            update_pointer_focus(
                                &display,
                                &mut wayland,
                                &windows,
                                &stacking_order,
                                session_locked,
                                &mut pointer_focus,
                                pointer_position,
                                &config,
                            )?;
                        }
                        if !pressed && let Some(interaction) = window_interaction.take() {
                            finish_window_interaction(&mut wayland, &windows, interaction)?;
                        }
                    }
                    LinuxInputEventKind::KeyboardKey { keycode, pressed } => {
                        let direction = if pressed {
                            KeyDirection::Down
                        } else {
                            KeyDirection::Up
                        };
                        keyboard.update_key(keycode, direction);
                        if pointer_focus.is_some() {
                            let serial = display.next_serial();
                            let _ = wayland.keyboard_key(
                                1,
                                time,
                                keycode,
                                if pressed {
                                    WaylandButtonState::Pressed
                                } else {
                                    WaylandButtonState::Released
                                },
                                serial,
                            );
                            let modifiers = keyboard.modifiers();
                            let _ = wayland.keyboard_modifiers(
                                1,
                                serial,
                                modifiers.depressed,
                                modifiers.latched,
                                modifiers.locked,
                                modifiers.group,
                            );
                        }
                    }
                    LinuxInputEventKind::PointerAxis {
                        horizontal,
                        vertical,
                        discrete_x,
                        discrete_y,
                    } => {
                        if pointer_focus.is_some() {
                            let _ = wayland.pointer_axis(
                                1, time, horizontal, vertical, discrete_x, discrete_y,
                            );
                        }
                    }
                    LinuxInputEventKind::TouchDown { slot, normalized } => {
                        if slot >= 0 {
                            let position = normalized_output_position(normalized, extent);
                            if let Some(surface) = hit_test_surface(
                                &windows,
                                &stacking_order,
                                position,
                                &config,
                                session_locked,
                            ) {
                                let local =
                                    surface_local_position(&windows, surface, position, &config);
                                wayland
                                    .touch_down(
                                        1,
                                        surface,
                                        time,
                                        slot,
                                        local,
                                        display.next_serial(),
                                    )
                                    .map_err(app_error)?;
                                touch_targets.insert(slot, surface);
                            }
                        }
                    }
                    LinuxInputEventKind::TouchMotion { slot, normalized } => {
                        let position = normalized_output_position(normalized, extent);
                        if wayland.drag_touch_slot(1) == Some(slot) {
                            let target = hit_test_surface(
                                &windows,
                                &stacking_order,
                                position,
                                &config,
                                session_locked,
                            );
                            let local = target.map_or(position, |surface| {
                                surface_local_position(&windows, surface, position, &config)
                            });
                            wayland
                                .drag_motion(1, target, time, local)
                                .map_err(app_error)?;
                            drag_position = position;
                            repaint = true;
                        } else if let Some(surface) = touch_targets.get(&slot).copied() {
                            let local =
                                surface_local_position(&windows, surface, position, &config);
                            wayland
                                .touch_motion(1, time, slot, local)
                                .map_err(app_error)?;
                        }
                    }
                    LinuxInputEventKind::TouchUp { slot } => {
                        if touch_targets.remove(&slot).is_some() {
                            wayland
                                .touch_up(1, time, slot, display.next_serial())
                                .map_err(app_error)?;
                        }
                        if wayland.drag_touch_slot(1) == Some(slot) {
                            wayland.drop_drag(1).map_err(app_error)?;
                            repaint = true;
                        }
                    }
                    LinuxInputEventKind::TouchCancel => {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                        if wayland.drag_touch_slot(1).is_some() {
                            wayland.cancel_drag(1).map_err(app_error)?;
                            repaint = true;
                        }
                    }
                    LinuxInputEventKind::DeviceAdded | LinuxInputEventKind::DeviceRemoved => {}
                }
            }
        }

        let actions = wayland.core_mut().drain_actions().collect::<Vec<_>>();
        for action in actions {
            match action {
                CompositorAction::PublishSurface(surface) => {
                    let snapshot = wayland
                        .core()
                        .world
                        .surface(surface)
                        .map(|surface| surface.snapshot().clone());
                    let Some(snapshot) = snapshot else { continue };
                    let Some(role) = snapshot.role else {
                        continue;
                    };
                    if !matches!(
                        role,
                        SurfaceRole::XdgToplevel
                            | SurfaceRole::XdgPopup
                            | SurfaceRole::Subsurface
                            | SurfaceRole::Cursor
                            | SurfaceRole::DragIcon
                            | SurfaceRole::SessionLock
                    ) {
                        continue;
                    }
                    let Some(attachment) = snapshot.attachment else {
                        continue;
                    };
                    let shm = wayland
                        .read_shm_buffer(attachment.buffer)
                        .map_err(app_error)?;
                    let image = shm_image_resource(attachment.buffer, snapshot.revision, shm)
                        .map_err(app_error)?;
                    let image = transform_surface_image(
                        image,
                        snapshot.buffer_scale,
                        snapshot.buffer_transform,
                        wayland.viewport(surface),
                    )
                    .map_err(app_error)?;
                    let (parent, offset, position) = if role == SurfaceRole::Subsurface {
                        let parent = wayland.core().subsurfaces.parent(surface);
                        let offset = wayland
                            .core()
                            .subsurfaces
                            .position(surface)
                            .map_or(PointI::default(), |position| position.offset);
                        let position = parent.and_then(|parent| windows.get(&parent)).map_or(
                            offset,
                            |parent| PointI {
                                x: parent.position.x + offset.x,
                                y: parent.position.y + offset.y,
                            },
                        );
                        (parent, offset, position)
                    } else if role == SurfaceRole::XdgPopup {
                        let (parent, geometry) = wayland.popup_placement(surface).unwrap_or((
                            None,
                            RectI {
                                x: 0,
                                y: 0,
                                width: image.extent.width,
                                height: image.extent.height,
                            },
                        ));
                        let offset = PointI {
                            x: geometry.x,
                            y: geometry.y,
                        };
                        let position = parent.and_then(|parent| windows.get(&parent)).map_or(
                            offset,
                            |parent| PointI {
                                x: parent.position.x + offset.x,
                                y: parent.position.y + offset.y,
                            },
                        );
                        (parent, offset, position)
                    } else if matches!(
                        role,
                        SurfaceRole::Cursor | SurfaceRole::DragIcon | SurfaceRole::SessionLock
                    ) {
                        (None, PointI::default(), PointI::default())
                    } else {
                        let position = windows.get(&surface).map_or_else(
                            || {
                                let offset = next_window_offset;
                                next_window_offset = (next_window_offset + 28) % 280;
                                PointI {
                                    x: work_area.x + 48 + offset,
                                    y: work_area.y + 48 + offset,
                                }
                            },
                            |window| window.position,
                        );
                        (None, PointI::default(), position)
                    };
                    let is_new = !windows.contains_key(&surface);
                    let (restore_geometry, maximized, fullscreen, minimized) = windows
                        .get(&surface)
                        .map_or((None, false, false, false), |window| {
                            (
                                window.restore_geometry,
                                window.maximized,
                                window.fullscreen,
                                window.minimized,
                            )
                        });
                    windows.insert(
                        surface,
                        ClientWindow {
                            revision: snapshot.revision,
                            role,
                            parent,
                            offset,
                            server_decorated: role == SurfaceRole::XdgToplevel
                                && wayland.decoration_mode(surface)
                                    != Some(crate::compositor_wayland::DecorationMode::ClientSide),
                            position,
                            size: image.extent,
                            requested_size: image.extent,
                            restore_geometry,
                            maximized,
                            fullscreen,
                            minimized,
                            rgba: image.pixels_rgba8.to_vec(),
                        },
                    );
                    if is_new && !matches!(role, SurfaceRole::Cursor | SurfaceRole::DragIcon) {
                        stacking_order.push(surface);
                    }
                    if session_locked && role == SurfaceRole::SessionLock {
                        wayland
                            .set_keyboard_focus(1, Some(surface), display.next_serial())
                            .map_err(app_error)?;
                    }
                    wayland
                        .release_buffer(attachment.buffer)
                        .map_err(app_error)?;
                    repaint = true;
                }
                CompositorAction::WithdrawSurface(surface) => {
                    windows.remove(&surface);
                    stacking_order.retain(|candidate| *candidate != surface);
                    if matches!(
                        window_interaction,
                        Some(WindowInteraction::Move { surface: candidate }
                            | WindowInteraction::Resize {
                                surface: candidate,
                                ..
                            }) if candidate == surface
                    ) {
                        window_interaction = None;
                    }
                    if touch_targets.values().any(|target| *target == surface) {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                    }
                    if pointer_focus == Some(surface) {
                        pointer_focus = None;
                    }
                    repaint = true;
                }
                CompositorAction::ActivateSurface {
                    surface,
                    application_id: _,
                    source_surface: _,
                } => {
                    if windows.contains_key(&surface) {
                        if let Some(window) = windows.get_mut(&surface) {
                            window.minimized = false;
                        }
                        stacking_order.retain(|candidate| *candidate != surface);
                        stacking_order.push(surface);
                        focus_toplevel(&display, &mut wayland, &windows, Some(surface))?;
                        repaint = true;
                    }
                }
                CompositorAction::MoveToplevel(surface) => {
                    if windows.get(&surface).is_some_and(|window| {
                        !window.maximized && !window.fullscreen && !window.minimized
                    }) {
                        window_interaction = Some(WindowInteraction::Move { surface });
                    }
                }
                CompositorAction::ResizeToplevel { surface, edge } => {
                    if windows.get(&surface).is_some_and(|window| {
                        !window.maximized && !window.fullscreen && !window.minimized
                    }) {
                        window_interaction = Some(WindowInteraction::Resize { surface, edge });
                    }
                }
                CompositorAction::MaximizeToplevel { surface, maximized } => {
                    set_window_maximized(
                        &mut wayland,
                        &mut windows,
                        surface,
                        maximized,
                        work_area,
                        &config,
                    )?;
                    repaint = true;
                }
                CompositorAction::FullscreenToplevel {
                    surface,
                    fullscreen,
                    output: _,
                } => {
                    set_window_fullscreen(
                        &mut wayland,
                        &mut windows,
                        surface,
                        fullscreen,
                        extent,
                        &config,
                    )?;
                    repaint = true;
                }
                CompositorAction::MinimizeToplevel(surface) => {
                    if let Some(window) = windows.get_mut(&surface) {
                        window.minimized = true;
                        stacking_order.retain(|candidate| *candidate != surface);
                        repaint = true;
                    }
                }
                CompositorAction::SessionLockRequested(lock) => {
                    if wayland.drag_active(1) {
                        wayland.cancel_drag(1).map_err(app_error)?;
                    }
                    session_locked = true;
                    pending_session_lock = Some(lock);
                    window_interaction = None;
                    pointer_focus = None;
                    wayland
                        .set_pointer_focus(1, None, pointer_position, display.next_serial())
                        .map_err(app_error)?;
                    wayland
                        .set_keyboard_focus(1, None, display.next_serial())
                        .map_err(app_error)?;
                    if !touch_targets.is_empty() {
                        touch_targets.clear();
                        wayland.touch_cancel(1).map_err(app_error)?;
                    }
                    repaint = true;
                }
                CompositorAction::SessionLockCancelled(lock) => {
                    if pending_session_lock == Some(lock) && !wayland.session_locked() {
                        pending_session_lock = None;
                        session_locked = false;
                        repaint = true;
                    }
                }
                CompositorAction::SessionUnlockRequested(lock) => {
                    if pending_session_lock == Some(lock) {
                        pending_session_lock = None;
                    }
                    session_locked = false;
                    repaint = true;
                }
                CompositorAction::StartDrag {
                    seat: _,
                    origin: _,
                    icon: _,
                }
                | CompositorAction::FinishDrag { icon: _ } => repaint = true,
                CompositorAction::RepaintOutput(_) => repaint = true,
                CompositorAction::ImportBuffer(_)
                | CompositorAction::ReleaseBuffer(_)
                | CompositorAction::DisconnectClient(_) => {}
            }
        }

        if repaint && seat.state() == SeatState::Enabled {
            let now = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            let mut output = if session_locked {
                let mut pixels = vec![0_u8; extent.width as usize * extent.height as usize * 4];
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
                pixels
            } else {
                policy.render(extent, now, first_modeset)?.to_vec()
            };
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
            for surface in &stacking_order {
                let Some(window) = windows.get(surface) else {
                    continue;
                };
                if matches!(window.role, SurfaceRole::Cursor | SurfaceRole::DragIcon) {
                    continue;
                }
                if window.minimized {
                    continue;
                }
                if (window.role == SurfaceRole::SessionLock) != session_locked {
                    continue;
                }
                let position = placements.get(surface).copied().unwrap_or(window.position);
                let outer = SizeI {
                    width: window.size.width + config.window_border * 2,
                    height: window.size.height + config.window_border * 2 + config.titlebar_height,
                };
                if window_is_decorated(window)
                    && let Some(frame) = &mut frame
                {
                    let pixels = frame.render(outer, now, true)?;
                    composite_rgba(&mut output, extent, pixels, outer, position, false);
                }
                if window_is_decorated(window) && window.role == SurfaceRole::XdgToplevel {
                    let icon_extent = SizeI {
                        width: config.titlebar_height.clamp(1, 24),
                        height: config.titlebar_height.clamp(1, 24),
                    };
                    for (index, name) in ["window.close", "window.maximize", "window.minimize"]
                        .into_iter()
                        .enumerate()
                    {
                        if let Some((_, icon)) = icon_layers
                            .iter_mut()
                            .find(|(candidate, _)| candidate == name)
                        {
                            let pixels = icon.render(icon_extent, now, false)?;
                            composite_rgba(
                                &mut output,
                                extent,
                                pixels,
                                icon_extent,
                                PointI {
                                    x: position.x + outer.width
                                        - config.window_border
                                        - (index as i32 + 1) * (icon_extent.width + 4),
                                    y: position.y
                                        + config.window_border
                                        + (config.titlebar_height - icon_extent.height) / 2,
                                },
                                true,
                            );
                        }
                    }
                }
                composite_rgba(
                    &mut output,
                    extent,
                    &window.rgba,
                    window.size,
                    PointI {
                        x: position.x
                            + if window_is_decorated(window) {
                                config.window_border
                            } else {
                                0
                            },
                        y: position.y
                            + if window_is_decorated(window) {
                                config.window_border + config.titlebar_height
                            } else {
                                0
                            },
                    },
                    true,
                );
            }
            if !session_locked {
                for widget in &mut widgets {
                    let widget_extent = resolved_widget_extent(extent, widget.width, widget.height);
                    let position = widget_position(extent, widget_extent, widget.anchor);
                    let pixels = widget.layer.render(widget_extent, now, false)?;
                    composite_rgba(&mut output, extent, pixels, widget_extent, position, false);
                }
            }
            if !session_locked
                && let Some(icon) = wayland.drag_icon(1)
                && let Some(icon) = windows.get(&icon)
            {
                composite_rgba(
                    &mut output,
                    extent,
                    &icon.rgba,
                    icon.size,
                    PointI {
                        x: drag_position.x.round() as i32,
                        y: drag_position.y.round() as i32,
                    },
                    true,
                );
            }
            let cursor = &wayland
                .core()
                .seats
                .get(&1)
                .expect("seat registered")
                .cursor;
            match *cursor {
                CursorImage::TelorgonDefault => {
                    if let Some(pointer) = &mut pointer {
                        let pixels = pointer.render(config.pointer_extent, now, false)?;
                        composite_rgba(
                            &mut output,
                            extent,
                            pixels,
                            config.pointer_extent,
                            PointI {
                                x: pointer_position.x.round() as i32,
                                y: pointer_position.y.round() as i32,
                            },
                            false,
                        );
                    }
                }
                CursorImage::Shape(shape) => {
                    let semantic_name = cursor_shape_icon_name(shape);
                    if let Some((_, icon)) = semantic_name.and_then(|name| {
                        icon_layers
                            .iter_mut()
                            .find(|(candidate, _)| candidate == name)
                    }) {
                        let pixels = icon.render(config.pointer_extent, now, false)?;
                        composite_rgba(
                            &mut output,
                            extent,
                            pixels,
                            config.pointer_extent,
                            PointI {
                                x: pointer_position.x.round() as i32,
                                y: pointer_position.y.round() as i32,
                            },
                            true,
                        );
                    } else if let Some(pointer) = &mut pointer {
                        let pixels = pointer.render(config.pointer_extent, now, false)?;
                        composite_rgba(
                            &mut output,
                            extent,
                            pixels,
                            config.pointer_extent,
                            PointI {
                                x: pointer_position.x.round() as i32,
                                y: pointer_position.y.round() as i32,
                            },
                            false,
                        );
                    }
                }
                CursorImage::ClientSurface {
                    surface,
                    hotspot_x,
                    hotspot_y,
                } => {
                    if let Some(cursor) = windows.get(&surface) {
                        composite_rgba(
                            &mut output,
                            extent,
                            &cursor.rgba,
                            cursor.size,
                            PointI {
                                x: pointer_position.x.round() as i32 - hotspot_x,
                                y: pointer_position.y.round() as i32 - hotspot_y,
                            },
                            true,
                        );
                    }
                }
                CursorImage::Hidden => {}
            }
            scanout_index = (scanout_index + 1) % scanout_buffers.len();
            if let Some(vulkan) = &mut vulkan_scanout {
                vulkan.render(scanout_index, extent, output)?;
            } else {
                scanout_buffers[scanout_index]
                    .map_write()
                    .map_err(app_error)?
                    .write_rgba8(&output)
                    .map_err(app_error)?;
            }
            let request = kms
                .primary_modeset_request(
                    connector.id,
                    &connector_properties,
                    crtc,
                    &crtc_properties,
                    plane.id,
                    &plane_properties,
                    mode_blob.id(),
                    framebuffers[scanout_index].id(),
                    extent.width as u32,
                    extent.height as u32,
                )
                .map_err(app_error)?;
            if first_modeset {
                request.test(true).map_err(app_error)?;
                let request = kms
                    .primary_modeset_request(
                        connector.id,
                        &connector_properties,
                        crtc,
                        &crtc_properties,
                        plane.id,
                        &plane_properties,
                        mode_blob.id(),
                        framebuffers[scanout_index].id(),
                        extent.width as u32,
                        extent.height as u32,
                    )
                    .map_err(app_error)?;
                request.commit(true, false).map_err(app_error)?;
            } else {
                request.commit(false, false).map_err(app_error)?;
            }
            if let Some(lock) = pending_session_lock.take() {
                wayland
                    .session_lock_frame_presented(lock)
                    .map_err(app_error)?;
            }
            let time = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);
            for (surface, window) in &windows {
                if window.revision != 0
                    && !window.minimized
                    && (window.role == SurfaceRole::SessionLock) == session_locked
                {
                    wayland
                        .surface_presented(*surface, time)
                        .map_err(app_error)?;
                }
            }
            display.flush_clients();
            first_modeset = false;
            repaint = false;
        }
    }
}

fn output_state(
    connector: &crate::presenter_vulkan_kms::KmsConnector,
    current_mode: usize,
    config: &LinuxDesktopConfig,
) -> AppResult<OutputState> {
    let name = format!(
        "DRM-{}-{}",
        connector.connector_type, connector.connector_type_id
    );
    let modes = connector
        .modes
        .iter()
        .map(|mode| OutputMode {
            size: mode.size(),
            refresh_millihertz: mode.refresh_millihertz(),
            preferred: mode.preferred(),
        })
        .collect();
    OutputState::new(
        OutputDescription {
            name: name.clone(),
            description: format!("Telorgon output {name}"),
            make: "Unknown".to_owned(),
            model: name,
            physical_millimeters: connector.physical_millimeters,
            logical_position: PointI::default(),
            scale: config.output_scale,
            transform: OutputTransform::Normal,
            modes,
        },
        current_mode,
    )
    .map_err(app_error)
}

fn is_primary_plane(kms: &KmsDevice, plane: u32) -> bool {
    KmsTopology::object_properties(kms, plane, KmsPropertyObject::Plane)
        .ok()
        .and_then(|properties| properties.named("type").map(|property| property.value))
        == Some(1)
}

fn update_pointer_focus(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    session_locked: bool,
    current: &mut Option<WaylandSurfaceId>,
    position: PointF,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let next = hit_test_surface(windows, stacking_order, position, config, session_locked);
    if next != *current {
        let local = next.map_or(position, |surface| {
            surface_local_position(windows, surface, position, config)
        });
        wayland
            .set_pointer_focus(1, next, local, display.next_serial())
            .map_err(app_error)?;
        *current = next;
    }
    Ok(())
}

fn route_pointer_motion(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    session_locked: bool,
    current: &mut Option<WaylandSurfaceId>,
    position: PointF,
    time: u32,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    if wayland.drag_active(1) && wayland.drag_touch_slot(1).is_none() {
        let target = hit_test_surface(windows, stacking_order, position, config, session_locked);
        let local = target.map_or(position, |surface| {
            surface_local_position(windows, surface, position, config)
        });
        wayland
            .drag_motion(1, target, time, local)
            .map_err(app_error)?;
    } else {
        update_pointer_focus(
            display,
            wayland,
            windows,
            stacking_order,
            session_locked,
            current,
            position,
            config,
        )?;
        if let Some(surface) = *current {
            let local = surface_local_position(windows, surface, position, config);
            let _ = wayland.pointer_motion(1, time, local);
        }
    }
    Ok(())
}

fn focus_toplevel(
    display: &Display,
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: Option<WaylandSurfaceId>,
) -> AppResult<()> {
    let previous = wayland
        .core()
        .seats
        .get(&1)
        .and_then(|seat| seat.keyboard_focus)
        .map(|focus| focus.surface);
    if previous == surface {
        return Ok(());
    }
    if let Some(previous) = previous
        && wayland
            .core()
            .world
            .surface(previous)
            .is_some_and(|surface| surface.snapshot().role == Some(SurfaceRole::XdgToplevel))
    {
        wayland
            .configure_toplevel(
                previous,
                windows.get(&previous).map(|window| window.requested_size),
                windows
                    .get(&previous)
                    .map_or_else(ToplevelState::default, |window| {
                        window_toplevel_states(window, false, false)
                    }),
            )
            .map_err(app_error)?;
    }
    wayland
        .set_keyboard_focus(1, surface, display.next_serial())
        .map_err(app_error)?;
    if let Some(surface) = surface
        && wayland
            .core()
            .world
            .surface(surface)
            .is_some_and(|surface| surface.snapshot().role == Some(SurfaceRole::XdgToplevel))
    {
        wayland
            .configure_toplevel(
                surface,
                windows.get(&surface).map(|window| window.requested_size),
                windows.get(&surface).map_or(
                    ToplevelState {
                        activated: true,
                        ..ToplevelState::default()
                    },
                    |window| window_toplevel_states(window, true, false),
                ),
            )
            .map_err(app_error)?;
    }
    Ok(())
}

fn apply_window_interaction(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    interaction: WindowInteraction,
    delta: PointF,
    output: SizeI,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let (surface, edge) = match interaction {
        WindowInteraction::Move { surface } => (surface, None),
        WindowInteraction::Resize { surface, edge } => (surface, Some(edge)),
    };
    let Some(window) = windows.get_mut(&surface) else {
        return Ok(());
    };
    let delta = PointI {
        x: delta.x.round() as i32,
        y: delta.y.round() as i32,
    };
    let Some(edge) = edge else {
        window.position.x = window
            .position
            .x
            .saturating_add(delta.x)
            .clamp(-window.size.width + 32, output.width - 32);
        window.position.y = window
            .position
            .y
            .saturating_add(delta.y)
            .clamp(0, output.height - config.titlebar_height.max(1));
        return Ok(());
    };
    let mut size = window.requested_size;
    let minimum_width = 64;
    let minimum_height = 48;
    if matches!(
        edge,
        ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
    ) {
        let next = size.width.saturating_sub(delta.x).max(minimum_width);
        window.position.x = window
            .position
            .x
            .saturating_add(size.width.saturating_sub(next));
        size.width = next;
    }
    if matches!(
        edge,
        ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
    ) {
        size.width = size.width.saturating_add(delta.x).max(minimum_width);
    }
    if matches!(
        edge,
        ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
    ) {
        let next = size.height.saturating_sub(delta.y).max(minimum_height);
        window.position.y = window
            .position
            .y
            .saturating_add(size.height.saturating_sub(next));
        size.height = next;
    }
    if matches!(
        edge,
        ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
    ) {
        size.height = size.height.saturating_add(delta.y).max(minimum_height);
    }
    size.width = size.width.min(output.width.max(minimum_width));
    size.height = size.height.min(output.height.max(minimum_height));
    if size != window.requested_size {
        window.requested_size = size;
        wayland
            .configure_toplevel(
                surface,
                Some(size),
                window_toplevel_states(window, true, true),
            )
            .map_err(app_error)?;
    }
    Ok(())
}

fn finish_window_interaction(
    wayland: &mut NativeCompositor<'_>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    interaction: WindowInteraction,
) -> AppResult<()> {
    let WindowInteraction::Resize { surface, .. } = interaction else {
        return Ok(());
    };
    if let Some(window) = windows.get(&surface) {
        wayland
            .configure_toplevel(
                surface,
                Some(window.requested_size),
                window_toplevel_states(window, true, false),
            )
            .map_err(app_error)?;
    }
    Ok(())
}

fn set_window_maximized(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    maximized: bool,
    work_area: RectI,
    config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let Some(window) = windows.get_mut(&surface) else {
        return Ok(());
    };
    if maximized {
        if !window.maximized && !window.fullscreen {
            window.restore_geometry = Some((window.position, window.requested_size));
        }
        window.maximized = true;
        window.fullscreen = false;
        window.minimized = false;
        window.position = PointI {
            x: work_area.x,
            y: work_area.y,
        };
        window.requested_size = SizeI {
            width: (work_area.width - config.window_border * 2).max(1),
            height: (work_area.height - config.window_border * 2 - config.titlebar_height).max(1),
        };
    } else {
        window.maximized = false;
        if let Some((position, size)) = window.restore_geometry.take() {
            window.position = position;
            window.requested_size = size;
        }
    }
    wayland
        .configure_toplevel(
            surface,
            Some(window.requested_size),
            window_toplevel_states(window, true, false),
        )
        .map_err(app_error)?;
    Ok(())
}

fn set_window_fullscreen(
    wayland: &mut NativeCompositor<'_>,
    windows: &mut BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    fullscreen: bool,
    output: SizeI,
    _config: &LinuxDesktopConfig,
) -> AppResult<()> {
    let Some(window) = windows.get_mut(&surface) else {
        return Ok(());
    };
    if fullscreen {
        if !window.maximized && !window.fullscreen {
            window.restore_geometry = Some((window.position, window.requested_size));
        }
        window.maximized = false;
        window.fullscreen = true;
        window.minimized = false;
        window.position = PointI::default();
        window.requested_size = output;
    } else {
        window.fullscreen = false;
        if let Some((position, size)) = window.restore_geometry.take() {
            window.position = position;
            window.requested_size = size;
        }
    }
    wayland
        .configure_toplevel(
            surface,
            Some(window.requested_size),
            window_toplevel_states(window, true, false),
        )
        .map_err(app_error)?;
    Ok(())
}

fn window_toplevel_states(window: &ClientWindow, activated: bool, resizing: bool) -> ToplevelState {
    ToplevelState {
        maximized: window.maximized,
        fullscreen: window.fullscreen,
        resizing,
        activated,
        ..ToplevelState::default()
    }
}

fn hit_test_decoration(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) -> Option<(WaylandSurfaceId, DecorationHit)> {
    for surface in stacking_order.iter().rev() {
        let Some(window) = windows.get(surface) else {
            continue;
        };
        if window.role != SurfaceRole::XdgToplevel
            || window.minimized
            || !window_is_decorated(window)
        {
            continue;
        }
        let outer = SizeI {
            width: window.size.width + config.window_border * 2,
            height: window.size.height + config.window_border * 2 + config.titlebar_height,
        };
        let local = PointI {
            x: (position.x - window.position.x as f32).floor() as i32,
            y: (position.y - window.position.y as f32).floor() as i32,
        };
        if local.x < 0 || local.y < 0 || local.x >= outer.width || local.y >= outer.height {
            continue;
        }
        let border = config.window_border.max(1);
        let left = local.x < border;
        let right = local.x >= outer.width - border;
        let top = local.y < border;
        let bottom = local.y >= outer.height - border;
        let edge = match (left, right, top, bottom) {
            (true, _, true, _) => Some(ResizeEdge::TopLeft),
            (_, true, true, _) => Some(ResizeEdge::TopRight),
            (true, _, _, true) => Some(ResizeEdge::BottomLeft),
            (_, true, _, true) => Some(ResizeEdge::BottomRight),
            (true, _, _, _) => Some(ResizeEdge::Left),
            (_, true, _, _) => Some(ResizeEdge::Right),
            (_, _, true, _) => Some(ResizeEdge::Top),
            (_, _, _, true) => Some(ResizeEdge::Bottom),
            _ => None,
        };
        if let Some(edge) = edge {
            return Some((*surface, DecorationHit::Resize(edge)));
        }
        if local.y < border + config.titlebar_height {
            let icon_extent = config.titlebar_height.clamp(1, 24);
            for (index, (name, hit)) in [
                ("window.close", DecorationHit::Close),
                ("window.maximize", DecorationHit::Maximize),
                ("window.minimize", DecorationHit::Minimize),
            ]
            .into_iter()
            .enumerate()
            {
                if !icons.iter().any(|(candidate, _)| candidate == name) {
                    continue;
                }
                let x = outer.width - border - (index as i32 + 1) * (icon_extent + 4);
                let y = border + (config.titlebar_height - icon_extent) / 2;
                if local.x >= x
                    && local.x < x + icon_extent
                    && local.y >= y
                    && local.y < y + icon_extent
                {
                    return Some((*surface, hit));
                }
            }
            return Some((*surface, DecorationHit::Titlebar));
        }
    }
    None
}

fn hit_test_surface(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    session_locked: bool,
) -> Option<WaylandSurfaceId> {
    stacking_order
        .iter()
        .rev()
        .filter_map(|surface| windows.get(surface).map(|window| (*surface, window)))
        .find(|(_, window)| {
            let origin = window_content_origin(window, config);
            window.role != SurfaceRole::Cursor
                && !window.minimized
                && (window.role == SurfaceRole::SessionLock) == session_locked
                && position.x >= origin.x as f32
                && position.y >= origin.y as f32
                && position.x < (origin.x + window.size.width) as f32
                && position.y < (origin.y + window.size.height) as f32
        })
        .map(|(surface, _)| surface)
}

fn normalized_output_position(normalized: PointF, extent: SizeI) -> PointF {
    PointF {
        x: normalized.x.clamp(0.0, 1.0) * (extent.width - 1) as f32,
        y: normalized.y.clamp(0.0, 1.0) * (extent.height - 1) as f32,
    }
}

fn cursor_shape_icon_name(shape: u32) -> Option<&'static str> {
    Some(match shape {
        1 => "cursor.default",
        2 => "cursor.context-menu",
        3 => "cursor.help",
        4 => "cursor.pointer",
        5 => "cursor.progress",
        6 => "cursor.wait",
        7 => "cursor.cell",
        8 => "cursor.crosshair",
        9 => "cursor.text",
        10 => "cursor.vertical-text",
        11 => "cursor.alias",
        12 => "cursor.copy",
        13 => "cursor.move",
        14 => "cursor.no-drop",
        15 => "cursor.not-allowed",
        16 => "cursor.grab",
        17 => "cursor.grabbing",
        18 => "cursor.e-resize",
        19 => "cursor.n-resize",
        20 => "cursor.ne-resize",
        21 => "cursor.nw-resize",
        22 => "cursor.s-resize",
        23 => "cursor.se-resize",
        24 => "cursor.sw-resize",
        25 => "cursor.w-resize",
        26 => "cursor.ew-resize",
        27 => "cursor.ns-resize",
        28 => "cursor.nesw-resize",
        29 => "cursor.nwse-resize",
        30 => "cursor.col-resize",
        31 => "cursor.row-resize",
        32 => "cursor.all-scroll",
        33 => "cursor.zoom-in",
        34 => "cursor.zoom-out",
        _ => return None,
    })
}

fn window_content_origin(window: &ClientWindow, config: &LinuxDesktopConfig) -> PointI {
    PointI {
        x: window.position.x
            + if window_is_decorated(window) {
                config.window_border
            } else {
                0
            },
        y: window.position.y
            + if window_is_decorated(window) {
                config.window_border + config.titlebar_height
            } else {
                0
            },
    }
}

fn window_is_decorated(window: &ClientWindow) -> bool {
    window.server_decorated && !window.fullscreen
}

fn surface_local_position(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    surface: WaylandSurfaceId,
    position: PointF,
    config: &LinuxDesktopConfig,
) -> PointF {
    let origin = windows.get(&surface).map_or(PointI::default(), |window| {
        window_content_origin(window, config)
    });
    PointF {
        x: position.x - origin.x as f32,
        y: position.y - origin.y as f32,
    }
}

fn constrain_pointer(
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    current: PointF,
    proposed: PointF,
    constraint: &PointerConstraintState,
    config: &LinuxDesktopConfig,
) -> PointF {
    let Some(window) = windows.get(&constraint.surface) else {
        return current;
    };
    let origin = window_content_origin(window, config);
    let local = PointF {
        x: proposed.x - origin.x as f32,
        y: proposed.y - origin.y as f32,
    };
    let surface = RectI {
        x: 0,
        y: 0,
        width: window.size.width,
        height: window.size.height,
    };
    let regions = constraint.region.as_ref().map_or_else(
        || vec![surface],
        |region| {
            region
                .rectangles()
                .iter()
                .filter_map(|rectangle| intersect_rect(*rectangle, surface))
                .collect()
        },
    );
    let Some(nearest) = regions
        .iter()
        .map(|rectangle| {
            let maximum_x = (rectangle.x + rectangle.width) as f32 - 0.001;
            let maximum_y = (rectangle.y + rectangle.height) as f32 - 0.001;
            let point = PointF {
                x: local.x.clamp(rectangle.x as f32, maximum_x),
                y: local.y.clamp(rectangle.y as f32, maximum_y),
            };
            let distance_x = local.x - point.x;
            let distance_y = local.y - point.y;
            (distance_x * distance_x + distance_y * distance_y, point)
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, point)| point)
    else {
        return current;
    };
    PointF {
        x: nearest.x + origin.x as f32,
        y: nearest.y + origin.y as f32,
    }
}

fn intersect_rect(left: RectI, right: RectI) -> Option<RectI> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = (left.x + left.width).min(right.x + right.width);
    let bottom_edge = (left.y + left.height).min(right.y + right.height);
    (right_edge > x && bottom_edge > y).then_some(RectI {
        x,
        y,
        width: right_edge - x,
        height: bottom_edge - y,
    })
}

fn shell_work_area(output: SizeI, widgets: &[WidgetLayer]) -> RectI {
    let mut area = RectI {
        x: 0,
        y: 0,
        width: output.width,
        height: output.height,
    };
    for widget in widgets {
        let reserved = widget.reserved_space.max(0);
        match widget.anchor {
            ShellWidgetAnchor::Top => {
                let reserved = reserved.min(area.height.saturating_sub(1));
                area.y = area.y.saturating_add(reserved);
                area.height = area.height.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Right | ShellWidgetAnchor::Bottom => {
                let extent = if widget.anchor == ShellWidgetAnchor::Right {
                    &mut area.width
                } else {
                    &mut area.height
                };
                let reserved = reserved.min(extent.saturating_sub(1));
                *extent = extent.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Left => {
                let reserved = reserved.min(area.width.saturating_sub(1));
                area.x = area.x.saturating_add(reserved);
                area.width = area.width.saturating_sub(reserved);
            }
            ShellWidgetAnchor::Floating => {}
        }
    }
    area
}

fn resolved_widget_extent(
    output: SizeI,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
) -> SizeI {
    SizeI {
        width: match width {
            ShellWidgetExtent::Fill => output.width,
            ShellWidgetExtent::Pixels(value) => value.round().max(1.0) as i32,
        },
        height: match height {
            ShellWidgetExtent::Fill => output.height,
            ShellWidgetExtent::Pixels(value) => value.round().max(1.0) as i32,
        },
    }
}

fn widget_position(output: SizeI, widget: SizeI, anchor: ShellWidgetAnchor) -> PointI {
    match anchor {
        ShellWidgetAnchor::Top | ShellWidgetAnchor::Left => PointI::default(),
        ShellWidgetAnchor::Right => PointI {
            x: output.width - widget.width,
            y: 0,
        },
        ShellWidgetAnchor::Bottom => PointI {
            x: 0,
            y: output.height - widget.height,
        },
        ShellWidgetAnchor::Floating => PointI {
            x: (output.width - widget.width) / 2,
            y: (output.height - widget.height) / 2,
        },
    }
}

fn composite_rgba(
    target: &mut [u8],
    target_size: SizeI,
    source: &[u8],
    source_size: SizeI,
    position: PointI,
    premultiplied: bool,
) {
    if source.len() < source_size.width.max(0) as usize * source_size.height.max(0) as usize * 4 {
        return;
    }
    for source_y in 0..source_size.height {
        let target_y = position.y + source_y;
        if !(0..target_size.height).contains(&target_y) {
            continue;
        }
        for source_x in 0..source_size.width {
            let target_x = position.x + source_x;
            if !(0..target_size.width).contains(&target_x) {
                continue;
            }
            let source_index =
                (source_y as usize * source_size.width as usize + source_x as usize) * 4;
            let target_index =
                (target_y as usize * target_size.width as usize + target_x as usize) * 4;
            let alpha = f32::from(source[source_index + 3]) / 255.0;
            let inverse = 1.0 - alpha;
            for channel in 0..3 {
                let source = f32::from(source[source_index + channel]);
                let destination = f32::from(target[target_index + channel]);
                let value = if premultiplied {
                    source + destination * inverse
                } else {
                    source * alpha + destination * inverse
                };
                target[target_index + channel] = value.round().clamp(0.0, 255.0) as u8;
            }
            target[target_index + 3] = 255;
        }
    }
}

fn app_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(error.to_string())
}
