use super::*;

pub(super) fn retained_requested_size(previous: Option<SizeI>, committed: SizeI) -> SizeI {
    previous.unwrap_or(committed)
}

pub(super) fn rounded_pointer_delta(start: PointF, current: PointF) -> PointI {
    PointI {
        x: (current.x - start.x).round() as i32,
        y: (current.y - start.y).round() as i32,
    }
}

pub(super) fn resize_drag_geometry(
    position_start: PointI,
    size_start: SizeI,
    edge: ResizeEdge,
    delta: PointI,
    output: SizeI,
) -> (PointI, SizeI) {
    const MINIMUM_WIDTH: i32 = 64;
    const MINIMUM_HEIGHT: i32 = 48;

    let mut position = position_start;
    let mut size = size_start;
    let maximum_width = output.width.max(MINIMUM_WIDTH);
    let maximum_height = output.height.max(MINIMUM_HEIGHT);
    if matches!(
        edge,
        ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
    ) {
        size.width = size_start
            .width
            .saturating_sub(delta.x)
            .clamp(MINIMUM_WIDTH, maximum_width);
        position.x = position_start
            .x
            .saturating_add(size_start.width.saturating_sub(size.width));
    }
    if matches!(
        edge,
        ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
    ) {
        size.width = size_start
            .width
            .saturating_add(delta.x)
            .clamp(MINIMUM_WIDTH, maximum_width);
    }
    if matches!(
        edge,
        ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
    ) {
        size.height = size_start
            .height
            .saturating_sub(delta.y)
            .clamp(MINIMUM_HEIGHT, maximum_height);
        position.y = position_start
            .y
            .saturating_add(size_start.height.saturating_sub(size.height));
    }
    if matches!(
        edge,
        ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
    ) {
        size.height = size_start
            .height
            .saturating_add(delta.y)
            .clamp(MINIMUM_HEIGHT, maximum_height);
    }
    (position, size)
}

pub(super) fn window_content_origin(window: &ClientWindow, config: &LinuxDesktopConfig) -> PointI {
    let offset = window_content_offset(window, config);
    let mut origin = PointI {
        x: window.position.x + offset.x,
        y: window.position.y + offset.y,
    };
    if let Some(anchor) = window.resize_anchor {
        let buffer_offset = anchor.committed_buffer_offset(window.requested_size, window.size);
        origin.x = origin.x.saturating_add(buffer_offset.x);
        origin.y = origin.y.saturating_add(buffer_offset.y);
    }
    origin
}

pub(super) fn window_content_offset(window: &ClientWindow, config: &LinuxDesktopConfig) -> PointI {
    if !window_is_decorated(window) {
        PointI::default()
    } else {
        window.chrome_content_offset.unwrap_or(PointI {
            x: config.window_border,
            y: config.window_border + config.titlebar_height,
        })
    }
}

pub(super) fn legacy_window_outer(window: &ClientWindow, config: &LinuxDesktopConfig) -> SizeI {
    SizeI {
        width: window.requested_size.width + config.window_border * 2,
        height: window.requested_size.height + config.window_border * 2 + config.titlebar_height,
    }
}

pub(super) fn wayland_resize_edge(edge: WindowResizeEdge) -> ResizeEdge {
    match edge {
        WindowResizeEdge::Top => ResizeEdge::Top,
        WindowResizeEdge::TopRight => ResizeEdge::TopRight,
        WindowResizeEdge::Right => ResizeEdge::Right,
        WindowResizeEdge::BottomRight => ResizeEdge::BottomRight,
        WindowResizeEdge::Bottom => ResizeEdge::Bottom,
        WindowResizeEdge::BottomLeft => ResizeEdge::BottomLeft,
        WindowResizeEdge::Left => ResizeEdge::Left,
        WindowResizeEdge::TopLeft => ResizeEdge::TopLeft,
    }
}

pub(super) fn window_is_decorated(window: &ClientWindow) -> bool {
    window.server_decorated && !window.fullscreen
}

pub(super) fn surface_local_position(
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

pub(super) fn constrain_pointer(
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

pub(super) fn intersect_rect(left: RectI, right: RectI) -> Option<RectI> {
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

pub(super) fn full_rect(size: SizeI) -> RectI {
    RectI {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
    }
}

pub(super) fn union_rect(left: RectI, right: RectI) -> RectI {
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

pub(super) fn union_surface_damage(damage: &[RectI], extent: SizeI) -> Option<RectI> {
    damage
        .iter()
        .filter_map(|rect| intersect_rect(*rect, full_rect(extent)))
        .reduce(union_rect)
}

#[cfg(feature = "profiler")]
pub(super) fn rect_area(rect: RectI) -> u64 {
    u64::try_from(rect.width)
        .ok()
        .and_then(|width| {
            u64::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .unwrap_or(0)
}

/// Returns the damage that must be applied to bring a retained scanout target from
/// `previous_version` to `current_version`. `None` deliberately means a full redraw.
pub(super) fn accumulated_damage(
    previous_version: u64,
    current_version: u64,
    history: &VecDeque<(u64, Option<RectI>)>,
    extent: SizeI,
) -> Option<RectI> {
    if previous_version == 0 || previous_version >= current_version {
        return None;
    }
    let oldest_version = history.front().map(|(version, _)| *version)?;
    if previous_version.saturating_add(1) < oldest_version {
        return None;
    }
    let mut combined = None::<RectI>;
    for (_, damage) in history
        .iter()
        .filter(|(version, _)| *version > previous_version && *version <= current_version)
    {
        let Some(damage) = damage else {
            return None;
        };
        combined = Some(combined.map_or(*damage, |old| union_rect(old, *damage)));
    }
    combined.and_then(|damage| intersect_rect(damage, full_rect(extent)))
}

pub(super) fn shell_work_area(output: SizeI, widgets: &[WidgetLayer]) -> RectI {
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

pub(super) fn resolved_widget_extent(
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

pub(super) fn widget_position(output: SizeI, widget: SizeI, anchor: ShellWidgetAnchor) -> PointI {
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
