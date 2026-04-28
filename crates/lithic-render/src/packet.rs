use std::fmt;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;

use crate::core::{ColorRgba8, RectI, SizeI};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderTargetId(pub u64);

impl RenderTargetId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RenderTargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RenderTarget({})", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderFrame {
    pub output_id: RenderTargetId,
    pub extent: SizeI,
    pub background: ColorRgba8,
    pub damage_rects: Arc<[RectI]>,
    pub ops: Vec<RenderOp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderOp {
    Rect(RenderRect),
    Blit(RenderBlit),
    Text(RenderText),
    Material(RenderMaterial),
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CornerRadii {
    pub top_left: i32,
    pub top_right: i32,
    pub bottom_right: i32,
    pub bottom_left: i32,
}

impl CornerRadii {
    pub const fn zero() -> Self {
        Self {
            top_left: 0,
            top_right: 0,
            bottom_right: 0,
            bottom_left: 0,
        }
    }

    pub const fn all(radius_px: i32) -> Self {
        Self {
            top_left: radius_px,
            top_right: radius_px,
            bottom_right: radius_px,
            bottom_left: radius_px,
        }
    }

    pub const fn top(radius_px: i32) -> Self {
        Self {
            top_left: radius_px,
            top_right: radius_px,
            bottom_right: 0,
            bottom_left: 0,
        }
    }

    pub const fn bottom(radius_px: i32) -> Self {
        Self {
            top_left: 0,
            top_right: 0,
            bottom_right: radius_px,
            bottom_left: radius_px,
        }
    }

    pub fn sanitize(self) -> Self {
        Self {
            top_left: self.top_left.max(0),
            top_right: self.top_right.max(0),
            bottom_right: self.bottom_right.max(0),
            bottom_left: self.bottom_left.max(0),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RenderRect {
    pub rect: RectI,
    pub color: ColorRgba8,
    pub corner_radii_px: CornerRadii,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderText {
    pub rect: RectI,
    pub text: String,
    pub color: ColorRgba8,
    pub font_size_px: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderBlit {
    pub texture_key: u64,
    pub dst_x: i32,
    pub dst_y: i32,
    pub width: i32,
    pub height: i32,
    pub src_x: i32,
    pub src_y: i32,
    pub src_width: i32,
    pub pixels_rgba8: Arc<[u8]>,
    pub dmabuf: Option<Arc<RenderDmabuf>>,
    pub content_version: u64,
    pub damage_rects: Arc<[RectI]>,
    pub corner_radii_px: CornerRadii,
}

#[derive(Clone, Debug)]
pub struct RenderDmabuf {
    pub width: i32,
    pub height: i32,
    pub format: u32,
    pub modifier: u64,
    pub planes: Arc<[RenderDmabufPlane]>,
    pub acquire_fence: Option<Arc<OwnedFd>>,
}

impl Eq for RenderDmabuf {}

impl PartialEq for RenderDmabuf {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.format == other.format
            && self.modifier == other.modifier
            && self.planes == other.planes
            && self.acquire_fence.as_ref().map(|fd| fd.as_raw_fd())
                == other.acquire_fence.as_ref().map(|fd| fd.as_raw_fd())
    }
}

#[derive(Clone, Debug)]
pub struct RenderDmabufPlane {
    pub fd: Arc<OwnedFd>,
    pub plane_index: u32,
    pub offset: u32,
    pub stride: u32,
}

impl PartialEq for RenderDmabufPlane {
    fn eq(&self, other: &Self) -> bool {
        self.fd.as_raw_fd() == other.fd.as_raw_fd()
            && self.plane_index == other.plane_index
            && self.offset == other.offset
            && self.stride == other.stride
    }
}

impl Eq for RenderDmabufPlane {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderMaterial {
    pub rect: RectI,
    pub corner_radii_px: CornerRadii,
    pub shader_name: String,
    pub shader_spirv_words: Option<Vec<u32>>,
    pub kind: RenderMaterialKind,
    pub passes: Vec<RenderMaterialPass>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderMaterialKind {
    Shadow {
        color: ColorRgba8,
        radius_px: i32,
        strength: u8,
    },
    BackdropBlur {
        radius_px: i32,
        passes: u8,
    },
    Glass {
        tint_color: ColorRgba8,
        opacity: u8,
        blur_radius_px: i32,
        passes: u8,
    },
    Tint {
        color: ColorRgba8,
        opacity: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderMaterialPass {
    BackdropCapture {
        source_rect: RectI,
    },
    Blur {
        radius_px: i32,
        passes: u8,
    },
    Tint {
        color: ColorRgba8,
        opacity: u8,
    },
    Shadow {
        color: ColorRgba8,
        radius_px: i32,
        strength: u8,
    },
}
