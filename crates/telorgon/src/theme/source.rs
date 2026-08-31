//! Strict Theme v4 source schema. Legacy versions are intentionally rejected at parse time.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::theme::{ThemeDomain, ThemeError, ThemeResult};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSource {
    pub format: ThemeFormat,
    pub domain: ThemeDomain,
    #[serde(default)]
    pub tokens: ThemeTokensSource,
    #[serde(default)]
    pub components: BTreeMap<String, BTreeMap<String, ComponentStyleSource>>,
}

impl ThemeSource {
    pub fn parse(source: &str) -> ThemeResult<Self> {
        toml::from_str(source)
            .map_err(|error| ThemeError::new(format!("invalid Theme v4 source: {error}")))
    }

    pub fn empty(domain: ThemeDomain) -> Self {
        Self {
            format: ThemeFormat::V4,
            domain,
            tokens: ThemeTokensSource::default(),
            components: BTreeMap::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeFormat {
    V4,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ValueSource<T> {
    Literal(T),
    Token { token: String },
}

impl<T> ValueSource<T> {
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Literal(_) => None,
            Self::Token { token } => Some(token),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeTokensSource {
    pub color: BTreeMap<String, ValueSource<String>>,
    pub length: BTreeMap<String, ValueSource<f32>>,
    pub duration: BTreeMap<String, ValueSource<u32>>,
    pub easing: BTreeMap<String, ValueSource<String>>,
    pub typography: BTreeMap<String, ValueSource<TypographySource>>,
    pub shadow: BTreeMap<String, ValueSource<Vec<ShadowSource>>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TypographySource {
    pub color: Option<ValueSource<String>>,
    pub size: Option<ValueSource<f32>>,
    pub line_height: Option<ValueSource<f32>>,
    pub family: Option<String>,
    pub weight: Option<u16>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowSource {
    pub x: ValueSource<f32>,
    pub y: ValueSource<f32>,
    pub blur: ValueSource<f32>,
    pub spread: ValueSource<f32>,
    pub color: ValueSource<String>,
}

impl Default for ValueSource<f32> {
    fn default() -> Self {
        Self::Literal(0.0)
    }
}

impl Default for ValueSource<String> {
    fn default() -> Self {
        Self::Literal("#00000000".to_owned())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComponentStyleSource {
    pub slots: BTreeMap<String, SlotStyleSource>,
    /// `variants.<axis>.<value>.slots.<slot>` overlays base slots before states.
    pub variants: BTreeMap<String, BTreeMap<String, VariantStyleSource>>,
    /// State source order is ignored; the component contract supplies precedence.
    pub states: BTreeMap<String, StateStyleSource>,
    pub transition: Option<TransitionSource>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct VariantStyleSource {
    pub slots: BTreeMap<String, SlotStyleSource>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateStyleSource {
    pub slots: BTreeMap<String, SlotStyleSource>,
    pub transition: Option<TransitionSource>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionSource {
    pub duration: ValueSource<u32>,
    pub easing: ValueSource<String>,
    #[serde(default)]
    pub repeat: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlotStyleSource {
    pub background: Option<ValueSource<String>>,
    pub foreground: Option<ValueSource<String>>,
    pub border_color: Option<ValueSource<String>>,
    pub border_width: Option<ValueSource<f32>>,
    pub outline_color: Option<ValueSource<String>>,
    pub outline_width: Option<ValueSource<f32>>,
    pub outline_offset: Option<ValueSource<f32>>,
    pub radius: Option<ValueSource<f32>>,
    pub padding: Option<ValueSource<f32>>,
    pub margin: Option<ValueSource<f32>>,
    pub width: Option<ValueSource<f32>>,
    pub height: Option<ValueSource<f32>>,
    pub opacity: Option<f32>,
    pub shadows: Option<ValueSource<Vec<ShadowSource>>>,
    pub typography: Option<ValueSource<TypographySource>>,
    pub translation_x: Option<ValueSource<f32>>,
    pub translation_y: Option<ValueSource<f32>>,
    pub scale_x: Option<f32>,
    pub scale_y: Option<f32>,
    pub rotation: Option<f32>,
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_requires_v4_and_domain() {
        assert!(ThemeSource::parse("format = 'v3'").is_err());
        assert!(ThemeSource::parse("format = 'v4'").is_err());
        let source = ThemeSource::parse("format = 'v4'\ndomain = 'application'").unwrap();
        assert_eq!(source.format, ThemeFormat::V4);
        assert_eq!(source.domain, ThemeDomain::Application);
    }

    #[test]
    fn legacy_flat_styles_are_rejected() {
        assert!(
            ThemeSource::parse(
                "format = 'v4'\ndomain = 'application'\n[styles.button]\nradius = 4"
            )
            .is_err()
        );
    }
}
