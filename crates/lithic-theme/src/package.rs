use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::foundation::{ColorRgba8, PointI, SizeI};

const PACKED_MAGIC: &[u8; 8] = b"LTHM0001";
const MAX_PACKED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SECTION_BYTES: u32 = 16 * 1024 * 1024;
const MAX_STRING_BYTES: usize = 4096;
const MAX_ASSETS: usize = 256;
const MAX_IMAGE_PIXELS: i32 = 4096 * 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeCapabilities {
    pub window_chrome: bool,
    pub cursor: bool,
    pub animations: bool,
    pub materials: bool,
    pub hot_reload: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePackage {
    pub name: String,
    pub api_version: u32,
    pub entry: String,
    pub capabilities: ThemeCapabilities,
    pub root_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub recipe: ThemeRecipe,
    pub assets: ThemeAssetStore,
}

impl ThemePackage {
    pub fn load(package_path: &Path) -> Result<Self, ThemePackageError> {
        if package_path.is_file() && is_packed_theme_path(package_path) {
            return Self::load_packed(package_path);
        }
        Self::load_directory_or_manifest(package_path)
    }

    fn load_directory_or_manifest(package_path: &Path) -> Result<Self, ThemePackageError> {
        let manifest_path = if package_path.is_dir() {
            package_path.join("theme.toml")
        } else {
            package_path.to_path_buf()
        };
        let root_dir = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let manifest = read_text_file(&manifest_path, "theme manifest")?;
        let recipe_path = root_dir.join("theme.recipe");
        let recipe = read_text_file(&recipe_path, "theme recipe")?;
        let assets = AssetStore::load_directory(root_dir.join("assets").as_path())?;
        Self::from_sections(&manifest, &recipe, assets, root_dir, manifest_path)
    }

    fn load_packed(package_path: &Path) -> Result<Self, ThemePackageError> {
        let (manifest, recipe, assets) = read_packed_sections(package_path)?;
        let root_dir = package_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::from_sections(
            &manifest,
            &recipe,
            assets,
            root_dir,
            package_path.to_path_buf(),
        )
    }

    fn from_sections(
        manifest: &str,
        recipe: &str,
        assets: AssetStore,
        root_dir: PathBuf,
        manifest_path: PathBuf,
    ) -> Result<Self, ThemePackageError> {
        let mut manifest_parser = KeyValueDocument::parse(manifest)?;
        let name = manifest_parser
            .take_string("", "name")?
            .ok_or_else(|| ThemePackageError::new("theme manifest is missing `name`"))?;
        let api_version = manifest_parser
            .take_u32("", "api_version")?
            .ok_or_else(|| ThemePackageError::new("theme manifest is missing `api_version`"))?;
        let entry = manifest_parser
            .take_string("", "entry")?
            .ok_or_else(|| ThemePackageError::new("theme manifest is missing `entry`"))?;
        if entry != "recipe:v1" {
            return Err(ThemePackageError::new(format!(
                "theme `{name}` uses unsupported entry `{entry}`; supported entry is `recipe:v1`"
            )));
        }
        let capabilities = ThemeCapabilities {
            window_chrome: manifest_parser
                .take_bool("capabilities", "window_chrome")?
                .unwrap_or(false),
            cursor: manifest_parser
                .take_bool("capabilities", "cursor")?
                .unwrap_or(false),
            animations: manifest_parser
                .take_bool("capabilities", "animations")?
                .unwrap_or(false),
            materials: manifest_parser
                .take_bool("capabilities", "materials")?
                .unwrap_or(false),
            hot_reload: manifest_parser
                .take_bool("capabilities", "hot_reload")?
                .unwrap_or(false),
        };

        let recipe = ThemeRecipe::parse(recipe, &assets)?;
        Ok(Self {
            name,
            api_version,
            entry,
            capabilities,
            root_dir,
            manifest_path,
            recipe,
            assets: assets.into_public(),
        })
    }

    pub fn image_asset(&self, id: &str) -> Result<&ThemeImageAsset, ThemePackageError> {
        self.assets.image(id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeRecipe {
    pub output_background: ColorRgba8,
    pub focused: ThemeWindowStyle,
    pub unfocused: ThemeWindowStyle,
    pub content_palette: Vec<ColorRgba8>,
    pub cursor: ThemeCursorAsset,
}

impl ThemeRecipe {
    fn parse(recipe: &str, assets: &AssetStore) -> Result<Self, ThemePackageError> {
        let mut doc = KeyValueDocument::parse(recipe)?;
        let output_background = required_color(&mut doc, "output", "background")?;
        let focused = ThemeWindowStyle::parse(&mut doc, "window.focused")?;
        let unfocused = ThemeWindowStyle::parse(&mut doc, "window.unfocused")?;
        let palette = doc
            .take_string("content", "palette")?
            .ok_or_else(|| ThemePackageError::new("theme recipe is missing `content.palette`"))?;
        let content_palette = palette
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(parse_color)
            .collect::<Result<Vec<_>, _>>()?;
        if content_palette.is_empty() {
            return Err(ThemePackageError::new(
                "theme recipe `content.palette` must contain at least one color",
            ));
        }
        let cursor_asset = doc
            .take_string("cursor", "asset")?
            .ok_or_else(|| ThemePackageError::new("theme recipe is missing `cursor.asset`"))?;
        let cursor = ThemeCursorAsset {
            hotspot: PointI {
                x: required_i32(&mut doc, "cursor", "hotspot_x")?,
                y: required_i32(&mut doc, "cursor", "hotspot_y")?,
            },
            image: assets.image(&cursor_asset)?.clone(),
        };
        Ok(Self {
            output_background,
            focused,
            unfocused,
            content_palette,
            cursor,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeCursorAsset {
    pub hotspot: PointI,
    pub image: ThemeImageAsset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeImageAsset {
    pub size: SizeI,
    pub pixels_rgba8: Arc<[u8]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeAssetStore {
    images: BTreeMap<String, ThemeImageAsset>,
}

impl ThemeAssetStore {
    pub fn image(&self, id: &str) -> Result<&ThemeImageAsset, ThemePackageError> {
        self.images
            .get(id)
            .ok_or_else(|| ThemePackageError::new(format!("theme asset `{id}` was not found")))
    }

    pub fn image_ids(&self) -> impl Iterator<Item = &str> {
        self.images.keys().map(String::as_str)
    }

    pub fn image_assets(&self) -> impl Iterator<Item = (&str, &ThemeImageAsset)> {
        self.images.iter().map(|(id, image)| (id.as_str(), image))
    }

    pub fn stable_texture_key(id: &str) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        for byte in id.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash | (1 << 63)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeWindowStyle {
    pub border_px: i32,
    pub titlebar_px: i32,
    pub radius_px: i32,
    pub titlebar_color: ColorRgba8,
    pub border_color: ColorRgba8,
    pub show_title_text: bool,
    pub title_text_color: ColorRgba8,
    pub shadow_color: ColorRgba8,
    pub shadow_radius_px: i32,
    pub shadow_offset: PointI,
    pub shadow_strength: u8,
    pub glass_tint_color: ColorRgba8,
    pub glass_opacity: u8,
    pub backdrop_blur_radius_px: i32,
    pub backdrop_blur_passes: u8,
    pub use_glass: bool,
    pub show_window_controls: bool,
    pub expand_color: ColorRgba8,
    pub expand_hover_color: ColorRgba8,
    pub close_color: ColorRgba8,
    pub close_hover_color: ColorRgba8,
}

impl ThemeWindowStyle {
    fn parse(doc: &mut KeyValueDocument, section: &str) -> Result<Self, ThemePackageError> {
        Ok(Self {
            border_px: required_i32(doc, section, "border_px")?,
            titlebar_px: required_i32(doc, section, "titlebar_px")?,
            radius_px: required_i32(doc, section, "radius_px")?,
            titlebar_color: required_color(doc, section, "titlebar_color")?,
            border_color: required_color(doc, section, "border_color")?,
            show_title_text: doc
                .take_bool(section, "show_title_text")?
                .unwrap_or(true),
            title_text_color: required_color(doc, section, "title_text_color")?,
            shadow_color: required_color(doc, section, "shadow_color")?,
            shadow_radius_px: required_i32(doc, section, "shadow_radius_px")?,
            shadow_offset: PointI {
                x: required_i32(doc, section, "shadow_offset_x")?,
                y: required_i32(doc, section, "shadow_offset_y")?,
            },
            shadow_strength: required_u8(doc, section, "shadow_strength")?,
            glass_tint_color: required_color(doc, section, "glass_tint_color")?,
            glass_opacity: required_u8(doc, section, "glass_opacity")?,
            backdrop_blur_radius_px: required_i32(doc, section, "backdrop_blur_radius_px")?,
            backdrop_blur_passes: required_u8(doc, section, "backdrop_blur_passes")?,
            use_glass: doc
                .take_bool(section, "use_glass")?
                .ok_or_else(|| missing_key(section, "use_glass"))?,
            show_window_controls: doc
                .take_bool(section, "show_window_controls")?
                .unwrap_or(true),
            expand_color: required_color(doc, section, "expand_color")?,
            expand_hover_color: required_color(doc, section, "expand_hover_color")?,
            close_color: required_color(doc, section, "close_color")?,
            close_hover_color: required_color(doc, section, "close_hover_color")?,
        })
    }
}

pub type ThemeStyle = ThemeRecipe;

#[derive(Default)]
struct AssetStore {
    images: BTreeMap<String, ThemeImageAsset>,
}

impl AssetStore {
    fn load_directory(assets_dir: &Path) -> Result<Self, ThemePackageError> {
        let mut store = Self::default();
        if !assets_dir.is_dir() {
            return Ok(store);
        }
        load_rgba_assets(assets_dir, assets_dir, &mut store)?;
        Ok(store)
    }

    fn load_packed(cursor: &mut ByteCursor<'_>) -> Result<Self, ThemePackageError> {
        let count = cursor.read_u32()? as usize;
        if count > MAX_ASSETS {
            return Err(ThemePackageError::new(format!(
                "theme package has {count} assets; maximum supported asset count is {MAX_ASSETS}"
            )));
        }
        let mut store = Self::default();
        for _ in 0..count {
            let id = cursor.read_string()?;
            let kind = cursor.read_u8()?;
            match kind {
                1 => {
                    let width = cursor.read_i32()?;
                    let height = cursor.read_i32()?;
                    validate_image_size(width, height)?;
                    let bytes = cursor.read_bytes_section("image asset")?;
                    validate_rgba_len(&id, width, height, bytes.len())?;
                    store.images.insert(
                        id,
                        ThemeImageAsset {
                            size: SizeI { width, height },
                            pixels_rgba8: Arc::from(bytes),
                        },
                    );
                }
                _ => {
                    return Err(ThemePackageError::new(format!(
                        "theme package asset `{id}` has unsupported kind {kind}"
                    )));
                }
            }
        }
        Ok(store)
    }

    fn image(&self, id: &str) -> Result<&ThemeImageAsset, ThemePackageError> {
        self.images
            .get(id)
            .ok_or_else(|| ThemePackageError::new(format!("theme asset `{id}` was not found")))
    }

    fn into_public(self) -> ThemeAssetStore {
        ThemeAssetStore {
            images: self.images,
        }
    }
}

fn load_rgba_assets(
    root: &Path,
    dir: &Path,
    store: &mut AssetStore,
) -> Result<(), ThemePackageError> {
    for entry in fs::read_dir(dir).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to read theme assets directory {}: {error}",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            ThemePackageError::new(format!(
                "failed to read an entry from theme assets directory {}: {error}",
                dir.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            ThemePackageError::new(format!("failed to inspect asset {}: {error}", path.display()))
        })?;
        if file_type.is_dir() {
            load_rgba_assets(root, &path, store)?;
            continue;
        }
        let is_rgba = path.extension().and_then(|ext| ext.to_str()) == Some("rgba");
        let is_rgba_hex = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".rgba.hex"));
        if !file_type.is_file() || (!is_rgba && !is_rgba_hex) {
            continue;
        }
        let metadata_path = if is_rgba_hex {
            path.with_file_name(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .trim_end_matches(".hex")
                    .to_string()
                    + ".meta",
            )
        } else {
            path.with_extension("rgba.meta")
        };
        let metadata = read_text_file(&metadata_path, "RGBA asset metadata")?;
        let mut doc = KeyValueDocument::parse(&metadata)?;
        let width = required_i32(&mut doc, "", "width")?;
        let height = required_i32(&mut doc, "", "height")?;
        validate_image_size(width, height)?;
        let pixels = if is_rgba_hex {
            decode_hex_rgba_asset(&path)?
        } else {
            fs::read(&path).map_err(|error| {
                ThemePackageError::new(format!(
                    "failed to read RGBA asset {}: {error}",
                    path.display()
                ))
            })?
        };
        validate_rgba_len(path.to_string_lossy().as_ref(), width, height, pixels.len())?;
        let mut id = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(stripped) = id.strip_suffix(".hex") {
            id = stripped.to_string();
        }
        store.images.insert(
            id,
            ThemeImageAsset {
                size: SizeI { width, height },
                pixels_rgba8: Arc::from(pixels),
            },
        );
    }
    Ok(())
}

fn decode_hex_rgba_asset(path: &Path) -> Result<Vec<u8>, ThemePackageError> {
    let source = read_text_file(path, "hex RGBA asset")?;
    let hex = source
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if hex.len() % 2 != 0 {
        return Err(ThemePackageError::new(format!(
            "hex RGBA asset {} has an odd number of hex digits",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[index..index + 2], 16).map_err(|error| {
            ThemePackageError::new(format!(
                "hex RGBA asset {} has invalid hex at byte {}: {error}",
                path.display(),
                index / 2
            ))
        })?;
        bytes.push(byte);
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePackageError {
    message: String,
}

impl ThemePackageError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ThemePackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ThemePackageError {}

#[derive(Default)]
struct KeyValueDocument {
    values: BTreeMap<(String, String), String>,
}

impl KeyValueDocument {
    fn parse(source: &str) -> Result<Self, ThemePackageError> {
        let mut doc = Self::default();
        let mut section = String::new();
        for (line_number, raw_line) in source.lines().enumerate() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_string();
                if section.len() > MAX_STRING_BYTES {
                    return Err(ThemePackageError::new(format!(
                        "section name on line {} is too long",
                        line_number + 1
                    )));
                }
                continue;
            }
            let Some((raw_key, raw_value)) = line.split_once('=') else {
                return Err(ThemePackageError::new(format!(
                    "invalid theme document line {}: expected `key = value`",
                    line_number + 1
                )));
            };
            let key = raw_key.trim();
            if key.is_empty() || key.len() > MAX_STRING_BYTES {
                return Err(ThemePackageError::new(format!(
                    "invalid key on line {}",
                    line_number + 1
                )));
            }
            doc.values.insert(
                (section.clone(), key.to_string()),
                parse_value(raw_value.trim(), line_number + 1)?,
            );
        }
        Ok(doc)
    }

    fn take_string(
        &mut self,
        section: &str,
        key: &str,
    ) -> Result<Option<String>, ThemePackageError> {
        let value = self.values.remove(&(section.to_string(), key.to_string()));
        if value.as_ref().is_some_and(|value| value.len() > MAX_STRING_BYTES) {
            return Err(ThemePackageError::new(format!(
                "`{}` is too long",
                dotted_key(section, key)
            )));
        }
        Ok(value)
    }

    fn take_u32(&mut self, section: &str, key: &str) -> Result<Option<u32>, ThemePackageError> {
        self.take_string(section, key)?
            .map(|value| {
                value.parse::<u32>().map_err(|error| {
                    ThemePackageError::new(format!(
                        "invalid integer for `{}`: {error}",
                        dotted_key(section, key)
                    ))
                })
            })
            .transpose()
    }

    fn take_i32(&mut self, section: &str, key: &str) -> Result<Option<i32>, ThemePackageError> {
        self.take_string(section, key)?
            .map(|value| {
                value.parse::<i32>().map_err(|error| {
                    ThemePackageError::new(format!(
                        "invalid integer for `{}`: {error}",
                        dotted_key(section, key)
                    ))
                })
            })
            .transpose()
    }

    fn take_u8(&mut self, section: &str, key: &str) -> Result<Option<u8>, ThemePackageError> {
        self.take_string(section, key)?
            .map(|value| {
                value.parse::<u8>().map_err(|error| {
                    ThemePackageError::new(format!(
                        "invalid byte value for `{}`: {error}",
                        dotted_key(section, key)
                    ))
                })
            })
            .transpose()
    }

    fn take_bool(&mut self, section: &str, key: &str) -> Result<Option<bool>, ThemePackageError> {
        self.take_string(section, key)?
            .map(|value| {
                value.parse::<bool>().map_err(|error| {
                    ThemePackageError::new(format!(
                        "invalid bool for `{}`: {error}",
                        dotted_key(section, key)
                    ))
                })
            })
            .transpose()
    }
}

struct ByteCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn expect_magic(&mut self, magic: &[u8]) -> Result<(), ThemePackageError> {
        let actual = self.read_exact(magic.len())?;
        if actual != magic {
            return Err(ThemePackageError::new(
                "theme package has an unsupported magic/version header",
            ));
        }
        Ok(())
    }

    fn read_utf8_section(&mut self, label: &str) -> Result<String, ThemePackageError> {
        let bytes = self.read_bytes_section(label)?;
        String::from_utf8(bytes).map_err(|error| {
            ThemePackageError::new(format!("theme package {label} is not valid UTF-8: {error}"))
        })
    }

    fn read_bytes_section(&mut self, label: &str) -> Result<Vec<u8>, ThemePackageError> {
        let len = self.read_u32()?;
        if len > MAX_SECTION_BYTES {
            return Err(ThemePackageError::new(format!(
                "theme package {label} section is {len} bytes; maximum is {MAX_SECTION_BYTES}"
            )));
        }
        Ok(self.read_exact(len as usize)?.to_vec())
    }

    fn read_string(&mut self) -> Result<String, ThemePackageError> {
        let len = self.read_u16()? as usize;
        if len > MAX_STRING_BYTES {
            return Err(ThemePackageError::new(
                "theme package string exceeds maximum supported length",
            ));
        }
        let bytes = self.read_exact(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            ThemePackageError::new(format!("theme package string is not valid UTF-8: {error}"))
        })
    }

    fn read_u8(&mut self) -> Result<u8, ThemePackageError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, ThemePackageError> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, ThemePackageError> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i32(&mut self) -> Result<i32, ThemePackageError> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], ThemePackageError> {
        let end = self.offset.checked_add(len).ok_or_else(|| {
            ThemePackageError::new("theme package section offset overflowed usize")
        })?;
        if end > self.bytes.len() {
            return Err(ThemePackageError::new(
                "theme package ended before a declared section could be read",
            ));
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

pub fn write_packed_theme(
    manifest: &str,
    recipe: &str,
    assets_dir: &Path,
    output_path: &Path,
) -> Result<(), ThemePackageError> {
    let assets = AssetStore::load_directory(assets_dir)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(PACKED_MAGIC);
    write_section(&mut bytes, manifest.as_bytes())?;
    write_section(&mut bytes, recipe.as_bytes())?;
    write_u32(&mut bytes, assets.images.len() as u32);
    for (id, image) in assets.images {
        write_string(&mut bytes, &id)?;
        bytes.push(1);
        bytes.extend_from_slice(&image.size.width.to_le_bytes());
        bytes.extend_from_slice(&image.size.height.to_le_bytes());
        write_section(&mut bytes, &image.pixels_rgba8)?;
    }
    if bytes.len() as u64 > MAX_PACKED_BYTES {
        return Err(ThemePackageError::new(format!(
            "packed theme would be {} bytes; maximum supported size is {} bytes",
            bytes.len(),
            MAX_PACKED_BYTES
        )));
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ThemePackageError::new(format!(
                "failed to create theme output directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    fs::write(output_path, bytes).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to write packed theme {}: {error}",
            output_path.display()
        ))
    })
}

pub fn unpack_packed_theme(
    package_path: &Path,
    output_dir: &Path,
) -> Result<(), ThemePackageError> {
    let (manifest, recipe, assets) = read_packed_sections(package_path)?;
    if output_dir.exists() {
        fs::remove_dir_all(output_dir).map_err(|error| {
            ThemePackageError::new(format!(
                "failed to clear theme staging directory {}: {error}",
                output_dir.display()
            ))
        })?;
    }
    fs::create_dir_all(output_dir.join("assets")).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to create theme staging directory {}: {error}",
            output_dir.display()
        ))
    })?;
    fs::write(output_dir.join("theme.toml"), manifest).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to write staged theme manifest {}: {error}",
            output_dir.join("theme.toml").display()
        ))
    })?;
    fs::write(output_dir.join("theme.recipe"), recipe).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to write staged theme recipe {}: {error}",
            output_dir.join("theme.recipe").display()
        ))
    })?;
    for (id, image) in assets.images {
        let relative = safe_asset_relative_path(&id)?;
        let path = output_dir.join("assets").join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ThemePackageError::new(format!(
                    "failed to create staged asset directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ThemePackageError::new("theme asset id has invalid file name"))?;
        let hex_path = path.with_file_name(format!("{file_name}.hex"));
        let meta_path = path.with_file_name(format!("{file_name}.meta"));
        fs::write(hex_path, encode_hex(&image.pixels_rgba8)).map_err(|error| {
            ThemePackageError::new(format!("failed to write staged asset `{id}`: {error}"))
        })?;
        fs::write(
            meta_path,
            format!("width = {}\nheight = {}\n", image.size.width, image.size.height),
        )
        .map_err(|error| {
            ThemePackageError::new(format!("failed to write staged asset metadata `{id}`: {error}"))
        })?;
    }
    Ok(())
}

fn read_packed_sections(
    package_path: &Path,
) -> Result<(String, String, AssetStore), ThemePackageError> {
    let metadata = fs::metadata(package_path).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to stat theme package {}: {error}",
            package_path.display()
        ))
    })?;
    if metadata.len() > MAX_PACKED_BYTES {
        return Err(ThemePackageError::new(format!(
            "theme package {} is {} bytes; maximum supported size is {} bytes",
            package_path.display(),
            metadata.len(),
            MAX_PACKED_BYTES
        )));
    }
    let bytes = fs::read(package_path).map_err(|error| {
        ThemePackageError::new(format!(
            "failed to read theme package {}: {error}",
            package_path.display()
        ))
    })?;
    let mut cursor = ByteCursor::new(&bytes);
    cursor.expect_magic(PACKED_MAGIC)?;
    let manifest = cursor.read_utf8_section("manifest")?;
    let recipe = cursor.read_utf8_section("recipe")?;
    let assets = AssetStore::load_packed(&mut cursor)?;
    if !cursor.is_empty() {
        return Err(ThemePackageError::new(
            "theme package has trailing bytes after asset table",
        ));
    }
    Ok((manifest, recipe, assets))
}

fn is_packed_theme_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("lthm" | "lithic-theme")
    )
}

fn safe_asset_relative_path(id: &str) -> Result<PathBuf, ThemePackageError> {
    let path = Path::new(id);
    if path.is_absolute() {
        return Err(ThemePackageError::new(format!(
            "theme asset id `{id}` must be relative"
        )));
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => clean.push(part),
            _ => {
                return Err(ThemePackageError::new(format!(
                    "theme asset id `{id}` may not contain `.` or `..`"
                )));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(ThemePackageError::new("theme asset id may not be empty"));
    }
    Ok(clean)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2 + bytes.len() / 32);
    for (index, byte) in bytes.iter().copied().enumerate() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
        if index % 32 == 31 {
            out.push('\n');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_section(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ThemePackageError> {
    if bytes.len() > MAX_SECTION_BYTES as usize {
        return Err(ThemePackageError::new(format!(
            "theme section is {} bytes; maximum is {MAX_SECTION_BYTES}",
            bytes.len()
        )));
    }
    write_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
    Ok(())
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), ThemePackageError> {
    if value.len() > u16::MAX as usize || value.len() > MAX_STRING_BYTES {
        return Err(ThemePackageError::new(format!(
            "theme asset id `{value}` is too long"
        )));
    }
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_text_file(path: &Path, label: &str) -> Result<String, ThemePackageError> {
    fs::read_to_string(path).map_err(|error| {
        ThemePackageError::new(format!("failed to read {label} {}: {error}", path.display()))
    })
}

fn parse_value(value: &str, line_number: usize) -> Result<String, ThemePackageError> {
    if value.starts_with('"') || value.ends_with('"') {
        if !value.starts_with('"') || !value.ends_with('"') || value.len() < 2 {
            return Err(ThemePackageError::new(format!(
                "invalid string literal on line {line_number}"
            )));
        }
        return Ok(value[1..value.len() - 1].to_string());
    }
    Ok(value.to_string())
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut previous_escape = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' if !previous_escape => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
        previous_escape = ch == '\\' && !previous_escape;
        if ch != '\\' {
            previous_escape = false;
        }
    }
    line
}

fn required_i32(
    doc: &mut KeyValueDocument,
    section: &str,
    key: &str,
) -> Result<i32, ThemePackageError> {
    doc.take_i32(section, key)?
        .ok_or_else(|| missing_key(section, key))
}

fn required_u8(
    doc: &mut KeyValueDocument,
    section: &str,
    key: &str,
) -> Result<u8, ThemePackageError> {
    doc.take_u8(section, key)?
        .ok_or_else(|| missing_key(section, key))
}

fn required_color(
    doc: &mut KeyValueDocument,
    section: &str,
    key: &str,
) -> Result<ColorRgba8, ThemePackageError> {
    doc.take_string(section, key)?
        .ok_or_else(|| missing_key(section, key))
        .and_then(|value| parse_color(&value))
}

fn missing_key(section: &str, key: &str) -> ThemePackageError {
    ThemePackageError::new(format!(
        "theme document is missing `{}`",
        dotted_key(section, key)
    ))
}

fn dotted_key(section: &str, key: &str) -> String {
    if section.is_empty() {
        key.to_string()
    } else {
        format!("{section}.{key}")
    }
}

fn parse_color(value: &str) -> Result<ColorRgba8, ThemePackageError> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 8 {
        return Err(ThemePackageError::new(format!(
            "invalid color `{value}`; expected #rrggbbaa"
        )));
    }
    let rgba = u32::from_str_radix(hex, 16)
        .map_err(|error| ThemePackageError::new(format!("invalid color `{value}`: {error}")))?;
    Ok(ColorRgba8::rgba(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    ))
}

fn validate_image_size(width: i32, height: i32) -> Result<(), ThemePackageError> {
    if width <= 0 || height <= 0 {
        return Err(ThemePackageError::new(format!(
            "theme image has invalid size {width}x{height}"
        )));
    }
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| ThemePackageError::new("theme image dimensions overflowed i32"))?;
    if pixels > MAX_IMAGE_PIXELS {
        return Err(ThemePackageError::new(format!(
            "theme image has {pixels} pixels; maximum supported count is {MAX_IMAGE_PIXELS}"
        )));
    }
    Ok(())
}

fn validate_rgba_len(
    label: &str,
    width: i32,
    height: i32,
    actual: usize,
) -> Result<(), ThemePackageError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| ThemePackageError::new(format!("theme image `{label}` size overflowed")))?;
    if actual != expected {
        return Err(ThemePackageError::new(format!(
            "theme image `{label}` has {actual} bytes; expected {expected} bytes for {width}x{height} RGBA8"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{ThemePackage, write_packed_theme};

    #[test]
    fn loads_recipe_theme_from_directory() {
        let root = write_sample_theme("theme-package");
        let package = ThemePackage::load(&root).unwrap();

        assert_eq!(package.name, "regolith-default");
        assert_eq!(package.entry, "recipe:v1");
        assert!(package.capabilities.window_chrome);
        assert!(package.capabilities.cursor);
        assert_eq!(package.recipe.cursor.image.size.width, 2);
        assert_eq!(
            package
                .image_asset("cursors/default.rgba")
                .unwrap()
                .pixels_rgba8
                .len(),
            16
        );
        assert!(
            package
                .assets
                .image_ids()
                .any(|id| id == "cursors/default.rgba")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_packed_recipe_theme() {
        let root = write_sample_theme("packed-theme");
        let output = root.join("target").join("regolith-default.lithic-theme");
        let manifest = fs::read_to_string(root.join("theme.toml")).unwrap();
        let recipe = fs::read_to_string(root.join("theme.recipe")).unwrap();
        write_packed_theme(&manifest, &recipe, root.join("assets").as_path(), &output).unwrap();

        let package = ThemePackage::load(&output).unwrap();

        assert_eq!(package.name, "regolith-default");
        assert_eq!(package.recipe.cursor.image.pixels_rgba8.len(), 16);

        fs::remove_dir_all(root).unwrap();
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
            "lithic-theme-{prefix}-{}-{timestamp}",
            std::process::id()
        ))
    }
}
