use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::theme_api::ThemePackage;

use super::MaterialSystemError;
use super::shader::{ShaderModuleAsset, ShaderOrigin};

pub struct MaterialRegistry {
    shaders: BTreeMap<String, ShaderModuleAsset>,
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        let mut registry = Self {
            shaders: BTreeMap::new(),
        };
        registry.reset_to_builtins();
        registry
    }
}

impl MaterialRegistry {
    pub fn load_theme_package(
        &mut self,
        package: &ThemePackage,
    ) -> Result<(), MaterialSystemError> {
        self.reset_to_builtins();

        let materials_dir = package.root_dir.join("materials");
        if !materials_dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(&materials_dir).map_err(|error| {
            MaterialSystemError::new(format!(
                "failed to read theme materials directory {}: {error}",
                materials_dir.display()
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                MaterialSystemError::new(format!(
                    "failed to read an entry from {}: {error}",
                    materials_dir.display()
                ))
            })?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("spv") {
                continue;
            }

            let shader = load_spirv_asset(&path, ShaderOrigin::ThemePackage(path.clone()))?;
            self.shaders.insert(shader.name.clone(), shader);
        }

        Ok(())
    }

    pub fn shader(&self, name: &str) -> Option<&ShaderModuleAsset> {
        self.shaders.get(name)
    }

    fn reset_to_builtins(&mut self) {
        self.shaders.clear();
        for (name, bytes) in built_in_shaders() {
            let shader = parse_spirv_bytes(bytes, name, ShaderOrigin::BuiltIn)
                .expect("built-in SPIR-V shader should parse");
            self.shaders.insert(name.to_string(), shader);
        }
    }
}

fn built_in_shaders() -> [(&'static str, &'static [u8]); 3] {
    [
        (
            "blur.spv",
            include_bytes!(concat!(env!("OUT_DIR"), "/material_builtins/blur.spv")),
        ),
        (
            "glass.spv",
            include_bytes!(concat!(env!("OUT_DIR"), "/material_builtins/glass.spv")),
        ),
        (
            "shadow.spv",
            include_bytes!(concat!(env!("OUT_DIR"), "/material_builtins/shadow.spv")),
        ),
    ]
}

fn load_spirv_asset(
    path: &Path,
    origin: ShaderOrigin,
) -> Result<ShaderModuleAsset, MaterialSystemError> {
    let bytes = fs::read(path).map_err(|error| {
        MaterialSystemError::new(format!(
            "failed to read SPIR-V shader {}: {error}",
            path.display()
        ))
    })?;
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            MaterialSystemError::new(format!(
                "theme shader path {} does not have a valid UTF-8 file name",
                path.display()
            ))
        })?;
    parse_spirv_bytes(&bytes, label, origin)
}

fn parse_spirv_bytes(
    bytes: &[u8],
    name: &str,
    origin: ShaderOrigin,
) -> Result<ShaderModuleAsset, MaterialSystemError> {
    if bytes.len() % 4 != 0 {
        return Err(MaterialSystemError::new(format!(
            "SPIR-V shader `{name}` has {} bytes, which is not 4-byte aligned",
            bytes.len()
        )));
    }

    let spirv_words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    if spirv_words.first().copied() != Some(0x0723_0203) {
        return Err(MaterialSystemError::new(format!(
            "SPIR-V shader `{name}` is missing the SPIR-V magic number"
        )));
    }

    Ok(ShaderModuleAsset {
        name: name.to_string(),
        spirv_words,
        origin,
    })
}
