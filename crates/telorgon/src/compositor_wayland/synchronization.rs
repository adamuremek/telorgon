use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;

use crate::compositor_wayland::{ProtocolObjectId, WaylandBufferId, WaylandSurfaceId};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferUseId(NonZeroU64);

impl BufferUseId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceFrameCallback {
    pub object: ProtocolObjectId,
    pub surface: WaylandSurfaceId,
    pub commit_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferRelease {
    pub use_id: BufferUseId,
    pub buffer: WaylandBufferId,
    pub release_fence_token: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct BufferUse {
    buffer: WaylandBufferId,
    surface: WaylandSurfaceId,
    commit_revision: u64,
    pending_outputs: usize,
    explicit_release: bool,
}

#[derive(Debug, Default)]
pub struct BufferUseTracker {
    next_id: u64,
    uses: BTreeMap<BufferUseId, BufferUse>,
    callbacks: Vec<SurfaceFrameCallback>,
}

impl BufferUseTracker {
    pub fn begin_use(
        &mut self,
        buffer: WaylandBufferId,
        surface: WaylandSurfaceId,
        commit_revision: u64,
        output_count: usize,
        explicit_release: bool,
    ) -> Result<BufferUseId, BufferUseError> {
        if commit_revision == 0 || output_count == 0 {
            return Err(BufferUseError::InvalidUse);
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(BufferUseError::IdentityExhausted)?;
        let id =
            BufferUseId(NonZeroU64::new(self.next_id).ok_or(BufferUseError::IdentityExhausted)?);
        self.uses.insert(
            id,
            BufferUse {
                buffer,
                surface,
                commit_revision,
                pending_outputs: output_count,
                explicit_release,
            },
        );
        Ok(id)
    }

    pub fn add_frame_callback(&mut self, callback: SurfaceFrameCallback) {
        self.callbacks.push(callback);
    }

    pub fn presented(
        &mut self,
        use_id: BufferUseId,
        release_fence_token: Option<u64>,
    ) -> Result<Option<BufferRelease>, BufferUseError> {
        let use_state = self
            .uses
            .get_mut(&use_id)
            .ok_or(BufferUseError::UnknownUse)?;
        if use_state.pending_outputs == 0 {
            return Err(BufferUseError::AlreadyCompleted);
        }
        use_state.pending_outputs -= 1;
        if use_state.pending_outputs != 0 {
            return Ok(None);
        }
        let use_state = self.uses.remove(&use_id).expect("use remains present");
        if use_state.explicit_release && release_fence_token.is_none() {
            return Err(BufferUseError::MissingReleaseFence);
        }
        Ok(Some(BufferRelease {
            use_id,
            buffer: use_state.buffer,
            release_fence_token,
        }))
    }

    pub fn take_callbacks_for(
        &mut self,
        surface: WaylandSurfaceId,
        through_revision: u64,
    ) -> Vec<SurfaceFrameCallback> {
        let mut ready = Vec::new();
        self.callbacks.retain(|callback| {
            if callback.surface == surface && callback.commit_revision <= through_revision {
                ready.push(*callback);
                false
            } else {
                true
            }
        });
        ready
    }

    pub fn cancel_surface(&mut self, surface: WaylandSurfaceId) -> usize {
        let before = self.uses.len() + self.callbacks.len();
        self.uses
            .retain(|_, use_state| use_state.surface != surface);
        self.callbacks
            .retain(|callback| callback.surface != surface);
        before - self.uses.len() - self.callbacks.len()
    }

    pub fn use_revision(&self, use_id: BufferUseId) -> Option<u64> {
        self.uses
            .get(&use_id)
            .map(|use_state| use_state.commit_revision)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferUseError {
    InvalidUse,
    IdentityExhausted,
    UnknownUse,
    AlreadyCompleted,
    MissingReleaseFence,
}

impl fmt::Display for BufferUseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Wayland buffer-use tracking failed: {self:?}")
    }
}

impl std::error::Error for BufferUseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffer_is_released_only_after_every_output_finishes() {
        let mut tracker = BufferUseTracker::default();
        let id = tracker
            .begin_use(
                WaylandBufferId::from_raw(1).unwrap(),
                WaylandSurfaceId::from_raw(2).unwrap(),
                3,
                2,
                false,
            )
            .unwrap();
        assert_eq!(tracker.presented(id, None).unwrap(), None);
        assert!(tracker.presented(id, None).unwrap().is_some());
    }
}
