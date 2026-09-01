use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use crate::assets::{AssetBundle, AssetError, AssetKey, AssetKind, CursorAsset, CursorThemeAsset};
use crate::platform::{
    MAX_CUSTOM_CURSOR_ANIMATION_DURATION_MS, MAX_CUSTOM_CURSOR_FRAME_DURATION_MS,
    MAX_CUSTOM_CURSOR_FRAMES, PointerIcon,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PointerHotspot {
    pub x: u16,
    pub y: u16,
}

impl PointerHotspot {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerFrame {
    pub asset: CursorAsset,
    pub duration_ms: Option<NonZeroU32>,
}

impl PointerFrame {
    pub const fn still(asset: CursorAsset) -> Self {
        Self {
            asset,
            duration_ms: None,
        }
    }

    pub const fn animated(asset: CursorAsset, duration_ms: NonZeroU32) -> Self {
        Self {
            asset,
            duration_ms: Some(duration_ms),
        }
    }
}

/// One semantic pointer graphic. Size is optional; theme and output defaults can supply it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerGraphic {
    frames: Arc<[PointerFrame]>,
    hotspot: PointerHotspot,
    size: Option<u16>,
}

impl PointerGraphic {
    pub fn new(asset: CursorAsset) -> Self {
        Self {
            frames: Arc::from([PointerFrame::still(asset)]),
            hotspot: PointerHotspot::default(),
            size: None,
        }
    }

    pub fn animated(frames: Vec<PointerFrame>) -> Result<Self, CursorThemeError> {
        if frames.len() < 2 || frames.len() > MAX_CUSTOM_CURSOR_FRAMES {
            return Err(CursorThemeError::InvalidFrameCount);
        }
        if frames.iter().any(|frame| frame.duration_ms.is_none()) {
            return Err(CursorThemeError::MissingFrameDuration);
        }
        if frames.iter().any(|frame| {
            frame
                .duration_ms
                .is_some_and(|duration| duration.get() > MAX_CUSTOM_CURSOR_FRAME_DURATION_MS)
        }) {
            return Err(CursorThemeError::FrameDurationTooLong);
        }
        let cycle_duration_ms = frames
            .iter()
            .filter_map(|frame| frame.duration_ms)
            .map(|duration| u64::from(duration.get()))
            .sum::<u64>();
        if cycle_duration_ms > MAX_CUSTOM_CURSOR_ANIMATION_DURATION_MS {
            return Err(CursorThemeError::AnimationDurationTooLong);
        }
        Ok(Self {
            frames: frames.into(),
            hotspot: PointerHotspot::default(),
            size: None,
        })
    }

    pub const fn hotspot(mut self, x: u16, y: u16) -> Self {
        self.hotspot = PointerHotspot::new(x, y);
        self
    }

    pub const fn size(mut self, physical_pixels: u16) -> Self {
        self.size = Some(physical_pixels);
        self
    }

    pub fn frames(&self) -> &[PointerFrame] {
        &self.frames
    }

    pub const fn pointer_hotspot(&self) -> PointerHotspot {
        self.hotspot
    }

    pub const fn physical_size(&self) -> Option<u16> {
        self.size
    }
}

impl From<CursorAsset> for PointerGraphic {
    fn from(value: CursorAsset) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PointerThemeFallback {
    #[default]
    System,
    ThemeDefault,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointerTheme {
    graphics: BTreeMap<PointerIcon, PointerGraphic>,
    default_size: Option<u16>,
    fallback: PointerThemeFallback,
}

impl PointerTheme {
    pub fn from_asset(
        asset: CursorThemeAsset,
        bundle: AssetBundle,
    ) -> Result<Self, CursorThemeError> {
        let entry = asset.resolve(bundle)?;
        let source = std::str::from_utf8(entry.bytes)
            .map_err(|_| CursorThemeError::ManifestNotUtf8(asset.key()))?;
        let value = source
            .parse::<toml::Table>()
            .map_err(|error| CursorThemeError::Manifest(error.to_string()))?;
        Self::from_table(&value, bundle)
    }

    pub fn set(mut self, icon: PointerIcon, graphic: impl Into<PointerGraphic>) -> Self {
        self.graphics.insert(icon, graphic.into());
        self
    }

    pub fn default_size(mut self, physical_pixels: u16) -> Self {
        self.default_size = (physical_pixels > 0).then_some(physical_pixels);
        self
    }

    pub const fn fallback(mut self, fallback: PointerThemeFallback) -> Self {
        self.fallback = fallback;
        self
    }

    pub fn graphic(&self, icon: PointerIcon) -> Option<&PointerGraphic> {
        self.graphics.get(&icon).or_else(|| {
            (self.fallback == PointerThemeFallback::ThemeDefault)
                .then(|| self.graphics.get(&PointerIcon::Default))
                .flatten()
        })
    }

    pub const fn physical_size(&self) -> Option<u16> {
        self.default_size
    }

    fn from_table(table: &toml::Table, bundle: AssetBundle) -> Result<Self, CursorThemeError> {
        let mut theme = PointerTheme::default();
        if let Some(size) = table.get("size").and_then(toml::Value::as_integer) {
            theme.default_size = Some(valid_size(size)?);
        }
        if let Some(fallback) = table.get("fallback").and_then(toml::Value::as_str) {
            theme.fallback = match fallback {
                "system" => PointerThemeFallback::System,
                "default" => PointerThemeFallback::ThemeDefault,
                _ => return Err(CursorThemeError::InvalidFallback(fallback.to_owned())),
            };
        }
        for (name, value) in table {
            let Some(icon) = PointerIcon::from_name(name) else {
                if matches!(name.as_str(), "size" | "fallback") {
                    continue;
                }
                return Err(CursorThemeError::UnknownPointerIcon(name.clone()));
            };
            let definition = value
                .as_table()
                .ok_or_else(|| CursorThemeError::InvalidEntry(name.clone()))?;
            let graphic = parse_graphic(name, definition, bundle, theme.default_size)?;
            theme.graphics.insert(icon, graphic);
        }
        Ok(theme)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointerThemeOverrides {
    graphics: BTreeMap<PointerIcon, PointerGraphic>,
}

impl PointerThemeOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, icon: PointerIcon, graphic: impl Into<PointerGraphic>) -> Self {
        self.graphics.insert(icon, graphic.into());
        self
    }

    pub fn pointer(self, graphic: impl Into<PointerGraphic>) -> Self {
        self.set(PointerIcon::Pointer, graphic)
    }

    pub fn text(self, graphic: impl Into<PointerGraphic>) -> Self {
        self.set(PointerIcon::Text, graphic)
    }

    pub fn default_pointer(self, graphic: impl Into<PointerGraphic>) -> Self {
        self.set(PointerIcon::Default, graphic)
    }

    pub fn graphic(&self, icon: PointerIcon) -> Option<&PointerGraphic> {
        self.graphics.get(&icon)
    }
}

/// Focused control over whether hosted Wayland clients may provide pixel cursor surfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ClientCursorMode {
    #[default]
    Allow,
    ThemeOnly,
}

/// Concise application-level pointer configuration; this is data, not a policy service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointerConfiguration {
    theme: Option<CursorThemeAsset>,
    overrides: PointerThemeOverrides,
    client_mode: ClientCursorMode,
}

impl PointerConfiguration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cursor_theme(mut self, theme: CursorThemeAsset) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn overrides(mut self, overrides: PointerThemeOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    pub const fn client_mode(mut self, client_mode: ClientCursorMode) -> Self {
        self.client_mode = client_mode;
        self
    }

    pub const fn theme(&self) -> Option<CursorThemeAsset> {
        self.theme
    }

    pub const fn pointer_overrides(&self) -> &PointerThemeOverrides {
        &self.overrides
    }

    pub const fn client_cursor_mode(&self) -> ClientCursorMode {
        self.client_mode
    }

    pub fn load_theme(
        &self,
        bundle: AssetBundle,
    ) -> Result<Option<PointerTheme>, CursorThemeError> {
        self.theme
            .map(|theme| PointerTheme::from_asset(theme, bundle))
            .transpose()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerRequest {
    Hidden,
    ClientSurface,
    Semantic(PointerIcon),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerResolution<'a> {
    Hidden,
    ClientSurface,
    Graphic(&'a PointerGraphic),
    System(PointerIcon),
}

/// Applies the fixed hidden → client → override → theme → system precedence.
pub fn resolve_pointer<'a>(
    request: PointerRequest,
    client_mode: ClientCursorMode,
    overrides: &'a PointerThemeOverrides,
    theme: Option<&'a PointerTheme>,
) -> PointerResolution<'a> {
    match request {
        PointerRequest::Hidden => PointerResolution::Hidden,
        PointerRequest::ClientSurface if client_mode == ClientCursorMode::Allow => {
            PointerResolution::ClientSurface
        }
        PointerRequest::ClientSurface => resolve_semantic(PointerIcon::Default, overrides, theme),
        PointerRequest::Semantic(icon) => resolve_semantic(icon, overrides, theme),
    }
}

fn resolve_semantic<'a>(
    icon: PointerIcon,
    overrides: &'a PointerThemeOverrides,
    theme: Option<&'a PointerTheme>,
) -> PointerResolution<'a> {
    overrides
        .graphic(icon)
        .or_else(|| theme.and_then(|theme| theme.graphic(icon)))
        .map_or(PointerResolution::System(icon), PointerResolution::Graphic)
}

fn parse_graphic(
    name: &str,
    definition: &toml::Table,
    bundle: AssetBundle,
    default_size: Option<u16>,
) -> Result<PointerGraphic, CursorThemeError> {
    let hotspot = definition
        .get("hotspot")
        .and_then(toml::Value::as_array)
        .map(|values| {
            if values.len() != 2 {
                return Err(CursorThemeError::InvalidHotspot(name.to_owned()));
            }
            Ok(PointerHotspot::new(
                valid_coordinate(values[0].as_integer(), name)?,
                valid_coordinate(values[1].as_integer(), name)?,
            ))
        })
        .transpose()?
        .unwrap_or_default();
    let size = definition
        .get("size")
        .and_then(toml::Value::as_integer)
        .map(valid_size)
        .transpose()?
        .or(default_size);
    let mut graphic = if let Some(frames) = definition.get("frames") {
        let frames = frames
            .as_array()
            .ok_or_else(|| CursorThemeError::InvalidEntry(name.to_owned()))?
            .iter()
            .map(|frame| {
                let frame = frame
                    .as_table()
                    .ok_or_else(|| CursorThemeError::InvalidEntry(name.to_owned()))?;
                let asset = cursor_asset(frame.get("asset"), name, bundle)?;
                let duration = frame
                    .get("duration_ms")
                    .and_then(toml::Value::as_integer)
                    .and_then(|value| u32::try_from(value).ok())
                    .and_then(NonZeroU32::new)
                    .ok_or(CursorThemeError::MissingFrameDuration)?;
                Ok(PointerFrame::animated(asset, duration))
            })
            .collect::<Result<Vec<_>, CursorThemeError>>()?;
        PointerGraphic::animated(frames)?
    } else {
        PointerGraphic::new(cursor_asset(definition.get("asset"), name, bundle)?)
    };
    graphic.hotspot = hotspot;
    graphic.size = size;
    if let Some(size) = size
        && (u32::from(hotspot.x) >= u32::from(size) || u32::from(hotspot.y) >= u32::from(size))
    {
        return Err(CursorThemeError::HotspotOutOfBounds(name.to_owned()));
    }
    Ok(graphic)
}

fn cursor_asset(
    value: Option<&toml::Value>,
    name: &str,
    bundle: AssetBundle,
) -> Result<CursorAsset, CursorThemeError> {
    let path = value
        .and_then(toml::Value::as_str)
        .ok_or_else(|| CursorThemeError::MissingAsset(name.to_owned()))?;
    let entry = bundle
        .iter()
        .find(|entry| entry.key.as_str() == path)
        .ok_or_else(|| CursorThemeError::MissingRegisteredAsset(path.to_owned()))?;
    if entry.kind != AssetKind::Cursor {
        return Err(CursorThemeError::Asset(AssetError::KindMismatch {
            key: entry.key,
            expected: AssetKind::Cursor,
            actual: entry.kind,
        }));
    }
    Ok(CursorAsset::new(entry.key))
}

fn valid_coordinate(value: Option<i64>, name: &str) -> Result<u16, CursorThemeError> {
    value
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| CursorThemeError::InvalidHotspot(name.to_owned()))
}

fn valid_size(value: i64) -> Result<u16, CursorThemeError> {
    let size = u16::try_from(value).map_err(|_| CursorThemeError::InvalidSize)?;
    if size == 0 {
        return Err(CursorThemeError::InvalidSize);
    }
    Ok(size)
}

#[derive(Debug, thiserror::Error)]
pub enum CursorThemeError {
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error("cursor theme `{0}` is not UTF-8")]
    ManifestNotUtf8(AssetKey),
    #[error("invalid cursor theme manifest: {0}")]
    Manifest(String),
    #[error("unknown pointer icon `{0}`")]
    UnknownPointerIcon(String),
    #[error("invalid cursor entry `{0}`")]
    InvalidEntry(String),
    #[error("cursor entry `{0}` has no asset")]
    MissingAsset(String),
    #[error("cursor asset `{0}` is not registered")]
    MissingRegisteredAsset(String),
    #[error("cursor entry `{0}` has an invalid hotspot")]
    InvalidHotspot(String),
    #[error("cursor entry `{0}` has a hotspot outside its declared size")]
    HotspotOutOfBounds(String),
    #[error("cursor size must be a positive u16")]
    InvalidSize,
    #[error("cursor animation must contain 2..={MAX_CUSTOM_CURSOR_FRAMES} frames")]
    InvalidFrameCount,
    #[error("each animated cursor frame requires a positive duration_ms")]
    MissingFrameDuration,
    #[error("cursor frame duration exceeds the hard limit")]
    FrameDurationTooLong,
    #[error("cursor animation cycle exceeds the hard limit")]
    AnimationDurationTooLong,
    #[error("cursor fallback must be `system` or `default`, got `{0}`")]
    InvalidFallback(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetEntry;

    const THEME: CursorThemeAsset = CursorThemeAsset::new(AssetKey::new("cursors/theme.toml"));
    const POINTER: CursorAsset = CursorAsset::new(AssetKey::new("cursors/pointer.svg"));
    static ENTRIES: [AssetEntry; 2] = [
        AssetEntry::embedded(
            THEME.key(),
            AssetKind::CursorTheme,
            "application/toml",
            b"fallback = \"system\"\nsize = 24\n[pointer]\nasset = \"cursors/pointer.svg\"\nhotspot = [3, 2]\n",
        ),
        AssetEntry::embedded(
            POINTER.key(),
            AssetKind::Cursor,
            "image/svg+xml",
            b"<svg/>",
        ),
    ];

    #[test]
    fn manifest_and_override_use_fixed_precedence() {
        let bundle = AssetBundle::new(&ENTRIES);
        let theme = PointerTheme::from_asset(THEME, bundle).unwrap();
        let override_graphic = PointerGraphic::new(POINTER).hotspot(1, 1);
        let overrides = PointerThemeOverrides::new().pointer(override_graphic.clone());
        assert_eq!(
            resolve_pointer(
                PointerRequest::Semantic(PointerIcon::Pointer),
                ClientCursorMode::Allow,
                &overrides,
                Some(&theme)
            ),
            PointerResolution::Graphic(&override_graphic)
        );
        assert_eq!(
            theme
                .graphic(PointerIcon::Pointer)
                .unwrap()
                .pointer_hotspot(),
            PointerHotspot::new(3, 2)
        );
    }
}
