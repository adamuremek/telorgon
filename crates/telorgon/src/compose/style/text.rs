use crate::core::ColorRgba8;
use crate::ui::TextStyle as RetainedTextStyle;

use super::Alignment;

/// Sparse, reusable text styling for composition code.
///
/// Unspecified fields inherit Telorgon's default text values. The runtime resolves this
/// authoring value into its retained text representation before mounting or patching a node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub color: Option<ColorRgba8>,
    pub size: Option<f32>,
    pub line_height: Option<f32>,
    pub weight: Option<u16>,
    pub text_align: Option<Alignment>,
}

impl TextStyle {
    pub const fn new() -> Self {
        Self {
            color: None,
            size: None,
            line_height: None,
            weight: None,
            text_align: None,
        }
    }

    pub const fn color(mut self, color: ColorRgba8) -> Self {
        self.color = Some(color);
        self
    }

    pub const fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub const fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Aligns glyph lines within the text element's content box.
    pub const fn text_align(mut self, alignment: Alignment) -> Self {
        self.text_align = Some(alignment);
        self
    }

    #[doc(hidden)]
    pub fn resolve(self) -> RetainedTextStyle {
        let size = self.size.unwrap_or(14.0);
        RetainedTextStyle {
            color: self.color.unwrap_or(ColorRgba8::rgba(27, 31, 40, 255)),
            size,
            line_height: self.line_height.unwrap_or(size * 1.25),
            family: crate::ui::StringId(1),
            weight: self.weight.unwrap_or(400),
            align: self.text_align.unwrap_or_default().into(),
        }
    }
}
