use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecorationHit {
    Frame,
    Titlebar,
    Resize(ResizeEdge),
    Close,
    Maximize,
    Minimize,
    SystemMenu,
    ShellAction(crate::ShellActionId),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_pointer_focus(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn route_pointer_motion(
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

pub(super) fn focus_toplevel(
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

pub(super) fn hit_test_decoration(
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
        let outer = window
            .chrome_outer
            .unwrap_or_else(|| legacy_window_outer(window, config));
        let local = PointI {
            x: (position.x - window.position.x as f32).floor() as i32,
            y: (position.y - window.position.y as f32).floor() as i32,
        };
        if local.x < 0 || local.y < 0 || local.x >= outer.width || local.y >= outer.height {
            continue;
        }
        if let Some(chrome) = &window.chrome {
            let point = PointF {
                x: local.x as f32,
                y: local.y as f32,
            };
            let role = chrome.hit_test(point.x, point.y);
            if role.is_none() && chrome.content.bounds.contains(point) {
                continue;
            }
            let hit = match role {
                Some(crate::WindowChromeRole::DragRegion)
                | Some(crate::WindowChromeRole::Action(WindowAction::BeginMove)) => {
                    DecorationHit::Titlebar
                }
                Some(crate::WindowChromeRole::Action(WindowAction::BeginResize(edge))) => {
                    DecorationHit::Resize(wayland_resize_edge(edge))
                }
                Some(crate::WindowChromeRole::Action(WindowAction::Close)) => DecorationHit::Close,
                Some(crate::WindowChromeRole::Action(WindowAction::Minimize)) => {
                    DecorationHit::Minimize
                }
                Some(crate::WindowChromeRole::Action(WindowAction::ToggleMaximize)) => {
                    DecorationHit::Maximize
                }
                Some(crate::WindowChromeRole::Action(WindowAction::ShowSystemMenu)) => {
                    DecorationHit::SystemMenu
                }
                Some(crate::WindowChromeRole::ShellAction(action)) => {
                    DecorationHit::ShellAction(action)
                }
                Some(
                    crate::WindowChromeRole::Frame
                    | crate::WindowChromeRole::Content
                    | crate::WindowChromeRole::Title
                    | crate::WindowChromeRole::AppIcon,
                )
                | None => DecorationHit::Frame,
            };
            return Some((*surface, hit));
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

#[allow(clippy::too_many_arguments)]
pub(super) fn set_decoration_pointer_cursor(
    wayland: &mut NativeCompositor<'_>,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    client_focus: Option<WaylandSurfaceId>,
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) {
    let next = decoration_pointer_request(frames, windows, stacking_order, position, config, icons)
        .map(pointer_request_cursor_image)
        .or_else(|| {
            client_focus
                .is_none()
                .then_some(CursorImage::TelorgonDefault)
        });
    let Some(next) = next else {
        return;
    };
    let Some(seat) = wayland.core_mut().seats.get_mut(&1) else {
        return;
    };
    if seat.cursor != next {
        seat.cursor = next;
    }
}

pub(super) fn decoration_pointer_request(
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
    windows: &BTreeMap<WaylandSurfaceId, ClientWindow>,
    stacking_order: &[WaylandSurfaceId],
    position: PointF,
    config: &LinuxDesktopConfig,
    icons: &[(String, Layer)],
) -> Option<PointerRequest> {
    let (surface, hit) = hit_test_decoration(windows, stacking_order, position, config, icons)?;
    if let (Some(frame), Some(window)) = (frames.get(&surface), windows.get(&surface)) {
        let local = PointF {
            x: position.x - window.position.x as f32,
            y: position.y - window.position.y as f32,
        };
        if let Some(region) = window
            .chrome
            .as_ref()
            .and_then(|chrome| chrome.hit_test_region(local.x, local.y))
            && let Some(request) = frame
                .layer
                .runtime
                .ui()
                .pointer_requests
                .get(region.node)
                .copied()
        {
            return Some(request);
        }
        if let Some(request) = frame
            .layer
            .runtime
            .ui()
            .pointer_requests
            .iter()
            .filter_map(|(node, request)| {
                frame
                    .layer
                    .runtime
                    .layout()
                    .computed(node)
                    .and_then(|computed| {
                        (computed.visible_rect.contains(local)
                            && computed.border_rect.contains(local))
                        .then_some((*request, computed.border_rect.area()))
                    })
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(request, _)| request)
        {
            return Some(request);
        }
    }
    Some(match hit {
        DecorationHit::Titlebar => PointerRequest::Semantic(PointerIcon::Move),
        DecorationHit::Resize(edge) => PointerRequest::Semantic(resize_edge_pointer_icon(edge)),
        DecorationHit::Close
        | DecorationHit::Maximize
        | DecorationHit::Minimize
        | DecorationHit::SystemMenu
        | DecorationHit::ShellAction(_) => PointerRequest::Semantic(PointerIcon::Pointer),
        DecorationHit::Frame => PointerRequest::Semantic(PointerIcon::Default),
    })
}

pub(super) fn invoke_shell_action(
    handlers: &[ShellActionHandler],
    action: crate::ShellActionId,
    surface: WaylandSurfaceId,
    frames: &BTreeMap<WaylandSurfaceId, WindowFrameLayer>,
) {
    let Some(handler) = handlers.iter().find(|handler| handler.id() == action) else {
        return;
    };
    let Some(frame) = frames.get(&surface) else {
        return;
    };
    handler.invoke(frame.model.clone());
}

pub(super) fn hit_test_surface(
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

pub(super) fn normalized_output_position(normalized: PointF, extent: SizeI) -> PointF {
    PointF {
        x: normalized.x.clamp(0.0, 1.0) * (extent.width - 1) as f32,
        y: normalized.y.clamp(0.0, 1.0) * (extent.height - 1) as f32,
    }
}
