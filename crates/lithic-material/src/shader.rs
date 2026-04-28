use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderModuleAsset {
    pub name: String,
    pub spirv_words: Vec<u32>,
    pub origin: ShaderOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderOrigin {
    BuiltIn,
    ThemePackage(PathBuf),
}
