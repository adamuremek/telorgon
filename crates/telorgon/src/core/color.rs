#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ColorRgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorRgba8 {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_ne_u32(self) -> u32 {
        u32::from_ne_bytes([self.r, self.g, self.b, self.a])
    }

    pub fn with_alpha_scale(self, opacity: f32) -> Self {
        let alpha = (self.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
        Self { a: alpha, ..self }
    }
}
