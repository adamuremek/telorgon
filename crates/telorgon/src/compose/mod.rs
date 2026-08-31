//! Ergonomic rerender-and-reconcile authoring values for Telorgon.
//!
//! A [`View`] is a short-lived description. The runtime retains component instances and foundation
//! nodes, evaluates only dirty components, and reconciles a view into those existing nodes.

mod component;
mod components;
mod context;
mod element;
mod event;
mod key;
mod signal;
pub mod style;

pub use component::{
    Component, ComponentFields, ComponentInstanceId, ErasedComponent, RenderedView,
};
pub use components::{
    Button, ButtonElement, Checkbox, Container, ContainerElement, Image, ImageElement, Slider,
    SliderElement, Switch, Text, TextElement, ToggleElement, ToggleKind, button, card, checkbox,
    column, image, row, slider, spacer, stack, switch, text,
};
pub use context::{InputsChangedContext, MountContext, RuntimeTarget, UnmountContext};
pub use element::{Element, ElementKind, ElementType, View, ViewError};
pub use event::{ComponentCallback, EventContext, EventDispatch, EventHandler};
pub use key::{Key, hashed_key};
pub use signal::{Signal, SignalDependency, SignalSnapshot, SignalSubscription, SignalWriter};
pub use style::{Alignment, Dimension, Insets, TextStyle};

/// Items used by the generated component implementation. Not a stable direct-use API.
#[doc(hidden)]
pub mod __private {
    pub use crate::compose::ComponentFields;
}

#[cfg(test)]
mod tests;
