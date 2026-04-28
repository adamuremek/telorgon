use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::evaluator::evaluate_recipe_theme;
use crate::{THEME_API_VERSION, ThemeFrame, ThemeInput, ThemePackage, ThemePackageError};

#[derive(Default)]
pub struct ThemeRuntime {
    active: Option<ThemePackage>,
}

impl ThemeRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_theme(&mut self, package_path: &Path) -> Result<ThemePackage, ThemeRuntimeError> {
        let package = ThemePackage::load(package_path).map_err(ThemeRuntimeError::from)?;
        self.activate(package)
    }

    pub fn active_theme(&self) -> Option<&ThemePackage> {
        self.active.as_ref()
    }

    pub fn evaluate(&mut self, input: &ThemeInput) -> Result<ThemeFrame, ThemeRuntimeError> {
        let package = self.active.as_ref().ok_or_else(|| {
            ThemeRuntimeError::new("cannot evaluate a theme frame before a theme is loaded")
        })?;
        Ok(evaluate_recipe_theme(&package.recipe, input))
    }

    fn activate(&mut self, package: ThemePackage) -> Result<ThemePackage, ThemeRuntimeError> {
        if package.api_version != THEME_API_VERSION {
            return Err(ThemeRuntimeError::new(format!(
                "theme `{}` targets API version {}, but the compositor expects {}",
                package.name, package.api_version, THEME_API_VERSION
            )));
        }
        if !package.capabilities.window_chrome {
            return Err(ThemeRuntimeError::new(format!(
                "theme `{}` does not provide required `window_chrome` capability",
                package.name
            )));
        }
        if !package.capabilities.cursor {
            return Err(ThemeRuntimeError::new(format!(
                "theme `{}` does not provide required `cursor` capability",
                package.name
            )));
        }

        self.active = Some(package.clone());
        Ok(package)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeRuntimeError {
    message: String,
}

impl ThemeRuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ThemeRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ThemeRuntimeError {}

impl From<ThemePackageError> for ThemeRuntimeError {
    fn from(error: ThemePackageError) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::foundation::SizeI;
    use crate::{OutputModel, ThemeInput, ThemeOutputId, ThemeViewId, WindowModel};

    use super::ThemeRuntime;

    #[test]
    fn evaluates_loaded_recipe_theme() {
        let root = write_sample_theme("runtime-theme");
        let mut runtime = ThemeRuntime::new();

        let package = runtime.load_theme(root.as_path()).unwrap();
        let frame = runtime.evaluate(&sample_input()).unwrap();

        assert_eq!(package.name, "regolith-default");
        assert_eq!(frame.output.background_color.r, 0x08);
        assert!(frame.output.cursor.is_some());
        assert!(frame.window_theme(ThemeViewId::new(10)).is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unloaded_runtime_refuses_to_evaluate() {
        let mut runtime = ThemeRuntime::new();
        let error = runtime.evaluate(&sample_input()).unwrap_err();
        assert!(error.to_string().contains("before a theme is loaded"));
    }

    fn sample_input() -> ThemeInput {
        ThemeInput {
            output: OutputModel {
                id: ThemeOutputId::new(5),
                name: "HDMI-A-1".to_string(),
                logical_size: SizeI {
                    width: 1920,
                    height: 1080,
                },
                scale: 1,
                keyboard_focused_window: Some(ThemeViewId::new(10)),
                pointer_focused_window: Some(ThemeViewId::new(10)),
            },
            windows: vec![WindowModel {
                id: ThemeViewId::new(10),
                title: "demo".to_string(),
                app_id: "demo".to_string(),
                mapped: true,
                focused: true,
                geometry: None,
                content_extent: SizeI {
                    width: 640,
                    height: 360,
                },
            }],
        }
    }

    fn write_sample_theme(prefix: &str) -> PathBuf {
        let root = unique_test_dir(prefix);
        fs::create_dir_all(root.join("assets/cursors")).unwrap();
        fs::write(
            root.join("theme.toml"),
            r#"
name = "regolith-default"
api_version = 1
entry = "recipe:v1"

[capabilities]
window_chrome = true
cursor = true
animations = true
materials = true
hot_reload = false
"#,
        )
        .unwrap();
        fs::write(root.join("theme.recipe"), sample_recipe()).unwrap();
        fs::write(
            root.join("assets/cursors/default.rgba.meta"),
            "width = 2\nheight = 2\n",
        )
        .unwrap();
        fs::write(root.join("assets/cursors/default.rgba"), vec![0xff; 16]).unwrap();
        root
    }

    fn sample_recipe() -> &'static str {
        r##"
[output]
background = "#080b12ff"

[content]
palette = "#3eb5d8ff,#d9784aff"

[cursor]
asset = "cursors/default.rgba"
hotspot_x = 0
hotspot_y = 0

[window.focused]
border_px = 3
titlebar_px = 28
radius_px = 8
titlebar_color = "#1b3147ff"
border_color = "#e5edffff"
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
titlebar_color = "#161b23ff"
border_color = "#7b89a3ff"
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
"##
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lithic-theme-runtime-{prefix}-{}-{timestamp}",
            std::process::id()
        ))
    }
}
