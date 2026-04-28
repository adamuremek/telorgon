# Lithic UI Widgets

`lithic-ui` is Lithic's reusable UI data model. It stores widget trees without depending on the
compositor, the theme runtime, or a renderer. That keeps the same tree usable for compositor chrome
now and for normal application UI later.

```rust
use lithic_ui::{Icon, control_group, hstack, icon_button, text, widget_action};

let titlebar = hstack(
    [
        text("Terminal", title_color),
        control_group(
            [
                icon_button(
                    Icon::ToggleExpand,
                    expand_color,
                    Some(widget_action("window.toggle_expand")),
                ),
                icon_button(
                    Icon::Close,
                    close_color,
                    Some(widget_action("window.close")),
                ),
            ],
            12,
            8,
            10,
        ),
    ],
    8,
);
```

The public model uses Flutter-like names: `Widget` is the base enum, and concrete widgets are
named `Text`, `Button`, `IconButton`, `HStack`, `VStack`, `Stack`, `Align`, `Padding`, and
`Spacer`. The compositor interprets `Action` names like `window.close` as window-management
commands. `Text` widgets in compositor chrome lower into Lithic `RenderOp::Text` packets, which
are currently shaped and rasterized through `lithic-text` into an atlas-backed glyph stream. Vulkan
renderers mirror that atlas into GPU image memory. A future client-side UI runtime can use the same
widget types with application-specific action names.
