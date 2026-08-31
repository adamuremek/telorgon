# Telorgon Theme Runtime

## Status

Theme v4 is the only accepted source format. The runtime owns immutable compiled application and
shell roots plus generation-safe preview scopes. Mounted visual nodes participate in one per-view
pipeline:

```text
interaction snapshot -> resolved target style -> animation sample -> atomic property patch
```

Application and shell catalogs publish stable, domain-qualified component style IDs, named slots,
variant axes, relevant state masks, deterministic state precedence, and explicit declarations for
model-only unstyled types. Domain IDs cannot cross-reference tokens or styles.

## Source and compilation

Theme documents require both `format = "v4"` and a `domain`. Tokens are typed, and references use
an explicit `{ token = "category.name" }` value. Unknown components, styles, slots, variants,
states, properties, token types, cycles, and cross-domain references are compilation errors.

```toml
format = "v4"
domain = "application"

[tokens.color]
button = "#365a9fff"
button_hovered = "#466db5ff"
focus = "#a6dbffff"

[tokens.length]
radius = 8
outline = 2

[tokens.duration]
fast = 120

[tokens.easing]
standard = "ease-out"

[components.button.default.slots.root]
background = { token = "color.button" }
radius = { token = "length.radius" }

[components.button.default.states.hovered.slots.root]
background = { token = "color.button_hovered" }

[components.button.default.states.focus-visible.slots.root]
outline_color = { token = "color.focus" }
outline_width = { token = "length.outline" }
outline_offset = 2

[components.button.default.states.hovered.transition]
duration = { token = "duration.fast" }
easing = { token = "easing.standard" }
```

Resolution order is component defaults, scoped theme, sparse local override, then accessibility and
component invariants. Variants resolve before state overlays. Each component contract supplies a
low-to-high state order, so TOML insertion order cannot affect the result.

## Scopes and replacement

`ThemeRuntime` owns separate application and shell root scopes. Preview scopes are generational;
discarding and recreating a preview cannot make an old `ThemeScopeId` valid again. Theme replacement
is atomic and domain checked. A successful replacement diffs stable style IDs and enqueues only
bindings that depend on changed entries; a failed replacement leaves the live theme unchanged.

Direct mutable registry access is not exposed. Applications use `AppRuntime::replace_theme`, while
advanced custom components register a `ComponentStyleContract`, explicit `ControlBehavior`, and a
`StyleBinding` from named slots to foundation nodes.

## Motion

Transitions run in one per-view track arena driven by `MonotonicInstant`; components do not allocate
timers or threads. Colors interpolate in linear premultiplied space. Opacity, border and outline
metrics, radii, two shadows, and transforms interpolate; layout and typography metrics snap
atomically. An interrupted transition samples its current value and retargets from that sample.

Reduced motion disables repeating and spatial motion and limits optional opacity fades to 100 ms.
Invisible repeating tracks suspend, and `FrameScheduler` stops requesting animation frames after all
tracks settle.

## Archives

The deterministic archive magic is `LTH4`. Theme v2/v3 documents and `LTH2`/`LTH3` archives are
rejected rather than imported.

Theme source parsing, compilation, validation, and deterministic archive encoding are library APIs
in `telorgon-theme`. Telorgon does not bundle theme authoring applications or command-line wrappers.
