extern crate self as lithic_compositor;

pub use lithic_core as core;
pub use lithic_render as render;
pub use lithic_theme as theme;
pub use lithic_ui as ui;

mod chrome;
mod command;
mod controller;
mod id;
mod runtime;
mod surface;

pub use chrome::{
    ChromeMaterial, WINDOW_ACTION_CLOSE, WINDOW_ACTION_TOGGLE_EXPAND, WindowChrome, WindowShadow,
};
pub use command::{CreateDesktopSurface, CreateLayerSurface, CreateWindowSurface, SurfaceCommand};
pub use controller::{
    HitRegion, HitRegionKind, HoveredAction, SurfaceController, SurfaceError, SurfaceResult,
    TickInput, TickOutput,
};
pub use id::SurfaceId;
pub use runtime::{CompositorConfig, CompositorRuntime, SurfaceRenderer, run_compositor};
pub use surface::{
    DesktopSurface, LayerAnchor, LayerSurface, LayerSurfaceRole, Surface, SurfaceContent,
    SurfaceKind, WindowSurface,
};
pub use ui::{
    Action, Align, Alignment, Button, ButtonRow, ControlGroup, CrossAxisAlignment, EdgeInsetsI,
    HStack, Icon, IconButton, MainAxisAlignment, Padding, Spacer, Stack, Text, VStack, Widget,
    WidgetTree,
};
