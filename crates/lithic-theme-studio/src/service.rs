#![allow(dead_code)]

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use lithic_compositor::{
    CreateWindowSurface, SurfaceCommand, SurfaceContent, SurfaceController, SurfaceId,
    SurfaceRenderer, TickInput, WindowChrome,
};
use lithic_core::{ColorRgba8, RectI, SizeI};
use lithic_render::{RenderTargetId, RenderedFrame, render_frame_software};
use lithic_theme::{
    OutputModel, ThemeInput, ThemeOutputId, ThemeRuntime, ThemeViewId, WindowModel,
    WindowSurfaceTheme,
};

#[derive(Clone, Debug)]
pub struct StudioService {
    pub staging_parent: PathBuf,
    pub staging_dir: PathBuf,
    pub default_export_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioSnapshot {
    pub staging_dir: PathBuf,
    pub export_path: PathBuf,
    pub code: String,
    pub assets: Vec<StudioAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioAsset {
    pub path: String,
    pub size: SizeI,
    pub pixels_rgba8: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewOutput {
    pub package_name: String,
    pub frame: RenderedFrame,
}

impl StudioService {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let exe_dir = std::env::current_exe()?
            .parent()
            .map(Path::to_path_buf)
            .ok_or("could not determine binary directory")?;
        Self::new_in(exe_dir.join("lithic-theme-studio-work"))
    }

    pub fn new_in(staging_parent: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let staging_dir = staging_parent.join("staging");
        let default_export_path = staging_parent.join("theme.lthm");
        fs::create_dir_all(&staging_parent)?;
        let service = Self {
            staging_parent,
            staging_dir,
            default_export_path,
        };
        service.ensure_staged_theme()?;
        Ok(service)
    }

    pub fn snapshot(&self) -> Result<StudioSnapshot, Box<dyn std::error::Error>> {
        Ok(StudioSnapshot {
            staging_dir: self.staging_dir.clone(),
            export_path: self.default_export_path.clone(),
            code: self.theme_code()?,
            assets: self.assets()?,
        })
    }

    pub fn ensure_staged_theme(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(self.staging_dir.join("src"))?;
        fs::create_dir_all(self.staging_dir.join("assets/cursors"))?;
        fs::create_dir_all(self.staging_dir.join("assets/icons"))?;
        fs::create_dir_all(self.staging_dir.join("assets/textures"))?;
        fs::create_dir_all(self.staging_dir.join("assets/materials"))?;
        if !self.staging_dir.join("theme.toml").exists() {
            fs::write(self.staging_dir.join("theme.toml"), default_manifest("staging"))?;
        }
        if !self.staging_dir.join("theme.recipe").exists() {
            fs::write(self.staging_dir.join("theme.recipe"), DEFAULT_RECIPE)?;
        }
        if !self.staging_dir.join("Cargo.toml").exists() {
            fs::write(self.staging_dir.join("Cargo.toml"), default_cargo_toml())?;
        }
        if !self.staging_dir.join("theme.chrome.rs").exists() {
            fs::write(self.staging_dir.join("theme.chrome.rs"), DEFAULT_THEME_CODE)?;
        }
        let cursor_meta = self.staging_dir.join("assets/cursors/default.rgba.meta");
        if !cursor_meta.exists() {
            fs::write(cursor_meta, "width = 2\nheight = 2\n")?;
        }
        let cursor_hex = self.staging_dir.join("assets/cursors/default.rgba.hex");
        if !cursor_hex.exists() {
            fs::write(cursor_hex, "ffffffff000000cc000000cc00000000\n")?;
        }
        self.validate_theme()
    }

    pub fn import_theme(&self, source: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if source.extension().and_then(|ext| ext.to_str()) == Some("lthm")
            || source.extension().and_then(|ext| ext.to_str()) == Some("lithic-theme")
        {
            lithic_theme::unpack_packed_theme(source, &self.staging_dir)?;
        } else if source.is_dir() {
            if self.staging_dir.exists() {
                fs::remove_dir_all(&self.staging_dir)?;
            }
            copy_dir(source, &self.staging_dir)?;
        } else {
            return Err(format!("unsupported import path {}", source.display()).into());
        }
        self.ensure_staged_theme()
    }

    pub fn theme_code(&self) -> Result<String, Box<dyn std::error::Error>> {
        read_text(self.staging_dir.join("theme.chrome.rs").as_path())
    }

    pub fn save_theme_code(&self, code: &str) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(self.staging_dir.join("theme.chrome.rs"), migrate_theme_code(code))?;
        Ok(())
    }

    pub fn format_theme_code(&self, code: &str) -> Result<String, Box<dyn std::error::Error>> {
        let path = self.staging_dir.join("theme.chrome.rs");
        fs::write(&path, code)?;
        let output = Command::new("rustfmt").arg(&path).output();
        match output {
            Ok(output) if output.status.success() => read_text(&path),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(format!("rustfmt failed:\n{stderr}{stdout}").into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let formatted = fallback_format_theme_code(code);
                fs::write(&path, &formatted)?;
                Ok(formatted)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn build_export(&self, output: Option<&Path>) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.validate_theme()?;
        let output = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.default_export_path.clone());
        if output.extension().and_then(|ext| ext.to_str()) != Some("lthm") {
            return Err("export path must end with `.lthm`".into());
        }
        let manifest = read_text(self.staging_dir.join("theme.toml").as_path())?;
        let recipe = read_text(self.staging_dir.join("theme.recipe").as_path())?;
        lithic_theme::write_packed_theme(
            &manifest,
            &recipe,
            self.staging_dir.join("assets").as_path(),
            &output,
        )?;
        Ok(output)
    }

    pub fn save_rgba_asset(
        &self,
        asset_path: &str,
        size: SizeI,
        pixels_rgba8: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if pixels_rgba8.len() != (size.width * size.height * 4).max(0) as usize {
            return Err("RGBA byte count does not match asset dimensions".into());
        }
        let relative = sanitize_asset_path(asset_path)?;
        let asset_file = self.staging_dir.join("assets").join(&relative);
        if let Some(parent) = asset_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let file_name = asset_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("asset path must end in a valid UTF-8 file name")?;
        fs::write(asset_file.with_file_name(format!("{file_name}.hex")), hex_bytes(pixels_rgba8))?;
        fs::write(
            asset_file.with_file_name(format!("{file_name}.meta")),
            format!("width = {}\nheight = {}\n", size.width, size.height),
        )?;
        self.validate_theme()
    }

    pub fn save_svg_asset(
        &self,
        _asset_path: &str,
        _source_svg: &str,
        _size: Option<SizeI>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Err("native SVG rasterization is planned for the next implementation slice".into())
    }

    pub fn assets(&self) -> Result<Vec<StudioAsset>, Box<dyn std::error::Error>> {
        let package = lithic_theme::ThemePackage::load(&self.staging_dir)?;
        Ok(package
            .assets
            .image_assets()
            .map(|(path, image)| StudioAsset {
                path: path.to_string(),
                size: image.size,
                pixels_rgba8: image.pixels_rgba8.to_vec(),
            })
            .collect())
    }

    pub fn preview(
        &self,
        title: &str,
        extent: SizeI,
    ) -> Result<PreviewOutput, Box<dyn std::error::Error>> {
        let mut runtime = ThemeRuntime::new();
        let package = runtime.load_theme(&self.staging_dir)?;
        let input = studio_theme_input(title, extent);
        let theme_frame = runtime.evaluate(&input)?;
        let window_theme = theme_frame
            .window_theme(ThemeViewId::new(1))
            .ok_or("theme did not produce chrome for preview window")?;
        let chrome = WindowChrome::from_theme_nodes(window_theme.chrome_nodes.iter());
        let surface_theme = read_surface_theme(self.staging_dir.join("window.focused.surface").as_path())?;
        let content_size = SizeI {
            width: 640,
            height: 420,
        };
        let mut controller = SurfaceController::new();
        controller.load_theme_package(&package);
        controller.submit(SurfaceCommand::CreateWindow(CreateWindowSurface {
            id: SurfaceId::new(1),
            geometry: RectI {
                x: (extent.width - content_size.width) / 2,
                y: (extent.height - content_size.height) / 2,
                width: content_size.width,
                height: content_size.height,
            },
            z_order: 1,
            title: title.to_string(),
            app_id: "lithic-theme-studio".to_string(),
            content: Some(SurfaceContent::from_rgba8(
                44,
                content_size,
                preview_content(content_size),
                1,
            )),
            chrome,
            surface_theme,
        }))?;
        controller.submit(SurfaceCommand::SetFocus {
            id: Some(SurfaceId::new(1)),
        })?;
        let tick = controller.tick(TickInput {
            output_id: RenderTargetId::new(1),
            extent,
            background: ColorRgba8::rgba(8, 11, 18, 255),
            frame_time_ns: 0,
        });
        let mut resolver = SurfaceRenderer::resolver();
        resolver.load_theme_package(&package)?;
        let frame = resolver.resolve_tick_frame(&tick)?;
        Ok(PreviewOutput {
            package_name: package.name,
            frame: render_frame_software(&frame),
        })
    }

    pub fn validate_theme(&self) -> Result<(), Box<dyn std::error::Error>> {
        lithic_theme::ThemePackage::load(&self.staging_dir)
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn studio_theme_input(title: &str, extent: SizeI) -> ThemeInput {
    ThemeInput {
        output: OutputModel {
            id: ThemeOutputId::new(1),
            name: "studio-preview".to_string(),
            logical_size: extent,
            scale: 1,
            keyboard_focused_window: Some(ThemeViewId::new(1)),
            pointer_focused_window: Some(ThemeViewId::new(1)),
        },
        windows: vec![WindowModel {
            id: ThemeViewId::new(1),
            title: title.to_string(),
            app_id: "lithic-theme-studio".to_string(),
            mapped: true,
            focused: true,
            geometry: Some(RectI {
                x: 160,
                y: 120,
                width: 640,
                height: 420,
            }),
            content_extent: SizeI {
                width: 640,
                height: 420,
            },
        }],
    }
}

fn preview_content(size: SizeI) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size.width * size.height * 4) as usize);
    for y in 0..size.height {
        for x in 0..size.width {
            let fy = y as f32 / size.height.max(1) as f32;
            let edge = (x.min(size.width - x - 1).min(y.min(size.height - y - 1)) as f32 / 48.0)
                .clamp(0.0, 1.0);
            let r = (0x22 as f32 * (1.0 - fy) + 0x18 as f32 * fy) as u8;
            let g = (0x2a as f32 * (1.0 - fy) + 0x20 as f32 * fy) as u8;
            let b = (0x34 as f32 * (1.0 - fy) + 0x2a as f32 * fy) as u8;
            let lift = (8.0 * edge) as u8;
            pixels.extend_from_slice(&[
                r.saturating_add(lift),
                g.saturating_add(lift),
                b.saturating_add(lift),
                0xff,
            ]);
        }
    }
    pixels
}

fn read_surface_theme(path: &Path) -> Result<Option<WindowSurfaceTheme>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let source = read_text(path)?;
    WindowSurfaceTheme::from_document(&source)
        .map(Some)
        .map_err(|error| format!("failed to read explicit surface theme: {error}").into())
}

fn migrate_theme_code(code: &str) -> String {
    ["Close", "Minimize", "Maximize", "Add", "Search", "More"]
        .into_iter()
        .fold(code.to_string(), |code, icon| {
            code.replace(&format!("Some(IconRef::{icon})"), "None")
        })
}

fn fallback_format_theme_code(code: &str) -> String {
    let mut formatted = code.trim().to_string();
    formatted.push('\n');
    formatted
}

fn sanitize_asset_path(path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Err("asset path must be relative".into());
    }
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => cleaned.push(part),
            _ => return Err("asset path may not contain `.` or `..`".into()),
        }
    }
    if cleaned.as_os_str().is_empty() {
        return Err("asset path may not be empty".into());
    }
    if cleaned.extension().and_then(|ext| ext.to_str()) != Some("rgba") {
        return Err("asset path must end with `.rgba`".into());
    }
    Ok(cleaned)
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn read_text(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()).into())
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter().copied() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn default_manifest(name: &str) -> String {
    format!(
        r#"name = "{name}"
api_version = 1
entry = "recipe:v1"

[capabilities]
window_chrome = true
cursor = true
animations = true
materials = true
hot_reload = false
"#
    )
}

fn default_cargo_toml() -> String {
    let studio_crate = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lithic_root = studio_crate
        .parent()
        .and_then(Path::parent)
        .unwrap_or(studio_crate);
    format!(
        r#"[package]
name = "lithic-theme-studio-session"
version = "0.1.0"
edition = "2024"

[dependencies]
lithic-compositor = {{ path = {compositor:?} }}
lithic-core = {{ path = {core:?} }}
lithic-theme = {{ path = {theme:?} }}

[workspace]
"#,
        compositor = lithic_root
            .join("crates/lithic-compositor")
            .display()
            .to_string(),
        core = lithic_root.join("crates/lithic-core").display().to_string(),
        theme = lithic_root.join("crates/lithic-theme").display().to_string(),
    )
}

const DEFAULT_THEME_CODE: &str = r#"pub fn design() {
    // Native Studio now owns the workbench directly. Theme code editing remains staged here
    // while the native editor, diagnostics, and build execution are brought online.
}
"#;

const DEFAULT_RECIPE: &str = r##"[output]
background = "#f0f0f0ff"

[content]
palette = "#0078d4ff,#2b579aff,#107c10ff,#d83b01ff"

[cursor]
asset = "cursors/default.rgba"
hotspot_x = 0
hotspot_y = 0

[window.focused]
border_px = 1
titlebar_px = 32
radius_px = 0
titlebar_color = "#f3f3f3ff"
border_color = "#8a8a8aff"
show_title_text = true
title_text_color = "#202020ff"
shadow_color = "#00000033"
shadow_radius_px = 28
shadow_offset_x = 0
shadow_offset_y = 10
shadow_strength = 51
glass_tint_color = "#00000000"
glass_opacity = 0
backdrop_blur_radius_px = 0
backdrop_blur_passes = 0
use_glass = false
show_window_controls = true
expand_color = "#f3f3f3ff"
expand_hover_color = "#e5e5e5ff"
close_color = "#f3f3f3ff"
close_hover_color = "#e81123ff"

[window.unfocused]
border_px = 1
titlebar_px = 32
radius_px = 0
titlebar_color = "#f7f7f7ff"
border_color = "#b0b0b0ff"
show_title_text = true
title_text_color = "#5f5f5fff"
shadow_color = "#00000022"
shadow_radius_px = 20
shadow_offset_x = 0
shadow_offset_y = 8
shadow_strength = 34
glass_tint_color = "#00000000"
glass_opacity = 0
backdrop_blur_radius_px = 0
backdrop_blur_passes = 0
use_glass = false
show_window_controls = true
expand_color = "#f7f7f7ff"
expand_hover_color = "#e5e5e5ff"
close_color = "#f7f7f7ff"
close_hover_color = "#e81123ff"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_paths_are_restricted_to_rgba_children() {
        assert!(sanitize_asset_path("icons/close.rgba").is_ok());
        assert!(sanitize_asset_path("../close.rgba").is_err());
        assert!(sanitize_asset_path("icons/close.svg").is_err());
    }

    #[test]
    fn staged_theme_can_preview() {
        let root = std::env::temp_dir().join(format!(
            "lithic-studio-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let service = StudioService::new_in(root.clone()).expect("service");
        let preview = service
            .preview(
                "Preview",
                SizeI {
                    width: 960,
                    height: 640,
                },
            )
            .expect("preview");
        assert_eq!(preview.frame.extent.width, 960);
        assert!(!preview.frame.pixels_rgba8.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
