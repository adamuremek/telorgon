# Presentation-Pipeline Restructuring Plan

**Document role:** active implementation direction.

**Status:** implemented restructuring of functionality that already existed. The automated
compilation, feature-matrix, unit, formatting, and dependency-boundary gates passed on 2026-08-29.
The post-extraction Windows compositor-visible hardware matrix in section 9.2 remains a user-run
qualification gate; it is not implied by compilation.

## Implementation result

The migration landed with the following concrete boundaries:

- `telorgon-presentation` owns neutral surface metrics, lifecycle/recovery, dispositions, linear frame
  and session contracts, capabilities, and completion-stage vocabulary;
- `telorgon-presenter-vulkan-wsi` owns the extracted Vulkan WSI implementation and hardware tests;
- `telorgon-presenter-dxgi` owns the Windows D3D11 device/context, HWND swapchain, resize, copy,
  native present, and D3D completion wait without a Vulkan dependency;
- `telorgon-bridge-vulkan-dxgi` owns shared textures, Vulkan imports, keyed mutexes, imported timeline
  synchronization, adapter validation, and dual-API retirement;
- `telorgon-presenter-softbuffer` owns native software transfer, damage, present, and lifecycle;
- the obsolete `telorgon-presenter-vulkan-winit` re-export facade has been removed; and
- `telorgon-app` prepares one revisioned runtime packet and selects explicit, statically typed
  software, Vulkan WSI, or Vulkan/DXGI assemblies without owning native presenter mechanics.

Hosted Vulkan and headless software remain unchanged and do not use fabricated presenter objects.

## 1. Objective

Separate Telorgon's rendering backends from its native presentation pipelines without changing the
behavior of any currently working path.

The restructuring establishes four explicit responsibilities:

1. The application runtime produces scene and surface state.
2. A renderer records and submits graphics work into a renderer-compatible target.
3. A bridge, when one is required, transports an image and synchronization between graphics APIs.
4. A platform presenter owns the native surface, swapchain, resize policy, and final handoff to the
   operating-system compositor.

This is an extraction and ownership-correction project. It is not an authorization to add every
possible renderer or presenter backend.

## 2. Scope

Only already implemented execution paths are in scope:

- Vulkan rendering;
- software rendering;
- Vulkan WSI presentation through `winit`;
- Windows Vulkan-to-D3D11/DXGI presentation;
- software window presentation through `softbuffer`;
- the existing hosted Vulkan path; and
- the existing headless software path.

The restructuring must preserve the behavior of the Windows DXGI resize solution, including its
direction-aware ordering:

- expansion prepares the larger presentation allocation before the native size commit; and
- contraction commits the native size before applying the smaller presentation allocation.

### 2.1 Non-goals

This plan does not add or pre-build:

- a Metal renderer or presenter;
- a D3D11 or D3D12 renderer;
- a DirectComposition presenter;
- Wayland-, Android-, or iOS-specific presenters;
- MoltenVK-specific presentation policy;
- empty crates reserved for hypothetical backends;
- arbitrary renderer/presenter pairings;
- a universal untyped native-handle or frame enum;
- silent CPU readback as an interoperability fallback;
- a scene, shader, layout, or public application API redesign; or
- a new presenter abstraction for hosted or headless execution where no native presentation step
  exists.

## 3. Architectural boundary

The intended dependency and data flow is:

```text
application host/runtime
        |
        | RenderRequest + SurfaceMetrics revision
        v
managed pipeline assembly
        |
        +--> platform presenter acquires a presentable frame
        |         |
        |         +--> optional cross-API bridge exposes a renderer target
        |         |
        |         +--> renderer records/submits into that target
        |         |
        |         +--> bridge transports completion when required
        |         |
        |         '--> presenter submits the native present operation
        |
        '--> host observes progress, recovery, and completion
```

The optional bridge exists only when renderer and presenter APIs differ. Vulkan WSI and software
presentation do not need a separate cross-API bridge. The current Windows path does: Vulkan renders
through imported D3D11 resources and DXGI performs the native presentation.

### 3.1 Responsibility rules

| Layer | Owns | Must not own |
|---|---|---|
| Application host/runtime | Scene preparation, input, layout, logical metrics, event-loop policy, and pipeline selection | API-specific swapchains, image interop, keyed mutexes, or pixel copies |
| Renderer | Device-local rendering resources, command recording, submission, and render completion | Native window resize policy or OS presentation |
| Bridge | Cross-API compatibility, shared images, imported memory, cross-API synchronization, and transport completion | Scene/runtime state or native event-loop policy |
| Presenter | Native surface/swapchain, drawable allocation, compositor-facing present, resize/reconfigure, and presentation recovery | Scene interpretation or renderer command generation |
| Managed assembly | A known-compatible renderer, bridge, and presenter combination | Open-ended runtime matching of arbitrary backends |

Dependency direction is enforced at the crate level:

- render crates do not depend on platform presenter crates;
- presenter crates do not depend on application runtime or scene implementation crates;
- cross-API dependencies live only in bridge crates; and
- `telorgon-app` selects complete assemblies rather than implementing their graphics details.

## 4. Target crate organization

The target adds crates only for boundaries already represented by working code.

### 4.1 Neutral contracts

#### `telorgon-render` — keep

Own the renderer-neutral request, scene, target, statistics, and renderer error contracts. It must
remain independent of window systems and presentation APIs.

#### `telorgon-presentation` — add

Own the neutral presentation vocabulary used by the application host and concrete managed
assemblies:

- `SurfaceMetrics` and its monotonic revision;
- presentation lifecycle state;
- acquire, reconfigure, suspend, resume, and shutdown outcomes;
- render, transport, present, and display completion stages;
- presentation capability descriptions;
- the linear acquired-frame contract; and
- the generic session contract used by concrete assemblies.

Allowed dependencies are `telorgon-core` and `telorgon-render`. This crate must not depend on Vulkan,
DXGI, D3D11, `softbuffer`, `winit`, or application-runtime types.

### 4.2 Renderers

#### `telorgon-renderer-vulkan` — keep

Continue to own Vulkan instance/device selection, resource allocation, render targets, pipelines,
command recording, submission, and render-completion primitives. External-image import utilities
that are generically useful to Vulkan may remain here; policy and ownership for a particular DXGI
presentation path move to its bridge.

#### `telorgon-renderer-software` — keep

Continue to own deterministic CPU rasterization and software render targets. Native window
surfaces and `softbuffer` presentation move out.

### 4.3 Presenters and bridge

#### `telorgon-presenter-vulkan-wsi` — add by extraction

Extract the existing Vulkan WSI path from `telorgon-presenter-vulkan-winit`. It owns:

- Vulkan surface creation from a native window handle;
- swapchain selection, creation, reconfiguration, and retirement;
- image acquisition and queue presentation;
- present modes and WSI synchronization;
- out-of-date, suboptimal, suspended, and surface-loss recovery; and
- present-wait or maintenance behavior already implemented by the current path.

It may depend on `telorgon-renderer-vulkan` because it produces Vulkan-native render targets. It must
not contain DXGI or D3D11 code.

#### `telorgon-presenter-dxgi` — add by extraction

Extract the API-neutral-to-Vulkan parts of the current Windows DXGI presenter into a Windows-only
crate. It owns:

- DXGI factory and adapter selection;
- the D3D11 presentation device and immediate context;
- the HWND swapchain and its buffers;
- `ResizeBuffers`, copy, and `Present1`;
- native present parameters and compositor-facing error recovery; and
- the already implemented expansion/contraction resize ordering at the native-presentation
  boundary.

This crate exposes D3D11/DXGI-compatible image requirements to a bridge, but contains no Vulkan
types or Vulkan synchronization calls.

#### `telorgon-bridge-vulkan-dxgi` — add by extraction

Extract the current Windows cross-API transport into its own Windows-only crate. It owns:

- creation and lifetime of D3D11 shareable textures;
- Vulkan memory/image aliases imported from those textures;
- adapter LUID compatibility validation;
- keyed-mutex sequencing;
- imported fence or timeline synchronization;
- the handoff from Vulkan render completion to the DXGI presenter;
- per-generation resource retirement; and
- rejection of device, generation, or extent mismatches.

It depends on `telorgon-presentation`, `telorgon-renderer-vulkan`, and
`telorgon-presenter-dxgi`. Neither the Vulkan renderer nor the DXGI presenter depends on this bridge.

#### `telorgon-presenter-softbuffer` — add by extraction

Extract the existing native software presentation code from `telorgon-app`. It owns:

- `softbuffer` context and surface lifetime;
- CPU framebuffer transfer;
- damage and buffer-age handling already supported by the current implementation;
- native present, resize, suspend, and resume; and
- conversion required specifically by the `softbuffer` surface format.

It may directly implement the software presentation session because the renderer and presenter
share a CPU-memory boundary; a separate bridge crate would add no useful ownership boundary.

### 4.4 Orchestration and non-presenting hosts

#### `telorgon-app` — narrow

Retain application declaration, runtime preparation, event-loop hosting, input, scheduling,
profiling integration, surface-metrics collection, resize transaction policy, worker ownership,
and selection of a supported managed assembly.

Remove from `telorgon-app`:

- pixel-format conversion for a specific presenter;
- DXGI/D3D11 object ownership;
- Vulkan swapchain or acquired-image details;
- shared-texture and keyed-mutex sequencing; and
- direct rendering into presentation-specific frame types.

The Windows `winit` host continues to decide when native size transactions occur. The concrete
DXGI assembly continues to decide when its allocations are prepared or committed. The neutral
presentation contract carries the revision and state needed to coordinate them.

#### `telorgon-embed` and `telorgon-app::headless` — keep

Hosted Vulkan and headless software rendering terminate at renderer-owned outputs rather than an
OS compositor. They remain renderer/host integrations and do not gain a fabricated presenter.

### 4.5 Transitional crate

#### Retired compatibility facade

The temporary `telorgon-presenter-vulkan-winit` facade was removed after all internal imports moved
to `telorgon-presenter-vulkan-wsi` and `telorgon-bridge-vulkan-dxgi`.

## 5. Supported managed assemblies

Application code selects only combinations Telorgon implements and tests:

| Assembly | Renderer | Bridge | Presenter | Current purpose |
|---|---|---|---|---|
| `VulkanWsi` | `telorgon-renderer-vulkan` | None | `telorgon-presenter-vulkan-wsi` | Native Vulkan WSI windows |
| `VulkanDxgi` | `telorgon-renderer-vulkan` | `telorgon-bridge-vulkan-dxgi` | `telorgon-presenter-dxgi` | Windows compositor-synchronized presentation |
| `SoftwareSoftbuffer` | `telorgon-renderer-software` | None | `telorgon-presenter-softbuffer` | Native software fallback/reference path |

A coarse enum in `telorgon-app` may select among these complete assemblies. Inside each variant,
frames and synchronization remain statically typed. This avoids an untyped matrix of renderer and
presenter combinations while leaving a clear place to add a future, fully implemented assembly.

## 6. Core contracts

The following shapes communicate ownership and invariants. Exact Rust names and signatures may be
adjusted during Phase 1, but weakening their guarantees requires an architecture review.

### 6.1 Revisioned surface metrics

```rust
pub struct SurfaceMetrics {
    pub revision: SurfaceRevision,
    pub logical_extent: LogicalExtent,
    pub physical_extent: PhysicalExtent,
    pub scale_factor: f64,
    pub color_space: ColorSpace,
    pub alpha_mode: AlphaMode,
}
```

Every acquired frame records the metrics revision and resource generation from which it was
created. A stale frame cannot be presented into a newer, incompatible generation.

### 6.2 Explicit lifecycle state

```rust
pub enum PresentationState {
    Unconfigured,
    Ready,
    NeedsReconfigure,
    Suspended,
    SurfaceLost,
    DeviceLost,
    Shutdown,
}
```

Zero-size windows produce `Suspended`, not a fake one-pixel drawable. Recoverable surface changes
produce `NeedsReconfigure` or `SurfaceLost`; device loss remains distinct.

### 6.3 Linear presentable frames

```rust
pub trait PresentableFrame<R> {
    type Target;
    type Receipt;

    fn target(&mut self) -> &mut Self::Target;
    fn submit_and_present(self, renderer: &mut R) -> Result<Self::Receipt, PresentationError>;
    fn discard(self);
}
```

An acquired frame token is not cloneable and is consumed exactly once by presentation or discard.
It retains every resource needed through its declared completion stage and is tied to one device,
surface generation, and metrics revision.

### 6.4 Presentation sessions

```rust
pub trait PresentationSession<R> {
    type Frame<'a>: PresentableFrame<R>
    where
        Self: 'a;

    fn configure(&mut self, metrics: SurfaceMetrics) -> Result<(), PresentationError>;
    fn acquire(&mut self) -> Result<AcquireDisposition<Self::Frame<'_>>, PresentationError>;
    fn poll(&mut self) -> Result<PresentationProgress, PresentationError>;
    fn suspend(&mut self) -> Result<(), PresentationError>;
    fn shutdown(&mut self) -> Result<(), PresentationError>;
}
```

The contract represents sequencing, not a promise that every backend performs the same operations.
For example, the Vulkan/DXGI assembly can acquire a shared bridge image while the WSI assembly
acquires a swapchain image.

### 6.5 Completion stages

Completion must not collapse into a single ambiguous "frame complete" signal:

- **Render completion:** the renderer is finished writing the target.
- **Transport completion:** a cross-API bridge has safely transferred ownership or visibility.
- **Present completion:** the native presentation API accepted the present operation and resources
  may be retired according to that API's contract.
- **Display completion:** the compositor or display reached the requested visible milestone when
  the platform can report one.

Backends may report that a later stage is unsupported, but must not relabel an earlier stage as
display completion.

## 7. Resource-ownership ledger

Before code moves, every graphics object must have one owner and one retirement proof.

| Resource | Target owner | Retirement evidence |
|---|---|---|
| Vulkan render device, queues, pipelines | `telorgon-renderer-vulkan` | Renderer device idle/lifetime rules already defined by renderer |
| Vulkan WSI surface and swapchain | `telorgon-presenter-vulkan-wsi` | WSI generation completion and present retirement |
| DXGI factory, D3D11 presentation device/context | `telorgon-presenter-dxgi` | Presenter shutdown/device-loss path |
| HWND swapchain and back buffers | `telorgon-presenter-dxgi` | Present completion plus swapchain-generation retirement |
| D3D11 shared textures | `telorgon-bridge-vulkan-dxgi` | Both Vulkan and D3D11 access for the generation are complete |
| Imported Vulkan image, memory, and image view | `telorgon-bridge-vulkan-dxgi` | Cross-API transport completion and generation retirement |
| Keyed mutex state and imported fence/timeline | `telorgon-bridge-vulkan-dxgi` | No outstanding frame token references the synchronization generation |
| Temporary DXGI copy source/destination references | Frame token or `telorgon-presenter-dxgi` | Native present operation has consumed the frame |
| Software raster buffer | `telorgon-renderer-software` or linear frame token | CPU render completion and surface copy completion |
| `softbuffer` context/surface | `telorgon-presenter-softbuffer` | Presenter suspend/shutdown rules |

Moving a resource without first recording its new owner and retirement proof is not permitted.

## 8. Migration plan

Each phase must compile and test independently. Temporary compatibility re-exports are preferred
over a flag-day migration.

### Phase 0 — Freeze behavioral evidence

Goal: preserve a trustworthy baseline before moving ownership.

Work:

- record the existing feature combinations and test commands;
- identify profiler event names that must remain stable;
- retain current software, hosted, headless, Vulkan WSI, and Vulkan/DXGI tests;
- document the manually verified Windows inward, outward, and mixed-direction resize behavior; and
- add narrowly scoped regression tests where an invariant can be verified without opening a native
  application.

Exit gate: baseline evidence is recorded and no runtime behavior changes.

### Phase 1 — Introduce `telorgon-presentation`

Goal: create the neutral vocabulary without moving backend implementations.

Work:

- add the crate and its lifecycle, metrics, disposition, receipt, completion, and error types;
- add linear frame and presentation-session contracts;
- convert or mirror existing neutral types behind compatibility re-exports; and
- add compile-time and state-machine tests for frame consumption, revision mismatch, suspend,
  reconfigure, and shutdown.

Exit gate: existing application paths use or adapt to the neutral types with unchanged output.

### Phase 2 — Adapt implementations in place

Goal: validate the contracts before changing crate ownership.

Work:

- adapt the existing Vulkan WSI presenter to a Vulkan session;
- adapt the existing DXGI managed path to a Vulkan/DXGI session;
- adapt the existing software presentation path to a software session; and
- make the application select these managed sessions while implementations remain in their current
  files.

Exit gate: all three managed assemblies exercise the same orchestration boundary; no source files
have moved solely for naming.

### Phase 3 — Extract Vulkan WSI presentation

Goal: isolate same-API native presentation.

Work:

- create `telorgon-presenter-vulkan-wsi`;
- move surface, swapchain, frame acquisition, presentation, and WSI recovery code;
- leave temporary re-exports in `telorgon-presenter-vulkan-winit`; and
- verify no DXGI symbols or application runtime types enter the new crate.

Exit gate: the `VulkanWsi` assembly passes existing tests and the compatibility import path still
builds for the agreed transition window.

### Phase 4 — Split DXGI presentation from Vulkan/DXGI transport

Goal: make the Windows platform presenter independent of the rendering API.

Perform this phase in two reviewable commits:

1. Extract DXGI factory, D3D11 device/context, HWND swapchain, buffer resize, copy, and native
   present into `telorgon-presenter-dxgi` while keeping an adapter in the old location.
2. Extract shared-texture creation, Vulkan import, keyed-mutex/fence sequencing, LUID validation,
   frame handoff, and retirement into `telorgon-bridge-vulkan-dxgi`.

The Windows resize transaction and its pre-grow/post-shrink behavior must remain semantically
unchanged throughout both commits.

Exit gate: `telorgon-presenter-dxgi` contains no Vulkan types, the renderer contains no DXGI types,
and cross-API ownership appears only in the bridge.

### Phase 5 — Extract software window presentation

Goal: separate CPU rendering from native buffer presentation.

Work:

- create `telorgon-presenter-softbuffer`;
- move `softbuffer` context/surface management, copy/format conversion, damage, resize, and present;
- retain runtime scene preparation in `telorgon-app`; and
- retain rasterization in `telorgon-renderer-software`.

Exit gate: software window output and deterministic renderer tests are unchanged, and
`telorgon-app` no longer imports `softbuffer` directly.

### Phase 6 — Reduce `telorgon-app` to orchestration

Goal: remove backend mechanics from the host.

Work:

- have the host prepare runtime state and collect scene deltas;
- create a revisioned render/presentation request;
- delegate acquisition, rendering, transport, and present to the selected managed assembly;
- receive explicit progress and recovery dispositions;
- keep native resize-transaction scheduling in the Windows/`winit` host; and
- remove direct calls shaped like presentation code rendering the application runtime itself.

Exit gate: `telorgon-app` owns selection and scheduling but no graphics-API resource or pixel-transfer
implementation.

### Phase 7 — Retire compatibility surfaces

Goal: leave one clear ownership path per responsibility.

Work:

- keep internal imports on the owning WSI, DXGI, and bridge packages;
- keep duplicated compatibility types and obsolete adapters out of the public surface;
- use explicit assembly variants instead of `ManagedVulkanPresenter`; and
- keep Cargo features and architecture documents aligned with those ownership boundaries.

Exit gate: the target dependency graph is enforced, no implementation is duplicated, and the
workspace documentation describes the landed structure as current behavior.

## 9. Verification gates

### 9.1 Automated gates for every phase

- format checks for changed Rust and Markdown sources;
- compilation and tests for every affected crate and relevant feature combination;
- `clippy` for affected targets where the workspace already supports it;
- the existing `test-1` all-target profiling build, without launching the executable or profiler;
- software renderer conformance and deterministic output tests;
- hosted Vulkan and headless software tests;
- Vulkan validation tests where hardware execution is explicitly enabled; and
- dependency checks proving forbidden platform/API imports have not crossed crate boundaries.

No verification step may leave an application, event loop, profiler, or server running in the
background.

### 9.2 Manual Windows hardware gate

The user-run native qualification pass must cover:

- inward-only resize;
- outward-only resize;
- mixed-direction and corner resize;
- repeated direction reversal during one drag;
- minimize and restore;
- suspend and resume;
- repeated swapchain/bridge generation recreation;
- clean shutdown with frames in flight;
- Vulkan validation output;
- keyed-mutex timeout or abandoned-state diagnostics; and
- absence of black gaps, stretched frames, squashed frames, or presentation-order flicker.

Automated compilation is necessary but cannot replace this compositor-visible test.

## 10. Compatibility and rollout policy

- Refactoring commits must not silently change backend selection or fallback order.
- A failed GPU presentation path must remain an explicit error unless the caller selected software
  fallback.
- Public moves use temporary re-exports when the existing item is part of a supported public API.
- Feature names remain stable during extraction unless a separate compatibility proposal approves a
  change.
- Profiler event names remain stable or receive an explicit versioned migration.
- Each phase updates `IMPLEMENTATION_STATUS.md` only for work that has actually landed and been
  validated.
- New crates are introduced in the same commit that gives them real extracted responsibility; no
  placeholder backend crates are added.

## 11. Risks and controls

| Risk | Control |
|---|---|
| A generic contract erases synchronization semantics | Keep renderer-specific target types and separate completion stages |
| Runtime backend selection creates invalid combinations | Select only the three managed assemblies; keep their internals statically typed |
| Resource moves introduce use-after-free or premature retirement | Require the ownership ledger and linear frame token before extraction |
| DXGI extraction regresses resize synchronization | Preserve resize transaction ordering and run the full manual Windows gate after Phases 2, 4, and 6 |
| Compatibility facades become permanent duplicate implementations | Facades only re-export or adapt; Phase 7 has explicit deletion criteria |
| The plan expands into speculative platforms | Add a crate only when moving a currently working implementation |
| Error recovery becomes an implicit fallback | Model recovery states explicitly and keep software selection caller-visible |

## 12. Completion criteria

The restructuring is complete when all of the following are true:

- render crates contain no native presentation or window-resize policy;
- presenter crates contain no scene/runtime interpretation or renderer command generation;
- Vulkan/DXGI interop exists only in `telorgon-bridge-vulkan-dxgi`;
- `telorgon-app` selects complete managed assemblies and contains no API-specific pixel or swapchain
  mechanics;
- every acquired frame is represented by a linear, generation-bound token;
- render, transport, present, and display completion remain distinguishable;
- software, hosted, and headless behavior remains intact;
- Vulkan WSI and Windows Vulkan/DXGI behavior remains intact;
- the compositor-visible Windows resize behavior passes the manual qualification matrix; and
- adding a future renderer, bridge, or presenter can follow these boundaries without modifying the
  neutral contracts merely to accommodate an API name.

Meeting these criteria completes the reorganization. It does not by itself claim support for any
new renderer, graphics API, window system, or operating system.
