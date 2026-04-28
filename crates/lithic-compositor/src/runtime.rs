use lithic_core::ColorRgba8;
use lithic_core::SizeI;
use lithic_material::MaterialSystem;
use lithic_render::{
    LiveRenderFrame, RenderError, RenderFrame, RenderResult, RenderTargetId, RenderedFrame,
    Renderer,
};
use lithic_theme::ThemePackage;

use crate::command::SurfaceCommand;
use crate::controller::{SurfaceController, SurfaceResult, TickInput, TickOutput};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CompositorConfig {
    pub default_background: ColorRgba8,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            default_background: ColorRgba8::rgba(0x00, 0x00, 0x00, 0xff),
        }
    }
}

pub fn run_compositor<R>(renderer: R, config: CompositorConfig) -> CompositorRuntime<R> {
    CompositorRuntime {
        controller: SurfaceController::new(),
        renderer: SurfaceRenderer::new(renderer),
        config,
    }
}

pub struct CompositorRuntime<R> {
    controller: SurfaceController,
    renderer: SurfaceRenderer<R>,
    config: CompositorConfig,
}

impl<R> CompositorRuntime<R> {
    pub fn controller(&self) -> &SurfaceController {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut SurfaceController {
        &mut self.controller
    }

    pub fn renderer(&self) -> &SurfaceRenderer<R> {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut SurfaceRenderer<R> {
        &mut self.renderer
    }

    pub fn config(&self) -> CompositorConfig {
        self.config
    }

    pub fn submit(&mut self, command: SurfaceCommand) -> SurfaceResult<()> {
        self.controller.submit(command)
    }

    pub fn tick(&self, input: TickInput) -> TickOutput {
        self.controller.tick(input)
    }

    pub fn tick_default_background(
        &self,
        output_id: RenderTargetId,
        extent: SizeI,
        frame_time_ns: u64,
    ) -> TickOutput {
        self.tick(TickInput {
            output_id,
            extent,
            background: self.config.default_background,
            frame_time_ns,
        })
    }
}

impl<R> CompositorRuntime<R>
where
    R: Renderer,
{
    pub fn render(&mut self, input: TickInput) -> RenderResult<RenderedFrame> {
        let tick = self.tick(input);
        self.renderer.render_tick(&tick)
    }
}

#[derive(Default)]
pub struct NoopSurfaceRenderer;

pub struct SurfaceRenderer<R = NoopSurfaceRenderer> {
    inner: R,
    material_system: MaterialSystem,
}

impl<R> SurfaceRenderer<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            material_system: MaterialSystem::default(),
        }
    }

    pub fn inner(&self) -> &R {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn material_system(&self) -> &MaterialSystem {
        &self.material_system
    }

    pub fn material_system_mut(&mut self) -> &mut MaterialSystem {
        &mut self.material_system
    }

    pub fn load_theme_package(&mut self, package: &ThemePackage) -> RenderResult<()> {
        self.material_system
            .load_theme_package(package)
            .map_err(material_error)
    }

    pub fn resolve_tick_frame(&self, tick: &TickOutput) -> RenderResult<RenderFrame> {
        self.material_system
            .resolve_frame(&tick.render_frame)
            .map_err(material_error)
    }

    pub fn resolve_live_frame(&self, tick: &TickOutput) -> RenderResult<LiveRenderFrame> {
        let frame = self.resolve_tick_frame(tick)?;
        Ok(LiveRenderFrame {
            output_id: frame.output_id,
            extent: frame.extent,
            frame,
        })
    }
}

impl SurfaceRenderer<NoopSurfaceRenderer> {
    pub fn resolver() -> Self {
        Self::new(NoopSurfaceRenderer)
    }
}

impl<R> SurfaceRenderer<R>
where
    R: Renderer,
{
    pub fn render_tick(&mut self, tick: &TickOutput) -> RenderResult<RenderedFrame> {
        let frame = self.resolve_tick_frame(tick)?;
        if self.inner.registered_extent(frame.output_id) != Some(frame.extent) {
            self.inner.register_target(frame.output_id, frame.extent)?;
        }
        self.inner.render(&frame, &Default::default())
    }
}

fn material_error(error: impl std::fmt::Display) -> RenderError {
    RenderError::new(format!("material resolution failed: {error}"))
}

#[cfg(test)]
mod tests {
    use lithic_core::{ColorRgba8, RectI, SizeI};
    use lithic_render::{
        CornerRadii, RenderFrame, RenderMaterial, RenderMaterialKind, RenderMaterialPass, RenderOp,
        RenderTargetId,
    };

    use super::{SurfaceRenderer, TickOutput};

    #[test]
    fn surface_renderer_resolves_tick_materials() {
        let renderer = SurfaceRenderer::resolver();
        let tick = TickOutput {
            render_frame: RenderFrame {
                output_id: RenderTargetId::new(1),
                extent: SizeI {
                    width: 64,
                    height: 48,
                },
                background: ColorRgba8::rgba(0, 0, 0, 255),
                damage_rects: Vec::<RectI>::new().into(),
                ops: vec![RenderOp::Material(RenderMaterial {
                    rect: RectI {
                        x: 4,
                        y: 5,
                        width: 24,
                        height: 18,
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
            },
            hit_regions: Vec::new(),
        };

        let resolved = renderer.resolve_tick_frame(&tick).unwrap();
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
}
