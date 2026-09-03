use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::compositor_wayland::{ResizeEdge, WaylandSurfaceId, XdgConfigure};
use crate::core::{PointI, SizeI};

/// Removes the next deferred SHM surface that does not already have a copy in flight.
///
/// A submitted surface remains in the queue so its latest deferred revision can be retried after
/// that surface's completion is observed. The bounded pass prevents an all-blocked queue from
/// spinning while still allowing unrelated surfaces behind a blocked entry to make progress.
pub(super) fn take_ready_deferred_shm_surface(
    deferred: &mut VecDeque<WaylandSurfaceId>,
    submitted: &BTreeSet<WaylandSurfaceId>,
) -> Option<WaylandSurfaceId> {
    let candidate_count = deferred.len();
    for _ in 0..candidate_count {
        let surface = deferred.pop_front()?;
        if submitted.contains(&surface) {
            deferred.push_back(surface);
        } else {
            return Some(surface);
        }
    }
    None
}

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
/// scheduler emits the configure. Keeping both prevents an older activation configure with the
/// same size from being mistaken for the end of the resize transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FinalResizeConfigure {
    pub size: SizeI,
    pub serial: Option<u32>,
    acknowledged: bool,
}

impl FinalResizeConfigure {
    pub fn pending(size: SizeI) -> Self {
        Self {
            size,
            serial: None,
            acknowledged: false,
        }
    }

    pub fn record_sent(&mut self, size: SizeI, serial: u32) {
        if self.size == size && self.serial.is_none() {
            self.serial = Some(serial);
        }
    }

    /// Retains acknowledgement independently from asynchronous image publication. A later SHM
    /// commit can supersede the worker result that carried this acknowledgement, but it still
    /// consumes the terminal configure and therefore completes the protocol transaction.
    pub fn observe_acknowledgement(&mut self, acknowledged: Option<XdgConfigure>) {
        self.acknowledged |= self
            .serial
            .zip(acknowledged)
            .is_some_and(|(serial, configure)| serial_was_superseded_by(serial, configure.serial));
    }

    pub fn was_acknowledged(self) -> bool {
        self.acknowledged
    }
}

/// Wayland serials are unsigned and may wrap. Protocol traffic cannot span half the serial space,
/// so a wrapping subtraction in the lower half identifies the same or a newer serial.
fn serial_was_superseded_by(serial: u32, acknowledged: u32) -> bool {
    acknowledged.wrapping_sub(serial) < (1_u32 << 31)
}

impl ConfigureScheduler {
    pub fn schedule_state(&mut self, surface: WaylandSurfaceId, size: SizeI, resizing: bool) {
        self.pending.insert(
            surface,
            PendingResizeConfigure {
                surface,
                size,
                resizing,
            },
        );
    }

    pub fn schedule_resize(&mut self, surface: WaylandSurfaceId, size: SizeI) {
        self.schedule_state(surface, size, true);
    }

    pub fn schedule_final(&mut self, surface: WaylandSurfaceId, size: SizeI) {
        self.schedule_state(surface, size, false);
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

    fn other_surface() -> WaylandSurfaceId {
        WaylandSurfaceId::from_raw(8).unwrap()
    }

    #[test]
    fn deferred_shm_copy_waits_for_its_own_submitted_copy() {
        let mut deferred = VecDeque::from([surface(), other_surface()]);
        let submitted = BTreeSet::from([surface()]);

        assert_eq!(
            take_ready_deferred_shm_surface(&mut deferred, &submitted),
            Some(other_surface())
        );
        assert_eq!(deferred, VecDeque::from([surface()]));
        assert_eq!(
            take_ready_deferred_shm_surface(&mut deferred, &submitted),
            None
        );
        assert_eq!(deferred, VecDeque::from([surface()]));
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
        assert!(!FinalResizeConfigure::pending(size).was_acknowledged());
    }

    #[test]
    fn the_final_or_a_newer_acked_configure_can_complete_a_resize() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState};

        let size = SizeI {
            width: 801,
            height: 603,
        };
        let configure = |serial| XdgConfigure {
            serial,
            size: Some(size),
            bounds: None,
            states: ToplevelState::default(),
            decoration: DecorationMode::ServerSide,
        };
        let mut final_resize = FinalResizeConfigure::pending(size);
        final_resize.record_sent(size, 4);

        final_resize.observe_acknowledgement(Some(configure(3)));
        assert!(!final_resize.was_acknowledged());
        final_resize.observe_acknowledgement(Some(configure(4)));
        assert!(final_resize.was_acknowledged());

        let mut newer = FinalResizeConfigure::pending(size);
        newer.record_sent(size, 4);
        newer.observe_acknowledgement(Some(configure(5)));
        assert!(newer.was_acknowledged());
    }

    #[test]
    fn final_configure_comparison_handles_serial_wraparound() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState};

        let size = SizeI {
            width: 801,
            height: 603,
        };
        let configure = |serial| XdgConfigure {
            serial,
            size: Some(size),
            bounds: None,
            states: ToplevelState::default(),
            decoration: DecorationMode::ServerSide,
        };
        let mut final_resize = FinalResizeConfigure::pending(size);
        final_resize.record_sent(size, u32::MAX);

        final_resize.observe_acknowledgement(Some(configure(1)));
        assert!(final_resize.was_acknowledged());

        let mut older = FinalResizeConfigure::pending(size);
        older.record_sent(size, u32::MAX);
        older.observe_acknowledgement(Some(configure(u32::MAX - 1)));
        assert!(!older.was_acknowledged());
    }

    #[test]
    fn acknowledgement_survives_a_superseded_image_publication() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState};

        let final_size = SizeI {
            width: 960,
            height: 640,
        };
        let configure = XdgConfigure {
            serial: 12,
            size: Some(final_size),
            bounds: None,
            states: ToplevelState::default(),
            decoration: DecorationMode::ServerSide,
        };
        let mut final_resize = FinalResizeConfigure::pending(final_size);
        final_resize.record_sent(final_size, configure.serial);

        final_resize.observe_acknowledgement(Some(configure));
        final_resize.observe_acknowledgement(None);

        assert!(final_resize.was_acknowledged());
    }

    #[test]
    fn newer_configure_supersedes_the_terminal_resize_for_cell_sized_clients() {
        use crate::compositor_wayland::{DecorationMode, ToplevelState};

        let final_size = SizeI {
            width: 960,
            height: 640,
        };
        let configure = XdgConfigure {
            serial: 13,
            size: Some(SizeI {
                width: 800,
                height: 600,
            }),
            bounds: None,
            states: ToplevelState::default(),
            decoration: DecorationMode::ServerSide,
        };
        let mut final_resize = FinalResizeConfigure::pending(final_size);
        final_resize.record_sent(final_size, 12);

        final_resize.observe_acknowledgement(Some(configure));

        assert!(final_resize.was_acknowledged());
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
    }
}
