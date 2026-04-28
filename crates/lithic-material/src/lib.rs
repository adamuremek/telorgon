extern crate self as lithic_material;
pub use lithic_core as foundation;
pub use lithic_render as render;
pub use lithic_theme as theme_api;

mod backdrop;
mod effect_graph;
mod registry;
mod shader;

use std::error::Error;
use std::fmt;

use crate::foundation::SizeI;
use crate::render::{RenderFrame, RenderOp};
use crate::theme_api::ThemePackage;

pub use registry::MaterialRegistry;
pub use shader::{ShaderModuleAsset, ShaderOrigin};

pub struct MaterialSystem {
    registry: MaterialRegistry,
}

impl Default for MaterialSystem {
    fn default() -> Self {
        Self {
            registry: MaterialRegistry::default(),
        }
    }
}

impl MaterialSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_theme_package(
        &mut self,
        package: &ThemePackage,
    ) -> Result<(), MaterialSystemError> {
        self.registry.load_theme_package(package)
    }

    pub fn resolve_frame(&self, frame: &RenderFrame) -> Result<RenderFrame, MaterialSystemError> {
        let mut resolved = frame.clone();
        for op in &mut resolved.ops {
            let RenderOp::Material(material) = op else {
                continue;
            };

            let shader = self.registry.shader(&material.shader_name).ok_or_else(|| {
                MaterialSystemError::new(format!(
                    "material shader `{}` is not registered",
                    material.shader_name
                ))
            })?;
            material.shader_spirv_words = Some(shader.spirv_words.clone());
            material.passes = effect_graph::plan_material_passes(material);
        }

        Ok(resolved)
    }

    pub fn registry(&self) -> &MaterialRegistry {
        &self.registry
    }
}

pub fn execute_material_op(
    frame_rgba8: &mut [u8],
    frame_extent: SizeI,
    material: &crate::render::RenderMaterial,
) {
    backdrop::execute_material_passes(frame_rgba8, frame_extent, material);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialSystemError {
    message: String,
}

impl MaterialSystemError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MaterialSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for MaterialSystemError {}

#[cfg(test)]
mod tests {
    use crate::foundation::{ColorRgba8, RectI, SizeI};
    use crate::render::{
        CornerRadii, RenderFrame, RenderMaterial, RenderMaterialKind, RenderMaterialPass, RenderOp,
        RenderTargetId,
    };

    use super::{MaterialSystem, execute_material_op};

    #[test]
    fn resolve_frame_populates_shader_modules_and_passes() {
        let system = MaterialSystem::new();
        let frame = RenderFrame {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 64,
                height: 48,
            },
            background: ColorRgba8::rgba(0, 0, 0, 255),
            damage_rects: Vec::<RectI>::new().into(),
            ops: vec![RenderOp::Material(RenderMaterial {
                rect: RectI {
                    x: 10,
                    y: 10,
                    width: 20,
                    height: 12,
                },
                corner_radii_px: CornerRadii::all(8),
                shader_name: "glass.spv".to_string(),
                shader_spirv_words: None,
                kind: RenderMaterialKind::Glass {
                    tint_color: ColorRgba8::rgba(0x70, 0x90, 0xa8, 0xff),
                    opacity: 120,
                    blur_radius_px: 8,
                    passes: 2,
                },
                passes: Vec::new(),
            })],
        };

        let resolved = system.resolve_frame(&frame).unwrap();
        let RenderOp::Material(material) = &resolved.ops[0] else {
            panic!("expected material op");
        };

        assert!(material.shader_spirv_words.is_some());
        assert!(matches!(
            material.passes.as_slice(),
            [
                RenderMaterialPass::BackdropCapture { .. },
                RenderMaterialPass::Blur { .. },
                RenderMaterialPass::Tint { .. }
            ]
        ));
    }

    #[test]
    fn executes_tint_pass_over_captured_backdrop() {
        let mut pixels = vec![0x20; 4 * 4 * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 0xff;
        }
        let material = RenderMaterial {
            rect: RectI {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
            corner_radii_px: CornerRadii::all(1),
            shader_name: "glass.spv".to_string(),
            shader_spirv_words: Some(vec![0x0723_0203, 0x0001_0000, 0, 0]),
            kind: RenderMaterialKind::Tint {
                color: ColorRgba8::rgba(0x80, 0x40, 0x20, 0xff),
                opacity: 128,
            },
            passes: vec![
                RenderMaterialPass::BackdropCapture {
                    source_rect: RectI {
                        x: 1,
                        y: 1,
                        width: 2,
                        height: 2,
                    },
                },
                RenderMaterialPass::Tint {
                    color: ColorRgba8::rgba(0x80, 0x40, 0x20, 0xff),
                    opacity: 128,
                },
            ],
        };

        execute_material_op(
            &mut pixels,
            SizeI {
                width: 4,
                height: 4,
            },
            &material,
        );

        let center = pixel_at(&pixels, 4, 1, 1);
        assert!(center.r > 0x20);
        assert!(center.g > 0x20);
        assert_eq!(center.a, 0xff);
    }

    fn pixel_at(bytes: &[u8], width: i32, x: i32, y: i32) -> ColorRgba8 {
        let offset = ((y * width + x) * 4) as usize;
        ColorRgba8::rgba(
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        )
    }
}
