use std::collections::BTreeMap;

use crate::compositor_wayland::{ResizeEdge, WaylandSurfaceId, XdgSurfaceState};
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

/// The terminal configure for one interactive resize.
///
/// The size is known when the pointer grab ends, but the protocol serial does not exist until the
/// scheduler emits the configure. Keeping both prevents an unrelated activation configure with the
/// same size from being mistaken for the end of the resize transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FinalResizeConfigure {
    pub size: SizeI,
    pub serial: Option<u32>,
}

impl FinalResizeConfigure {
    pub fn pending(size: SizeI) -> Self {
        Self { size, serial: None }
    }

    pub fn record_sent(&mut self, size: SizeI, serial: u32) {
        if self.size == size {
            self.serial = Some(serial);
        }
    }

    pub fn was_acknowledged(self, xdg_surface: Option<&XdgSurfaceState>) -> bool {
        self.serial.is_some_and(|serial| {
            xdg_surface.is_some_and(|xdg_surface| xdg_surface.configure_was_acknowledged(serial))
        })
    }
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
    fn final_resize_does_not_complete_before_its_configure_is_sent() {
        let size = SizeI {
            width: 801,
            height: 603,
        };
        assert!(!FinalResizeConfigure::pending(size).was_acknowledged(None));
    }

    #[test]
    fn only_the_emitted_final_serial_can_complete_a_resize() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState, XdgConfigure};

        let surface = surface();
        let size = SizeI {
            width: 801,
            height: 603,
        };
        let mut xdg = XdgSurfaceState::new(surface);
        for serial in [4, 5] {
            xdg.queue_configure(XdgConfigure {
                serial,
                size: Some(size),
                bounds: None,
                states: ToplevelState::default(),
                decoration: DecorationMode::ServerSide,
            })
            .unwrap();
        }
        let mut final_resize = FinalResizeConfigure::pending(size);
        final_resize.record_sent(size, 4);

        assert!(!final_resize.was_acknowledged(Some(&xdg)));
        xdg.ack_configure(5).unwrap();
        assert!(final_resize.was_acknowledged(Some(&xdg)));
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
