# Custom Windows, Assets, Icons, and Pointers

## Document role

This document describes Telorgon's current composable window-chrome, project-asset, application-icon,
and pointer-theme APIs. It also records the host/protocol boundary and compatibility decisions made
during the refactor.

## Complete managed-GUI example

`asset_catalog!` belongs to `telorgon-macros` and is re-exported by `telorgon`, so applications add
only the normal `telorgon` dependency. The macro embeds a project directory, generates nested typed
constants from its paths, and exposes one cheap `AssetBundle`.

```text
assets/
  icons/app.svg
  icons/app-32.png
  icons/close.svg
  icons/minimize.svg
  icons/maximize.svg
  cursors/default.toml
  cursors/arrow.svg
  cursors/link.svg
  cursors/text.svg
  cursors/resize-ew.svg
```

```rust,ignore
use telorgon::app::*;
use telorgon::{
    Background, ClientCursorMode, PointerGraphic, PointerIcon, PointerThemeOverrides,
};

asset_catalog! { pub mod assets = "assets"; }

const WHITE: ColorRgba8 = ColorRgba8::rgba(255, 255, 255, 255);

fn app_icons() -> AppIconProfile {
    AppIconProfile::new()
        // Desktop icon-theme name used when the environment can resolve one.
        .named("com.example.studio")
        // A scalable project-owned fallback recolored from its alpha mask.
        .icon(Icon::new(assets::icons::APP).tint(WHITE))
        // Exact raster variants win for their declared size.
        .icon_at(32, assets::icons::APP_32)
}

fn pointers() -> PointerThemeOverrides {
    // Override only exceptional states here; all other states come from default.toml or the OS.
    PointerThemeOverrides::new().set(
        PointerIcon::EwResize,
        PointerGraphic::new(assets::cursors::RESIZE_EW)
            .size(32)
            .hotspot(16, 16)
            .tint(WHITE),
    )
}

#[component]
struct Editor {}

impl Component for Editor {
    fn view(&self) -> impl View {
        column()
            .padding(24.0)
            .child(text("Document title").pointer_icon(PointerIcon::Text))
            .child(text("The application owns this content."))
    }
}

#[component]
struct ManagedFrame {}

impl Component for ManagedFrame {
    fn view(&self) -> impl View {
        let frame = BoxDecoration::new()
            .background(Background::Color(ColorRgba8::rgba(24, 27, 35, 255)))
            .uniform_border(1.0, ColorRgba8::rgba(71, 78, 96, 255))
            .corner_radius(14.0)
            .shadow(Shadow {
                offset: PointF { x: 0.0, y: 10.0 },
                blur: 28.0,
                spread: 0.0,
                color: ColorRgba8::rgba(0, 0, 0, 112),
            });
        let control = |label, icon, action| {
            button(label)
                .icon(icon)
                .icon_tint(WHITE)
                .icon_size(16.0)
                .width(38.0)
                .height(30.0)
                .decoration(
                    BoxDecoration::new()
                        .background(Background::Color(ColorRgba8::rgba(42, 47, 59, 255)))
                        .corner_radius(8.0),
                )
                .window_action(action)
        };

        window_frame()
            .decoration(frame)
            .child(
                row()
                    .height(46.0)
                    .padding((8.0, 8.0))
                    .gap(6.0)
                    .child(
                        image(assets::icons::APP)
                            .tint(WHITE)
                            .width(20.0)
                            .height(20.0)
                            .window_app_icon(),
                    )
                    .child(text("Studio").window_title())
                    .child(spacer())
                    .child(control(
                        "Minimize",
                        assets::icons::MINIMIZE,
                        WindowAction::Minimize,
                    ))
                    .child(control(
                        "Maximize",
                        assets::icons::MAXIMIZE,
                        WindowAction::ToggleMaximize,
                    ))
                    .child(control("Close", assets::icons::CLOSE, WindowAction::Close))
                    .window_drag_region(),
            )
            // Thin transparent overlays can be placed wherever resize affordances belong.
            .child(
                stack()
                    .width(6.0)
                    .window_resize(WindowResizeEdge::Left),
            )
            .content_slot(
                window_content_slot()
                    .margin((46.0, 6.0, 6.0, 6.0))
                    .child(Editor::default()),
            )
    }
}

fn main() -> telorgon::Result<()> {
    Application::gui("Studio")
        .assets(assets::bundle())
        .cursor_theme(assets::cursors::DEFAULT)
        .pointer_overrides(pointers())
        .window(
            Window::new("Studio")
                .size(1100, 720)
                .custom_frame()
                .icon(app_icons())
                .content(ManagedFrame::default()),
        )
        .run()
}
```

The visible label passed to `.icon(...)` becomes the icon-only button's accessible name. Button
hover, focus, pressed, busy, and disabled visuals continue to come from normal component styling;
the window host consumes only the semantic `WindowAction` after hit testing.

## Complete Wayland desktop example

The compositor receives one immutable `WindowChromeModel` per toplevel. Its closure creates a fresh
component for that toplevel, so title, activation, capabilities, state, and client/fallback icon
metadata never live in a global singleton frame.

```rust,ignore
#[component(no_default)]
struct ServerFrame {
    #[input]
    model: WindowChromeModel,
}

impl Component for ServerFrame {
    fn view(&self) -> impl View {
        let title = if self.model.active {
            text(&self.model.title).weight(600)
        } else {
            text(&self.model.title).weight(400)
        };
        let icon = self
            .model
            .app_icon_image
            .map(ImageSource::from)
            .or_else(|| self.model.app_icon.map(ImageSource::from))
            .unwrap_or_else(|| ImageSource::from(assets::icons::APP));

        window_frame()
            .decoration(
                BoxDecoration::new()
                    .uniform_border(1.0, ColorRgba8::rgba(64, 70, 84, 255))
                    .corner_radius(if self.model.state == WindowChromeState::Maximized {
                        0.0
                    } else {
                        12.0
                    }),
            )
            .child(
                row()
                    .height(42.0)
                    .child(image(icon).width(20.0).height(20.0).window_app_icon())
                    .child(title.window_title())
                    .child(spacer())
                    .child(
                        button("Close")
                            .icon(assets::icons::CLOSE)
                            .window_action(WindowAction::Close),
                    )
                    .window_drag_region(),
            )
            .content_slot(window_content_slot().margin((42.0, 5.0, 5.0, 5.0)))
    }
}

Application::desktop_environment("Telorgon")
    .assets(assets::bundle())
    .app_icon(app_icons())
    .cursor_theme(assets::cursors::DEFAULT)
    .pointer_overrides(pointers())
    .client_cursor_mode(ClientCursorMode::Allow)
    .compositor(
        Compositor::new()
            .window_frame(|model| ServerFrame { model })
            .policy(MyCompositorPolicy::default()),
    )
    .shell_widget(ShellWidget::new("panel").content(MyPanel::default()))
    .run()?;
```

## The window-frame contract

`WindowFrame` is an overlay stack specialized by a type-state contract. It cannot become a `View`
until exactly one `WindowContentSlot` has been supplied. All visuals remain ordinary composition:

- `BoxDecoration` owns reusable background, per-side `Border`, `Outline`, `CornerRadii`, and up to
  two `Shadow` values. The same values style containers, controls, popups, and frames.
- Normal layout owns title-bar height, control size, gaps, padding, title placement, icon placement,
  and resize-region placement.
- `WindowChromeViewExt` adds only meaning: frame title, app icon, drag region, resize edge, system
  menu, or a `WindowAction`.
- Action regions take precedence over an overlapping drag parent, which allows buttons to be
  nested directly inside a draggable title bar.

`window_system_menu()` is the semantic hook for a project-defined menu. Telorgon's managed host
does not inject a stock menu, so the component's normal callback owns the menu content and behavior.

Corner radius, border, outline, and shadow are paint properties; they are not advertised to Wayland
clients. Wayland receives protocol globals, configure state, and icon-size preferences. The host
derives semantic hit regions from the final computed layout and performs the corresponding native or
xdg-toplevel operation.

## Assets and icons

The generated catalog uses `include_bytes!`, normalized slash-separated keys, compile-time typed
handles (`IconAsset`, `ImageAsset`, `CursorAsset`, and `CursorThemeAsset`), and a validated immutable
bundle. SVG parsing disables file and string href resolution, and both encoded input and decoded
dimensions have hard bounds. Raster and SVG sources share a bounded decode/raster cache.

`Icon` is the target-neutral value for icon artwork. It converts directly into `image(...)`, the
composable `.icon(...)` button slot, legacy `IconButton`, `AppIconProfile`, managed Winit window
metadata, and compositor-owned chrome. `AppIconProfile` prefers an exact-sized source, then a
scalable source, then the nearest raster size.

Tinting treats the source as an alpha mask: opaque SVG artwork becomes the requested color while
transparent and antialiased edges retain their alpha. This makes a black SVG turn truly white,
rather than multiplying its existing RGB channels. The same contract works through software and
Vulkan rendering, native window icons, and compositor cursors. Tinting is available at the level
that owns the artwork:

```rust,ignore
const WHITE: ColorRgba8 = ColorRgba8::rgba(255, 255, 255, 255);

let standalone = image(assets::icons::APP).tint(WHITE);
let close = button("Close")
    .icon(assets::icons::CLOSE)
    .icon_tint(WHITE);
let native_icon = Icon::new(assets::icons::APP).tint(WHITE);
let arrow = PointerGraphic::new(assets::cursors::ARROW)
    .size(32)
    .hotspot(2, 2)
    .tint(WHITE);
```

Use `.without_tint()` (or `.without_icon_tint()` for a button) to return to the source colors.
Although the operation works for any decoded image, it is intended primarily for monochrome SVG
icons and cursors.

On Wayland, `xdg_toplevel_icon_v1` is commit-synchronized and follows the protocol's lifetime rules:

- only square `wl_shm` buffers with a positive scale are accepted;
- one size/scale pair replaces the earlier pair;
- an icon becomes immutable after `set_icon`;
- an empty assignment resets the toplevel icon;
- the committed snapshot survives destruction of the temporary icon object; and
- a referenced `wl_buffer` must remain alive until that icon object is destroyed.

The manager advertises preferred logical sizes 16, 24, 32, 48, and 64. The compositor selects the
closest submitted image for its frame, otherwise using the desktop environment's fallback
`AppIconProfile`.

## Pointer API

`PointerViewExt::pointer_icon` assigns a semantic shape to any view, while `hide_pointer` requests
no pointer over that region. Window drag, resize, and action regions select `Move`, the matching
eight-direction resize shape, or `Pointer` automatically. `PointerThemeOverrides` handles concise
code-local exceptions; a registered TOML cursor theme supplies the full mapping.

Every `PointerGraphic` can be tinted independently in code. For example, all of the cursor SVGs in
one catalog can share the same white color while retaining their individual sizes and hotspots:

```rust,ignore
const WHITE: ColorRgba8 = ColorRgba8::rgba(255, 255, 255, 255);

PointerThemeOverrides::new()
    .set(PointerIcon::Default, PointerGraphic::new(assets::cursors::ARROW).size(32).hotspot(2, 2).tint(WHITE))
    .set(PointerIcon::Move, PointerGraphic::new(assets::cursors::MOVE).size(32).hotspot(16, 16).tint(WHITE))
    .set(PointerIcon::Pointer, PointerGraphic::new(assets::cursors::POINTER).size(32).hotspot(6, 2).tint(WHITE))
    .set(PointerIcon::EwResize, PointerGraphic::new(assets::cursors::RESIZE_EW).size(32).hotspot(16, 16).tint(WHITE))
    .set(PointerIcon::NsResize, PointerGraphic::new(assets::cursors::RESIZE_NS).size(32).hotspot(16, 16).tint(WHITE))
    .set(PointerIcon::Text, PointerGraphic::new(assets::cursors::TEXT).size(32).hotspot(16, 16).tint(WHITE));
```

```toml
fallback = "system"
size = 32

[default]
asset = "cursors/arrow.svg"
hotspot = [2, 2]

[pointer]
asset = "cursors/link.svg"
hotspot = [6, 2]

[text]
asset = "cursors/text.svg"
hotspot = [15, 15]
```

The hotspot is the pixel inside the cursor image that sits exactly on the pointer coordinate. It is
usually the arrow tip, pointing-finger tip, or I-beam center. Each semantic cursor may declare its
own square raster size and hotspot. Different states do not need the same size. Animated graphics
do require all frames in that one animation to share geometry and hotspot so the pointer does not
jump between frames.

Resolution order is fixed and inspectable: hidden request, permitted client cursor surface,
application override, registered theme, then system cursor. `ClientCursorMode` is intentionally the
only policy-like setting: `Allow` honors a focused Wayland client's cursor surface and `ThemeOnly`
forces compositor artwork. It does not affect managed GUI applications.

## Vocabulary and compatibility

The normalized vocabulary is `BoxDecoration`, `box_style`, `corner_radius`, `uniform_border`,
`WindowFrame`, `WindowContentSlot`, `WindowChromeModel`, `WindowAction`, `PointerIcon`, and
`AppIconProfile`. Earlier `style`, `radius`, and `border` composition builders remain deprecated
forwarders. `TitleBar` aliases are available alongside the earlier `Titlebar` spellings in the
imperative shell API. `ShadowFrame` remains a compatibility adapter only; new frames and ordinary
containers use `BoxDecoration::shadow` or `BoxDecoration::shadows`.

## Rejected alternatives and verification

- A single global frame component was rejected because title, activation, state, capability, and
  icon data are per toplevel.
- A fixed `WindowControls` composer was rejected because it would own visuals and placement that
  belong to application composition.
- Window-only `Border` and `WindowShadow` values were rejected in favor of reusable box paint
  primitives.
- Raw string asset lookup at call sites was rejected in favor of typed generated handles.
- A general cursor-policy service was rejected in favor of immutable pointer configuration plus the
  narrowly scoped `ClientCursorMode` decision.
- Native frame geometry constants were rejected for custom frames; hit regions come from retained
  layout.

Portable tests cover asset-catalog generation, asset/profile validation, pointer precedence,
semantic pointer retention, window-frame type-state and hit regions, and exact protocol-profile
lookup. Host builds compile the Winit icon/frame/pointer integration, and an
`x86_64-unknown-linux-gnu` feature check compiles the Wayland protocol, compositor, and KMS path
without launching an application or server.
