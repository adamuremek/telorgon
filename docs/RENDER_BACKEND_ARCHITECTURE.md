# Telorgon Render Backend Architecture

## Document status

This document is the working specification for adapting Telorgon rendering to Vulkan, Metal,
Direct3D, console graphics APIs, host render graphs, and future modern explicit APIs. It refines the
rendering sections of [PROJECT_SCOPE_AND_ARCHITECTURE.md](PROJECT_SCOPE_AND_ARCHITECTURE.md).

The concrete Rust packages and current GPU-first execution order are specified in
[VULKAN_IMPLEMENTATION_PLAN.md](VULKAN_IMPLEMENTATION_PLAN.md). The initial concrete lifetime,
completion, hosted-frame, presentation-token, and deferred-destruction contract is fixed in
[GPU_OWNERSHIP_AND_SYNCHRONIZATION.md](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md). The initial retained
scene, delta, GPU record, upload, shader, binding, color, and semantic resource-use contract is fixed
in [SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md). Gate 5 fixes direct Metal as the
second backend and the platform sequence in
[PLATFORM_IMPLEMENTATION_ORDER.md](PLATFORM_IMPLEMENTATION_ORDER.md). Shared conformance and
production evidence are fixed in
[ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md).
Borrowed native-window handles, surface generations, opaque external-content identities, and typed
platform import/acquire/release payloads are fixed in
[PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md#12-native-handles-and-external-gpu-resources).

The interface described here is planned. It is not validated by the current modeled Vulkan crate.
It remains provisional until a real Vulkan implementation and a materially different second
backend pass the applicable Gate 6 conformance requirements.

Closed console adaptability cannot be guaranteed without access to a vendor SDK and an operational
vendor backend. Telorgon can guarantee that proprietary APIs are isolated behind implementable,
source-available contracts and that the open core does not require proprietary types.

## 1. Goals

The backend architecture must:

- make Vulkan the first operational and diagnostic reference backend;
- map efficiently to Metal, Direct3D 12, and modern console APIs;
- allow private vendor crates to implement closed APIs without forking UI, scene, or render-planning
  packages;
- support Telorgon-owned devices and presentation as well as host-owned devices and command recording;
- preserve explicit lifetime, memory, synchronization, and failure behavior;
- expose capabilities and fallbacks instead of silently emulating unsupported features at unknown
  cost;
- keep graphics API types out of application, component, layout, text, theme, and scene APIs;
- avoid virtual dispatch and allocation per UI primitive or draw;
- keep presentation, platform protocols, and graphics execution as separable integrations; and
- produce comparable correctness, resource, and performance diagnostics on every backend.

## 2. Non-goals

The backend interface is not:

- a general replacement for Vulkan, Metal, Direct3D, or a game engine rendering API;
- a promise to expose every feature of every GPU;
- a lowest-common-denominator API that disables modern capabilities;
- a runtime shader-transpilation requirement;
- a stable binary plugin ABI;
- a way for ordinary application components to issue native graphics commands; or
- permission for the Vulkan implementation to define public cross-backend terminology.

The backend service provider interface is a source-level Rust crate API. A stable C ABI for dynamic
backend plugins would be a separate future project with its own versioning and safety model.

## 3. Layer boundaries

```text
Application and shell components
             |
             v
Backend-neutral retained scene
UI instances | painter order | logical resources | damage
             |
             v
Render planner
passes | dependencies | batches | uploads | capability fallbacks
             |
             v
Rendering hardware interface and backend SPI
resources | usage | commands | completion | diagnostics
             |
             v
Concrete backend
Vulkan | Metal | D3D12 | private console | portability adapter
             |
             +-----------------------+
             |                       |
             v                       v
Owned presenter              Host frame/render graph
```

Each boundary removes a different kind of policy:

- the retained scene knows UI rendering semantics but no GPU API;
- the planner knows Telorgon pipelines and logical pass dependencies but no native handles;
- the RHI knows explicit GPU concepts but no widgets, layouts, or shell protocols;
- a backend owns API lowering, native resources, shader artifacts, and diagnostics; and
- a presenter owns native image acquisition and presentation but not scene compilation.

## 4. Portability variation points

The interface must account for real API differences rather than rename Vulkan concepts:

| Concern | Vulkan | Metal | Direct3D 12 | Console/vendor implication |
| --- | --- | --- | --- | --- |
| Resource binding | Descriptor sets/pools | Argument buffers or encoder bindings | Root signatures and descriptor heaps | Backend selects native binding strategy |
| Resource state | Layouts, access masks, stages | Encoder and hazard-tracking rules | Resource states and barriers | Planner declares usage; backend lowers hazards |
| Memory | Device memory and allocations | Storage modes and heaps | Heap/resource classes | RHI describes intent, not heap flags |
| Commands | Command buffers and pools | Command buffers/encoders | Command lists/allocators | Frame contract cannot expose one native model |
| Synchronization | Fences, binary/timeline semaphores | Events, command-buffer completion | Fences and queue waits | Completion and external sync are capability-driven |
| Presentation | `VkSwapchainKHR` | `CAMetalLayer` drawables | DXGI swapchains | Presenter is separate from device execution |
| Shaders | SPIR-V | Metallib/MSL | DXIL/HLSL | Shader bundles carry backend artifacts |
| Pipeline caches | Driver pipeline cache | Binary archives/functions | Pipeline state libraries | Cache interface is advisory and backend-owned |
| Coordinates | Vulkan viewport conventions | Metal conventions | D3D conventions | Scene uses one convention; backend transforms |
| External resources | Platform extensions and handle types | IOSurface/shared events | Shared handles/fences | Native interop lives in backend/platform packages |

Proprietary backends may have additional memory regions, tiling modes, command encodings, shader
formats, display queues, or performance rules. The backend SPI leaves these as implementation and
extension concerns while requiring observable behavior through common capabilities and tests.

## 5. Dispatch and type model

The primary execution model is compile-time backend selection. Conceptually:

```rust,ignore
trait Backend {
    type Device;
    type Frame<'frame>;
    type Buffer;
    type Texture;
    type TextureView;
    type Sampler;
    type Pipeline;
    type Completion;

    fn capabilities(device: &Self::Device) -> &BackendCapabilities;
}

struct Renderer<B: Backend> {
    planner: RenderPlanner,
    resources: ResourceRegistry<B>,
    executor: BackendExecutor<B>,
}
```

This is illustrative, not a frozen trait definition. Associated backend types prevent native
handles from becoming untyped integers and allow a single-backend application or console title to
use static dispatch. Desktop runtime selection may use a coarse enum or type-erased renderer at the
renderer boundary; it does not add dynamic calls per primitive, glyph, or draw.

The render planner emits compact arrays and pass descriptions. Backend calls consume batches or
command spans. The design explicitly rejects a trait-object call for every UI node or GPU command.

## 6. Capability model

Capabilities are immutable for a device and divided into required baseline behavior and optional
clusters. A backend must fail creation with a precise report if it cannot supply the selected
profile.

### 6.1 Core UI baseline

The initial hardware baseline requires equivalent behavior for:

- vertex and fragment pipelines;
- indexed and instanced drawing;
- sampled 2D textures and samplers;
- uniform and structured read-only data;
- dynamic vertex/index/instance uploads;
- premultiplied-alpha blending;
- scissor rectangles and a clipping fallback;
- color render targets and render-to-texture;
- buffer-to-buffer and buffer-to-texture copies;
- RGBA/BGRA 8-bit output through backend-selected compatible formats;
- sRGB decode/encode behavior or an explicit shader fallback;
- completion notification sufficient for deferred destruction; and
- one graphics-capable queue or equivalent execution stream.

Backend-native format choice is negotiated. The scene never assumes a particular Vulkan or DXGI
format enumeration.

### 6.2 Optional capability clusters

| Capability | Enables | Required fallback |
| --- | --- | --- |
| `Compute2d` | Compute vector rasterization, filters, and large parallel work | Raster or CPU preparation path where supported by the feature |
| `BindlessResources` | Descriptor indexing/argument-buffer/heap-oriented image batching | Bounded binding pages and cached bind groups |
| `StencilClipping` | Efficient complex nested clips | Mask texture or bounded geometry fallback |
| `AdvancedBlend` | Native advanced blend equations | Intermediate target or shader implementation |
| `HdrOutput` | Wide-gamut and HDR targets | SDR tone-mapped output |
| `TimelineSync` | Fine-grained frame and external synchronization | Backend completion tokens/fences |
| `ExternalImages` | Zero-copy compositor/video/engine images | Explicit unsupported result; no hidden readback |
| `ExternalSynchronization` | GPU-side waits/signals for imported content | Explicit unsupported result |
| `AsyncCompute` | Overlapped compute passes | Execute on graphics queue |
| `MemorylessTargets` | Tile-memory-efficient intermediates | Ordinary transient texture |
| `TimestampQueries` | GPU timing diagnostics | Mark timing unavailable, never synthesize it |
| `HostCommandRecording` | Game-engine/render-graph command-only mode | Owned submission mode only |

Fallback selection belongs to the render planner. The backend reports capabilities and performs
native lowering; it does not silently replace a requested GPU feature with a CPU copy.

## 7. Resource and memory model

Cross-backend resource descriptions express purpose:

- `BufferDescription`: size, logical usage, update frequency, memory intent, and debug label;
- `TextureDescription`: dimensions, logical format, usage, sample count, mip policy, color intent,
  and debug label;
- `MemoryIntent`: device-local, upload, readback, transient, or host-provided; and
- `LifetimeClass`: persistent, per-view, frame, pass-transient, imported, or host-borrowed.

The backend chooses heaps, memory types, storage modes, tiling, alignment, residency strategy, and
suballocation. A host can supply an allocator adapter in hosted mode. The common API does not expose
Vulkan memory-property flags, Metal storage modes, D3D heap types, or vendor memory-region names.

Resources use typed generational handles at the planner boundary and typed native objects inside the
backend. Destruction is deferred until a backend `CompletionPoint` proves all relevant work is
finished. Imported and host-borrowed resources are never destroyed by Telorgon.

Resource creation does not imply immediate upload or synchronization. Upload plans are batched into
host-visible frame work and accounted separately from device-local allocation.

## 8. Resource usage and synchronization

The planner declares logical resource usage for each pass:

- sampled read;
- uniform/structured read;
- storage read or write;
- color/depth attachment read or write;
- copy source or destination;
- resolve source or destination;
- present/external ownership; and
- host-declared initial and final usage.

Pass descriptions declare reads, writes, ordering dependencies, and queue preference. A backend
barrier compiler maps these to Vulkan stages/access/layouts, D3D12 resource states, Metal encoder and
hazard rules, or vendor equivalents.

The common planner does not emit raw Vulkan barriers. The backend may merge transitions, split
encoders, fuse compatible passes, or omit hazards that its API tracks automatically while preserving
the declared dependency graph.

The baseline assumes one graphics execution stream. Multiple queues are optional and selected only
when the backend and host can express ownership transfer safely. Hosted mode accepts the host's
initial/final resource usage and may return a completion value instead of submitting.

Gate 4 fixes the first plan vocabulary and its Vulkan synchronization2 lowering, including host
staging, scene-buffer uploads, texture updates, intermediates, targets, and readback. Other backends
translate those semantic uses rather than importing Vulkan flags.

## 9. Render plan and command model

`RenderPlan` is immutable for one prepared frame and contains:

- target and pass descriptions;
- logical resource reads and writes;
- upload and copy ranges;
- pipeline and binding keys;
- adjacent ordered draw batches;
- compute dispatches where capabilities select them;
- resolves, blits, and intermediate-target lifetimes;
- external waits/signals; and
- diagnostics/accounting labels.

It deliberately contains dependencies and intent rather than a serialized Vulkan command buffer.
Backends lower the plan into their native command model. They may optimize without changing painter
order, resource visibility, target contents, damage semantics, or observable output.

The initial plan uses typed generational scene slots, a separate draw-index buffer, and adjacent-only
batching. Exact delta validation, batch keys, and vertexless instanced draw behavior are defined by
[Gate 4](SCENE_GPU_ABI_AND_SHADERS.md#5-render-plan-and-batching).

Application custom drawing cannot append unvalidated native commands to a portable plan. Portable
custom rendering uses Telorgon materials and shader bundles. Native integration uses an explicit
backend extension pass or an external surface, making the loss of portability visible at the call
site and in diagnostics.

## 10. Binding model

Shader interfaces declare semantic binding roles rather than descriptor-set policy:

- frame/view data;
- scene instance data;
- material data;
- sampled images and samplers;
- glyph/image atlas pages;
- pass-local inputs and outputs; and
- small draw/pass constants.

The backend maps these roles to descriptor sets, argument buffers, root signatures, descriptor
tables, direct encoder bindings, or vendor mechanisms. Capabilities report binding counts, alignment,
small-constant limits, dynamic-offset behavior, and bindless support.

Pipeline and binding caches use logical interface and resource keys. Application code never manages
descriptor pools, root parameters, or argument-buffer offsets.

These remain portable semantic roles. The Vulkan baseline's exact four-set descriptor ABI and its
deliberate lack of push constants, vertex attributes, descriptor indexing, or scalar-layout
requirements are fixed by Gate 4.

## 11. Shader artifacts

A `ShaderBundle` contains:

- a stable logical shader identifier and version;
- an interface manifest describing stages, bindings, vertex inputs, constants, outputs, and required
  capabilities;
- backend-specific compiled artifacts;
- specialization/variant keys; and
- reflection hashes used to reject incompatible artifacts.

Vulkan first consumes SPIR-V. Direct3D may consume DXIL, Metal may consume a metallib, and private
console backends may consume vendor-compiled binaries. The build tool may generate multiple
artifacts from shared source where legally and technically possible, but the runtime does not require
cross-compilation or access to proprietary compilers.

Every backend validates that its artifact matches the logical interface manifest. A missing artifact
or unsupported feature produces a pipeline-creation error naming the shader, backend, and capability.

Gate 4 fixes the initial Vulkan GLSL-to-SPIR-V toolchain, manifest/reflection checks, artifact hashes,
generated Rust metadata, and required shader variants. Runtime compilation remains prohibited.

## 12. Coordinate, color, and alpha conventions

The retained scene defines one convention:

- logical coordinates start at the top-left with positive Y downward;
- transforms and clips operate in logical coordinates before output scaling;
- texture sampling uses a logical top-left origin independent of native texture origin; individual
  scene records explicitly choose normalized UVs or stable atlas texel rectangles;
- authoring, image, intermediate, and target colors have explicit encodings;
- blend shaders normalize input and emit linear premultiplied alpha; and
- target color space, transfer function, alpha mode, and HDR intent are explicit.

The planner/backend owns viewport transforms, projection correction, winding adjustments, pixel
center rules, sRGB conversion, and target encoding. Shader code cannot rely on an undocumented
Vulkan clip-space convention.

## 13. Device ownership and host integration

### 13.1 Owned mode

The backend owns adapter/device/queue selection, allocator, frame slots, submission, and device
recovery. A managed host owns the separate backend/platform presenter and coordinates surface
recovery. Both expose application-level configuration rather than native setup requirements.

### 13.2 Hosted mode

The host supplies a backend-specific `HostedDeviceContext` containing approved native objects and
constraints. The common contract records:

- which queues or command contexts Telorgon may use;
- whether Telorgon may submit or only record;
- allocator and resource-creation callbacks or native ownership rules;
- frame-slot and completion information;
- pipeline/shader cache integration;
- target initial/final usage and synchronization;
- thread-affinity and command-recording rules; and
- device-loss and shutdown notification.

Hosted mode never assumes ownership from receiving a native handle. Borrowed, shared, and transferred
ownership are distinct types. Telorgon cannot wait for device idle, reset host command pools, change
global device state, or submit independently unless the host contract explicitly permits it.

## 14. Presentation boundary

Presentation is implemented by a `Presenter` associated with a backend/platform pair, not by the
retained scene or RHI core. It owns:

- native presentation-surface creation;
- image acquisition;
- size, format, color-space, alpha, and present-mode negotiation;
- resize, suspend, resume, and out-of-date handling;
- presentation waits/signals; and
- full recreation or terminal surface-loss reporting.

Vulkan platform surfaces, DXGI swapchains, `CAMetalLayer`, console display queues, headless images,
and host render targets therefore do not need one fake universal swapchain type.

The managed Windows implementation may pair the Vulkan renderer with a DXGI HWND presenter without
turning D3D11 into a second render backend. The presenter owns same-adapter D3D11 shared textures,
their imported Vulkan aliases, the cross-API fence, and the final GPU copy/present operation. The
renderer still sees only a validated `VulkanTarget`; portable scene and RHI code never sees COM,
HWND, or DXGI types.

Command-only embedding uses no `Presenter`. The host provides `RenderArea` targets and owns final
composition and presentation.

## 15. Native interop and external images

Native interop is opt-in and split by backend and platform. General code sees opaque imported image
identities plus capabilities. Backend/platform extension traits handle actual memory handles,
IOSurface objects, D3D shared resources, Vulkan file descriptors/Win32 handles, or vendor objects.

Those extension types obey Gate 9's linear owning-lease, generation, validation, acquire-before-use,
release-after-final-read, and failure-cleanup rules. Portable packages never store a native pointer,
FD, integer handle, or cross-API native-handle enum.

An import description includes ownership, lifetime, dimensions, logical format, color/alpha intent,
initial/final usage, content version, damage, and acquire/release synchronization. Unsupported import
paths return a structured error. They do not fall back to CPU readback unless the caller explicitly
requests and budgets a copy path.

## 16. Errors, diagnostics, and recovery

Backend errors are structured into adapter selection, unsupported capability, resource allocation,
shader artifact, pipeline creation, target/surface, synchronization, out-of-memory, device loss, and
host-contract violations. Error values retain backend-native diagnostic codes without exposing them
as portable semantics.

Every backend reports:

- backend/API and adapter identity;
- capability and limit snapshot;
- selected fallback paths;
- resource counts and memory high-water marks;
- upload/copy bytes;
- passes, batches, draws, dispatches, and barriers;
- pipeline/cache behavior;
- CPU recording and available GPU timing; and
- device/surface recovery events.

Device loss invalidates backend resources but not the CPU retained scene. Owned mode may recreate a
compatible device and restore resources from the retained snapshot. Hosted mode reports loss and
waits for the host to supply a replacement context.

## 17. Vendor and closed-console backend contract

A private console backend is a normal source dependency compiled with its vendor SDK. It implements
the backend SPI, supplies shader artifact loading/build integration, maps host/device ownership,
provides presentation or host-target integration, and runs the conformance suite.

The open packages contain no proprietary identifiers or conditional code for unknown SDKs. A vendor
backend may add a typed extension package for platform-specific capabilities, but portable UI and
scene packages cannot depend on it.

The backend SPI carries a source API version and explicit capability schema version. Incompatible
changes fail at compile time or backend creation. Rust binary compatibility across compiler versions
is not promised.

“Adapter-ready” means the public backend SPI can be implemented in a private crate without modifying
core packages. “Supported” requires an operational backend on real vendor hardware. “Production-
qualified” additionally requires the vendor's correctness, performance, packaging, certification,
and lifecycle gates.

### 17.1 Backend package template

A backend is organized as focused modules rather than one renderer implementation file:

```text
telorgon-renderer-<backend>/
├── lib.rs          Exports, registration, and crate documentation
├── adapter.rs      Adapter discovery and capability reporting
├── device.rs       Owned and hosted device contexts
├── memory.rs       Native allocation and residency strategy
├── resource.rs     Buffers, textures, views, samplers, and lifetime registry
├── shader.rs       Artifact loading and interface validation
├── binding.rs      Native binding layout and cache lowering
├── pipeline.rs     Graphics/compute pipeline creation and caches
├── command.rs      Native command context and pass lowering
├── sync.rs         Completion, queue, and external synchronization
├── executor.rs     Render-plan execution and accounting
├── presenter.rs    Optional platform presentation implementation
├── interop.rs      Optional typed native/external-resource extensions
└── diagnostics.rs  Native errors, labels, timing, memory, and capture hooks
```

Small backends may combine genuinely inseparable modules, but adapter discovery, device ownership,
execution, presentation, and native interop remain separate responsibilities.

The backend author supplies one integration descriptor that connects device factories, capability
reporting, shader artifacts, executor construction, optional presenters, and conformance hooks. They
do not reimplement retained scenes, UI batching policy, application components, or shell behavior.

### 17.2 Backend porting workflow

1. Implement the trace-visible capability and limit report.
2. Implement owned or hosted device creation without presentation.
3. Pass resource, lifetime, usage, binding, and command conformance.
4. Load backend shader artifacts and pass offscreen visual conformance.
5. Add hosted render-area recording and multi-view tests.
6. Add a presenter if the platform permits Telorgon-owned presentation.
7. Add optional external-image, synchronization, HDR, compute, or bindless clusters individually.
8. Record native/lowered/fallback/unsupported status and performance evidence.

This sequence lets a closed backend become useful in a host engine without first exposing or
implementing a vendor presentation stack.

## 18. Conformance architecture

This section defines the backend-facing shape. Gate 6 fixes its exact evidence layers, shared test
matrix, visual comparison, hardware behavior, device profiles, waivers, and reports in
[Acceptance and qualification](ACCEPTANCE_AND_QUALIFICATION.md).

### 18.1 Trace backend

A non-rendering trace backend validates resource lifetimes, pass dependencies, binding compatibility,
usage transitions, target contracts, completion ordering, and deterministic plans. It does not
qualify image correctness or GPU performance.

### 18.2 Backend test suites

Every operational backend runs shared suites for:

- resource creation, updates, deferred destruction, and exhaustion behavior;
- pass dependency and usage-transition correctness;
- pipeline and shader-interface compatibility;
- analytic boxes, text, images, paths, clips, layers, blending, and materials;
- premultiplied alpha, sRGB, wide-gamut/HDR fallbacks, and target formats;
- direct, offscreen, subregion, multi-view, and multiple-window targets;
- owned submission and hosted command-only recording;
- resize, suspend/resume, surface loss, device loss, and recovery;
- external images and synchronization when capabilities advertise them;
- software/GPU image comparison within documented tolerances; and
- counters and timing integrity.

Backend-specific tests may add native validation layers, API debug modes, GPU capture, vendor tools,
memory-budget stress, and certification requirements.

### 18.3 Portability matrix

The backend report records each capability as native, lowered, fallback, unsupported, or untested.
Documentation must not use one “supported” checkmark that hides the difference.
These capability paths are metadata distinct from Gate 6's pass/fail/skip/unsupported/waived test
outcomes; neither vocabulary may be collapsed into a single checkmark.

## 19. Stabilization gates

The backend architecture is not stable until:

1. the Vulkan backend performs real rendering and presentation in owned mode;
2. Vulkan hosted command recording works inside a host frame schedule;
3. the trace backend validates plans and lifetime rules;
4. direct Metal implements the SPI without changes to scene packages and supplies hosted plus macOS
   arm64 evidence;
5. shared visual, resource, recovery, and multi-view conformance suites pass;
6. optional fallbacks have measured costs and diagnostics;
7. native interop remains isolated from portable APIs; and
8. an external/private backend implementation review confirms that proprietary code can remain
   outside the open workspace.

Console adaptability remains a design claim until a real vendor backend is implemented. The claim
must always be phrased separately from operational console support.

## 20. Open decisions after Gate 4

- static generic versus coarse type-erased runtime backend selection at public host boundaries;
- pipeline-cache serialization and host cache ownership;
- allocator integration without coupling to Vulkan allocation concepts;
- which post-baseline material source strategy should supply equivalent Metal, D3D12, and vendor
  artifacts while retaining the versioned bundle/interface contract;
- whether the Gate 3 opaque completion-domain contract needs refinement after a materially different
  Metal, D3D12, or console backend validates it;
- which optional capability clusters are required by shell/compositor profiles; and
- how vendor packages consume the conformance harness without exposing proprietary artifacts.
