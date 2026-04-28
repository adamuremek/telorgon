extern crate self as lithic_scene;

pub use lithic_core as core;

mod entity;
mod world;

pub use entity::EntityId;
pub use world::{SceneComponent, SceneWorld};
