use crate::render::{RenderMaterial, RenderMaterialKind, RenderMaterialPass};

pub fn plan_material_passes(material: &RenderMaterial) -> Vec<RenderMaterialPass> {
    match material.kind {
        RenderMaterialKind::Shadow {
            color,
            radius_px,
            strength,
        } => vec![RenderMaterialPass::Shadow {
            color,
            radius_px,
            strength,
        }],
        RenderMaterialKind::BackdropBlur { radius_px, passes } => vec![
            RenderMaterialPass::BackdropCapture {
                source_rect: material.rect,
            },
            RenderMaterialPass::Blur { radius_px, passes },
        ],
        RenderMaterialKind::Glass {
            tint_color,
            opacity,
            blur_radius_px,
            passes,
        } => {
            let mut planned = vec![RenderMaterialPass::BackdropCapture {
                source_rect: material.rect,
            }];
            if blur_radius_px > 0 && passes > 0 {
                planned.push(RenderMaterialPass::Blur {
                    radius_px: blur_radius_px,
                    passes,
                });
            }
            planned.push(RenderMaterialPass::Tint {
                color: tint_color,
                opacity,
            });
            planned
        }
        RenderMaterialKind::Tint { color, opacity } => vec![
            RenderMaterialPass::BackdropCapture {
                source_rect: material.rect,
            },
            RenderMaterialPass::Tint { color, opacity },
        ],
    }
}
