# Lithic Compositor Runtime

`lithic-compositor` is Lithic's compositor-mode entrypoint. It is separate from a future
client-side `runApp` UI framework, but follows the same general shape: a host constructs
a Lithic runtime, submits declarative state changes, and asks Lithic to tick/render.

```rust
let mut app = lithic_compositor::run_compositor(renderer, Default::default());
app.submit(SurfaceCommand::CreateWindow(...))?;
let tick = app.tick(TickInput { ... });
```

## Ownership

- Lithic owns surface state, window chrome models, hit-test metadata, render-frame generation,
  and GPU rendering through `lithic-renderer-vulkan`.
- Basalt owns Wayland protocol, compositor policy, focus decisions, client buffer lifetime,
  KMS/DRM presentation, and page flips.
- Basalt should drive Lithic with `SurfaceCommand`s instead of mutating Lithic internals.

## Surface Model

The root object is `Surface`: position, size, visibility, ordering, opacity, and optional
pixel content. Specialized behavior is represented with `SurfaceKind`:

- `WindowSurface`: application content plus configurable `WindowChrome`.
- `LayerSurface`: shell layers such as taskbars, docks, panels, overlays, and backgrounds.
- `DesktopSurface`: desktop/background interaction surfaces.

`LayerSurface` intentionally replaces a hardcoded taskbar primitive. A taskbar is a layer
with `LayerSurfaceRole::Taskbar`, not a distinct root abstraction.

## Current Status

The current slice emits existing Lithic `RenderFrame`s and hit-test regions, and `WindowChrome`
can be built from the declarative `ThemeNode` tree. Theme chrome is lowered into reusable
`lithic-ui` widgets such as `Text`, `HStack`, `VStack`, `ButtonRow`, and `ControlGroup`; window-control buttons now
emit action hit regions so the host can route close/expand actions. Title text now lowers into
`RenderOp::Text` and is drawn by Lithic's atlas-backed `lithic-text` renderer. The Vulkan renderer
now has a color-image render target, render pass, blended rounded/solid-quad graphics pipeline, and
sampled glyph-atlas graphics pipeline for frames made from rects and text. Ordered mixed frames
still use the transfer-backed path where opaque rectangular fills and fully opaque rectangular blits
are recorded as Vulkan buffer fills/copies. Material and unsupported mixed ops still use the mapped
output-buffer fallback.
Basalt's live compositor-display
path and direct-display demo now mirror domain/content/theme state through `LithicSurfaceSync`
and render through Lithic's `SurfaceRenderer` before handing rendered frames to Basalt's presenter.
Basalt's live compositor chrome input also uses Lithic
action/titlebar hit regions.
Theme evaluation for the live compositor path is now driven by domain/content snapshots rather
than by Basalt render scene state.
Render material resolution for Basalt's live, direct, and offscreen paths is owned by
`SurfaceRenderer`, which uses `lithic-material` internally.

The old Basalt shell-scene render extractor has been removed from the active architecture. Remaining
follow-up work is focused on serializing `WindowChrome` packages and replacing the remaining mapped
output-buffer material fallback with real Vulkan graphics pipelines.
