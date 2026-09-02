use std::collections::BTreeMap;

use crate::compositor_wayland::{ResizeEdge, WaylandSurfaceId, XdgConfigure};
use crate::core::{PointI, SizeI};

/// The one configure that should be emitted for a surface at the end of an input turn.
/// Replacing an entry is intentional: raw motion is compositor-local preview state, not a
/// one-configure-per-event protocol stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingResizeConfigure {
    pub surface: WaylandSurfaceId,
    pub size: SizeI,
    pub resizing: bool,
}

#[derive(Debug, Default)]
pub(super) struct ConfigureScheduler {
    pending: BTreeMap<WaylandSurfaceId, PendingResizeConfigure>,
}

impl ConfigureScheduler {
    pub fn schedule_resize(&mut self, surface: WaylandSurfaceId, size: SizeI) {
        self.pending.insert(
            surface,
            PendingResizeConfigure {
                surface,
                size,
                resizing: true,
            },
        );
    }

    pub fn schedule_final(&mut self, surface: WaylandSurfaceId, size: SizeI) {
        self.pending.insert(
            surface,
            PendingResizeConfigure {
                surface,
                size,
                resizing: false,
            },
        );
    }

    pub fn cancel(&mut self, surface: WaylandSurfaceId) {
        self.pending.remove(&surface);
    }

    pub fn defer(&mut self, configure: PendingResizeConfigure) {
        self.pending.insert(configure.surface, configure);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = PendingResizeConfigure> + '_ {
        std::mem::take(&mut self.pending).into_values()
    }
}

/// Fixed opposite edges retained while an xdg-toplevel resize transaction is in flight.
/// This lets a cell-based client commit a size smaller than the configured maximum without
/// turning top/left resizing into window movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResizeAnchor {
    edge: ResizeEdge,
    fixed_right: i32,
    fixed_bottom: i32,
}

impl ResizeAnchor {
    pub fn new(position: PointI, size: SizeI, edge: ResizeEdge) -> Self {
        Self {
            edge,
            fixed_right: position.x.saturating_add(size.width),
            fixed_bottom: position.y.saturating_add(size.height),
        }
    }

    pub fn reconcile_position(self, preview_position: PointI, committed_size: SizeI) -> PointI {
        PointI {
            x: if resizes_left(self.edge) {
                self.fixed_right.saturating_sub(committed_size.width)
            } else {
                preview_position.x
            },
            y: if resizes_top(self.edge) {
                self.fixed_bottom.saturating_sub(committed_size.height)
            } else {
                preview_position.y
            },
        }
    }

    pub fn committed_buffer_offset(self, preview_size: SizeI, committed_size: SizeI) -> PointI {
        PointI {
            x: if resizes_left(self.edge) {
                preview_size.width.saturating_sub(committed_size.width)
            } else {
                0
            },
            y: if resizes_top(self.edge) {
                preview_size.height.saturating_sub(committed_size.height)
            } else {
                0
            },
        }
    }
}

fn resizes_left(edge: ResizeEdge) -> bool {
    matches!(
        edge,
        ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
    )
}

fn resizes_top(edge: ResizeEdge) -> bool {
    matches!(
        edge,
        ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
    )
}

pub(super) fn acknowledged_final_resize(
    expected_size: Option<SizeI>,
    acknowledged: Option<XdgConfigure>,
) -> bool {
    expected_size.is_some()
        && acknowledged
            .is_some_and(|configure| !configure.states.resizing && configure.size == expected_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface() -> WaylandSurfaceId {
        WaylandSurfaceId::from_raw(7).unwrap()
    }

    #[test]
    fn high_rate_motion_coalesces_to_one_configure_per_turn() {
        let mut scheduler = ConfigureScheduler::default();
        for width in 1..=10_000 {
            scheduler.schedule_resize(surface(), SizeI { width, height: 480 });
        }

        assert_eq!(
            scheduler.drain().collect::<Vec<_>>(),
            vec![PendingResizeConfigure {
                surface: surface(),
                size: SizeI {
                    width: 10_000,
                    height: 480,
                },
                resizing: true,
            }]
        );
    }

    #[test]
    fn same_turn_release_replaces_motion_with_one_final_configure() {
        let mut scheduler = ConfigureScheduler::default();
        scheduler.schedule_resize(
            surface(),
            SizeI {
                width: 801,
                height: 600,
            },
        );
        scheduler.schedule_final(
            surface(),
            SizeI {
                width: 801,
                height: 600,
            },
        );

        assert_eq!(
            scheduler.drain().collect::<Vec<_>>(),
            vec![PendingResizeConfigure {
                surface: surface(),
                size: SizeI {
                    width: 801,
                    height: 600,
                },
                resizing: false,
            }]
        );
    }

    #[test]
    fn a_deferred_configure_is_replaced_by_newer_motion() {
        let mut scheduler = ConfigureScheduler::default();
        scheduler.defer(PendingResizeConfigure {
            surface: surface(),
            size: SizeI {
                width: 700,
                height: 500,
            },
            resizing: true,
        });
        scheduler.schedule_resize(
            surface(),
            SizeI {
                width: 900,
                height: 650,
            },
        );

        assert_eq!(
            scheduler.drain().collect::<Vec<_>>(),
            vec![PendingResizeConfigure {
                surface: surface(),
                size: SizeI {
                    width: 900,
                    height: 650,
                },
                resizing: true,
            }]
        );
    }

    #[test]
    fn destroyed_surface_cancels_its_pending_configure() {
        let mut scheduler = ConfigureScheduler::default();
        scheduler.schedule_resize(
            surface(),
            SizeI {
                width: 900,
                height: 700,
            },
        );
        scheduler.cancel(surface());
        assert!(scheduler.drain().next().is_none());
    }

    #[test]
    fn only_the_matching_final_ack_reconciles_a_resize_commit() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState};

        let size = SizeI {
            width: 801,
            height: 603,
        };
        let configure = |configured_size, resizing| XdgConfigure {
            serial: 4,
            size: Some(configured_size),
            bounds: None,
            states: ToplevelState {
                resizing,
                ..ToplevelState::default()
            },
            decoration: DecorationMode::ServerSide,
        };
        assert!(!acknowledged_final_resize(
            Some(size),
            Some(configure(size, true))
        ));
        assert!(!acknowledged_final_resize(
            Some(size),
            Some(configure(
                SizeI {
                    width: 800,
                    height: 600,
                },
                false,
            ))
        ));
        assert!(acknowledged_final_resize(
            Some(size),
            Some(configure(size, false))
        ));
    }

    #[test]
    fn left_and_top_edges_stay_fixed_when_client_snaps_size() {
        let anchor = ResizeAnchor::new(
            PointI { x: 100, y: 80 },
            SizeI {
                width: 800,
                height: 600,
            },
            ResizeEdge::TopLeft,
        );
        assert_eq!(
            anchor.reconcile_position(
                PointI { x: 43, y: 29 },
                SizeI {
                    width: 832,
                    height: 624,
                },
            ),
            PointI { x: 68, y: 56 }
        );
        assert_eq!(
            anchor.committed_buffer_offset(
                SizeI {
                    width: 850,
                    height: 650,
                },
                SizeI {
                    width: 832,
                    height: 624,
                },
            ),
            PointI { x: 18, y: 26 }
        );
    }
}
