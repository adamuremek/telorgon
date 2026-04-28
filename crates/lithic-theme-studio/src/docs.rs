pub const STUDIO_REFERENCE: &str = r#"# Lithic Theme Studio Reference

## Mental Model
Studio themes are Rust functions that build a `Theme`. The important chrome path is
`WindowSurfaceTheme`, with `SurfaceTheme` for frame-level styling, `SurfaceFrame` for regions,
and `FrameElement` for the things inside regions.

## Add A Button
Buttons belong inside a `chrome_button_group`. Use `SurfaceRequest::Compositor(...)` for compositor
actions and `SurfaceRequest::App("name".to_string())` for theme/app-level actions.

## Use Uploaded Icons
Icons are asset-only. Upload an SVG or RGBA image in the Assets panel, then reference the generated
`.rgba` path with `IconRef::Asset(AssetRef::new("icons/close.rgba"))`.

## Border Width And Radius
Frame border and corner radius live on `surface_theme()`. `radius(0)` gives square corners.

## Header And Content Regions
A typical window has a header and content region. The content region should usually include
`app_content()`.

## Text
Window title text uses `TextValue::WindowTitle`; literal text uses `TextValue::Literal`.

## Colors
Most color helpers expect packed RGBA hex as `0xrrggbbaa`. Recipe-style strings use `#rrggbbaa`.

## Troubleshooting
If a custom icon does not appear, confirm the asset path ends in `.rgba`, is package-relative, and
appears in the Assets list.
"#;
