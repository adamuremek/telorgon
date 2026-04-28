mod docs;
mod service;

use lithic_app::{AppContext, AppEvent, Application, Command, WindowConfig};
use lithic_core::{ColorRgba8, RectI, SizeI};
use lithic_render::{CornerRadii, RenderFrame, RenderOp, RenderRect, RenderTargetId};
use lithic_ui::{
    Axis, CodeEditor, EdgeInsetsI, ImageCanvas, ImageData, List, ListItem, Panel, PanelStyle,
    SplitPane, SplitPaneItem, TextDocument, Widget, WidgetTree, button, text, vstack,
    widget_action,
};
use service::{StudioService, StudioSnapshot};
use std::sync::Arc;

fn main() {
    if let Err(error) = run() {
        eprintln!("lithic-theme-studio: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let service = StudioService::new()?;
    let snapshot = service.snapshot()?;
    let preview = service.preview(
        "Fixed 640 x 420 Window",
        SizeI {
            width: 960,
            height: 640,
        },
    )?;
    let app = NativeStudioApp {
        service,
        snapshot,
        preview: Some(ImageData {
            size: preview.frame.extent,
            pixels_rgba8: preview.frame.pixels_rgba8,
        }),
        status: "Native Studio ready".to_string(),
    };
    println!("Lithic Theme Studio");
    println!("  staging: {}", app.snapshot.staging_dir.display());
    println!("  export:  {}", app.snapshot.export_path.display());
    lithic_app::run_native(app).map_err(Into::into)
}

#[derive(Clone, Debug)]
struct NativeStudioApp {
    service: StudioService,
    snapshot: StudioSnapshot,
    preview: Option<ImageData>,
    status: String,
}

impl Application for NativeStudioApp {
    fn window_config(&self) -> WindowConfig {
        WindowConfig {
            title: "Lithic Theme Studio".to_string(),
            initial_size: SizeI {
                width: 1440,
                height: 900,
            },
            min_size: Some(SizeI {
                width: 1100,
                height: 680,
            }),
        }
    }

    fn event(&mut self, event: AppEvent, ctx: &mut AppContext) {
        match event {
            AppEvent::Started => ctx.request_redraw(),
            AppEvent::Action(action) if action == "studio.preview" => {
                match self.service.preview(
                    "Fixed 640 x 420 Window",
                    SizeI {
                        width: 960,
                        height: 640,
                    },
                ) {
                    Ok(preview) => {
                        self.preview = Some(ImageData {
                            size: preview.frame.extent,
                            pixels_rgba8: preview.frame.pixels_rgba8,
                        });
                        self.status = "Preview rendered".to_string();
                    }
                    Err(error) => self.status = error.to_string(),
                }
                ctx.request_redraw();
            }
            AppEvent::Action(action) if action == "studio.export" => {
                match self.service.build_export(None) {
                    Ok(path) => self.status = format!("Exported {}", path.display()),
                    Err(error) => self.status = error.to_string(),
                }
                ctx.request_redraw();
            }
            AppEvent::CloseRequested => ctx.command(Command::Quit),
            _ => {}
        }
    }

    fn view(&self) -> WidgetTree {
        WidgetTree::new(Widget::SplitPane(SplitPane {
            axis: Axis::Horizontal,
            panes: vec![
                SplitPaneItem {
                    id: "assets".to_string(),
                    child: self.assets_panel(),
                    min_size_px: 240,
                    fraction: 20,
                    collapsed: false,
                },
                SplitPaneItem {
                    id: "editor".to_string(),
                    child: self.editor_panel(),
                    min_size_px: 420,
                    fraction: 42,
                    collapsed: false,
                },
                SplitPaneItem {
                    id: "preview".to_string(),
                    child: self.preview_panel(),
                    min_size_px: 420,
                    fraction: 38,
                    collapsed: false,
                },
            ],
        }))
    }

    fn render(&mut self, _ctx: &mut AppContext) -> RenderFrame {
        RenderFrame {
            output_id: RenderTargetId::new(1),
            extent: SizeI {
                width: 1440,
                height: 900,
            },
            background: ColorRgba8::rgba(16, 18, 23, 255),
            damage_rects: Arc::from([]),
            ops: vec![
                RenderOp::Rect(RenderRect {
                    rect: RectI {
                        x: 0,
                        y: 0,
                        width: 1440,
                        height: 52,
                    },
                    color: ColorRgba8::rgba(23, 26, 33, 255),
                    corner_radii_px: CornerRadii::zero(),
                }),
                RenderOp::Rect(RenderRect {
                    rect: RectI {
                        x: 0,
                        y: 52,
                        width: 1440,
                        height: 848,
                    },
                    color: ColorRgba8::rgba(21, 24, 32, 255),
                    corner_radii_px: CornerRadii::zero(),
                }),
            ],
        }
    }
}

impl NativeStudioApp {
    fn assets_panel(&self) -> Widget {
        let items = self
            .snapshot
            .assets
            .iter()
            .map(|asset| ListItem {
                id: asset.path.clone(),
                label: asset.path.clone(),
                detail: Some(format!("{} x {}", asset.size.width, asset.size.height)),
                thumbnail: Some(ImageData {
                    size: asset.size,
                    pixels_rgba8: asset.pixels_rgba8.clone(),
                }),
                action: Some(widget_action(format!("studio.asset.{}", asset.path))),
            })
            .collect();
        panel(
            "Assets",
            Widget::List(List {
                items,
                selected: None,
            }),
        )
    }

    fn editor_panel(&self) -> Widget {
        panel(
            "Theme Rust",
            vstack(
                [
                    button(
                        "Run Preview",
                        ColorRgba8::rgba(238, 242, 248, 255),
                        ColorRgba8::rgba(32, 38, 51, 255),
                        Some(widget_action("studio.preview")),
                    ),
                    Widget::CodeEditor(CodeEditor {
                        id: "theme-code".to_string(),
                        document: TextDocument {
                            text: self.snapshot.code.clone(),
                            cursor_byte: 0,
                            selection_anchor_byte: None,
                            scroll_line: 0,
                            undo_depth: 0,
                            redo_depth: 0,
                        },
                        language: "rust".to_string(),
                        diagnostics: Vec::new(),
                        completions: Vec::new(),
                        hover: None,
                    }),
                ],
                8,
            ),
        )
    }

    fn preview_panel(&self) -> Widget {
        panel(
            "Preview",
            vstack(
                [
                    Widget::ImageCanvas(ImageCanvas {
                        id: "preview-canvas".to_string(),
                        image: self.preview.clone(),
                        zoom_percent: 100,
                        pan_x: 0,
                        pan_y: 0,
                        hit_regions: Vec::new(),
                    }),
                    text(&self.status, ColorRgba8::rgba(203, 213, 225, 255)),
                    button(
                        "Export .lthm",
                        ColorRgba8::rgba(238, 242, 248, 255),
                        ColorRgba8::rgba(32, 38, 51, 255),
                        Some(widget_action("studio.export")),
                    ),
                    text(docs::STUDIO_REFERENCE, ColorRgba8::rgba(148, 163, 184, 255)),
                ],
                8,
            ),
        )
    }
}

fn panel(title: &str, child: Widget) -> Widget {
    Widget::Panel(Panel {
        title: Some(title.to_string()),
        child: Box::new(child),
        style: PanelStyle {
            background: Some(ColorRgba8::rgba(21, 24, 32, 255)),
            border_color: Some(ColorRgba8::rgba(43, 49, 61, 255)),
            radius_px: 6,
            padding: EdgeInsetsI::all(12),
        },
    })
}
