//! Pure startup declarations for GUI applications and Linux desktop environments.

use std::fmt;
use std::path::PathBuf;

use crate::assets::{
    AppIconProfile, AssetBundle, ClientCursorMode, CursorThemeAsset, PointerConfiguration,
    PointerThemeOverrides,
};
use crate::compose::{Component, ErasedComponent, RuntimeTarget};
use crate::core::{ColorRgba8, SizeI};
use crate::runtime::CompositionDriver;
use crate::window_chrome::{ShellActionId, WindowChromeModel};

use crate::application_host::{AppError, AppResult, WindowDecorationMode, WindowOptions};

/// Renderer policy selected by an application declaration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Renderer {
    /// Uses the entrypoint's platform default renderer policy.
    #[default]
    Auto,
    /// Requires the Vulkan renderer.
    Vulkan,
    /// Requires the deterministic software renderer.
    Software,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinuxDesktopConfig {
    pub drm_device: PathBuf,
    pub seat_name: String,
    pub socket_name: Option<String>,
    pub output_scale: i32,
    pub window_border: i32,
    pub titlebar_height: i32,
    /// Opaque content veil during interactive resize and while awaiting the final client image.
    /// Only the preview geometry changes during the drag; the client receives its final size on
    /// release. Alpha must be 255 so stale content and resize gaps cannot show through.
    pub resize_preview_color: ColorRgba8,
    pub pointer_extent: SizeI,
}

impl Default for LinuxDesktopConfig {
    fn default() -> Self {
        Self {
            drm_device: PathBuf::from("/dev/dri/card0"),
            seat_name: "seat0".to_owned(),
            socket_name: None,
            output_scale: 1,
            window_border: 4,
            titlebar_height: 32,
            resize_preview_color: ColorRgba8 {
                r: 38,
                g: 42,
                b: 48,
                a: 255,
            },
            pointer_extent: SizeI {
                width: 32,
                height: 32,
            },
        }
    }
}

impl LinuxDesktopConfig {
    fn validate(&self) -> AppResult<()> {
        if !self.drm_device.is_absolute()
            || self.seat_name.trim().is_empty()
            || self
                .socket_name
                .as_ref()
                .is_some_and(|name| name.trim().is_empty())
            || self.output_scale <= 0
            || self.window_border < 0
            || self.titlebar_height < 0
            || self.resize_preview_color.a != 255
            || self.pointer_extent.width <= 0
            || self.pointer_extent.height <= 0
        {
            Err(AppError::new("invalid Linux desktop configuration"))
        } else {
            Ok(())
        }
    }
}

/// Namespace for Telorgon's two application constructors.
pub struct Application {
    _private: (),
}

impl Application {
    /// Begins one ordinary managed GUI application declaration.
    pub fn gui(name: impl Into<String>) -> GuiApplication {
        GuiApplication {
            name: name.into(),
            renderer: Renderer::Auto,
            assets: AssetBundle::EMPTY,
            pointer: PointerConfiguration::default(),
        }
    }

    /// Begins one Linux desktop-environment declaration.
    pub fn desktop_environment(name: impl Into<String>) -> DesktopEnvironment {
        DesktopEnvironment {
            name: name.into(),
            renderer: Renderer::Auto,
            linux: LinuxDesktopConfig::default(),
            assets: AssetBundle::EMPTY,
            pointer: PointerConfiguration::default(),
            app_icon: AppIconProfile::new(),
        }
    }
}

impl fmt::Debug for Application {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Application")
    }
}

/// Incomplete GUI application declaration that still requires its initial window.
pub struct GuiApplication {
    name: String,
    renderer: Renderer,
    assets: AssetBundle,
    pointer: PointerConfiguration,
}

impl GuiApplication {
    /// Selects the renderer policy for this application.
    pub fn renderer(mut self, renderer: Renderer) -> Self {
        self.renderer = renderer;
        self
    }

    /// Registers the project's generated media catalog for every managed GUI subsystem.
    pub fn assets(mut self, assets: AssetBundle) -> Self {
        self.assets = assets;
        self
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.pointer = self.pointer.cursor_theme(theme);
        self
    }

    pub fn pointer_overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.pointer = self.pointer.overrides(overrides);
        self
    }

    /// Installs the single initial window supported by the current managed runtime.
    pub fn window(self, window: ReadyWindow) -> ReadyGuiApplication {
        ReadyGuiApplication {
            name: self.name,
            renderer: self.renderer,
            assets: self.assets,
            pointer: self.pointer,
            window,
        }
    }
}

impl fmt::Debug for GuiApplication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GuiApplication")
            .field("name", &self.name)
            .field("renderer", &self.renderer)
            .field("assets", &self.assets.len())
            .field("has_window", &false)
            .finish()
    }
}

/// Complete GUI application declaration.
pub struct ReadyGuiApplication {
    name: String,
    renderer: Renderer,
    assets: AssetBundle,
    pointer: PointerConfiguration,
    window: ReadyWindow,
}

impl ReadyGuiApplication {
    /// Replaces the renderer policy without changing the declared window.
    pub fn renderer(mut self, renderer: Renderer) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn assets(mut self, assets: AssetBundle) -> Self {
        self.assets = assets;
        self
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.pointer = self.pointer.cursor_theme(theme);
        self
    }

    pub fn pointer_overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.pointer = self.pointer.overrides(overrides);
        self
    }

    /// Runs the managed GUI application.
    pub fn run(self) -> AppResult<()> {
        #[cfg(any(
            feature = "application-software",
            all(feature = "application-vulkan-windows", target_os = "windows")
        ))]
        {
            return crate::application_host::native::run_gui(self);
        }

        #[cfg(not(any(
            feature = "application-software",
            all(feature = "application-vulkan-windows", target_os = "windows")
        )))]
        {
            let _ = self.into_parts()?;
            Err(AppError::new(
                "no managed GUI runtime is enabled in this build",
            ))
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> AppResult<(
        CompositionDriver,
        WindowOptions,
        Renderer,
        AssetBundle,
        PointerConfiguration,
    )> {
        validate_application_name(&self.name)?;
        self.assets
            .validate()
            .map_err(|error| AppError::new(error.to_string()))?;
        self.pointer
            .load_theme(self.assets)
            .map_err(|error| AppError::new(error.to_string()))?;
        let (driver, options) = self.window.into_parts()?;
        Ok((driver, options, self.renderer, self.assets, self.pointer))
    }
}

impl fmt::Debug for ReadyGuiApplication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GuiApplication")
            .field("name", &self.name)
            .field("renderer", &self.renderer)
            .field("assets", &self.assets.len())
            .field("window", &self.window)
            .finish()
    }
}

/// Incomplete initial managed application window.
pub struct Window {
    options: WindowOptions,
}

impl Window {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            options: WindowOptions {
                title: title.into(),
                ..WindowOptions::default()
            },
        }
    }

    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.options.size = SizeI { width, height };
        self
    }

    pub fn minimum_size(mut self, width: i32, height: i32) -> Self {
        self.options.min_size = Some(SizeI { width, height });
        self
    }

    pub fn without_minimum_size(mut self) -> Self {
        self.options.min_size = None;
        self
    }

    /// Selects whether the native platform draws its standard non-client frame.
    pub fn decorations(mut self, decorations: WindowDecorationMode) -> Self {
        self.options.decorations = decorations;
        self
    }

    pub fn without_system_frame(self) -> Self {
        self.decorations(WindowDecorationMode::Hidden)
    }

    /// Uses a composition root built with [`crate::window_frame`] as client-side chrome.
    pub fn custom_frame(self) -> Self {
        self.without_system_frame()
    }

    /// Assigns the icon profile shared by native window metadata and composed client chrome.
    pub fn icon(mut self, profile: AppIconProfile) -> Self {
        self.options.icon = profile;
        self
    }

    /// Completes this window with its composition root.
    pub fn content<C: Component>(self, component: C) -> ReadyWindow {
        ReadyWindow {
            options: self.options,
            content: CompositionDriver::new(component),
        }
    }
}

impl fmt::Debug for Window {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Window")
            .field("options", &self.options)
            .field("has_content", &false)
            .finish()
    }
}

/// Complete initial managed application window.
pub struct ReadyWindow {
    options: WindowOptions,
    content: CompositionDriver,
}

impl ReadyWindow {
    fn into_parts(self) -> AppResult<(CompositionDriver, WindowOptions)> {
        if self.options.title.trim().is_empty() {
            return Err(AppError::new("Window title must not be empty"));
        }
        if self.options.size.width <= 0 || self.options.size.height <= 0 {
            return Err(AppError::new("Window size must be positive"));
        }
        self.options
            .icon
            .validate()
            .map_err(|error| AppError::new(error.to_string()))?;
        Ok((self.content, self.options))
    }
}

impl fmt::Debug for ReadyWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Window")
            .field("options", &self.options)
            .field("has_content", &true)
            .finish()
    }
}

/// Placement anchor for a shell widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellWidgetAnchor {
    #[default]
    Top,
    Right,
    Bottom,
    Left,
    Floating,
}

/// One shell-widget extent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellWidgetExtent {
    Fill,
    Pixels(f32),
}

impl Default for ShellWidgetExtent {
    fn default() -> Self {
        Self::Pixels(320.0)
    }
}

/// Incomplete shell-widget declaration.
pub struct ShellWidget {
    name: String,
    anchor: ShellWidgetAnchor,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
    reserved_space: f32,
}

impl ShellWidget {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            anchor: ShellWidgetAnchor::default(),
            width: ShellWidgetExtent::Fill,
            height: ShellWidgetExtent::Pixels(36.0),
            reserved_space: 0.0,
        }
    }

    pub fn anchor(mut self, anchor: ShellWidgetAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn width(mut self, width: ShellWidgetExtent) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: ShellWidgetExtent) -> Self {
        self.height = height;
        self
    }

    pub fn reserve_space(mut self, logical_pixels: f32) -> Self {
        self.reserved_space = logical_pixels;
        self
    }

    /// Completes this shell widget with its composition root.
    pub fn content<C: Component>(self, component: C) -> ReadyShellWidget {
        ReadyShellWidget {
            name: self.name,
            anchor: self.anchor,
            width: self.width,
            height: self.height,
            reserved_space: self.reserved_space,
            content: CompositionDriver::for_target(component, RuntimeTarget::ShellWidget),
        }
    }
}

impl fmt::Debug for ShellWidget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellWidget")
            .field("name", &self.name)
            .field("anchor", &self.anchor)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("reserved_space", &self.reserved_space)
            .field("has_content", &false)
            .finish()
    }
}

/// Complete shell-widget declaration.
pub struct ReadyShellWidget {
    name: String,
    anchor: ShellWidgetAnchor,
    width: ShellWidgetExtent,
    height: ShellWidgetExtent,
    reserved_space: f32,
    content: CompositionDriver,
}

impl ReadyShellWidget {
    fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::new("ShellWidget name must not be empty"));
        }
        let valid_extent = |extent: ShellWidgetExtent| match extent {
            ShellWidgetExtent::Fill => true,
            ShellWidgetExtent::Pixels(value) => value.is_finite() && value > 0.0,
        };
        if !valid_extent(self.width)
            || !valid_extent(self.height)
            || !self.reserved_space.is_finite()
            || self.reserved_space < 0.0
        {
            return Err(AppError::new(
                "ShellWidget extents and reserved space must be finite and positive",
            ));
        }
        debug_assert_eq!(self.content.target(), RuntimeTarget::ShellWidget);
        Ok(())
    }

    #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
    pub(crate) fn into_runtime_parts(
        self,
    ) -> (
        String,
        ShellWidgetAnchor,
        ShellWidgetExtent,
        ShellWidgetExtent,
        f32,
        CompositionDriver,
    ) {
        (
            self.name,
            self.anchor,
            self.width,
            self.height,
            self.reserved_space,
            self.content,
        )
    }
}

impl fmt::Debug for ReadyShellWidget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellWidget")
            .field("name", &self.name)
            .field("anchor", &self.anchor)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("reserved_space", &self.reserved_space)
            .field("has_content", &true)
            .finish()
    }
}

/// A named Telorgon composition used by the compositor for shell-owned pixels.
pub struct CompositorVisual {
    name: String,
    content: CompositionDriver,
}

/// One frame action authorized by the compositor declaration.
pub(crate) struct ShellActionHandler {
    id: ShellActionId,
    #[cfg_attr(
        not(all(feature = "desktop-wayland-linux", target_os = "linux")),
        allow(dead_code)
    )]
    invoke: Box<dyn Fn(WindowChromeModel)>,
}

impl ShellActionHandler {
    pub(crate) const fn id(&self) -> ShellActionId {
        self.id
    }

    #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
    pub(crate) fn invoke(&self, model: WindowChromeModel) {
        (self.invoke)(model);
    }
}

impl fmt::Debug for ShellActionHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellActionHandler")
            .field("id", &self.id)
            .finish()
    }
}

/// Creates a fresh compositor-owned frame composition for one Wayland toplevel.
///
/// Implementations are reusable templates. The host supplies one immutable
/// [`WindowChromeModel`] for each frame instance, while the template owns any shared visual
/// configuration needed to construct that instance.
pub trait WindowFrameTemplate: 'static {
    type Component: Component;

    fn compose(&self, model: WindowChromeModel) -> Self::Component;
}

impl<F, C> WindowFrameTemplate for F
where
    F: Fn(WindowChromeModel) -> C + 'static,
    C: Component,
{
    type Component = C;

    fn compose(&self, model: WindowChromeModel) -> Self::Component {
        self(model)
    }
}

/// Type-erased storage for one reusable [`WindowFrameTemplate`].
///
/// The model is the only shell-owned input. Everything visual and interactive is authored with
/// normal composition primitives plus the explicit frame/content/action roles.
pub struct WindowFrameFactory {
    #[cfg_attr(
        not(all(feature = "desktop-wayland-linux", target_os = "linux")),
        allow(dead_code)
    )]
    compose: Box<dyn Fn(WindowChromeModel) -> Box<dyn ErasedComponent>>,
}

impl WindowFrameFactory {
    fn new<T>(template: T) -> Self
    where
        T: WindowFrameTemplate,
    {
        Self {
            compose: Box::new(move |model| Box::new(template.compose(model))),
        }
    }

    #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
    pub(crate) fn compose(&self, model: WindowChromeModel) -> CompositionDriver {
        CompositionDriver::from_erased_for_target(self.candidate(model), RuntimeTarget::Compositor)
    }

    #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
    pub(crate) fn candidate(&self, model: WindowChromeModel) -> Box<dyn ErasedComponent> {
        (self.compose)(model)
    }
}

impl fmt::Debug for WindowFrameFactory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WindowFrameFactory")
    }
}

impl CompositorVisual {
    fn new(name: impl Into<String>, component: impl Component) -> Self {
        Self {
            name: name.into(),
            content: CompositionDriver::for_target(component, RuntimeTarget::Compositor),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for CompositorVisual {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompositorVisual")
            .field("name", &self.name)
            .field("has_content", &true)
            .finish()
    }
}

/// Incomplete compositor declaration.
pub struct Compositor {
    window_frame: Option<WindowFrameFactory>,
    pointer: Option<CompositorVisual>,
    icons: Vec<CompositorVisual>,
    shell_actions: Vec<ShellActionHandler>,
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compositor {
    pub const fn new() -> Self {
        Self {
            window_frame: None,
            pointer: None,
            icons: Vec::new(),
            shell_actions: Vec::new(),
        }
    }

    /// Supplies the composed server-side frame for each window and each relevant model change.
    pub fn window_frame<T>(mut self, template: T) -> Self
    where
        T: WindowFrameTemplate,
    {
        self.window_frame = Some(WindowFrameFactory::new(template));
        self
    }

    /// Uses an ordinary Telorgon component for the default compositor-owned pointer image.
    pub fn pointer<C: Component>(mut self, component: C) -> Self {
        self.pointer = Some(CompositorVisual::new("default", component));
        self
    }

    /// Adds a semantic shell icon. Names are stable compositor keys such as `window.close`.
    pub fn icon<C: Component>(mut self, name: impl Into<String>, component: C) -> Self {
        self.icons.push(CompositorVisual::new(name, component));
        self
    }

    /// Authorizes one custom frame action and supplies its host-side handler.
    pub fn shell_action<F>(mut self, id: ShellActionId, handler: F) -> Self
    where
        F: Fn(WindowChromeModel) + 'static,
    {
        self.shell_actions.push(ShellActionHandler {
            id,
            invoke: Box::new(handler),
        });
        self
    }

    /// Completes the compositor with its full-output visual rendered behind client windows.
    pub fn background<C: Component>(self, background: C) -> ReadyCompositor {
        ReadyCompositor {
            background: CompositionDriver::for_target(background, RuntimeTarget::Compositor),
            window_frame: self.window_frame,
            pointer: self.pointer,
            icons: self.icons,
            shell_actions: self.shell_actions,
        }
    }

    /// Compatibility alias for [`Compositor::background`].
    #[deprecated(
        since = "0.1.15",
        note = "the component is a compositor background visual; use `background`"
    )]
    pub fn policy<C: Component>(self, background: C) -> ReadyCompositor {
        self.background(background)
    }
}

impl fmt::Debug for Compositor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Compositor")
            .field("has_background", &false)
            .field("has_window_frame", &self.window_frame.is_some())
            .field("has_pointer", &self.pointer.is_some())
            .field("icons", &self.icons.len())
            .field("shell_actions", &self.shell_actions.len())
            .finish()
    }
}

/// Complete compositor declaration.
pub struct ReadyCompositor {
    background: CompositionDriver,
    window_frame: Option<WindowFrameFactory>,
    pointer: Option<CompositorVisual>,
    icons: Vec<CompositorVisual>,
    shell_actions: Vec<ShellActionHandler>,
}

#[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
type CompositorRuntimeParts = (
    CompositionDriver,
    Option<WindowFrameFactory>,
    Option<CompositionDriver>,
    Vec<(String, CompositionDriver)>,
    Vec<ShellActionHandler>,
);

impl ReadyCompositor {
    fn validate(&self) -> AppResult<()> {
        debug_assert_eq!(self.background.target(), RuntimeTarget::Compositor);
        for visual in self.pointer.iter().chain(self.icons.iter()) {
            debug_assert_eq!(visual.content.target(), RuntimeTarget::Compositor);
        }
        if self.icons.iter().any(|icon| icon.name.trim().is_empty()) {
            return Err(AppError::new("Compositor icon names must not be empty"));
        }
        let mut names = std::collections::HashSet::new();
        if self
            .icons
            .iter()
            .any(|icon| !names.insert(icon.name.as_str()))
        {
            return Err(AppError::new("Compositor icon names must be unique"));
        }
        let mut actions = std::collections::HashSet::new();
        if self
            .shell_actions
            .iter()
            .any(|handler| !actions.insert(handler.id()))
        {
            return Err(AppError::new("Compositor shell action IDs must be unique"));
        }
        Ok(())
    }

    pub fn window_frame(&self) -> Option<&WindowFrameFactory> {
        self.window_frame.as_ref()
    }

    pub fn pointer(&self) -> Option<&CompositorVisual> {
        self.pointer.as_ref()
    }

    pub fn icons(&self) -> &[CompositorVisual] {
        &self.icons
    }

    pub fn authorizes_shell_action(&self, id: ShellActionId) -> bool {
        self.shell_actions.iter().any(|handler| handler.id == id)
    }

    #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
    pub(crate) fn into_runtime_parts(self) -> CompositorRuntimeParts {
        (
            self.background,
            self.window_frame,
            self.pointer.map(|visual| visual.content),
            self.icons
                .into_iter()
                .map(|visual| (visual.name, visual.content))
                .collect(),
            self.shell_actions,
        )
    }
}

impl fmt::Debug for ReadyCompositor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Compositor")
            .field("has_background", &true)
            .field("has_window_frame", &self.window_frame.is_some())
            .field("has_pointer", &self.pointer.is_some())
            .field("icons", &self.icons.len())
            .field("shell_actions", &self.shell_actions.len())
            .finish()
    }
}

/// Desktop-environment declaration that still requires its compositor.
pub struct DesktopEnvironment {
    name: String,
    renderer: Renderer,
    linux: LinuxDesktopConfig,
    assets: AssetBundle,
    pointer: PointerConfiguration,
    app_icon: AppIconProfile,
}

impl DesktopEnvironment {
    pub fn renderer(mut self, renderer: Renderer) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn linux(mut self, config: LinuxDesktopConfig) -> Self {
        self.linux = config;
        self
    }

    /// Registers one catalog shared by shell composition and hosted Wayland clients.
    pub fn assets(mut self, assets: AssetBundle) -> Self {
        self.assets = assets;
        self
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.pointer = self.pointer.cursor_theme(theme);
        self
    }

    pub fn pointer_overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.pointer = self.pointer.overrides(overrides);
        self
    }

    pub fn client_cursor_mode(mut self, mode: ClientCursorMode) -> Self {
        self.pointer = self.pointer.client_mode(mode);
        self
    }

    /// Sets the desktop environment's fallback icon profile for client toplevels.
    pub fn app_icon(mut self, profile: AppIconProfile) -> Self {
        self.app_icon = profile;
        self
    }

    pub fn compositor(self, compositor: ReadyCompositor) -> DesktopEnvironmentWithCompositor {
        DesktopEnvironmentWithCompositor {
            name: self.name,
            renderer: self.renderer,
            linux: self.linux,
            assets: self.assets,
            pointer: self.pointer,
            app_icon: self.app_icon,
            compositor,
        }
    }
}

impl fmt::Debug for DesktopEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopEnvironment")
            .field("name", &self.name)
            .field("renderer", &self.renderer)
            .field("assets", &self.assets.len())
            .field("has_compositor", &false)
            .finish()
    }
}

/// Desktop-environment declaration that still requires its first shell widget.
pub struct DesktopEnvironmentWithCompositor {
    name: String,
    renderer: Renderer,
    linux: LinuxDesktopConfig,
    assets: AssetBundle,
    pointer: PointerConfiguration,
    app_icon: AppIconProfile,
    compositor: ReadyCompositor,
}

impl DesktopEnvironmentWithCompositor {
    pub fn renderer(mut self, renderer: Renderer) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn linux(mut self, config: LinuxDesktopConfig) -> Self {
        self.linux = config;
        self
    }

    pub fn assets(mut self, assets: AssetBundle) -> Self {
        self.assets = assets;
        self
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.pointer = self.pointer.cursor_theme(theme);
        self
    }

    pub fn pointer_overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.pointer = self.pointer.overrides(overrides);
        self
    }

    pub fn client_cursor_mode(mut self, mode: ClientCursorMode) -> Self {
        self.pointer = self.pointer.client_mode(mode);
        self
    }

    pub fn app_icon(mut self, profile: AppIconProfile) -> Self {
        self.app_icon = profile;
        self
    }

    pub fn shell_widget(self, widget: ReadyShellWidget) -> ReadyDesktopEnvironment {
        ReadyDesktopEnvironment {
            name: self.name,
            renderer: self.renderer,
            linux: self.linux,
            assets: self.assets,
            pointer: self.pointer,
            app_icon: self.app_icon,
            compositor: self.compositor,
            shell_widgets: vec![widget],
        }
    }
}

impl fmt::Debug for DesktopEnvironmentWithCompositor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopEnvironment")
            .field("name", &self.name)
            .field("renderer", &self.renderer)
            .field("assets", &self.assets.len())
            .field("has_compositor", &true)
            .field("shell_widgets", &0)
            .finish()
    }
}

/// Complete desktop-environment declaration.
pub struct ReadyDesktopEnvironment {
    name: String,
    renderer: Renderer,
    linux: LinuxDesktopConfig,
    assets: AssetBundle,
    pointer: PointerConfiguration,
    app_icon: AppIconProfile,
    compositor: ReadyCompositor,
    shell_widgets: Vec<ReadyShellWidget>,
}

impl ReadyDesktopEnvironment {
    pub fn renderer(mut self, renderer: Renderer) -> Self {
        self.renderer = renderer;
        self
    }

    pub fn linux(mut self, config: LinuxDesktopConfig) -> Self {
        self.linux = config;
        self
    }

    pub fn assets(mut self, assets: AssetBundle) -> Self {
        self.assets = assets;
        self
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.pointer = self.pointer.cursor_theme(theme);
        self
    }

    pub fn pointer_overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.pointer = self.pointer.overrides(overrides);
        self
    }

    pub fn client_cursor_mode(mut self, mode: ClientCursorMode) -> Self {
        self.pointer = self.pointer.client_mode(mode);
        self
    }

    pub fn app_icon(mut self, profile: AppIconProfile) -> Self {
        self.app_icon = profile;
        self
    }

    pub fn shell_widget(mut self, widget: ReadyShellWidget) -> Self {
        self.shell_widgets.push(widget);
        self
    }

    /// Validates this declaration and enters the Linux desktop-environment runtime.
    pub fn run(self) -> AppResult<()> {
        #[cfg(all(feature = "desktop-wayland-linux", target_os = "linux"))]
        {
            crate::application_host::desktop_wayland::run(self)
        }
        #[cfg(not(all(feature = "desktop-wayland-linux", target_os = "linux")))]
        {
            let _ = self.into_parts()?;
            Err(AppError::new(
                "the Linux Wayland desktop runtime requires target Linux and feature desktop-wayland-linux",
            ))
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> AppResult<(
        String,
        ReadyCompositor,
        Vec<ReadyShellWidget>,
        Renderer,
        LinuxDesktopConfig,
        AssetBundle,
        PointerConfiguration,
        AppIconProfile,
    )> {
        validate_application_name(&self.name)?;
        self.assets
            .validate()
            .map_err(|error| AppError::new(error.to_string()))?;
        self.pointer
            .load_theme(self.assets)
            .map_err(|error| AppError::new(error.to_string()))?;
        self.app_icon
            .validate()
            .map_err(|error| AppError::new(error.to_string()))?;
        self.compositor.validate()?;
        for widget in &self.shell_widgets {
            widget.validate()?;
        }
        self.linux.validate()?;
        Ok((
            self.name,
            self.compositor,
            self.shell_widgets,
            self.renderer,
            self.linux,
            self.assets,
            self.pointer,
            self.app_icon,
        ))
    }
}

impl fmt::Debug for ReadyDesktopEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopEnvironment")
            .field("name", &self.name)
            .field("renderer", &self.renderer)
            .field("assets", &self.assets.len())
            .field("has_compositor", &true)
            .field("shell_widgets", &self.shell_widgets)
            .finish()
    }
}

fn validate_application_name(name: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        Err(AppError::new("Application name must not be empty"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{ComponentFields, View, text};

    #[test]
    fn resize_preview_color_is_configurable_but_must_hide_stale_content() {
        let mut config = LinuxDesktopConfig::default();
        config.drm_device = std::env::current_dir().unwrap().join("card0");
        assert_eq!(config.resize_preview_color.a, 255);
        config.resize_preview_color = ColorRgba8 {
            r: 20,
            g: 40,
            b: 60,
            a: 255,
        };
        assert!(config.validate().is_ok());
        config.resize_preview_color.a = 128;
        assert!(config.validate().is_err());
    }

    struct Root;

    impl ComponentFields for Root {
        type InputSnapshot = ();

        fn update_inputs(&mut self, _incoming: Self) -> bool {
            false
        }

        fn capture_inputs(&self) -> Self::InputSnapshot {}

        fn restore_inputs(&mut self, _snapshot: Self::InputSnapshot) -> bool {
            false
        }
    }

    impl Component for Root {
        fn view(&self) -> impl View {
            text(format!("{:?}", self.runtime_target()))
        }
    }

    #[test]
    fn mode_specific_roots_tag_their_composition_targets() {
        let window = Window::new("Window").content(Root);
        assert_eq!(window.content.target(), RuntimeTarget::Application);

        let widget = ShellWidget::new("Panel").content(Root);
        assert_eq!(widget.content.target(), RuntimeTarget::ShellWidget);

        let compositor = Compositor::new().background(Root);
        assert_eq!(compositor.background.target(), RuntimeTarget::Compositor);
    }

    #[test]
    fn deprecated_policy_alias_still_completes_the_background_visual() {
        #[allow(deprecated)]
        let compositor = Compositor::new().policy(Root);
        assert_eq!(compositor.background.target(), RuntimeTarget::Compositor);
    }

    #[test]
    fn named_frame_functions_implement_the_template_contract() {
        fn compose(_model: WindowChromeModel) -> Root {
            Root
        }

        let compositor = Compositor::new().window_frame(compose).background(Root);
        assert!(compositor.window_frame().is_some());
    }

    #[test]
    fn custom_shell_actions_require_unique_declaration_authorization() {
        let action = ShellActionId::named("window.pin");
        let compositor = Compositor::new()
            .shell_action(action, |_| {})
            .shell_action(action, |_| {})
            .background(Root);

        assert!(compositor.validate().is_err());
    }

    #[test]
    fn shell_widget_validation_happens_before_host_selection() {
        let result = Application::desktop_environment("Telorgon")
            .compositor(Compositor::new().background(Root))
            .shell_widget(ShellWidget::new("").reserve_space(-1.0).content(Root))
            .into_parts();
        assert!(result.is_err());
    }

    #[test]
    fn both_application_modes_own_renderer_selection() {
        let gui = Application::gui("Counter")
            .renderer(Renderer::Software)
            .window(Window::new("Counter").content(Root));
        assert_eq!(gui.renderer, Renderer::Software);

        let desktop = Application::desktop_environment("Telorgon")
            .renderer(Renderer::Vulkan)
            .compositor(Compositor::new().background(Root))
            .shell_widget(ShellWidget::new("Panel").content(Root));
        assert_eq!(desktop.renderer, Renderer::Vulkan);
    }

    #[test]
    fn complete_declarations_have_content_without_optional_storage() {
        let application =
            Application::gui("Counter").window(Window::new("Counter").size(480, 320).content(Root));
        let debug = format!("{application:?}");
        assert!(debug.contains("has_content: true"));
        assert!(debug.contains("renderer: Auto"));
    }

    #[test]
    fn managed_windows_retain_custom_frame_and_icon_options() {
        let icon = crate::IconAsset::new(crate::AssetKey::new("icons/app.svg"));
        let (_, options) = Window::new("Studio")
            .custom_frame()
            .icon(AppIconProfile::new().named("com.example.studio").icon(icon))
            .content(Root)
            .into_parts()
            .unwrap();

        assert_eq!(options.decorations, WindowDecorationMode::Hidden);
        assert_eq!(options.icon.name(), Some("com.example.studio"));
        assert_eq!(options.icon.preferred(64), Some(crate::Icon::new(icon)));
    }
}
