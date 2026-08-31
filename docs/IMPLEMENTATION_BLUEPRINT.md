# Telorgon File and API Implementation Blueprint

## Status and authority

This document completes implementation-planning Gate 1. It is the authoritative file, dependency,
and API-responsibility blueprint for the first three Vulkan delivery slices:

1. removing renderer ownership from the UI runtime and removing the false Vulkan facade;
2. rendering a Telorgon scene primitive with real offscreen Vulkan work; and
3. presenting that scene through an owned Winit/Vulkan surface.

It refines [VULKAN_IMPLEMENTATION_PLAN.md](VULKAN_IMPLEMENTATION_PLAN.md). The current-to-target
crate and API moves are fixed in [MIGRATION_PLAN.md](MIGRATION_PLAN.md). Exact frame,
synchronization, target, presentation, hosted-recording, destruction, and recovery ownership is
fixed in [GPU_OWNERSHIP_AND_SYNCHRONIZATION.md](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md). Retained-scene
identity/deltas, GPU records, batching, uploads, resource uses, and shader interfaces are fixed in
[SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md). Managed, hosted, desktop,
shell/compositor, Metal, and mobile delivery order is fixed in
[PLATFORM_IMPLEMENTATION_ORDER.md](PLATFORM_IMPLEMENTATION_ORDER.md). Evidence layers, shared tests,
hardware-run policy, and qualification reports are fixed in
[ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md). Gate 7's later component/runtime
split is fixed in [AUTHORING_AND_COMPONENT_RUNTIME.md](AUTHORING_AND_COMPONENT_RUNTIME.md). It
starts after the Slice 1 renderer extraction and may not silently change the dependency direction or
responsibility split defined here. Gate 8's still-later domain primitive/component packages and
behavior are fixed in [APPLICATION_AND_SHELL_PRIMITIVES.md](APPLICATION_AND_SHELL_PRIMITIVES.md) and
must not be folded into the mechanical renderer extraction.
Gate 9's later platform/runtime extraction, managed/embedded host split, service adapters, and
source layout are fixed in [PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md).
Those later owners supersede the temporary Slice 1–3 Winit/Softbuffer host placement after its
migration prerequisites pass; they do not broaden the initial Vulkan slices.

The Rust declarations below are normative API shapes. An implementer may adjust syntax required by
the compiler, but a different owner, dependency, or call direction requires updating this document
first.

## 1. Decisions fixed by this gate

### 1.1 The UI runtime does not own a renderer

`AppRuntime<App>` owns application state, mounted UI, layout, text, scene compilation, input,
scheduling, commands, and queued scene deltas. It does not own a renderer, render target, swapchain,
graphics device, or readback mechanism.

This replaces the current `AppRuntime<App, R: Renderer>` shape. Preparing UI state and executing GPU
work become separate calls controlled by an owned host or embedding host.

### 1.2 One backend device serves multiple per-view scenes

A renderer backend has:

- one device-level object for shared pipelines, allocators, caches, queues, and diagnostics;
- one backend scene object per independent `UiView`/`AppRuntime`;
- one frame context supplied by an owned presenter or embedding host; and
- one render target supplied for each render call.

Scene deltas update the per-view backend scene. They do not mutate a process-global renderer scene.
This is required for several independent UI views to share one GPU device without sharing UI state.

### 1.3 Rendering, submission, and presentation are separate operations

The backend records or encodes rendering into a supplied frame context and target. The owned
presenter acquires and presents. A hosted frame may forbid Telorgon from submitting. Therefore a
backend render result cannot claim that work was submitted or presented.

### 1.4 Readback is optional and separate

Readback is not part of the baseline rendering trait. Software and hardware backends may implement
an explicit readback capability used by tests, export, or diagnostics. The Vulkan implementation
performs a real image-to-staging transfer and wait; it never calls the software renderer.

### 1.5 The first Vulkan backend is direct, while the common contract stays internal

The first implementation uses `ash` directly in `telorgon-renderer-vulkan`. The backend-neutral
`RenderBackend` contract lives in `telorgon-render` and remains internal/unstable. Do not create a
large public `telorgon-rhi` crate before the first GPU path. A focused `telorgon-rhi` package may be
extracted after real Vulkan semantics and the trace/second backend identify genuinely common
mechanisms.

### 1.6 WSI is not part of the Vulkan renderer core

Window handles, `ash-window`, Winit, `VkSurfaceKHR`, swapchain acquire, and presentation live in
`telorgon-presenter-vulkan-wsi`. `telorgon-renderer-vulkan` remains usable for offscreen and hosted
rendering without a window/event-loop dependency.

## 2. Dependency graph for slices 1–3

```text
telorgon-core / telorgon-scene / telorgon-layout / telorgon-text / telorgon-ui
                              |                 telorgon-gpu-abi
                              v                    |       |
                        telorgon-render <------------+       |
                         /         \                       |
                        v           v <--------------------+
               telorgon-app     telorgon-renderer-vulkan
                    |                  |
                    |                  v
                    +----> telorgon-presenter-vulkan-wsi
                                      |
                                      v
                              managed native assembly

telorgon-gpu-abi ---> telorgon-shader-build --generates--> packaged Vulkan shader bundle
```

Dependency rules:

- `telorgon-render` has no dependency on a concrete backend, platform, or window system.
- `telorgon-gpu-abi` depends only on the narrow POD/layout dependency set. It owns no renderer,
  allocation, scene state, or Vulkan type.
- `telorgon-app` has no required dependency on Vulkan. Backend/presenter dependencies are optional
  managed-host features.
- `telorgon-renderer-vulkan` depends on `telorgon-render`, `telorgon-gpu-abi`, `ash`, `gpu-allocator`,
  `bytemuck`, and `thiserror`; it does not depend on `winit`, `ash-window`, Softbuffer, or
  `SoftwareRenderer`.
- `telorgon-presenter-vulkan-wsi` depends on `telorgon-renderer-vulkan`, `ash-window`,
  `raw-window-handle`, `winit`, and the small platform/application types required to drive a window.
- `telorgon-shader-build` is a tool. No application or runtime crate depends on it.
- Backend-native Vulkan types may cross only between Vulkan backend/presenter/interop packages.

## 3. `telorgon-render` file and API blueprint

### 3.1 Required files

```text
crates/telorgon/src/render/
├── lib.rs              Curated exports; no implementation bodies
├── backend.rs          RenderBackend and explicit optional capability traits
├── error.rs            Backend-neutral error category and context
├── request.rs          Render request, load/store intent, and render region
├── stats.rs            Recorded-work statistics; no presentation claims
├── target.rs           Backend-neutral target metadata and semantic usages
├── readback.rs         Explicit readback request/result and capability trait
├── compiler.rs         Existing UI/layout/text-to-retained-scene compiler
├── scene.rs            Existing retained render scene and deltas
└── software.rs         Frozen deterministic software implementation
```

Do not split the frozen software rasterizer merely to satisfy the tree. Split it only when a
correctness change makes a focused file necessary. The new interface files must remain small and
single-purpose.

### 3.2 Backend contract

`backend.rs` defines this responsibility shape:

```rust,ignore
pub trait RenderBackend {
    type Scene;
    type FrameContext<'frame>
    where
        Self: 'frame;
    type Target<'frame>
    where
        Self: 'frame;

    fn create_scene(&self) -> RenderResult<Self::Scene>;

    fn apply_scene_delta(
        &self,
        scene: &mut Self::Scene,
        delta: &RenderSceneDelta,
    ) -> RenderResult<SceneUpdateStats>;

    fn render<'frame>(
        &self,
        scene: &mut Self::Scene,
        frame: &mut Self::FrameContext<'frame>,
        target: &Self::Target<'frame>,
        request: &RenderRequest,
    ) -> RenderResult<RenderStats>;
}
```

Required semantics:

- `create_scene` allocates per-view backend state but does not create a window or presentation
  surface.
- `apply_scene_delta` updates retained backend resources or queues exact uploads for that scene.
- `render` records/executes only against the supplied context and target.
- dropping `Scene` schedules safe backend destruction; it must not force device idle.
- `FrameContext` and `Target` are lifetime-bearing backend-specific associated types so native APIs
  are not erased into untyped integers and hosted/acquired borrows remain expressible.
- shared device methods synchronize internal queues, allocation, caches, and garbage; `&mut Scene`
  preserves exclusive mutation of one view.
- the trait does not contain `present`, `acquire`, `submit`, `wait_idle`, or required `readback`.

`RenderBackend` is for static dispatch. Object safety is not a Gate 1 requirement.

### 3.3 Common requests and results

`request.rs` owns:

```rust,ignore
pub struct RenderRequest {
    pub force: bool,
    pub load: TargetLoad,
    pub store: TargetStore,
    pub region: Option<RectI>,
}

pub enum TargetLoad {
    Preserve,
    Clear(ColorRgba8),
}

pub enum TargetStore {
    Store,
    Discard,
}
```

`TargetLoad::Preserve` is required for embedded subregions and host-composited targets. A backend
must return an unsupported/error result rather than silently clearing when preservation cannot be
honored.

`target.rs` owns backend-neutral metadata used for validation and diagnostics:

```rust,ignore
pub struct RenderTargetInfo {
    pub extent: SizeI,
    pub region: RectI,
    pub sample_count: u8,
    pub color_space: ColorSpace,
    pub alpha_mode: AlphaMode,
}

pub enum ColorSpace {
    Linear,
    Srgb,
    Extended,
    BackendDefined,
}

pub enum AlphaMode {
    Opaque,
    Premultiplied,
}
```

Backend-specific target formats and native handles remain in the backend target type. Exact target
encoding rules and the shader ABI are fixed in
[Scene-to-GPU ABI and shader contract](SCENE_GPU_ABI_AND_SHADERS.md#7-shader-descriptor-and-stage-abi).

`stats.rs` separates work that was recorded from work a presenter/host later submitted:

```rust,ignore
pub struct SceneUpdateStats {
    pub epoch: u64,
    pub upload_bytes_queued: u64,
    pub descriptor_writes_queued: u32,
}

pub struct RenderStats {
    pub recorded: bool,
    pub epoch: u64,
    pub upload_bytes_recorded: u64,
    pub passes: u32,
    pub barriers: u32,
    pub batches: u32,
    pub draws: u32,
    pub dispatches: u32,
    pub damage_area: f32,
}
```

No field is populated from a predicted model. If a metric is unavailable, diagnostics report it as
unavailable instead of estimating it.

### 3.4 Errors

`error.rs` replaces the string-only renderer error with a stable category and retained context:

```rust,ignore
pub enum RenderErrorKind {
    Unsupported,
    InvalidTarget,
    InvalidScene,
    OutOfMemory,
    DeviceLost,
    HostContract,
    Internal,
}

pub struct RenderError {
    kind: RenderErrorKind,
    context: String,
    backend_code: Option<i64>,
}
```

Backend packages map native errors into these categories and may expose typed native detail through
their own error type. Surface loss, out-of-date, suboptimal, acquire not-ready, and presentation
failures belong to typed presenter results/errors. Recovery policy belongs to the owner/presenter,
not `RenderError` itself.

### 3.5 Optional readback

`readback.rs` owns CPU pixel output without requiring it from every renderer:

```rust,ignore
pub struct ReadbackRequest {
    pub region: RectI,
    pub format: ReadbackFormat,
}

pub struct ReadbackImage {
    pub extent: SizeI,
    pub row_bytes: usize,
    pub pixels: Vec<u8>,
}

pub trait RenderReadback<B: RenderBackend> {
    type Pending;

    fn record_readback<'frame>(
        &self,
        backend: &B,
        frame: &mut B::FrameContext<'frame>,
        target: &B::Target<'frame>,
        request: &ReadbackRequest,
    ) -> RenderResult<Self::Pending>;
}
```

The Vulkan implementation records the copy in the caller's graphics frame and binds the pending
readback to that frame's owned or hosted completion point. It has no separate hidden transfer
submission. Mapping and an optional explicit bounded wait remain outside `RenderBackend`; see the
[Gate 3 ownership contract](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md#13-readback).

## 4. `telorgon-app` file and API blueprint

### 4.1 Required files

```text
crates/telorgon/src/application_host/
├── lib.rs                  Curated public exports only
├── application.rs          Application trait and WindowConfig
├── context.rs              AppContext and commands exposed during events
├── error.rs                AppError/AppResult
├── event.rs                AppEvent and public input-facing values
├── input.rs                InputCoalescer and routing helpers
├── scheduler.rs            FrameScheduler
├── delta_queue.rs          SceneDeltaQueue and coalescing
├── runtime.rs              Renderer-free AppRuntime
├── headless.rs             Explicit software reference/test convenience
└── native/
    ├── mod.rs              Managed entry-point selection
    ├── winit_host.rs       Winit lifecycle and event conversion
    ├── software.rs         Temporary Softbuffer assembly
    └── vulkan.rs           Vulkan presenter assembly after Slice 3
```

The mechanical extraction from the current `lib.rs` must preserve public re-exports during this
gate. It must not mix Gate 7's component/input/command redesign into the renderer-removal patch;
after Slice 1, those APIs migrate through Gate 7's ordered slices rather than remaining undefined.

### 4.2 Renderer-free runtime

`runtime.rs` changes the central type to:

```rust,ignore
pub struct AppRuntime<App: Application> {
    app: App,
    ui: MountedUi<App::Action>,
    layout: LayoutEngine,
    text: RetainedTextSystem,
    scene: RenderScene,
    compiler: SceneCompiler,
    scheduler: FrameScheduler,
    deltas: SceneDeltaQueue,
    commands: VecDeque<Command>,
    input: InputCoalescer,
    extent: SizeF,
    // Existing pointer/focus/event-routing state remains here.
}
```

Its frame-facing API is:

```rust,ignore
impl<App: Application> AppRuntime<App> {
    pub fn new(app: App) -> AppResult<Self>;
    pub fn needs_frame(&self) -> bool;
    pub fn prepare_frame(&mut self, force: bool) -> AppResult<PreparedFrame>;
    pub fn pop_scene_delta(&mut self) -> Option<RenderSceneDelta>;
    pub fn scene_snapshot(&mut self) -> RenderSceneDelta;
}

pub struct PreparedFrame {
    pub changed: bool,
    pub scene_epoch: u64,
    pub diagnostics: FrameDiagnostics,
}
```

`prepare_frame` performs scheduling, layout, text/scene compilation, and delta creation. It does not
call a renderer. It returns `changed = false` without compiling when an unforced frame is idle.

`pop_scene_delta` transfers queued deltas to whichever backend scene the host selected.
`scene_snapshot` exists for backend/device recovery and initializing another backend scene. It does
not replace ordinary delta updates.

Remove these current runtime members and methods:

- generic parameter `R: Renderer`;
- `renderer` and `target` fields;
- `renderer()` and `renderer_mut()`;
- `frame()` as a combined prepare/render/present operation;
- `readback()`; and
- `recover_renderer()`.

Recovery becomes host orchestration: create a replacement backend scene, request a snapshot from the
runtime, apply it, then atomically replace the host's old scene after in-flight work is safe.

### 4.3 Managed software transition

`native/software.rs` temporarily owns both `SoftwareRenderer` and the Softbuffer surface. Its redraw
flow becomes:

1. flush Winit input into `AppRuntime`;
2. call `prepare_frame(false)`;
3. drain scene deltas into the software backend scene;
4. call the software backend render operation;
5. copy only reported damage into Softbuffer; and
6. present through Softbuffer.

This preserves existing tools while proving the runtime/renderer separation. It does not add new
software rendering features.

At Slice 3 exit, `run_native` selects the Vulkan assembly for the Vulkan application profile.
Software remains available through an explicit headless/software profile rather than being an
internal fallback after Vulkan initialization succeeds.

## 5. `telorgon-renderer-vulkan` file and API blueprint

### 5.1 Required files

```text
crates/telorgon/src/renderer_vulkan/
├── lib.rs             Curated exports and sealed implementation wiring
├── config.rs          VulkanConfig, validation, feature policy, and budgets
├── error.rs           VkResult/allocation mapping into RenderError
├── entry.rs           Loader, VulkanInstance, layers, and instance extensions
├── adapter.rs         AdapterReport, rejection reasons, and device selection
├── device.rs          VulkanDevice, queues, capabilities, and shared caches
├── scene.rs           VulkanScene: per-view retained GPU resources
├── target.rs          VulkanTarget and native target validation
├── memory.rs          gpu-allocator owner, allocations, and memory reporting
├── buffer.rs          Allocated buffers and typed GPU transfer wrappers
├── image.rs           Allocated images, views, samplers, and usage state
├── descriptor.rs      Layouts, pools, sets, and reusable bindings
├── shader.rs          Embedded bundle loading and module/schema checks
├── pipeline.rs        Layouts, pipelines, and pipeline-cache ownership
├── upload.rs          Mapped staging ring and dirty-range/image copies
├── sync.rs            Semantic transitions, timelines, fences, and completion
├── frame.rs           Owned and hosted Vulkan frame-context representations
├── executor.rs        RenderBackend implementation and command recording
├── readback.rs        Explicit Vulkan image-to-staging readback capability
├── interop.rs         Backend-specific hosted/native extension contracts
└── diagnostics.rs     Debug utils, names, labels, counters, and callbacks
```

The current `src/lib.rs` model is replaced. Do not copy it into `model.rs` and leave two competing
Vulkan implementations. Backend-neutral planner logic worth retaining must move to `telorgon-render`
under Gate 2 review.

### 5.2 Public backend types

The Vulkan package initially exports these primary object types:

```rust,ignore
pub struct VulkanConfig { /* policy and budgets, no handles */ }
pub struct VulkanInstance { /* cloneable instance-level owner */ }
pub struct VulkanDevice { /* shared device-level owner */ }
pub struct VulkanScene { /* one UI view's resources */ }
pub struct VulkanFrameContext<'frame> { /* sealed validated recording adapter */ }
pub struct VulkanRecordingFrame<'device> { /* exclusive owned frame slot; !Send */ }
pub struct VulkanRecordedFrame { /* finished owned frame; submit exactly once */ }
pub struct VulkanHostedFrame<'host> { /* borrowed command-only recording interval; !Send */ }
pub struct HostedFrameReceipt { /* resource pins requiring host commit/discard */ }
pub struct VulkanTarget<'frame> { /* non-owning validated backend-native target */ }
pub struct CompletionPoint { /* opaque device/domain completion proof */ }
pub struct SubmissionReceipt { /* completion and actual submission facts */ }
pub struct VulkanCapabilities { /* queried support report */ }
pub struct VulkanDiagnostics { /* actual counters and messages */ }
```

Focused modules also export the Gate 3 request, outcome, policy, capability, and error values needed
to use those objects. They do not expose native handle ownership.

Raw `ash::vk` handles are not re-exported from `lib.rs`. Narrow `unsafe` constructors/accessors used
for hosted/native interop live under `telorgon_renderer_vulkan::interop` and document ownership,
threading, validity, and synchronization requirements.

### 5.3 Device API

The device-level API shape is:

```rust,ignore
impl VulkanInstance {
    pub fn load(
        config: &VulkanConfig,
        extensions: &[InstanceExtensionRequest<'_>],
    ) -> RenderResult<Self>;
    pub fn adapters(&self) -> RenderResult<Vec<AdapterReport>>;
}

impl VulkanDevice {
    pub fn create_owned(
        instance: VulkanInstance,
        config: &VulkanConfig,
        selection: &DeviceSelection,
        presentation: Option<PresentationRequirement<'_>>,
    ) -> RenderResult<Self>;

    pub fn capabilities(&self) -> &VulkanCapabilities;
    pub fn diagnostics(&self) -> &VulkanDiagnostics;
}
```

Slice 3 adds surface-aware construction without making the renderer depend on Winit. The presenter
supplies instance-extension requests and a narrow unsafe `BorrowedVulkanSurface`/
`PresentationRequirement` for adapter and queue selection, as fixed by Gate 3.

`VulkanDevice` implements `RenderBackend` with:

```rust,ignore
type Scene = VulkanScene;
type FrameContext<'frame> = VulkanFrameContext<'frame>;
type Target<'frame> = VulkanTarget<'frame>;
```

`VulkanFrameContext` has no public constructor. Owned and hosted recording frames expose it only for
their valid interval through a focused `context_mut()` method. It preserves the public owned/hosted
state machines instead of exposing an untyped command buffer.

### 5.4 Slice 2 offscreen assembly

The first hardware test constructs:

```text
VulkanInstance
    -> DeviceSelection
    -> VulkanDevice
    -> VulkanScene
    -> VulkanRecordingFrame from an owned offscreen frame slot
    -> allocated VulkanTarget image/view
    -> apply one real RenderSceneDelta
    -> render one Telorgon box
    -> record explicit Vulkan readback in the same frame
    -> finish and submit through the explicit test owner
    -> wait on its CompletionPoint and map for pixel assertion
```

Submission in this test is performed by an owned offscreen frame helper in the Vulkan package. It is
not hidden inside `RenderBackend::render`, preserving hosted command-only semantics.

## 6. `telorgon-presenter-vulkan-wsi` blueprint

Gate 5 applies this blueprint first to Windows x86-64, then reuses it for the separately qualified
Linux Wayland and X11 profiles. Hosted Vulkan does not use this package.

### 6.1 Package and files

```text
crates/telorgon-presenter-vulkan-wsi/
├── Cargo.toml
└── src/
    ├── lib.rs          Curated presenter exports
    ├── error.rs        Surface/acquire/present error mapping
    ├── surface.rs      raw-window-handle to VkSurfaceKHR construction
    ├── swapchain.rs    Capabilities, selection, images/views, and retirement
    ├── frame.rs        Acquired image plus frame/target pairing
    ├── presenter.rs    Acquire, submit, and present state machine
    └── recovery.rs     Resize, suspend, out-of-date, and surface-loss handling
```

The presenter package contains no UI, layout, scene compiler, or widget logic.

### 6.2 Presenter API shape

```rust,ignore
pub struct VulkanWinitPresenter { /* surface and swapchain state */ }

pub enum AcquireOutcome<'a> {
    Ready(AcquiredVulkanFrame<'a>),
    Suspended,
    NotReady,
    NeedsReconfigure,
}

pub struct AcquiredVulkanFrame<'a> {
    // Private mutable presenter borrow, generation, image index, and acquire synchronization.
}

impl VulkanWinitPresenter {
    pub fn acquire(
        &mut self,
        device: &VulkanDevice,
        frame: &VulkanRecordingFrame<'_>,
    )
        -> Result<AcquireOutcome<'_>, PresentError>;

    pub fn resize(&mut self, extent: SizeI);
    pub fn suspend(&mut self) -> Result<(), PresentError>;
    pub fn resume(&mut self, extent: SizeI) -> Result<(), PresentError>;
}

impl AcquiredVulkanFrame<'_> {
    pub fn target(&self) -> VulkanTarget<'_>;
    pub fn submit_and_present(
        self,
        device: &VulkanDevice,
        frame: VulkanRecordedFrame,
    ) -> Result<PresentOutcome, PresentError>;
    pub fn discard(self, device: &VulkanDevice) -> Result<(), PresentError>;
}
```

The acquired token borrows the presenter mutably and does not own a frame slot; it records the
reserved slot identity for submit validation. Consuming it performs present or explicit discard, so
these invalid sequences are unrepresentable or rejected:

- acquiring a second image while one is outstanding;
- presenting an image from another presenter/device;
- reconfiguring while an acquired image is live;
- reusing frame resources before completion; and
- destroying swapchain views still referenced by in-flight work.

### 6.3 Managed Vulkan redraw call graph

```text
Winit RedrawRequested
  -> AppRuntime::flush_input(timestamp)
  -> AppRuntime::prepare_frame(false)
  -> drain AppRuntime::pop_scene_delta()
       -> VulkanDevice::apply_scene_delta(&mut VulkanScene, delta)
  -> VulkanDevice::begin_owned_frame(wait policy)
  -> VulkanWinitPresenter::acquire(&VulkanDevice, &recording_frame)
  -> let target = acquired.target()
  -> VulkanDevice::render(
         &mut VulkanScene,
         recording_frame.context_mut(),
         &target,
         RenderRequest
     )
  -> recording_frame.finish()
  -> acquired.submit_and_present(&device, recorded_frame)
  -> VulkanDevice::maintain()
  -> drain application commands and request the next frame only if needed
```

If `prepare_frame` reports no change and no forced/external presentation is required, the host does
not acquire a swapchain image.

## 7. GPU ABI and shader-build package blueprint

```text
crates/telorgon/src/gpu_abi/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── color.rs
    ├── view.rs
    ├── spatial.rs
    ├── clip.rs
    ├── box_instance.rs
    ├── shadow_instance.rs
    ├── glyph_instance.rs
    ├── image_instance.rs
    ├── material_instance.rs
    └── layout.rs

crates/telorgon-shader-build/
├── Cargo.toml
├── src/
    ├── main.rs         Argument parsing and build orchestration
    ├── manifest.rs     Shader bundle manifest and source hashes
    ├── compile.rs      shaderc invocation/configuration
    ├── validate.rs     SPIR-V validation and declared-interface checks
    ├── reflect.rs      Focused SPIR-V metadata inspection
    └── generate_rust.rs
├── shaders/vulkan/     Authoritative GLSL organized by pipeline family
└── bundle.toml         Declared entries, interfaces, targets, and variants
```

`telorgon-gpu-abi` contains only the exact POD transfer records and layout constants consumed by scene
conversion, backends, shader validation, and tests. The complete record fields, offsets, descriptor
sets, color contract, upload contract, variants, manifest contents, and source-file responsibilities
are fixed in [Scene-to-GPU ABI and shader contract](SCENE_GPU_ABI_AND_SHADERS.md).

The renderer includes generated artifacts and generated Rust metadata; its ordinary `build.rs` does
not search the host system for shader compilers. `telorgon-shader-build` is the only shader compiler
owner and is not a runtime dependency.

## 8. Umbrella exports and feature behavior

During Slices 1–2:

- preserve existing top-level application and software exports where possible;
- stop re-exporting the modeled Vulkan types as if they were operational;
- expose real Vulkan types only under `telorgon::renderer_vulkan`; and
- keep `run_native` on the temporary software assembly until the Vulkan presenter passes Slice 3.

At Slice 3 exit:

- the documented Vulkan application profile makes `run_native` use the Vulkan presenter;
- `run_native_software` is an explicit reference/fallback entry point behind a software feature;
- headless tests remain explicitly software unless they request a Vulkan hardware fixture; and
- renderer/presenter dependencies are selected by features rather than all enabled unconditionally.

Exact public compatibility aliases and deprecations are fixed by Gate 2.

## 9. Slice file-change boundaries

### Slice 1: Runtime/backend separation

Allowed primary changes:

- split `telorgon-render` interface files;
- split `telorgon-app` source files and make `AppRuntime` renderer-free;
- adapt software/headless paths and tests to the new separation;
- remove `SoftwareRenderer` delegation and false operational claims from the Vulkan crate; and
- update umbrella exports and status documentation.

Do not add a real Vulkan device in the same change. Slice 1 exits with existing CPU tests passing and
the architectural boundary compile-checked.

### Slice 2: Real offscreen Vulkan

Allowed primary changes:

- add the selected Vulkan dependencies;
- create the Vulkan modules through `executor.rs`, `target.rs`, and explicit `readback.rs` needed by
  one box;
- generate/package the initial shader bundle; and
- add hardware-gated offscreen tests.

Do not add Winit or `ash-window` to `telorgon-renderer-vulkan`.

### Slice 3: Owned presentation

Allowed primary changes:

- add `telorgon-presenter-vulkan-wsi`;
- add managed Winit/Vulkan assembly in `telorgon-app`;
- implement swapchain recovery states;
- switch the Vulkan managed profile after acceptance tests pass; and
- retain the software host as an explicit separate profile.

Do not begin external-image, shell, multi-view hosted, material-effect, or broad UI feature work in
these slices.

## 10. Gate 1 reference audit

```text
Concern:
Separate UI preparation, backend scene state, rendering, submission, and presentation.

Telorgon files/contracts affected:
telorgon-app AppRuntime; telorgon-render Renderer; telorgon-renderer-vulkan; new Vulkan/Winit presenter.

Reference paths and symbols inspected:
../other-rendering-libs/wgpu/wgpu-hal/src/lib.rs — Api, Surface, Device, Queue,
CommandEncoder, acquire/present safety contracts.
../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/context_vk.h
and context_vk.cc — owned/embedder context data and command-buffer creation.
../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/
command_queue_vk.cc and command_pool_vk_unittests.cc — submission and completion-safe recycling.
../other-rendering-libs/slint/internal/core/ and internal/renderers/ — platform renderer selection and
window-renderer separation.

Invariants extracted:
Surface acquire/configure/present has a state machine separate from device command encoding.
Command buffers and their allocation pools must remain alive until execution completes.
Submission/presentation externally synchronize queue access.
Embedding supplies different device/queue ownership from an owned application.
A UI runtime can select different renderers without putting platform surfaces in the UI tree.

Approaches rejected:
Keeping AppRuntime generic over and owning a renderer; putting swapchain methods on RenderBackend;
requiring readback from every backend; representing every native target as a u64; using wgpu itself
as Telorgon's reference Vulkan implementation.

Telorgon-specific decision:
Renderer-free AppRuntime, one backend scene per view, shared backend device, associated native
frame/target types, concrete owned presenter, and separate optional readback.

Tests/diagnostics derived:
Idle preparation performs no acquire/render; two backend scenes share one device; outstanding acquire
is rejected; command resources cannot be reset before completion; hosted rendering can record without
submission; Vulkan readback performs a real transfer.

Known gaps:
The Vulkan GPU ABI is fixed by `SCENE_GPU_ABI_AND_SHADERS.md`, and platform order is fixed by
`PLATFORM_IMPLEMENTATION_ORDER.md`. The conformance/qualification matrix is fixed by
`ACCEPTANCE_AND_QUALIFICATION.md`. Exact ownership and synchronization are fixed by
`GPU_OWNERSHIP_AND_SYNCHRONIZATION.md`.
```

## 11. Gate completion criteria

Gate 1 is complete when:

- the file lists and dependency direction above are included in the canonical docs index;
- every Slice 1–3 responsibility has exactly one package owner;
- the UI runtime, backend, presenter, and readback call directions are explicit;
- Codex is instructed not to invent a competing RHI, WSI placement, or renderer-owned runtime; and
- remaining ambiguity is assigned to a named later planning gate rather than left implicit.
