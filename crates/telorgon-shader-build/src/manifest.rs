use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct BundleSource {
    pub schema_version: u32,
    pub interface_major: u32,
    pub interface_minor: u32,
    pub target: String,
    pub shader: Vec<ShaderSource>,
}

#[derive(Debug, Deserialize)]
pub struct ShaderSource {
    pub name: String,
    pub source: String,
    pub artifact: String,
    pub stage: String,
}

#[derive(Debug, Serialize)]
pub struct GeneratedBundle {
    pub schema_version: u32,
    pub interface_major: u32,
    pub interface_minor: u32,
    pub target: String,
    pub source_config_hash: String,
    pub bundle_hash: String,
    pub shader: Vec<GeneratedShader>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedShader {
    pub name: String,
    pub source: String,
    pub source_hash: String,
    pub artifact: String,
    pub artifact_hash: String,
    pub stage: String,
    pub entry_point: String,
}
