use std::sync::Arc;

use crate::core::SizeI;
use crate::presenter_softbuffer::SoftbufferPresenter;
use crate::render::{RenderBackend, RenderRequest, RenderTargetInfo, TargetLoad, TargetStore};
use crate::renderer_software::{SoftwareRenderer, SoftwareScene, SoftwareSurface, SoftwareTarget};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use super::resize::ResizeUpdate;
use super::winit_host::{NativePresentation, PreparedPresentationFrame, PresentationAction};

pub(crate) struct SoftwarePresentation {
    presenter: SoftbufferPresenter,
    renderer: SoftwareRenderer,
    scene: SoftwareScene,
    framebuffer: SoftwareSurface,
    presented: bool,
}

impl SoftwarePresentation {
    pub(crate) fn new(display: OwnedDisplayHandle) -> Result<Self, String> {
        let renderer = SoftwareRenderer;
        let scene = renderer
            .create_scene()
            .map_err(|error| format!("software scene creation failed: {error}"))?;
        Ok(Self {
            presenter: SoftbufferPresenter::from_display(display)
                .map_err(|error| error.to_string())?,
            renderer,
            scene,
            framebuffer: SoftwareSurface::default(),
            presented: false,
        })
    }

    pub(crate) fn attach(&mut self, window: Arc<Window>) -> Result<(), String> {
        self.presenter
            .attach(window)
            .map_err(|error| error.to_string())?;
        self.presented = false;
        #[cfg(feature = "profiler")]
        crate::profiler::instant!("renderer.software");
        Ok(())
    }

    pub(crate) fn present(&mut self, frame: PreparedPresentationFrame) -> Result<bool, String> {
        #[cfg(feature = "profiler")]
        let _presentation = crate::profiler::start_presentation("presentation.software");
        self.presenter
            .configure(frame.metrics)
            .map_err(|error| error.to_string())?;
        for delta in frame.deltas {
            #[cfg(feature = "profiler")]
            let _span = crate::profiler::span!("delta.apply");
            self.renderer
                .apply_scene_delta(&mut self.scene, &delta)
                .map_err(|error| format!("software scene update failed: {error}"))?;
        }
        if !frame.force_present && !frame.changed && self.presented {
            #[cfg(feature = "profiler")]
            crate::profiler::instant!("presentation.idle");
            return Ok(false);
        }
        let logical_extent = frame.metrics.logical_extent;
        let extent = SizeI {
            width: logical_extent.width.ceil().max(1.0) as i32,
            height: logical_extent.height.ceil().max(1.0) as i32,
        };
        let target = SoftwareTarget::new(RenderTargetInfo::full(extent));
        let clear = self.scene.background();
        let force_render = frame.force_present || !self.presented;
        let stats = {
            #[cfg(feature = "profiler")]
            let _span = crate::profiler::span!("software.raster");
            let mut frame = self.framebuffer.begin_frame();
            self.renderer
                .render(
                    &mut self.scene,
                    &mut frame,
                    &target,
                    &RenderRequest {
                        force: force_render,
                        load: TargetLoad::Clear(clear),
                        store: TargetStore::Store,
                        region: None,
                    },
                )
                .map_err(|error| format!("software render failed: {error}"))?
        };
        if !stats.recorded && self.presented {
            #[cfg(feature = "profiler")]
            crate::profiler::instant!("presentation.idle");
            return Ok(false);
        }
        #[cfg(feature = "profiler")]
        {
            crate::profiler::counter!("render.upload_bytes", stats.upload_bytes_recorded);
            crate::profiler::counter!("render.buffer_copies", stats.buffer_copies);
            crate::profiler::counter!("render.buffer_allocations", stats.buffer_allocations);
            crate::profiler::counter!("render.descriptor_writes", stats.descriptor_writes);
            crate::profiler::counter!("render.batches", stats.batches);
            crate::profiler::counter!("render.draws", stats.draws);
            crate::profiler::counter!("render.damage_area", stats.damage_area);
            crate::profiler::counter!("framebuffer.bytes", self.framebuffer.pixels_rgba8().len());
        }
        {
            #[cfg(feature = "profiler")]
            let _span = crate::profiler::span!("surface.copy");
            self.presenter
                .present(&self.framebuffer)
                .map_err(|error| error.to_string())?;
        }
        self.presented = true;
        #[cfg(feature = "profiler")]
        crate::profiler::instant!("presentation.presented");
        Ok(true)
    }
}

impl NativePresentation for SoftwarePresentation {
    fn attach(&mut self, window: Arc<Window>) -> Result<(), String> {
        SoftwarePresentation::attach(self, window)
    }

    fn resume(&mut self, window: Arc<Window>) -> Result<(), String> {
        if !self.presenter.is_attached() {
            SoftwarePresentation::attach(self, window)
        } else {
            Ok(())
        }
    }

    fn resize(&mut self, _update: ResizeUpdate) -> Result<(), String> {
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), String> {
        self.presenter.suspend();
        self.presented = false;
        Ok(())
    }

    fn present(&mut self, frame: PreparedPresentationFrame) -> Result<PresentationAction, String> {
        SoftwarePresentation::present(self, frame).map(|presented| {
            if presented {
                PresentationAction::Submitted
            } else {
                PresentationAction::Idle
            }
        })
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.presenter.shutdown();
        Ok(())
    }
}
