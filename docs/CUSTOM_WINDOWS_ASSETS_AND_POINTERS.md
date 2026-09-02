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
  icons/restore.svg
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

The convenience path is a reusable value, not a closure. `EasyWindowFrame` receives one immutable
`WindowChromeModel` per toplevel and resolves it against a complete `WindowChromeDesign`. Control
icons live beside their control visuals in that design; they do not need separate compositor icon
declarations.

```rust,ignore
use telorgon::app::*;
use telorgon::{Easing, TransitionSpec};

const RESTING: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(35, 40, 52, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(225, 230, 242, 255),
};

const HOVERED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(60, 70, 96, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const PRESSED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .background(Background::Color(ColorRgba8::rgba(82, 96, 132, 255)))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const FOCUSED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new()
        .uniform_border(2.0, ColorRgba8::rgba(130, 155, 255, 255))
        .corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
};

const DISABLED: WindowControlVisual = WindowControlVisual {
    decoration: BoxDecoration::new().corner_radius(7.0),
    icon_tint: ColorRgba8::rgba(120, 126, 142, 255),
};

const BUTTON: WindowControlButtonStyle = WindowControlButtonStyle {
    width: 38.0,
    height: 30.0,
    icon_size: 15.0,
    resting: RESTING,
    hovered: Some(HOVERED),
    pressed: Some(PRESSED),
    focused: Some(FOCUSED),
    disabled: Some(DISABLED),
    transition: Some(TransitionSpec {
        duration_ms: 90,
        easing: Easing::EaseOut,
        repeat: false,
    }),
};

const CLOSE_BUTTON: WindowControlButtonStyle = WindowControlButtonStyle {
    resting: WindowControlVisual {
        decoration: BoxDecoration::new()
            .background(Background::Color(ColorRgba8::rgba(77, 35, 45, 255)))
            .corner_radius(7.0),
        icon_tint: ColorRgba8::rgba(255, 220, 225, 255),
    },
    hovered: Some(WindowControlVisual {
        decoration: BoxDecoration::new()
            .background(Background::Color(ColorRgba8::rgba(196, 52, 72, 255)))
            .corner_radius(7.0),
        icon_tint: ColorRgba8::rgba(255, 255, 255, 255),
    }),
    ..BUTTON
};

const NORMAL: WindowChromeStateStyle = WindowChromeStateStyle {
    title_bar_visible: true,
    frame_radius: 14.0,
    content_radius: 9.0,
    shadow: Some(Shadow {
        offset: PointF { x: 0.0, y: 12.0 },
        blur: 30.0,
        spread: 0.0,
        color: ColorRgba8::rgba(0, 0, 0, 128),
    }),
    resize_regions: true,
    resize_edge: 6.0,
    resize_hit_slop: Insets::all(3.0),
    content_margin: Insets::new(44.0, 6.0, 6.0, 6.0),
};

const TEST_CHROME: WindowChromeDesign = WindowChromeDesign {
    active: WindowChromePalette {
        frame_background: ColorRgba8::rgba(23, 27, 38, 255),
        frame_border: ColorRgba8::rgba(101, 119, 184, 255),
        frame_border_width: 1.0,
        title_color: ColorRgba8::rgba(245, 247, 255, 255),
        title_weight: 650,
    },
    inactive: WindowChromePalette {
        frame_background: ColorRgba8::rgba(31, 34, 43, 255),
        frame_border: ColorRgba8::rgba(65, 70, 85, 255),
        frame_border_width: 1.0,
        title_color: ColorRgba8::rgba(174, 179, 193, 255),
        title_weight: 450,
    },
    normal: NORMAL,
    maximized: WindowChromeStateStyle {
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        resize_regions: false,
        resize_edge: 0.0,
        resize_hit_slop: Insets::ZERO,
        content_margin: Insets::new(44.0, 0.0, 0.0, 0.0),
        ..NORMAL
    },
    tiled: WindowChromeStateStyle {
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        ..NORMAL
    },
    fullscreen: WindowChromeStateStyle {
        title_bar_visible: false,
        frame_radius: 0.0,
        content_radius: 0.0,
        shadow: None,
        resize_regions: false,
        resize_edge: 0.0,
        resize_hit_slop: Insets::ZERO,
        content_margin: Insets::ZERO,
    },
    title_bar: WindowTitleBarStyle {
        height: 44.0,
        padding: Insets::symmetric(7.0, 8.0),
        gap: 8.0,
        title_size: 14.0,
        app_icon_region_size: 30.0,
        app_icon_size: 20.0,
        show_client_icon: true,
        fallback_app_icon: Some(assets::icons::APP),
        app_icon_opens_system_menu: true,
    },
    controls: WindowControlsDesign {
        minimize: WindowControlDesign { icon: assets::icons::MINIMIZE, style: BUTTON },
        maximize: WindowControlDesign { icon: assets::icons::MAXIMIZE, style: BUTTON },
        restore: WindowControlDesign { icon: assets::icons::RESTORE, style: BUTTON },
        close: WindowControlDesign { icon: assets::icons::CLOSE, style: CLOSE_BUTTON },
        gap: 6.0,
    },
    content_background: ColorRgba8::rgba(15, 18, 26, 255),
};

#[component]
struct DesktopBackground {}

impl Component for DesktopBackground {
    fn view(&self) -> impl View {
        stack().background(ColorRgba8::rgba(10, 12, 18, 255))
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
            .window_frame(easy_window_frame(TEST_CHROME))
            .background(DesktopBackground::default()),
    )
    .shell_widget(ShellWidget::new("panel").content(MyPanel::default()))
    .run()?;
```

`WindowChromeDesign::validate()` can be called before assembling the declaration to report invalid
nonfinite or negative metrics early. At runtime the easy frame selects active/inactive palettes,
normal/maximized/tiled/fullscreen geometry, client/fallback app icons, capability-filtered controls,
maximize versus restore artwork, resizable tiled edges, and every declared control state. The state
styles are code-local bindings, so no theme-catalog entry is required.

`Compositor::icon(name, component)` remains a separate semantic icon registry for compositor-wide
artwork such as cursor or legacy fallback names. It is not where `EasyWindowFrame` receives its
button icons. Likewise, `.background(...)` takes a widget because it describes pixels behind client
windows; focus, placement, security, workspace, and activation decisions remain host policy.

## The window-frame contract

`WindowFrame` is an overlay stack specialized by a type-state contract. It cannot become a `View`
until exactly one `WindowContentSlot` has been supplied. All visuals remain ordinary composition:

- `BoxDecoration` owns reusable background, per-side `Border`, `Outline`, `CornerRadii`, and up to
  two `Shadow` values. The same values style containers, controls, popups, and frames.
- Normal layout owns title-bar height, control size, gaps, padding, title placement, icon placement,
  and resize-region placement.
- `WindowChromeViewExt` adds only meaning: frame title, app icon, drag region, resize edge, system
  menu, built-in `WindowAction`, or declaration-authorized `ShellActionId`.
- Default precedence is control/action over resize over drag. `window_hit_priority(...)` can
  override it, while `window_hit_slop(...)` enlarges input geometry without changing layout or
  paint. `PointerViewExt::pointer_icon(...)` can override the semantic cursor.
- `WindowEdgeMask` and `WindowTilingState` describe tiled adjacency and the subset of edges that
  remain resizable.

`EasyWindowFrame` confines resize input to the painted outer frame and the state style's
`content_margin`. `resize_edge` is the maximum inward thickness, the painted
`frame_border_width` is included automatically, and `resize_hit_slop` extends only toward the
outside of the window. Diagonal targets are L-shaped unions of the two adjoining border strips, so
their larger corner reach does not claim the titlebar or client-area square inside the corner.

`window_system_menu()` is the semantic hook for a project-defined menu. Telorgon's managed host
does not inject a stock menu, so the component's normal callback owns the menu content and behavior.

Corner radius, border, outline, and shadow are paint properties; they are not advertised to Wayland
clients. Wayland receives protocol globals, configure state, and icon-size preferences. The host
derives semantic hit regions from the final computed layout and performs the corresponding native or
xdg-toplevel operation.

For expert frames, implement `WindowFrameTemplate` as an ordinary named value. Closures still
implement the trait for compatibility, but are not required:

```rust,ignore
const PIN: ShellActionId = ShellActionId::named("window.pin");

#[component(no_default)]
struct StudioFrame {
    #[input]
    model: WindowChromeModel,
}

impl Component for StudioFrame {
    fn view(&self) -> impl View {
        window_frame()
            .gap(4.0)
            .child(
                row()
                    .height(42.0)
                    .child(text(&self.model.title).window_title())
                    .child(spacer())
                    .child(button("Pin").window_shell_action(PIN))
                    .child(button("Close").window_action(WindowAction::Close))
                    .window_drag_region(),
            )
            .child(
                stack()
                    .height(5.0)
                    .window_resize(WindowResizeEdge::Top)
                    .window_hit_slop(Insets::all(4.0))
                    .window_hit_priority(500),
            )
            .content_slot(window_content_slot().margin((42.0, 5.0, 5.0, 5.0)))
    }
}

struct StudioFrameTemplate;

impl WindowFrameTemplate for StudioFrameTemplate {
    type Component = StudioFrame;

    fn compose(&self, model: WindowChromeModel) -> StudioFrame {
        StudioFrame { model }
    }
}

fn pin_window(model: WindowChromeModel) {
    // The handler exists only because this action ID was authorized by the declaration.
}

Compositor::new()
    .window_frame(StudioFrameTemplate)
    .shell_action(PIN, pin_window)
    .background(DesktopBackground::default());
```

An unregistered shell action is inert. This keeps arbitrary frame regions from becoming an
unrestricted host-command channel while still permitting project-specific controls.

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
code-local exceptions; a registered TOML cursor theme supplies the full mapping. When the Linux
desktop compositor has no exact system or registered directional-resize graphic, it tries
`AllResize` and then `Default`, preventing a partial cursor table from making the pointer vanish.

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
`WindowFrame`, `WindowContentSlot`, `WindowFrameTemplate`, `EasyWindowFrame`,
`WindowChromeDesign`, `WindowChromeModel`, `WindowAction`, `PointerIcon`, `AppIconProfile`, and
`Compositor::background`. `Compositor::policy` remains a deprecated compatibility alias. Earlier
`style`, `radius`, and `border` composition builders remain deprecated forwarders. `TitleBar`
aliases are available alongside the earlier `Titlebar` spellings in the imperative shell API.
`ShadowFrame` remains a compatibility adapter only; new frames and ordinary containers use
`BoxDecoration::shadow` or `BoxDecoration::shadows`.

## Rejected alternatives and verification

- A single global frame component was rejected because title, activation, state, capability, and
  icon data are per toplevel.
- A mandatory fixed `WindowControls` composer was rejected. `EasyWindowFrame` is now an optional
  convenience layer with a complete design value; the low-level frame keeps normal composition.
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
