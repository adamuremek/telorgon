extern crate self as lithic_theme;
pub use lithic_core as foundation;

mod abi;
pub mod dsl;
mod evaluator;
mod id;
mod loader;
mod material;
mod model;
mod node;
mod package;
pub mod surface;
mod transition;

pub use abi::THEME_API_VERSION;
pub use id::{ThemeOutputId, ThemeViewId};
pub use loader::{ThemeRuntime, ThemeRuntimeError};
pub use material::{MaterialDecl, MaterialKind};
pub use model::{
    CursorTheme, OutputModel, OutputTheme, ThemeFrame, ThemeImage, ThemeInput, WindowModel,
    WindowTheme,
};
pub use node::{ThemeNode, WindowControlButton, WindowControlHoverEffect, WindowControlKind};
pub use package::{
    ThemeAssetStore, ThemeCapabilities, ThemeCursorAsset, ThemeImageAsset, ThemePackage,
    ThemePackageError, ThemeRecipe, ThemeStyle, ThemeWindowStyle, unpack_packed_theme,
    write_packed_theme,
};
pub use surface::{
    AssetRef, BorderPaint, ButtonPaint, ButtonShape, ChromeButton, ChromeButtonGroup,
    CompositorRequest, CrossAxisAlignment, EdgeInsetsI, FontWeight, FrameElement, FrameRegion,
    FrameRegionRole, FrameSlot, IconRef, RowLayout, ShadowPaint, SurfaceFrame, SurfacePaint,
    SurfaceRequest, SurfaceTheme, TextAlignment, TextElement, TextElementStyle, TextValue,
    WindowData, WindowSurfaceTheme,
};
pub use transition::{ThemeTransition, TransitionCurve};
