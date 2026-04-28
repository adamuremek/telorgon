extern crate self as lithic_app;

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lithic_core::SizeI;
use lithic_render::{RenderFrame, RenderGraph, RenderResult, RenderTargetId, RenderedFrame, Renderer};
use lithic_ui::WidgetTree;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, Size};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub initial_size: SizeI,
    pub min_size: Option<SizeI>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Lithic App".to_string(),
            initial_size: SizeI {
                width: 1280,
                height: 800,
            },
            min_size: Some(SizeI {
                width: 960,
                height: 600,
            }),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(u64);

impl TimerId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    RequestRedraw,
    SetTitle(String),
    SetClipboard(String),
    OpenFileDialog { extensions: Vec<String> },
    SaveFileDialog { extension: String },
    StartTimer { id: TimerId, interval: Duration },
    StopTimer(TimerId),
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEvent {
    Started,
    WindowResized(SizeI),
    RedrawRequested,
    Timer(TimerId),
    FileOpened(PathBuf),
    FileSaved(PathBuf),
    Clipboard(String),
    Action(String),
    CloseRequested,
}

pub trait Application {
    fn window_config(&self) -> WindowConfig {
        WindowConfig::default()
    }

    fn started(&mut self, _ctx: &mut AppContext) {}
    fn event(&mut self, event: AppEvent, ctx: &mut AppContext);
    fn view(&self) -> WidgetTree;
    fn render(&mut self, ctx: &mut AppContext) -> RenderFrame;
}

#[derive(Debug, Default)]
pub struct AppContext {
    commands: VecDeque<Command>,
    timers: BTreeMap<TimerId, Instant>,
    next_timer_id: u64,
}

impl AppContext {
    pub fn command(&mut self, command: Command) {
        self.commands.push_back(command);
    }

    pub fn request_redraw(&mut self) {
        self.command(Command::RequestRedraw);
    }

    pub fn next_timer_id(&mut self) -> TimerId {
        self.next_timer_id += 1;
        TimerId::new(self.next_timer_id)
    }

    pub fn drain_commands(&mut self) -> impl Iterator<Item = Command> + '_ {
        self.commands.drain(..)
    }

    pub fn remember_timer(&mut self, id: TimerId, deadline: Instant) {
        self.timers.insert(id, deadline);
    }
}

#[derive(Debug)]
pub struct AppError {
    message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Default)]
pub struct HeadlessRuntime<R> {
    pub renderer: R,
    pub target_id: RenderTargetId,
}

impl<R> HeadlessRuntime<R>
where
    R: Renderer,
{
    pub fn run_once<A>(&mut self, app: &mut A, extent: SizeI) -> RenderResult<RenderedFrame>
    where
        A: Application,
    {
        let mut ctx = AppContext::default();
        app.started(&mut ctx);
        app.event(AppEvent::Started, &mut ctx);
        app.event(AppEvent::WindowResized(extent), &mut ctx);
        let frame = app.render(&mut ctx);
        self.renderer.register_target(self.target_id, extent)?;
        self.renderer.render(&frame, &RenderGraph::default())
    }
}

pub fn winit_available() -> bool {
    let _ = winit::event_loop::EventLoop::<()>::with_user_event();
    true
}

pub fn run_native<A>(app: A) -> AppResult<()>
where
    A: Application + 'static,
{
    let event_loop = EventLoop::new().map_err(|error| AppError::new(error.to_string()))?;
    let mut runtime = WinitRuntime {
        app,
        ctx: AppContext::default(),
        window: None,
        window_id: None,
    };
    event_loop
        .run_app(&mut runtime)
        .map_err(|error| AppError::new(error.to_string()))
}

struct WinitRuntime<A> {
    app: A,
    ctx: AppContext,
    window: Option<Window>,
    window_id: Option<WindowId>,
}

impl<A> ApplicationHandler for WinitRuntime<A>
where
    A: Application,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let config = self.app.window_config();
        let mut attributes = WindowAttributes::default()
            .with_title(config.title)
            .with_inner_size(Size::Logical(LogicalSize::new(
                config.initial_size.width as f64,
                config.initial_size.height as f64,
            )));
        if let Some(min_size) = config.min_size {
            attributes = attributes.with_min_inner_size(Size::Logical(LogicalSize::new(
                min_size.width as f64,
                min_size.height as f64,
            )));
        }
        match event_loop.create_window(attributes) {
            Ok(window) => {
                self.window_id = Some(window.id());
                self.window = Some(window);
                self.app.started(&mut self.ctx);
                self.app.event(AppEvent::Started, &mut self.ctx);
                self.flush_commands(event_loop);
            }
            Err(error) => {
                eprintln!("lithic-app: failed to create native window: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.app.event(AppEvent::CloseRequested, &mut self.ctx);
                self.flush_commands(event_loop);
            }
            WindowEvent::Resized(size) => {
                self.app.event(
                    AppEvent::WindowResized(SizeI {
                        width: size.width as i32,
                        height: size.height as i32,
                    }),
                    &mut self.ctx,
                );
                self.flush_commands(event_loop);
            }
            WindowEvent::RedrawRequested => {
                self.app.event(AppEvent::RedrawRequested, &mut self.ctx);
                let _ = self.app.view();
                let _ = self.app.render(&mut self.ctx);
                self.flush_commands(event_loop);
            }
            _ => {}
        }
    }
}

impl<A> WinitRuntime<A>
where
    A: Application,
{
    fn flush_commands(&mut self, event_loop: &ActiveEventLoop) {
        for command in self.ctx.drain_commands() {
            match command {
                Command::RequestRedraw => {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
                Command::SetTitle(title) => {
                    if let Some(window) = &self.window {
                        window.set_title(&title);
                    }
                }
                Command::Quit => event_loop.exit(),
                Command::SetClipboard(_)
                | Command::OpenFileDialog { .. }
                | Command::SaveFileDialog { .. }
                | Command::StartTimer { .. }
                | Command::StopTimer(_) => {}
            }
        }
    }
}
