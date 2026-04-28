extern crate self as lithic_text;

use std::collections::HashMap;
use std::fmt;

use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, Metrics, PhysicalGlyph, Shaping, SwashCache, SwashContent,
};
use lithic_core::{ColorRgba8, RectI};

const DEFAULT_ATLAS_SIZE: i32 = 1024;
const GLYPH_PADDING_PX: i32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextError {
    message: String,
}

impl TextError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TextError {}

pub type TextResult<T> = Result<T, TextError>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub color: ColorRgba8,
    pub font_size_px: i32,
    pub line_height_px: i32,
}

impl TextStyle {
    pub fn new(color: ColorRgba8, font_size_px: i32) -> Self {
        Self {
            color,
            font_size_px,
            line_height_px: (font_size_px as f32 * 1.25).ceil() as i32,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TextLayoutRequest<'a> {
    pub rect: RectI,
    pub text: &'a str,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedText {
    pub rect: RectI,
    pub glyphs: Vec<AtlasGlyph>,
    pub atlas_version: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    pub dst_x: i32,
    pub dst_y: i32,
    pub width_px: i32,
    pub height_px: i32,
    pub atlas_x: i32,
    pub atlas_y: i32,
    pub color: ColorRgba8,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GlyphAtlasView<'a> {
    pub width_px: i32,
    pub height_px: i32,
    pub version: u64,
    pub pixels_a8: &'a [u8],
}

#[derive(Clone, Debug)]
pub struct GlyphAtlas {
    width_px: i32,
    height_px: i32,
    pixels_a8: Vec<u8>,
    entries: HashMap<CacheKey, GlyphAtlasEntry>,
    cursor_x: i32,
    cursor_y: i32,
    row_height_px: i32,
    version: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct GlyphAtlasEntry {
    x: i32,
    y: i32,
    width_px: i32,
    height_px: i32,
}

impl GlyphAtlas {
    pub fn new(width_px: i32, height_px: i32) -> TextResult<Self> {
        if width_px <= 0 || height_px <= 0 {
            return Err(TextError::new(format!(
                "glyph atlas size must be positive, got {width_px}x{height_px}"
            )));
        }

        Ok(Self {
            width_px,
            height_px,
            pixels_a8: vec![0; width_px as usize * height_px as usize],
            entries: HashMap::new(),
            cursor_x: GLYPH_PADDING_PX,
            cursor_y: GLYPH_PADDING_PX,
            row_height_px: 0,
            version: 1,
        })
    }

    pub fn view(&self) -> GlyphAtlasView<'_> {
        GlyphAtlasView {
            width_px: self.width_px,
            height_px: self.height_px,
            version: self.version,
            pixels_a8: &self.pixels_a8,
        }
    }

    pub fn clear(&mut self) {
        self.pixels_a8.fill(0);
        self.entries.clear();
        self.cursor_x = GLYPH_PADDING_PX;
        self.cursor_y = GLYPH_PADDING_PX;
        self.row_height_px = 0;
        self.version = self.version.saturating_add(1);
    }

    fn get_or_insert(
        &mut self,
        key: CacheKey,
        width_px: i32,
        height_px: i32,
        pixels_a8: &[u8],
    ) -> TextResult<GlyphAtlasEntry> {
        if let Some(entry) = self.entries.get(&key) {
            return Ok(*entry);
        }

        if width_px <= 0 || height_px <= 0 {
            let entry = GlyphAtlasEntry {
                x: 0,
                y: 0,
                width_px: 0,
                height_px: 0,
            };
            self.entries.insert(key, entry);
            return Ok(entry);
        }

        if width_px + GLYPH_PADDING_PX * 2 > self.width_px
            || height_px + GLYPH_PADDING_PX * 2 > self.height_px
        {
            return Err(TextError::new(format!(
                "glyph image {width_px}x{height_px} does not fit atlas {}x{}",
                self.width_px, self.height_px
            )));
        }

        if self.cursor_x + width_px + GLYPH_PADDING_PX > self.width_px {
            self.cursor_x = GLYPH_PADDING_PX;
            self.cursor_y += self.row_height_px + GLYPH_PADDING_PX;
            self.row_height_px = 0;
        }

        if self.cursor_y + height_px + GLYPH_PADDING_PX > self.height_px {
            return Err(TextError::new("glyph atlas is full"));
        }

        let entry = GlyphAtlasEntry {
            x: self.cursor_x,
            y: self.cursor_y,
            width_px,
            height_px,
        };
        self.copy_glyph_pixels(entry, pixels_a8)?;
        self.entries.insert(key, entry);
        self.cursor_x += width_px + GLYPH_PADDING_PX;
        self.row_height_px = self.row_height_px.max(height_px);
        self.version = self.version.saturating_add(1);
        Ok(entry)
    }

    fn copy_glyph_pixels(&mut self, entry: GlyphAtlasEntry, pixels_a8: &[u8]) -> TextResult<()> {
        let required_len = entry.width_px as usize * entry.height_px as usize;
        if pixels_a8.len() < required_len {
            return Err(TextError::new(format!(
                "glyph image has {} alpha bytes but needs {required_len}",
                pixels_a8.len()
            )));
        }

        for row in 0..entry.height_px {
            let dst_offset = ((entry.y + row) * self.width_px + entry.x) as usize;
            let src_offset = (row * entry.width_px) as usize;
            let width = entry.width_px as usize;
            self.pixels_a8[dst_offset..dst_offset + width]
                .copy_from_slice(&pixels_a8[src_offset..src_offset + width]);
        }

        Ok(())
    }
}

pub struct FontTextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: GlyphAtlas,
}

impl FontTextRenderer {
    pub fn new() -> TextResult<Self> {
        Self::with_atlas_size(DEFAULT_ATLAS_SIZE, DEFAULT_ATLAS_SIZE)
    }

    pub fn with_atlas_size(width_px: i32, height_px: i32) -> TextResult<Self> {
        Ok(Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(width_px, height_px)?,
        })
    }

    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) -> TextResult<()> {
        let faces_before = self.font_system.db().len();
        self.font_system.db_mut().load_font_data(bytes);
        let faces_after = self.font_system.db().len();
        if faces_after <= faces_before {
            return Err(TextError::new("font bytes did not add any font faces"));
        }
        self.atlas.clear();
        Ok(())
    }

    pub fn atlas(&self) -> GlyphAtlasView<'_> {
        self.atlas.view()
    }

    pub fn prepare_text(&mut self, request: TextLayoutRequest<'_>) -> TextResult<PreparedText> {
        let physical_glyphs = self.layout_physical_glyphs(request);
        match self.prepare_physical_glyphs(request, &physical_glyphs) {
            Ok(prepared) => Ok(prepared),
            Err(error) if error.message == "glyph atlas is full" => {
                self.atlas.clear();
                self.prepare_physical_glyphs(request, &physical_glyphs)
            }
            Err(error) => Err(error),
        }
    }

    fn layout_physical_glyphs(&mut self, request: TextLayoutRequest<'_>) -> Vec<PhysicalGlyph> {
        if request.rect.width <= 0
            || request.rect.height <= 0
            || request.text.is_empty()
            || request.style.color.a == 0
        {
            return Vec::new();
        }

        let font_size = request.style.font_size_px.max(1) as f32;
        let line_height = request.style.line_height_px.max(request.style.font_size_px) as f32;
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        let mut buffer = buffer.borrow_with(&mut self.font_system);
        buffer.set_size(
            Some(request.rect.width as f32),
            Some(request.rect.height as f32),
        );
        buffer.set_text(request.text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(false);

        let mut glyphs = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                glyphs.push(glyph.physical(
                    (request.rect.x as f32, request.rect.y as f32 + run.line_y),
                    1.0,
                ));
            }
        }
        glyphs
    }

    fn prepare_physical_glyphs(
        &mut self,
        request: TextLayoutRequest<'_>,
        physical_glyphs: &[PhysicalGlyph],
    ) -> TextResult<PreparedText> {
        let mut glyphs = Vec::new();
        for glyph in physical_glyphs {
            let image = self
                .swash_cache
                .get_image(&mut self.font_system, glyph.cache_key)
                .clone();
            let Some(image) = image else {
                continue;
            };

            let width_px = image.placement.width as i32;
            let height_px = image.placement.height as i32;
            if width_px <= 0 || height_px <= 0 {
                continue;
            }

            let alpha = glyph_image_alpha(&image)?;
            let entry = self
                .atlas
                .get_or_insert(glyph.cache_key, width_px, height_px, &alpha)?;

            glyphs.push(AtlasGlyph {
                dst_x: glyph.x + image.placement.left,
                dst_y: glyph.y - image.placement.top,
                width_px: entry.width_px,
                height_px: entry.height_px,
                atlas_x: entry.x,
                atlas_y: entry.y,
                color: request.style.color,
            });
        }

        Ok(PreparedText {
            rect: request.rect,
            glyphs,
            atlas_version: self.atlas.version,
        })
    }
}

fn glyph_image_alpha(image: &cosmic_text::SwashImage) -> TextResult<Vec<u8>> {
    let width = image.placement.width as usize;
    let height = image.placement.height as usize;
    let pixel_count = width * height;

    match image.content {
        SwashContent::Mask => {
            if image.data.len() < pixel_count {
                return Err(TextError::new("glyph mask is shorter than its placement"));
            }
            Ok(image.data[..pixel_count].to_vec())
        }
        SwashContent::SubpixelMask | SwashContent::Color => {
            if image.data.len() < pixel_count * 4 {
                return Err(TextError::new(
                    "glyph RGBA data is shorter than its placement",
                ));
            }
            let mut alpha = Vec::with_capacity(pixel_count);
            for pixel in image.data.chunks_exact(4).take(pixel_count) {
                alpha.push(pixel[3].max(pixel[0]).max(pixel[1]).max(pixel[2]));
            }
            Ok(alpha)
        }
    }
}

#[cfg(test)]
mod tests {
    use lithic_core::{ColorRgba8, RectI};

    use crate::{FontTextRenderer, TextLayoutRequest, TextStyle};

    #[test]
    fn prepares_text_into_atlas_glyphs() {
        let mut renderer = FontTextRenderer::with_atlas_size(256, 256).unwrap();
        let prepared = renderer
            .prepare_text(TextLayoutRequest {
                rect: RectI {
                    x: 4,
                    y: 5,
                    width: 160,
                    height: 32,
                },
                text: "Lithic",
                style: TextStyle::new(ColorRgba8::rgba(0xff, 0xff, 0xff, 0xff), 16),
            })
            .unwrap();

        assert!(!prepared.glyphs.is_empty());
        assert!(prepared.atlas_version >= 1);
        assert!(renderer.atlas().pixels_a8.iter().any(|alpha| *alpha > 0));
    }

    #[test]
    fn reuses_cached_glyphs_without_growing_atlas_version() {
        let mut renderer = FontTextRenderer::with_atlas_size(256, 256).unwrap();
        let request = TextLayoutRequest {
            rect: RectI {
                x: 0,
                y: 0,
                width: 160,
                height: 32,
            },
            text: "AA",
            style: TextStyle::new(ColorRgba8::rgba(0xff, 0xff, 0xff, 0xff), 16),
        };

        renderer.prepare_text(request).unwrap();
        let version = renderer.atlas().version;
        renderer.prepare_text(request).unwrap();

        assert_eq!(renderer.atlas().version, version);
    }
}
