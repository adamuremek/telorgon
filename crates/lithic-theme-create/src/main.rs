use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("lithic-theme-create: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).map(PathBuf::from).ok_or_else(|| {
        "usage: lithic-theme-create <theme-name-or-path>".to_string()
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "theme path must have a valid UTF-8 final component".to_string())?
        .to_string();
    if path.exists() {
        return Err(format!("{} already exists", path.display()).into());
    }

    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("assets/cursors"))?;
    fs::create_dir_all(path.join("assets/icons"))?;
    fs::create_dir_all(path.join("assets/textures"))?;
    fs::create_dir_all(path.join("assets/materials"))?;

    write(path.join("Cargo.toml").as_path(), &cargo_toml(&name))?;
    write(path.join("src/main.rs").as_path(), MAIN_RS)?;
    write(path.join("theme.toml").as_path(), &theme_toml(&name))?;
    write(path.join("theme.recipe").as_path(), THEME_RECIPE)?;
    write(
        path.join("assets/cursors/default.rgba.meta").as_path(),
        "width = 2\nheight = 2\n",
    )?;
    write(
        path.join("assets/cursors/default.rgba.hex").as_path(),
        "ffffffff000000cc000000cc00000000\n",
    )?;

    println!("{}", path.display());
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()).into())
}

fn cargo_toml(name: &str) -> String {
    let theme_crate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(".."))
        .join("lithic-theme");
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
lithic-theme = {{ path = "{}" }}
"#
        ,
        theme_crate.display()
    )
}

fn theme_toml(name: &str) -> String {
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

const MAIN_RS: &str = r#"use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("theme.toml"))?;
    let recipe = std::fs::read_to_string(root.join("theme.recipe"))?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("theme");
    let output = root.join("target").join(format!("{name}.lithic-theme"));
    lithic_theme::write_packed_theme(&manifest, &recipe, root.join("assets").as_path(), &output)?;
    println!("{}", output.display());
    Ok(())
}
"#;

const THEME_RECIPE: &str = r##"[output]
background = "#101014ff"

[content]
palette = "#3eb5d8ff,#d9784aff,#7cba6bff,#d2a443ff"

[cursor]
asset = "cursors/default.rgba"
hotspot_x = 0
hotspot_y = 0

[window.focused]
border_px = 3
titlebar_px = 28
radius_px = 8
titlebar_color = "#26384aff"
border_color = "#f0f4ffff"
title_text_color = "#f3f6fbff"
shadow_color = "#00000070"
shadow_radius_px = 28
shadow_offset_x = 0
shadow_offset_y = 12
shadow_strength = 110
glass_tint_color = "#78a4cfff"
glass_opacity = 116
backdrop_blur_radius_px = 10
backdrop_blur_passes = 2
use_glass = true
expand_color = "#8bc8ffe4"
expand_hover_color = "#a6dbffff"
close_color = "#ff7b7bec"
close_hover_color = "#ff9a9aff"

[window.unfocused]
border_px = 2
titlebar_px = 28
radius_px = 8
titlebar_color = "#202229ff"
border_color = "#8892a6ff"
title_text_color = "#f3f6fbff"
shadow_color = "#00000058"
shadow_radius_px = 22
shadow_offset_x = 0
shadow_offset_y = 8
shadow_strength = 96
glass_tint_color = "#00000000"
glass_opacity = 0
backdrop_blur_radius_px = 0
backdrop_blur_passes = 0
use_glass = false
expand_color = "#8bc8ffe4"
expand_hover_color = "#a6dbffff"
close_color = "#ff7b7bec"
close_hover_color = "#ff9a9aff"
"##;
