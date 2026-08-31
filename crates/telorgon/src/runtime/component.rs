use std::convert::Infallible;

use crate::ui::UiRoot;

use crate::runtime::{CreateContext, Ui, UnmountContext, UpdateContext};

/// Immutable component configuration with runtime-owned per-instance state.
pub trait Component: Sized + 'static {
    type State: 'static;
    type Action: 'static;

    fn create(&self, cx: &mut CreateContext<'_>) -> Self::State;

    fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot;

    fn action(
        &self,
        state: &mut Self::State,
        action: Self::Action,
        cx: &mut UpdateContext<'_, Self>,
    );

    fn unmount(&self, _state: &mut Self::State, _cx: &mut UnmountContext<'_>) {}
}

/// Convenience action type for components that never receive an action.
pub type NoAction = Infallible;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentId {
    pub(crate) view: u64,
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LifecycleState {
    Allocated,
    Creating,
    Mounting,
    Mounted,
    Unmounting,
    Dead,
}
