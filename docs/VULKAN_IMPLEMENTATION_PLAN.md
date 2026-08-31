# Telorgon GPU-First Vulkan Implementation Plan

## Document status

This is the execution plan for replacing the current CPU model in `telorgon-renderer-vulkan` with a
real Vulkan renderer. It refines the target contracts in
[RENDER_BACKEND_ARCHITECTURE.md](RENDER_BACKEND_ARCHITECTURE.md) into immediate Rust package and
implementation decisions.

The current crate is **operational, limited**, not production-qualified. The dependency versions below were checked
on 2026-08-20. They are compatible-version requirements, not permission to update dependencies
blindly; Cargo must lock reviewed versions and normal dependency auditing still applies.

Before each implementation slice, complete the applicable source review in
[REFERENCE_IMPLEMENTATIONS.md](REFERENCE_IMPLEMENTATIONS.md). The review is required to extract
lifetime, synchronization, surface-recovery, and embedding pitfalls from multiple established
systems before writing Telorgon's version.

The exact file ownership, dependency graph, runtime/backend separation, initial Rust API shapes, and
Slice 1–3 call flow are fixed in [IMPLEMENTATION_BLUEPRINT.md](IMPLEMENTATION_BLUEPRINT.md). Frame,
target, queue, completion, presentation, hosted-recording, destruction, and recovery ownership is
fixed in [GPU_OWNERSHIP_AND_SYNCHRONIZATION.md](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md). Retained-scene
records and deltas, GPU layouts, descriptor bindings, shader interfaces, batching, uploads, color,
and resource-use lowering are fixed in
[SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md). The Windows-first, hosted, Linux,
shell, Metal, and mobile sequence is fixed in
[PLATFORM_IMPLEMENTATION_ORDER.md](PLATFORM_IMPLEMENTATION_ORDER.md). Evidence layers, shared tests,
hardware-run policy, validation results, and qualification profiles are fixed in
[ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md).
Platform surface generations, borrowed raw handles, typed Linux DMA-BUF/sync-FD imports, and shell
host release obligations are fixed in
[PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md#12-native-handles-and-external-gpu-resources).

## 1. Current-phase directive

Real Vulkan rendering is the next rendering milestone. Renderer work must go directly to GPU device
creation, allocation, shader execution, command recording, synchronization, and presentation.

During this phase:

- treat the software renderer as feature-frozen: it receives fixes and test maintenance, not a new
  roadmap of effects or GPU-like behavior;
- keep `SoftwareRenderer` as a deterministic headless/test reference and as a temporary way to run
  the existing desktop tools;
- limit software-renderer work to correctness fixes, test infrastructure, and maintenance required
  to keep the repository healthy;
- do not require a new visual feature to be implemented in the software renderer before implementing
  it in Vulkan;
- do not use CPU rasterization, a CPU framebuffer upload, or `SoftwareRenderer` as an internal
  fallback inside `telorgon-renderer-vulkan`;
- do not extend the current synthetic `u64` handles, `Vec<T>` “GPU buffers,” predicted draw counts,
  or modeled swapchain bookkeeping as if they were a path to real GPU objects; replace them;
- make the first delivered visual milestone render a Telorgon scene box or quad with an actual Vulkan
  pipeline, command buffer, queue submission, and device image; and
- allow CPU readback only as an explicit test/export operation implemented by a real GPU image-to-
  staging copy. Ordinary drawing and presentation must never read pixels back.

When choosing between a software-renderer enhancement and the next incomplete Vulkan slice below,
the Vulkan slice takes priority unless the software change fixes a correctness regression blocking
the workspace.

A local triangle can be useful while bringing up a driver, but it is not a project milestone. The
first accepted proof must consume Telorgon scene data.

## 2. Selected Rust stack

### 2.1 Runtime Vulkan backend

| Package | Version line | Placement | Responsibility |
| --- | --- | --- | --- |
| [`ash`](https://docs.rs/ash/) | `0.38` | `telorgon-renderer-vulkan` | Direct Vulkan bindings, loader, core/extension entry points, and typed native handles |
| [`gpu-allocator`](https://docs.rs/gpu-allocator/) | `0.28` | `telorgon-renderer-vulkan` | Device-memory suballocation for persistent and staging buffers/images |
| [`bytemuck`](https://docs.rs/bytemuck/) | `1` | `telorgon-gpu-abi` and Vulkan backend | Checked POD definitions/conversion for instance, uniform, index, and staging uploads |
| [`thiserror`](https://docs.rs/thiserror/) | `2` | backend and presenter packages | Structured setup, allocation, execution, surface, and device-loss errors |

`ash` is the reference binding because Telorgon needs direct control over Vulkan objects, extensions,
synchronization, external memory, host-provided devices, and command recording. Higher-level Vulkan
frameworks must not define Telorgon's reference backend contract.

Use `ash`'s dynamically loaded mode for normal desktop builds. A machine with a Vulkan-capable driver
can then load the platform Vulkan loader without making the Vulkan SDK a link-time application
dependency. A statically linked loader may be an explicit platform profile later; it is not the
default.

Enable only the `std` and `vulkan` features of `gpu-allocator`. Its default feature set also enables
other graphics APIs; those dependencies do not belong in the Vulkan package. The allocator is a
backend-private implementation detail and does not define the cross-backend memory interface.

Use `bytemuck::Pod`/`Zeroable` only on explicitly `#[repr(C)]` GPU transfer structures whose layouts
are compile-time asserted. Do not cast UI/runtime structs directly into shader data.

### 2.2 Owned window presentation

| Package | Version line | Placement | Responsibility |
| --- | --- | --- | --- |
| [`raw-window-handle`](https://docs.rs/raw-window-handle/) | `0.6` | platform/presenter boundary | Short-lived borrowed display/window handles; never portable runtime state |
| [`ash-window`](https://docs.rs/ash-window/) | `0.13` | Vulkan Winit presenter | Required instance-extension enumeration and `VkSurfaceKHR` creation |
| [`winit`](https://docs.rs/winit/) | workspace `0.30.13` | managed desktop platform package | Window and event-loop ownership for the convenience application path |

`ash-window` and `winit` must not become dependencies of the renderer core. Owned presentation lives
in a Vulkan/platform presenter package and hands a render target plus acquire/present synchronization
to the backend. Hosted/game-engine rendering therefore does not pull in a window or event loop.

### 2.3 Shader build tooling

Use [`shaderc`](https://docs.rs/shaderc/) `0.10`, `spirv-tools` `0.13`, and `rspirv` `0.13` only in a
dedicated `telorgon-shader-build` tool package. Shaderc compiles the initial GLSL 450 sources to
SPIR-V; SPIR-V Tools validates the Vulkan 1.3 environment; and `rspirv` inspects declared entry
points, descriptors, stage I/O, offsets, strides, and capabilities. They must not be normal
dependencies or build dependencies of `telorgon-renderer-vulkan`, because native shader compiler and
validation tooling do not belong in every consumer build.

The shader workflow is:

1. keep reviewed shader source as the authority;
2. generate SPIR-V 1.6 artifacts for Vulkan 1.3 with fixed warning, optimization, and debug-info
   policies;
3. validate the generated modules against the Vulkan 1.3 SPIR-V environment;
4. inspect and compare entry points, stages, descriptor sets/bindings, stage I/O, record
   offsets/strides, and required capabilities with the declared manifest and `telorgon-gpu-abi`;
5. record source/artifact hashes, compiler options, target environment, entry points, binding schema,
   and interface version in a shader-bundle manifest and generated Rust module;
6. commit or package the generated Vulkan artifacts so ordinary Telorgon builds only use
   `include_bytes!`; and
7. have CI regenerate and compare the complete bundle to catch stale artifacts.

The current `build.rs` probing for `glslc` or `glslangValidator` is transitional. It must not remain
the distribution model once the shader-build package exists. The long-term shader bundle may contain
DXIL, metallib, or private vendor binaries compiled by their own tools; runtime shader compilation is
not part of the normal application path.

### 2.4 Diagnostics

Use `ash::ext::debug_utils` directly for Vulkan object names, command labels, and validation
messages. Route messages into a Telorgon-provided diagnostic sink. A library must not install a global
logger or tracing subscriber. If the workspace later adopts `tracing`, integration must be an
optional adapter rather than a backend requirement.

### 2.5 Deliberately excluded from the reference backend

- Do not use `wgpu` as the Vulkan implementation. A `telorgon-renderer-wgpu` portability adapter may
  exist later, but it cannot replace the direct reference used for external memory, host scheduling,
  and Vulkan-specific diagnostics.
- Do not use Vulkano or another high-level Vulkan object model in `telorgon-renderer-vulkan`; Telorgon's
  lifetime, capability, and hosted-device contracts must remain visible and testable.
- Do not use Softbuffer for the Vulkan presentation path.
- Do not add both `gpu-allocator` and `vk-mem`. Start with one allocator and measure it before
  considering a replacement.
- Do not add an asynchronous runtime merely to render frames. Worker threads and async scheduling are
  host policy.

## 3. Cargo dependency placement

Add compatible requirements at the workspace level when implementation begins:

```toml
[workspace.dependencies]
ash = { version = "0.38", default-features = false, features = ["std", "loaded"] }
ash-window = "0.13"
raw-window-handle = "0.6"
gpu-allocator = { version = "0.28", default-features = false, features = ["std", "vulkan"] }
bytemuck = { version = "1", features = ["derive", "must_cast"] }
thiserror = "2"
shaderc = "0.10"
spirv-tools = "0.13"
rspirv = "0.13"
sha2 = "0.11"
serde = { version = "1", features = ["derive"] }
toml = "0.9"
```

Then keep package dependencies narrow:

```toml
# telorgon-renderer-vulkan/Cargo.toml
[dependencies]
ash.workspace = true
bytemuck.workspace = true
gpu-allocator.workspace = true
thiserror.workspace = true
telorgon-render.workspace = true
telorgon-gpu-abi.workspace = true

# telorgon-gpu-abi/Cargo.toml
[dependencies]
bytemuck.workspace = true

# Vulkan/Winit presenter package
[dependencies]
ash.workspace = true
ash-window.workspace = true
raw-window-handle.workspace = true
winit.workspace = true
telorgon-renderer-vulkan.workspace = true

# telorgon-shader-build/Cargo.toml
[dependencies]
shaderc.workspace = true
spirv-tools.workspace = true
rspirv.workspace = true
sha2.workspace = true
serde.workspace = true
toml.workspace = true
telorgon-gpu-abi.workspace = true
```

Do not add shader compilation/reflection tools, `ash-window`, or `winit` to the backend-core
manifest. Do not enable `gpu-allocator`'s default features. Review the resolved Cargo graph after
each dependency addition. The exact record and bundle contract is in
[Scene-to-GPU ABI and shader contract](SCENE_GPU_ABI_AND_SHADERS.md).

## 4. Initial Vulkan baseline

The first operational profile targets Vulkan 1.3 on Windows x86-64. Linux x86-64 Wayland and X11
are the next separately qualified managed profiles after hosted Vulkan; Android arm64 Vulkan is the
first mobile renderer profile. Apple platforms use the future direct Metal backend rather than
making MoltenVK the definition of Telorgon portability. Gate 5 controls the full order.

The initial required device capability set is intentionally small:

- graphics queue and, for owned presentation, a queue compatible with the chosen surface;
- dynamic rendering;
- Synchronization 2;
- timeline semaphores;
- shader demote-to-helper-invocation, used by the current fragment clipping and rounded-corner
  shaders;
- sampled images, storage/uniform buffers, transfer buffers, and color attachments;
- the formats required for RGBA/BGRA UI targets and an `R8_UNORM` glyph atlas; and
- `VK_KHR_swapchain` only for the owned presenter.

Descriptor indexing, buffer device address, mesh shaders, ray tracing, subgroups, advanced blend
extensions, dedicated transfer queues, and HDR are optional. The first renderer must have a correct
path without them. Vulkan 1.2 plus promoted extensions may become a measured compatibility profile
after the 1.3 path is operational; it must not delay initial GPU bring-up.

Enable `VK_LAYER_KHRONOS_validation`, synchronization validation, and GPU-assisted validation in
development configurations when installed. They are not shipped or enabled by default in release
applications.

## 5. Backend source organization

Replace the current single modeled implementation file with focused modules. The expected shape is:

```text
crates/telorgon/src/renderer_vulkan/
├── lib.rs             Curated exports only
├── config.rs          Required/optional capabilities and limits
├── error.rs           Typed backend errors and Vulkan result mapping
├── entry.rs           Loader, instance, layers, and instance extensions
├── adapter.rs         Physical-device enumeration, scoring, and reports
├── device.rs          Logical device, queues, features, and ownership
├── scene.rs           Per-view retained Vulkan resources
├── target.rs          Backend-native target validation and metadata
├── memory.rs          gpu-allocator wrapper and budget reporting
├── buffer.rs          Typed buffers and mapped/staging operations
├── image.rs           Images, views, samplers, and layout tracking
├── descriptor.rs      Layouts, pools, sets, and binding caches
├── shader.rs          Embedded SPIR-V bundle and module validation
├── pipeline.rs        Pipeline layouts, graphics pipelines, and cache
├── upload.rs          Persistent staging ring and dirty-range copies
├── sync.rs            Barriers, timelines, fences, and completion tokens
├── frame.rs           Per-frame command pools/buffers and deferred release
├── executor.rs        Render-plan translation and command recording
├── readback.rs        Explicit test/export image-to-staging path
├── interop.rs         Typed Vulkan-native hosted/external-image interfaces
└── diagnostics.rs     Debug utilities, labels, counters, and callbacks
```

Owned WSI code belongs in a separate presenter package with `surface.rs`, `swapchain.rs`,
`presenter.rs`, and `recovery.rs`. Do not recreate a monotelorgon `vulkan.rs` or leave all work in
`lib.rs`.

## 6. Required interface corrections before presentation

The present `Renderer` interface and modeled Vulkan crate contain CPU assumptions that must be
removed in the first implementation slice:

1. Split scene execution from presentation. Rendering records work against a target supplied by an
   owned presenter or host; a renderer does not assume every target is a swapchain image.
2. Make readback a separate optional capability instead of a mandatory presentation result.
3. Remove `SoftwareRenderer` and `readback_reference` from `VulkanRenderer`. The Vulkan package must
   not call the software renderer while applying a scene or returning readback.
4. Replace synthetic integer command pools, command buffers, fences, semaphores, images, views, and
   framebuffers with real `ash::vk` handles owned by typed RAII wrappers.
5. Replace `GpuBuffer<T> { values: Vec<T> }` with device/staging allocations. A CPU mirror may exist
   only where delta application actually needs it and must not be called a GPU buffer.
6. Replace `RenderTargetId` as the rendering contract with a typed render-target description carrying
   format, extent, sample count, usage, image/view identity, initial/final usage, and synchronization.
7. Obtain upload, barrier, command-buffer, submission, draw, descriptor, and memory statistics from
   performed backend operations rather than predictions.
8. Move swapchain image ownership and recreation out of the renderer core and into the presenter.
9. Use lifetime-bearing frame/target associated types and shared device methods so acquired and
   host-borrowed validity is represented without preventing multi-view device sharing.
10. Split a recording frame, a submit-once recorded frame, a submission completion point, and an
    acquired presentation token into distinct types.

The exact Rust trait names remain internal until validated, but the responsibility split is:

```text
SceneDelta -> retained backend scene -> RenderPlan -> Vulkan executor -> provided RenderTarget
                                                        |
                                    owned Presenter or host frame context

Optional Readback: rendered image -> GPU copy -> mapped staging allocation
```

Do not attempt to stabilize the entire cross-backend RHI before drawing the first Telorgon primitive.
Introduce only the backend-neutral render-plan, resource-usage, target, and completion concepts
needed by the real Vulkan path, keep them internal, and validate them later with the trace backend
and a materially different Metal or D3D12 backend.

## 7. GPU data path

The normal owned frame path is:

1. validate/apply an epoch-ordered `SceneDelta` atomically to the retained CPU mirror;
2. convert only changed slots to `telorgon-gpu-abi` records and copy coalesced instance ranges plus
   atlas/image regions into a persistently mapped staging ring;
3. record buffer/image copies into the current Vulkan command buffer;
4. emit Synchronization 2 barriers from semantic resource usages;
5. begin dynamic rendering on the supplied target;
6. bind persistent pipelines, Gate 4 descriptor sets, instance storage, and draw-index storage;
7. emit vertexless instanced draws for adjacent compatible batches;
8. end rendering and transition the target to the host/presenter-requested final usage;
9. finish to a submit-once recorded frame;
10. submit and signal a completion value in owned mode, or return a receipt requiring host
    commit/discard in hosted command-only mode; and
11. present only by consuming an owned presenter's acquired token.

Persistent device buffers grow geometrically and are replaced through deferred destruction after
their last completion value. They are not recreated every frame. Frame-local command pools,
descriptor arenas, and staging ranges are reused only after their fence/timeline completion.

The initial pipelines cover, in order:

1. premultiplied-alpha box/quad fill from Telorgon box instances;
2. scissor/clip and spatial transforms;
3. `R8_UNORM` glyph-atlas sampling;
4. ordinary RGBA image sampling;
5. borders and corner radii; and
6. material/intermediate passes only after the direct primitives are operational.

Do not make descriptor indexing a baseline requirement. Without it, group ordered image draws by
compatible binding and update/reuse descriptor sets. With it, a capability-selected path may use a
texture table, but both paths consume the same scene and render-plan contracts.

The exact byte layouts, four descriptor sets, draw indirection, upload coalescing/growth behavior,
color conversion, and barriers are normative in
[Scene-to-GPU ABI and shader contract](SCENE_GPU_ABI_AND_SHADERS.md); this execution summary does not
define an alternate data path.

## 8. Implementation sequence and exit criteria

### Slice 1: Remove the false Vulkan facade

- add the selected backend dependencies;
- split renderer execution, presentation, and optional readback contracts;
- remove the Vulkan crate's `SoftwareRenderer` field and delegation;
- rename or delete modeled types that falsely imply native Vulkan resources; and
- keep current model tests only if relabeled as planner/data-structure tests outside the Vulkan
  operational path.

Exit: `telorgon-renderer-vulkan` contains no CPU rasterizer and no method can report a submitted Vulkan
frame without executing Vulkan work.

### Slice 2: Real offscreen GPU primitive

- load Vulkan through `ash`;
- create an instance with debug utilities in development;
- enumerate and score physical devices with a written capability report;
- create a device and graphics queue;
- create the allocator, command pool, command buffer, fence/timeline, device target image, and staging
  resource;
- create a real shader module, pipeline layout, and dynamic-rendering graphics pipeline; and
- consume a minimal Gate 4 scene delta and draw one Telorgon box into the device image.

An explicit GPU readback test may copy that image into staging and verify expected pixels.

Exit: a test proves shader execution and color output on actual Vulkan hardware with zero validation
errors. Synthetic counters or SPIR-V files alone do not satisfy this slice.

### Slice 3: Owned Winit presentation

- deliver and qualify the first path on Windows x86-64 before claiming Linux support;
- use `raw-window-handle` and `ash-window` to create `VkSurfaceKHR`;
- select format, extent, image count, composite alpha, and present mode from queried capabilities;
- create views and per-image state for a real swapchain;
- keep presentation-wait semaphores per swapchain image and frame-slot completion independent;
- acquire, render the Telorgon box scene, submit, and present;
- handle zero-sized/suspended windows without busy looping; and
- recreate on resize, suboptimal status, and out-of-date status without an ordinary
  `vkDeviceWaitIdle`.

Exit: the managed entry point can select Vulkan and display retained Telorgon UI without a CPU
framebuffer or Softbuffer copy.

### Slice 4: Retained resources and batching

- allocate persistent device buffers for boxes, glyphs, images, clips, spatial nodes, materials, and
  draw indices;
- upload dirty ranges and atlas pages through the staging ring;
- reuse descriptors, samplers, pipelines, and command resources;
- implement ordered batching and measured draw counts; and
- defer resource destruction by completion value.

Exit: one changed control does not rebuild or upload the entire scene, and warmed frames create no
per-frame Vulkan objects.

### Slice 5: Current UI visual coverage

- render boxes, text, ordinary images, clipping, scrolling, opacity, and current focus/interaction
  visuals;
- implement Gate 4's premultiplied-alpha, sRGB, coordinate, sample, and target-load/store
  conventions; and
- compare explicit Vulkan readbacks with software reference images only in conformance tests.

Exit: the current gallery's core specimens have Vulkan visual tests within documented tolerances.
The application itself is still user-run; automated work must not leave a GUI or service running.

### Slice 6: Hosted device and render-area mode

This is Gate 5 P2 and follows the first Windows owned path before broad Linux/mobile service work.

- accept a host-approved instance/device, queues, allocator callbacks or allocation service, formats,
  command buffer/frame context, target, usage state, and synchronization contract;
- support several `UiView` values sharing pipelines and atlases on one `UiDevice`;
- record into host-selected full targets or subregions;
- return resource usages, completion requirements, and statistics without independent submission when
  command-only mode is requested; and
- prove that an unchanged view records nothing and allocates nothing.

Exit: a test host composites multiple independent Telorgon views without Telorgon owning the event loop,
swapchain, frame pacing, or queue submission.

Implementation status: qualified. The borrowed-device, host-target/subregion, command-only frame,
linear host completion receipt, and window-system-free multi-view APIs are implemented. The accepted
E6 developer-hardware fixture recorded two independent views into two target subregions with zero
Telorgon submissions, zero Telorgon command-buffer begin/end operations, two host completion receipts,
and zero validation errors.

### Slice 7: External images and shell prerequisites

Only after owned and hosted application rendering is operational, implement platform-specific
external-memory/image import and acquire/release synchronization. Protocol objects and shell policy
remain outside the renderer.

The first shell proof is Gate 5's protocol-neutral Linux Vulkan host; Telorgon does not implement
Wayland or another display protocol.

Exit: an imported image is sampled without CPU readback, with real synchronization and explicit
unsupported results on platforms that cannot provide the required interop.

Implementation status: the same-device borrowed-image profile is qualified by the accepted E8
developer-hardware run. It bound a linear host-image lease directly into the retained image pipeline,
recorded real acquire/release synchronization, sampled with zero external pixel upload, retired one
host completion receipt, and reported zero validation errors. The owning Linux adapter is now
compile-complete behind `target_os = "linux"`: it enumerates deterministic single-plane RGBA/BGRA
fourcc/format/modifier import records for an exact requested usage, validates the selected modifier
and layout again at import, consumes DMA-BUF and acquire sync-FD ownership, performs
foreign-queue-family acquire/release barriers, exports a one-shot release sync FD, and blocks receipt
commit until that export obligation is resolved. Its ignored E8 Linux hardware fixture selects a
jointly importable/exportable advertised tuple and cross-compiles for `x86_64-unknown-linux-gnu`;
execution on a qualifying Linux driver is deliberately deferred and remains required before the P4
profile is qualified. Multi-plane/YUV, protected content, and protocol-host integration remain
explicitly unsupported or outside the renderer.

## 9. Owned and hosted Vulkan rules

Owned mode may create the Vulkan instance, device, queues, allocator, command pools, swapchain, and
submissions. It still exposes device/surface loss and does not hide infinite recovery loops.

Hosted mode must:

- distinguish borrowed from owned native objects;
- never destroy objects supplied by the host;
- never call `vkDeviceWaitIdle` during ordinary rendering;
- never submit, present, change queue ownership, or install callbacks unless the host contract grants
  that responsibility;
- accept initial/final target usage and preserve unrelated target contents;
- return every touched resource usage and required synchronization edge;
- return a linear resource-pinning receipt that the host commits to an explicit monotonic completion
  point after submission or discards only when it did not submit;
- use host allocation callbacks/services where required; and
- make thread use, caches, transient budgets, and diagnostic callbacks explicit configuration.

Vulkan-native interoperability types live only in `telorgon-renderer-vulkan` or its presenter/interop
extensions. UI, layout, theme, text, and backend-neutral scene packages must not import `ash`.

The first hosted profile is command-only: Telorgon does not begin, end, reset, or submit the host
command buffer. The exact safety contract and shutdown requirements are defined by Gate 3.

## 10. Validation and evidence

Gate 6 controls the complete evidence taxonomy, shared case matrix, qualification profiles, and
reports in [ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md). Every GPU slice adds
evidence only at the layer it actually claims:

- compile-only tests for type and package boundaries;
- planner tests that require no graphics device;
- hardware-gated offscreen tests for actual shader execution, copies, barriers, and readback;
- validation-layer runs with synchronization validation enabled;
- owned-presentation tests for acquire/submit/present and resize/loss handling;
- hosted-mode tests using a real host command buffer/target contract;
- RenderDoc or equivalent capture review for draw/batch/resource claims; and
- device/driver/build metadata attached to timing results.

In an explicitly selected developer hardware run, a missing suitable device, loader, layer, or
surface integration is a serialized `skip` with its exact reason. Under
`TELORGON_TEST_MODE=qualification`, the same missing required prerequisite is `fail`. Portable
workspace tests never initialize a GPU by default, and no run may silently fall back to software and
pass. CPU-only, Vulkan-hardware, managed, hosted, validation, visual, and performance jobs remain
separate so one evidence class cannot hide another.

## 11. Operational definition of done

`telorgon-renderer-vulkan` changes from **modeled** to **operational** only when all of these are true:

- `ash::Entry`, `ash::Instance`, `ash::Device`, a physical device, and real queues are used;
- buffers and images use bound `VkDeviceMemory` through the allocator;
- SPIR-V executes in a Vulkan graphics pipeline;
- command buffers contain real copies, barriers, dynamic rendering, bindings, and draws;
- a queue submission completes under fences or timeline semaphores;
- owned mode acquires and presents real swapchain images;
- hosted mode or its explicitly scheduled later milestone is not falsely claimed by owned mode;
- ordinary presentation has no CPU rasterization, framebuffer upload, or readback;
- reported metrics are counted from performed Vulkan operations; and
- relevant validation tests pass on documented hardware.

Production qualification remains a later implementation state requiring the full E0–E9 matrix,
declared device/driver profile, recovery, platform, performance, and external-interop evidence in
[ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md). `PERFORMANCE.md` describes the
current evidence and performance principles.

## 12. Primary implementation references

- [`ash` crate documentation](https://docs.rs/ash/)
- [`ash-window` crate documentation](https://docs.rs/ash-window/)
- [`gpu-allocator` crate documentation](https://docs.rs/gpu-allocator/)
- [`raw-window-handle` crate documentation](https://docs.rs/raw-window-handle/)
- [`shaderc` crate documentation](https://docs.rs/shaderc/)
- [Khronos Vulkan Guide: synchronization](https://docs.vulkan.org/guide/latest/synchronization.html)
- [Khronos Vulkan Guide: validation overview](https://docs.vulkan.org/guide/latest/validation_overview.html)
- [Khronos Vulkan Tutorial: swapchains](https://docs.vulkan.org/tutorial/latest/03_Drawing_a_triangle/01_Presentation/01_Swap_chain.html)

These references explain APIs and tooling; Telorgon's ownership, package, render-plan, presentation,
and embedding rules are defined by this repository's architecture documents.
