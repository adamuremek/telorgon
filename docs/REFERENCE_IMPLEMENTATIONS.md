# Telorgon Reference-Implementation Study Guide

## Status and purpose

This document tells implementers how to use the downloaded source projects in
`../other-rendering-libs` as read-only engineering references. The goal is to learn established
invariants, failure handling, ownership rules, tests, and performance techniques before Telorgon
implements the same class of mechanism.

Reference code is evidence, not authority. It may have different product goals, compatibility
constraints, abstractions, licensing, or historical compromises. Vulkan, Metal, Direct3D, platform,
and vendor specifications remain the authority for API behavior. Telorgon's scope and backend
documents remain the authority for Telorgon's design.

## 1. Mandatory workflow

Use this workflow before changing a graphics or platform boundary:

1. **Name the concern.** Examples are swapchain recreation, non-coherent memory flushing, descriptor
   lifetime, hosted command recording, external synchronization, or damage-driven UI batching.
2. **Read Telorgon first.** Identify the applicable target contract, current implementation state, and
   code that calls or owns the mechanism.
3. **Inspect two references.** Use the routing matrix below. Prefer one low-level backend and one
   production UI/embedding system so the comparison is not circular.
4. **Trace complete lifetimes.** Read construction, normal use, error handling, resize/loss, deferred
   destruction, and tests. A single successful creation function is not enough.
5. **Cross-check the specification.** Confirm extension promotion, feature enablement, valid usage,
   ownership, synchronization, and platform restrictions in primary documentation.
6. **Extract invariants, not syntax.** Write down what must always be true, what can fail, who owns
   each object, and what must wait for what.
7. **Derive tests.** Every adopted failure-handling or lifetime rule should produce a Telorgon test,
   assertion, diagnostic, or hardware validation case.
8. **Implement the smallest Telorgon-shaped solution.** Do not import an entire abstraction because
   another project needed it.
9. **Record the audit.** Include the template from section 5 in the task handoff or an architecture
   note when the decision changes a lasting boundary.

If `../other-rendering-libs` is unavailable in another checkout, report that fact. Do not pretend a
reference review occurred. Continue only when the task is safe using Telorgon's documents and official
specifications, or ask for the source library to be restored when the review is required for a
high-risk mechanism.

## 2. Graphics-backend reference routing

All paths are relative to the Telorgon repository root.

### 2.1 Vulkan instance, adapter, device, and queues

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/instance.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/adapter.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/device.rs`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/context_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/capabilities_vk.cc`

Study loader/extension discovery, physical-device rejection and scoring, queue-family selection,
feature/property chains, required versus optional capabilities, debug setup, device-loss propagation,
and shutdown ordering. Compare wgpu's cross-backend HAL constraints with Impeller's UI-renderer
requirements rather than adopting either wholesale.

### 2.2 Swapchains, surfaces, frame acquisition, and recovery

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/swapchain/`
- `../other-rendering-libs/imgui/backends/imgui_impl_vulkan.cpp`
- `../other-rendering-libs/imgui/examples/example_win32_vulkan/main.cpp`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/`
- `../other-rendering-libs/base/libs/hwui/renderthread/VulkanSurface.cpp`

Study surface-capability queries, format/color-space/composite-alpha selection, present queues, image
count, zero extent, minimized/suspended windows, `SUBOPTIMAL`/`OUT_OF_DATE`, resize races, acquire and
present semaphores, per-image state, old-swapchain retirement, and failure recovery. Dear ImGui is a
useful minimal integration reference; it is not a model for Telorgon's retained renderer ownership.

### 2.3 Commands, barriers, synchronization, and frame reuse

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/command.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/semaphore_list.rs`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/barrier_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/command_buffer_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/command_pool_vk.cc`
- `../other-rendering-libs/base/libs/hwui/renderthread/VulkanManager.cpp`

Study semantic usage-to-barrier conversion, stage/access pairing, image layout state, queue ownership,
timeline and binary semaphore roles, fence reuse, command-pool reset safety, frames in flight,
completion-based deferred destruction, and the avoidance of device-wide idle waits. Look for tests
covering unusual submission order and resource reuse, not only happy-path command encoding.

### 2.4 Memory, buffers, images, uploads, and descriptors

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/device.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/descriptor.rs`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/allocator_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/device_buffer_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/descriptor_pool_vk.cc`
- `../other-rendering-libs/egui/crates/egui-wgpu/src/renderer.rs`

Study memory-type selection, mapped-memory coherency, flush/invalidate alignment, suballocation,
dedicated allocation, staging-ring reuse, partial texture uploads, row alignment, buffer growth,
descriptor-pool exhaustion, descriptor lifetime, texture replacement, sampler reuse, resource
labeling, and memory-budget diagnostics. Egui is especially useful for small UI texture deltas and
renderer callbacks; wgpu and Impeller are stronger lifetime/failure references.

### 2.5 Pipelines, shaders, batching, and render passes

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/device.rs`
- `../other-rendering-libs/imgui/backends/vulkan/`
- `../other-rendering-libs/egui/crates/egui-wgpu/src/renderer.rs`
- `../other-rendering-libs/vello/vello/src/`
- `../other-rendering-libs/vello/vello_shaders/`
- `../other-rendering-libs/qtdeclarative/src/quick/scenegraph/`

Study pipeline-layout compatibility, shader/binding ABI validation, premultiplied alpha, sRGB and
linear targets, dynamic rendering/pass boundaries, pipeline caches, ordered batching, scissor and
clip handling, texture binding changes, intermediate targets, and GPU-driven vector work. Vello's
compute-heavy vector pipeline is an optional capability reference, not the baseline Telorgon UI path.

### 2.6 External images, mobile surfaces, and shell composition

Primary references:

- `../other-rendering-libs/flutter/engine/src/flutter/shell/platform/embedder/`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/`
- `../other-rendering-libs/base/core/java/android/view/SurfaceControl.java`
- `../other-rendering-libs/base/core/java/android/hardware/HardwareBuffer.java`
- `../other-rendering-libs/base/core/java/android/hardware/SyncFence.java`
- `../other-rendering-libs/base/core/jni/android_hardware_HardwareBuffer.cpp`
- `../other-rendering-libs/base/libs/hwui/renderthread/VulkanSurface.cpp`

Study producer/consumer ownership, external format and plane metadata, acquire/release fences,
hardware-buffer lifetime, protected content, transform/crop/damage metadata, surface replacement,
composition transactions, callback lifetime, and explicit unsupported paths. Protocol semantics and
window policy remain outside Telorgon even when Android or Flutter provides a useful composition
example.

### 2.7 Cross-API backend boundary

Primary references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/lib.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/`
- `../other-rendering-libs/wgpu/wgpu-hal/src/metal/`
- `../other-rendering-libs/wgpu/wgpu-hal/src/dx12/`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/metal/`
- `../other-rendering-libs/qtdeclarative/src/quick/scenegraph/adaptations/`
- `../other-rendering-libs/zed/crates/gpui_windows/src/directx_renderer.rs`
- `../other-rendering-libs/zed/crates/gpui_wgpu/src/wgpu_renderer.rs`

Compare which concepts remain common and which are backend-specific: resource usage, heap/storage
classes, descriptor/argument binding, render-pass encoding, queue completion, presentation,
timestamps, shader artifacts, native handles, and device loss. The review must identify where an
apparently Vulkan-neutral type would force an awkward Metal, D3D12, or private-console mapping.

Do not make wgpu Telorgon's reference renderer merely because its HAL is informative. Telorgon's direct
Vulkan backend remains the operational reference; wgpu is a source of tested variation points and
may later be a separate adapter.

## 3. UI-runtime and rendering-structure routing

### 3.1 Renderer/platform separation and embedding

- `../other-rendering-libs/slint/internal/renderers/`
- `../other-rendering-libs/slint/internal/backends/winit/`
- `../other-rendering-libs/flutter/engine/src/flutter/shell/platform/embedder/`
- `../other-rendering-libs/egui/crates/egui-wgpu/src/winit.rs`
- `../other-rendering-libs/egui/crates/epaint/src/shapes/paint_callback.rs`
- `../other-rendering-libs/qtdeclarative/src/quick/scenegraph/`
- `../other-rendering-libs/zed/crates/gpui/src/platform/`

Study renderer selection without core coupling, event-loop ownership, host callbacks, custom render
areas, frame requests, platform surface loss, embedded GPU work, and how external content enters a UI
scene. Keep Telorgon's owned and hosted entry points explicit rather than merging them behind hidden
global behavior.

### 3.2 Retained UI, invalidation, and incremental work

- `../other-rendering-libs/xilem/xilem_core/`
- `../other-rendering-libs/xilem/masonry/`
- `../other-rendering-libs/slint/internal/core/`
- `../other-rendering-libs/egui/crates/epaint/src/`
- `../other-rendering-libs/support/compose/runtime/`
- `../other-rendering-libs/support/compose/ui/`
- `../other-rendering-libs/react-native/packages/react-native/ReactCommon/react/renderer/`

Study identity, reconciliation, dirty propagation, layout/paint separation, frame scheduling,
virtualization, semantics, input routing, and compact render data. Immediate-mode systems can still
inform painting and texture update mechanics, but they do not override Telorgon's mounted retained
runtime.

### 3.3 Application and shell primitive domains

- `../other-rendering-libs/support/compose/foundation/`
- `../other-rendering-libs/support/compose/material3/`
- `../other-rendering-libs/flutter/packages/flutter/lib/src/widgets/`
- `../other-rendering-libs/flutter/packages/flutter/lib/src/material/`
- `../other-rendering-libs/base/packages/SystemUI/`
- `../other-rendering-libs/base/libs/WindowManager/Shell/`
- `../other-rendering-libs/qtdeclarative/src/quickcontrols/`
- `../other-rendering-libs/qtdeclarative/src/quicktemplates/`

Use application toolkits to study control composition, accessibility, density, touch targets, and
adaptive layout. Use SystemUI/window-shell sources to study trusted chrome, panels, outputs,
workspaces, overlays, and surface-host boundaries. Preserve Telorgon's rule that application and shell
are separate public design domains over shared foundations, with behavior-bearing controls and
shell facilities classified as components rather than mislabeled as low-level primitives. Gate 8's
exact routing and acceptance matrix is in
[Application and shell primitives](APPLICATION_AND_SHELL_PRIMITIVES.md).

## 4. Required pitfall checklist

A graphics implementation review must explicitly consider applicable items from this list:

- missing instance/device extension or feature enablement;
- queue-family selection that works for graphics but not presentation;
- zero-sized, minimized, suspended, lost, suboptimal, or out-of-date surfaces;
- swapchain resources destroyed while frames still reference them;
- image-layout, stage-mask, access-mask, or queue-ownership mistakes;
- command pool, descriptor pool, staging range, or transient target reused before completion;
- non-coherent mapped memory not flushed/invalidated at required alignment;
- memory type, heap budget, dedicated allocation, or resource aliasing mistakes;
- descriptor exhaustion, stale bindings, or texture replacement during in-flight frames;
- pipeline/shader binding-layout mismatch;
- sRGB/linear confusion, incorrect premultiplication, composite alpha, or channel order;
- incorrect row pitch, copy alignment, glyph atlas format, or partial upload bounds;
- device loss, allocation failure, shader/pipeline failure, and unavailable optional capability;
- hidden CPU readback, full-frame upload, device-wide idle, independent submission, or worker thread;
- native handle destroyed by the wrong owner;
- external image used without acquire/release synchronization or retained beyond its contract;
- hosted target initial/final usage or unrelated pixels not preserved;
- damage, clipping, transforms, or painter order invalidated too broadly or too narrowly; and
- diagnostics or counters inferred from modeled CPU state instead of performed GPU operations.

Do not mark an item “handled” merely because a reference project has code for it. State how Telorgon
handles it and name the test or invariant that prevents regression.

## 5. Reference-audit template

Use this compact template in the task handoff. Put it in a lasting architecture note when the
decision changes a cross-package contract.

```text
Concern:
Telorgon files/contracts affected:
Reference revisions, paths, and symbols inspected:
Official specification sections checked:
Invariants extracted:
Failure and recovery cases extracted:
Approaches rejected and why:
Telorgon-specific decision:
Tests/diagnostics derived:
Known gaps requiring hardware or vendor validation:
```

Record a reference repository's commit identifier when available so later readers can reproduce the
review. Paths alone are insufficient if the downloaded sources change.

## 6. Completed cross-package audits

### 6.1 Gate 4 scene-to-GPU and shader boundary

```text
Concern:
Stable retained-scene updates, compact GPU data, painter-order batching, shader-visible layouts,
partial uploads, color/alpha correctness, and safe transient-resource reuse.

Telorgon files/contracts affected:
SCENE_GPU_ABI_AND_SHADERS.md; target telorgon-scene, telorgon-render, telorgon-gpu-abi,
telorgon-renderer-vulkan, and telorgon-shader-build packages.

Reference revisions, paths, and symbols inspected:
Egui fd54387eac03f57ca772a8fb590ceaadf780f31c — epaint/src/mesh.rs Vertex/Mesh;
egui-wgpu/src/renderer.rs texture updates, buffers, bind groups, scissor, draw order, and blend state;
egui-wgpu/src/egui.wgsl coordinate, packed-color, gamma, and output entry points.
Flutter Impeller 51fd9afadf309ba5337320bd3653f5345c156cb9 — core/vertex_buffer.h,
core/host_buffer.h, core/shader_types.h, renderer/pipeline_descriptor.h,
entity/contents/solid_color_contents.cc, entity/contents/texture_contents.cc, solid-fill and
glyph-atlas shaders.
wgpu d99c241a3b9dcc0f6674d990d007d79e94d39862 — HAL shader/layout and backend variation concepts.

Official specification sections checked:
Vulkan shader interfaces, descriptor sets/pipeline layout compatibility, formats, framebuffer and
blending, Vulkan SPIR-V environment, shader memory layouts, image copies, and synchronization2
examples; unified SPIR-V specification.

Invariants extracted:
Painter order remains authoritative; partial texture and buffer updates are bounds-checked; stable
bindings change less often than per-image bindings; transient storage is not recycled while busy;
pipeline identity includes target/state/interface compatibility; shader metadata must agree with
host layout; shader output is linear premultiplied before blending.

Failure and recovery cases extracted:
Buffer growth, sampler/texture replacement, invalid source or destination rectangles, optimized-out
shader bindings, unavailable format features, non-coherent host writes, resource read-to-overwrite
transitions, and attachment-to-sampled transitions.

Approaches rejected and why:
Flattening every retained primitive every frame loses stable-slot update benefits; putting pipeline
keys in the scene couples it to backend decisions; native-endian packed color is not a byte ABI;
runtime reflection/compilation adds startup/toolchain risk; global texture sorting violates painter
order; hidden transfer submission violates hosted ownership.

Telorgon-specific decision:
Typed generational scene slots and epoch deltas; separate u32 draw indirection; adjacent-only
batching; exact repr(C) telorgon-gpu-abi records; four-set Vulkan baseline; offline validated and
reflected shader bundle; linear premultiplied rendering; semantic use lowering under Gate 3
completion ownership.

Tests/diagnostics derived:
ABI offset and reflection goldens, corrupt-bundle rejection, delta atomicity and slot-reuse tests,
partial-upload and atlas-growth tests, adjacent-batching tests, shader color math goldens, trace use
validation, hardware conformance images, and actual byte/copy/barrier/draw diagnostics.

Known gaps requiring hardware or vendor validation:
Format/filter support across target adapters, compiler/driver interface behavior, visual tolerances,
performance of storage-buffer indirection, non-Vulkan binding translations, HDR, and proprietary
console artifact pipelines.
```

### 6.2 Gate 5 platform implementation order

Gate 5 compared Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` platform/window/Winit
boundaries with Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` embedder renderer,
task-runner, compositor, multi-view, lifecycle, and shutdown contracts. It cross-checked Winit 0.30.13
platform/lifecycle documentation, Vulkan WSI, Android Activity guidance, and Apple Metal drawable
guidance.

The audit fixes Windows Vulkan -> hosted Vulkan -> separate Linux Wayland/X11 -> protocol-neutral
Linux shell, then direct Metal/macOS -> shared mobile foundation -> Android Vulkan and iOS Metal.
The complete concern, symbols, invariants, failure cases, rejected approaches, tests, and primary
links are recorded in
[Platform implementation order](PLATFORM_IMPLEMENTATION_ORDER.md#18-gate-5-reference-audit).

### 6.3 Gate 6 acceptance and qualification

Gate 6 compared wgpu revision `d99c241a3b9dcc0f6674d990d007d79e94d39862` compile-fail, no-GPU
validation/trace, multi-adapter GPU, capability/expectation, report, and image-comparison test
layers with Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` Impeller backend-tagged
golden/screenshot infrastructure and Vulkan embedder native-call interception tests. It
cross-checked Khronos Vulkan core, synchronization, and GPU-assisted validation guidance plus the
Vulkan CTS/conformant-product boundary.

The audit fixes Telorgon's E0–E9 evidence layers, explicit result outcomes, developer versus
qualification hardware behavior, exact/edge-masked linear-premultiplied goldens, structural
performance invariants, device/profile reports, and narrow expiring waivers. The complete paths,
symbols, derived failures, rejected approaches, and primary links are recorded in
[Acceptance and qualification](ACCEPTANCE_AND_QUALIFICATION.md#15-gate-6-reference-and-specification-audit).

### 6.4 Gate 7 authoring and component runtime

Gate 7 compared Xilem revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` component/view state,
rebuild, teardown, message, keyed-sequence, task-abort, and controlled-text behavior with Slint
revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` property dependency tracking, lazy invalidation,
event-loop future polling, and cancellation. Flutter revision
`51fd9afadf309ba5337320bd3653f5345c156cb9` supplied lifecycle, keyed child-update, editable-text,
selection, and composition failure comparisons. Rust's `Future`/`Waker` contracts and Unicode UAX
No. 29 supplied the executor-neutral polling and grapheme/word-boundary rules.

The audit fixes Telorgon's mount-once component scopes, owner-scoped state and read dependencies,
typed upward actions, explicit keyed structural containers, deterministic teardown, host-executor
task scopes, and revisioned UTF-8 text-editing boundary. The complete paths, rejected alternatives,
derived failures, tests, and primary links are recorded in
[Authoring and component runtime](AUTHORING_AND_COMPONENT_RUNTIME.md#17-gate-7-reference-and-specification-audit).

### 6.5 Gate 8 application and shell primitives

Gate 8 compared Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` button/check/radio/
slider/tabs/menu/editable-text behavior with AndroidX Compose support revision
`491d5b9a1de8225097e39684c3412f40f227a0f7` clickable/selectable/toggleable semantics,
`TextFieldState`, and lazy layout. Qt Declarative revision
`3e2d6bd456a8e850bcf641de77d1d5d8bc8419ef` supplied abstract-button, combo-box, popup placement,
close, and focus-restoration failure comparisons. Android platform base revision
`1cdfff555f4a21f71ccc978290e2e212e2f8b168` supplied SystemUI model/view-model separation and
WindowManager Shell policy, external task surface, chrome input-region, and snap-feedback examples.
The audit cross-checked WAI-ARIA 1.2, current WAI-ARIA Authoring Practices control/keyboard patterns,
and WCAG 2.2 focus, pointer, dragging-alternative, concurrent-input, and target-size criteria.

The audit fixes Telorgon's foundation/primitive/component/facility cut, controlled value/actions,
specialized controllers, universal activation/focus/semantics behavior, adaptive density/input,
overlay/text/virtualization contracts, catalog tiers, and host-authoritative protocol-neutral shell
models/requests. The complete paths, rejected alternatives, failures, tests, and official links are
recorded in
[Application and shell primitives](APPLICATION_AND_SHELL_PRIMITIVES.md#15-gate-8-reference-and-specification-audit).

### 6.6 Gate 9 platform integration

Gate 9 compared Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144`
Winit event-loop/input/IME/AccessKit integration, Slint revision
`69ecb713f5c62d1b6fe986ff822a57f22152b4d9` platform/window-adapter/deadline/clipboard/
accessibility separation, and Flutter engine revision
`51fd9afadf309ba5337320bd3653f5345c156cb9` embedder task-runner, metrics, multi-view,
semantics/action, platform-message, and external-texture ownership.

The audit cross-checked Winit and raw-window-handle lifetime/event contracts; AccessKit adapter
activation/callback rules; Android lifecycle, `InputConnection`, and accessibility provider APIs;
UIKit/AppKit lifecycle, text, accessibility, pasteboard, and drag/drop APIs; Windows UI Automation
and clipboard provider behavior; and Vulkan external memory/synchronization, DMA-BUF, and DRM
modifier extensions. It fixes independent lifecycle axes, queued non-reentrant callbacks, lossless
input translation, revisioned native text conversion, per-view semantics, multi-format data offers,
narrow capability-checked services, borrowed handles, linear external-resource imports, and opaque
protocol user-gesture grants. The complete decisions, revisions, tests, and implementation blueprint
are recorded in [Platform integration contract](PLATFORM_INTEGRATION_CONTRACT.md).

### 6.7 Vulkan Slice 4 retained-resource implementation audit

Concern:
Persistent scene buffers, partial uploads, command/descriptor reuse, ordered batching, and safe
retirement for the first real Vulkan primitive path.

Reference source, revision, and symbols reviewed:

- egui revision `fd54387e` in
  `other-rendering-libs/egui/crates/egui-wgpu/src/renderer.rs`: persistent vertex/index storage,
  geometric capacity growth, and stable binding reuse; and
- Flutter engine revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in Impeller's Vulkan
  command-pool recycler and its tests: command pool/buffer reset only after completion and bounded
  resource recycling.

Official contracts cross-checked:
Khronos Vulkan [command-buffer lifecycle/reset rules](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html),
[synchronization2 examples](https://docs.vulkan.org/guide/latest/synchronization_examples.html),
and [buffer-copy command requirements](https://docs.vulkan.org/spec/latest/chapters/copies.html).

Invariants extracted:
Device-local scene storage survives ordinary frames; mapped staging belongs to a completion-gated
frame slot; growth uploads the complete live mirror and pins old storage through its last submission;
descriptor contents change only after their slot is reusable; draw-index order stays authoritative;
and only adjacent compatible items merge.

Failure and recovery cases extracted:
Insufficient fixed staging or device-local budget returns typed exhaustion; a busy frame slot is not
reset; a dropped pending receipt transfers pins to deferred retirement; allocation or recording
failure retains CPU dirty state for retry; and buffer-handle reuse cannot bypass descriptor refresh.

Approaches rejected and why:
Per-frame command/descriptor creation creates native churn; host-visible scene buffers weaken the
intended residency path; replacing an in-use buffer without a completion pin is unsafe; global batch
sorting changes painter order; and hidden transfer submissions or waits violate the ownership
contract.

Telorgon-specific decision:
Each Vulkan scene owns geometric device-local box, spatial, and draw-index buffers plus CPU mirrors
and coalesced dirty ranges. Each device preallocates bounded frame slots containing one command pool,
command buffer, descriptor arena, and mapped staging allocation. Rendering records synchronization2
barriers and grouped `vkCmdCopyBuffer2` regions in the caller's frame, caches descriptor bindings by
scene plus buffer generation, emits vertexless instanced draws for adjacent batches, and derives
runtime counters from the recorded operations. Completion receipts or device-owned retirement lists
retain every referenced allocation.

Tests and diagnostics derived:
Geometric-growth, dirty-range, adjacent-batching, growth-validation, typed-budget, and 10,000-box
single-record-update tests run portably. The ignored E4 test checks real copies, allocations,
descriptor reuse, warmed zero-upload behavior, pixels, and validation; the Windows E5 test checks the
same reusable frame lifecycle through presentation.

Known gaps requiring hardware or vendor validation:
The extended E4 and post-rewrite E5 tests pass on developer hardware with zero validation errors.
Storage-buffer indirection performance and allocator behavior still require qualification profiles,
and the new glyph, image, clip, and material paths require their explicit Slice 5 hardware runs.
Hosted, external-image, effect, and non-Vulkan paths remain later slices.

### 6.8 Vulkan Slice 5 visual-coverage implementation audit

Concern:
Mixed painter-order rendering, stable primitive/texture bindings, sampled-image lifetime, clip and
spatial lookup, color/alpha conventions, and comparison with the software reference.

Invariants applied:
One global draw-index stream remains authoritative across pipeline changes; descriptor contents used
by an earlier draw are never rewritten for a later draw; raw clip/spatial IDs index explicit tables
rather than dense-instance positions; sampled resource replacement preserves the prior image while a
submission may still reference it; authoring sRGB is decoded before premultiplied blending; and a
batch clip/resource key must match its primitive.

Telorgon-specific decision:
Each reusable frame slot owns one view set, one scene set, four stable primitive sets, and a bounded
sampled-image descriptor arena. The retained scene owns optional device-local buffers for each live
primitive family, an R8 glyph atlas, versioned RGBA resources, built-in material parameter words,
and CPU mirrors needed for dirty updates and copy-on-write replacement. Compiler geometry is already
view-space after scroll and transform resolution, so compiler-emitted spatial records are identity;
manually-authored scenes can still use axis-aligned spatial records.

Tests and boundary:
Portable tests cover mixed ordering, glyph/image/material rasterization, clipping, spatial
translation, opacity, resource validation, shader hashes, and workspace regressions. Ignored
hardware tests cover a targeted mixed scene and the current Gallery against software readbacks. They
are compiled in ordinary verification but remain pending developer-hardware execution. General
material graphs, shadows/filters, clip masks, external images, hosted recording, and cross-vendor
qualification are outside this slice.

No external source was copied into Telorgon during these audits. The references supplied invariants,
failure cases, and comparison points for independently written contracts.

### 6.9 Linux DMA-BUF negotiation and FD ownership audit

Concern:
Linux DMA-BUF format/modifier negotiation, exact-use capability checks, importing owning memory and
sync FDs, and preserving a narrow profile that can be compile-verified before Linux hardware is
available.

Reference source, revision, and symbols reviewed:

- wgpu revision `d99c241a3b9dcc0f6674d990d007d79e94d39862` in
  `other-rendering-libs/wgpu/wgpu-hal/src/vulkan/device.rs`:
  `texture_from_dmabuf_fd` and `import_dmabuf_memory`; and
- Flutter engine revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in Impeller's
  `android/ahb_texture_source_vk.cc`, `swapchain/ahb/external_semaphore_vk.cc`, and
  `swapchain/ahb/ahb_swapchain_impl_vk.cc`: external-image creation plus temporary sync-FD import
  and one-shot export ownership.

Official contracts cross-checked:
Khronos Vulkan [`VK_EXT_image_drm_format_modifier`](https://registry.khronos.org/vulkan/specs/latest/man/html/VK_EXT_image_drm_format_modifier.html),
[`VkPhysicalDeviceImageDrmFormatModifierInfoEXT`](https://registry.khronos.org/vulkan/specs/latest/man/html/VkPhysicalDeviceImageDrmFormatModifierInfoEXT.html),
[`VkImportMemoryFdInfoKHR`](https://registry.khronos.org/vulkan/specs/latest/man/html/VkImportMemoryFdInfoKHR.html),
and [`VkImportSemaphoreFdInfoKHR`](https://registry.khronos.org/vulkan/specs/latest/man/html/VkImportSemaphoreFdInfoKHR.html).

Invariants extracted:
Negotiation returns exact DRM-fourcc/modifier tuples and is followed by an image-format query for the
actual usage; import repeats those checks rather than trusting a stale advertised record; only a
successful Vulkan memory/semaphore import consumes the respective FD; sync-FD semaphore imports are
temporary; and an exportable tuple must be selected when a test producer and consumer share the
same fixture.

Failure and recovery cases extracted:
Unsupported modifiers or usage, invalid modifier sentinels, plane-count/layout disagreement,
out-of-bounds damage, no compatible memory type, Vulkan allocation/import failure before FD
consumption, and repeated release export.

Approaches rejected and why:
Assuming `DRM_FORMAT_MOD_LINEAR` bypasses real producer/consumer negotiation; advertising every
enumerated modifier without the secondary external-image query can return unusable tuples; treating
all-ones as Linux's invalid modifier sentinel encodes the kernel ABI incorrectly; and exposing
multi-plane/YUV records before descriptor conversion and per-plane binding exist would make a false
capability claim.

Telorgon-specific decision and tests:
The hosted Linux device exposes deterministic single-memory-plane RGBA/BGRA capability records for
an exact requested usage and retains import/export/dedicated-only flags and maximum extent. Portable
metadata/release-state tests cover invalid sentinels, plane indices, layout and damage failures, plus
one-shot export. A Linux-only owning-FD drop test and the negotiated E8 hardware fixture compile for
`x86_64-unknown-linux-gnu`; the latter remains deliberately unexecuted until Linux hardware is
available. No reference source was copied.

### 6.10 Per-view environment read publication audit

Concern:
Publishing one validated per-view environment into component dependencies without ambient global
state, duplicated validation ownership, torn aspect updates, or broad downstream invalidation.

Reference source, revision, and symbols reviewed:

- Xilem revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `xilem_core/src/environment.rs`: the driver-owned `Environment`, typed resource slots, and
  `Provides::{build,rebuild,teardown,message}` scoped replacement/restoration; and
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/context.rs` and `internal/core/properties.rs`: separate context properties,
  dependency-registering `Property::get`, and equality-suppressing `Property::set`.

Invariants extracted:
One view/driver owns environment publication; descendants receive read access rather than a second
mutable owner; independent environment concerns use independent dependency outputs; equal values do
not dirty consumers; and scoped handles cannot silently become process-global platform state.

Approaches rejected and why:
A generic ambient type map would hide dependencies and repeat Xilem's documented mutable-resource
limitation; six writable runtime states could tear one accepted environment update; copying the
complete value set into every aspect would duplicate variable-size occlusion data; and putting
platform conversion into the primitive package would reverse the Gate 9 adapter boundary.

Telorgon-specific decision and tests:
`EnvironmentState` remains the sole validating owner. `EnvironmentReadBinding` retains one shared
immutable snapshot in owner-scoped runtime state and derives six aspect wrappers that share snapshot
storage while comparing only their matching fields. Publication verifies the reported change set
and contiguous revision before staging the single source. Portable tests prove coherent combined
reads, selective observer output, unchanged suppression, stale/gap rejection, and cross-owner
failure. No reference source was copied.

### 6.11 Platform identity implementation audit

Concern:
Representing host-owned views, data offers, admitted requests, and native-surface continuity without
native handles, process-global allocation, stale-slot aliasing, or cross-domain ID reuse.

Reference source, revision, and symbols reviewed:

- Xilem revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `xilem_core/src/view_ctx.rs` and `xilem_core/src/view_sequences/impl_vec.rs`: owner-relative
  `ViewId`, packed index/generation routing, stale-message comparison, slot reuse, and generation
  overflow handling; and
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/partial_renderer.rs`: cache-local indices paired with a generation that advances
  when resource continuity is lost.

Invariants extracted:
An identity is meaningful only in the map/host that issued it; slot reuse must change generation;
stale values compare unequal before lookup; distinct domains use distinct Rust types; and native
resource replacement advances a logical generation instead of treating handle bits as continuity.

Failure cases and rejected approaches:
Zero cannot be a valid slot, generation, request, or available-surface value. A packed value with a
wrapping generation can eventually alias stale work, so the value type does not provide a wrapping
successor; the future allocation owner must reject exhaustion. A process-global atomic would hide
the issuing owner, raw Winit/protocol/pointer values would leak adapters, and duplicating
`TextSessionId` or future accessibility-node identity would reverse their existing package owners.

Telorgon-specific decision and tests:
`ViewId` and `DataOfferId` retain two private nonzero 32-bit fields. `RequestId` and
`NativeSurfaceGeneration` are distinct private nonzero 64-bit values. The identity module uses no
crate dependency and performs no allocation. Portable tests prove zero rejection,
stale-generation inequality, owner-map behavior, compact optional layout, thread-transferable value
traits, direct focused/root exports, and umbrella compilation. No reference source was copied.

### 6.12 Host-stream event stamp implementation audit

Concern:
Giving every platform-to-runtime event deterministic arrival order and same-domain monotonic timing
without letting neutral value packages read ambient time, convert native timestamps, or own an event
queue.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/animations.rs`, `internal/core/platform.rs`, and `internal/core/input.rs`:
  `animations::Instant`, `Platform::duration_since_start`, `Instant::now`, and
  `ClickState::check_repeat`; and
- Xilem revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs` and `masonry_core/src/util.rs`:
  `EventLoopRunner::redraw`, its ambient `Instant::now` sample, and the documented paint-time jitter
  limitation.

Invariants extracted:
A monotonic value names one host-selected clock domain; the platform boundary supplies clock
samples; values from different domains are not comparable; event order cannot rely on timestamp
resolution alone; and elapsed-time consumers must receive neutral values rather than obtain wall
time implicitly.

Failure cases and rejected approaches:
Two events can share one receipt instant, so timestamps alone cannot provide strict ordering. An
ambient `std::time::Instant::now` call inside stamping would hide the host clock and prevent
deterministic fixtures. Using the optional native source timestamp as order would fail when a source
clock is absent, unmappable, coarse, or reordered. A cloneable sequencer could fork and issue the
same next sequence twice. Wrapping at `u64::MAX` would make stale and new stream positions alias.

Telorgon-specific decision and tests:
The existing `MonotonicInstant` representation moved from the runtime timer module to neutral
`telorgon-core`; the runtime compatibility path and platform stamp path therefore use one exact Rust
type. A non-cloneable `EventStampStream` owns only the last accepted stamp, assigns sequences from
one, permits equal receipt instants, validates optional source time in the same domain, and leaves
state unchanged on regression, invalid mapping, or exhaustion. Portable tests cover strict order,
clock-resolution ties, source presence/absence, error atomicity, no-wrap exhaustion, direct/root
exports, and runtime compatibility. No reference source was copied.

### 6.13 Platform capability value implementation audit

Concern:
Describing service support, unavailable causes, permissions, limits, required execution context,
and recent-user-gesture policy without hiding optional behavior behind no-ops, importing an adapter,
or allowing a neutral query to perform the operation it describes.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/platform.rs` and `internal/core/api.rs`: optional
  `Platform::new_event_loop_proxy`, clipboard defaults, `Platform::open_url`, and
  `PlatformError::{NoPlatform, NoEventLoopProvider, Unsupported}`;
- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`:
  `FlutterTaskRunnerDescription`, `FlutterCustomTaskRunners`, required thread association, and
  embedder-supplied platform/render/UI runners; and
- React Native revision `2d427ba77bbf17bc487e25bef4d011097ba4fff5` in
  `packages/react-native/Libraries/PermissionsAndroid/PermissionsAndroid.js`:
  `PermissionStatus` and distinct granted, denied, and never-ask-again request results.

Invariants extracted:
Adapter presence, platform support, event-loop/executor availability, permission state, and current
scope availability are different facts. Required execution context must be declared before an
operation is admitted. Permission denial does not prove that a service is unsupported. Service-
specific operations and limits remain typed, and a query result must not invoke or silently no-op
the operation it describes.

Failure cases and rejected approaches:
A boolean loses unavailable cause, permission, limits, and execution requirements. Treating denied
permission as unsupported prevents later prompting or settings recovery. A string-keyed capability
map erases operation/limit types. `None` or a silent no-op cannot distinguish an absent adapter from
empty service data. Treating an unknown maximum as unbounded can admit unsafe payload sizes.
Creating an executor or prompting permission during a query makes discovery stateful and unsuitable
for embedded hosts.

Telorgon-specific decision and tests:
`Support<T>` carries either one closed unavailable reason or the exact typed descriptor.
`CapabilityDescriptor<Operations, Limits>` retains private typed operation/limit payloads plus
permission, execution, and recent-user-gesture requirements. `CapabilityLimit<T>::Unspecified`
makes no unlimited claim, while `NoCapabilityLimits` explicitly marks services without a limit
record. Portable tests prove permission/support separation, unavailable reason preservation without
payload construction, typed bounded limits, explicit executor/gesture requirements, mapping
behavior, thread-transferable values, and direct/root exports. No reference source was copied.

### 6.14 Platform lifecycle axes implementation audit

Concern:
Representing view lifetime, application activity, visibility, and native-surface continuity without
one combined state that creates false coupling, while still making redundant callbacks, terminal
closure, and surface-generation replacement explicit.

Reference source, revision, and symbols reviewed:

- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: `ApplicationHandler::{resumed,suspended}` and
  `MasonryState::{handle_resumed,handle_suspended}`;
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/backends/winit/event_loop.rs` and `internal/backends/winit/lib.rs`:
  `ApplicationHandler::{resumed,suspended}`, `WindowEvent::Occluded`, and the renderer
  `resumed`/`suspended` handling; and
- React Native revision `2d427ba77bbf17bc487e25bef4d011097ba4fff5` in
  `packages/react-native/ReactAndroid/src/main/java/com/facebook/react/bridge/ReactContext.java`:
  `LifecycleState` plus `ReactContext::{onHostResume,onHostPause,onHostDestroy}`.

Invariants extracted:
Lifetime, activity, visibility, surface availability, and presenter readiness are separate facts.
Native callbacks may repeat. Hidden, occluded, background, or suspended does not universally imply
that the native surface was destroyed. Closed view lifetime is terminal. Replacing or recreating a
native surface must advance an opaque generation so work for a retired surface cannot bind to its
replacement.

Failure cases and rejected approaches:
A combined lifecycle enum creates a Cartesian state space and incorrectly makes legal axis
combinations unreachable. Treating every occlusion or suspension as surface loss does not match all
platforms. Treating a resumed callback as proof of visibility or presentation readiness conflates
distinct owners. Pointer-derived identities, generation reuse, and wrapping sequences can revive
stale work. Closing a view while its surface remains current leaves retirement ordering ambiguous.
Lifecycle observation must not destroy native objects, drive a presenter, invoke callbacks, or
choose process exit.

Telorgon-specific decision and tests:
Four small value enums and one non-cloneable `ViewLifecycle` owner accept observations through
separate methods. Lifetime transitions are forward-only and terminal, equal observations are
idempotent, and other axes remain independent until closure. The owner remembers its latest surface
generation after retirement, requires a strictly newer replacement, and requires retirement before
closed lifetime commits. Rejections are atomic. Portable tests cover independent suspended/hidden
surface combinations, redundant native observations, invalid lifetime movement, post-close
changes, surface-generation reuse/backward movement, close ordering, and direct/root exports. No
reference source was copied.

### 6.15 Platform view publication and close protocol implementation audit

Concern:
Publishing identity-scoped lifecycle facts coherently and distinguishing a cancellable close
request from host-enforced native destruction without letting either neutral value execute native
cleanup or choose application exit.

Reference source, revision, and symbols reviewed:

- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs` and `masonry_winit/src/app_driver.rs`:
  `WinitWindowEvent::CloseRequested`, `AppDriver::on_close_requested`, and the default
  `DriverCtx::exit` behavior;
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/window.rs` and `internal/backends/winit/event_loop.rs`:
  `Window::request_close`, `CloseRequestResponse::{HideWindow,KeepWindowShown}`, and routed
  `WindowEvent::CloseRequested`; and
- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`: `FlutterAddViewInfo`,
  `FlutterRemoveViewInfo`, `FlutterRemoveViewResult`, and the view-scoped asynchronous callbacks.

Invariants extracted:
Every publication and close fact must cite one stable view identity. A complete snapshot changes as
one revision rather than exposing individually torn axes. Repeated native observations must not
invent revisions. A close request enters application routing and may be refused; native destruction
is observed truth and cannot be vetoed. View removal is distinct from process shutdown, especially
for multi-view and embedded hosts.

Failure cases and rejected approaches:
Public snapshot fields let portable code invent platform truth and make additive metrics/focus
fields source-breaking. Advancing a revision for a redundant callback creates false changes.
Mutating the lifecycle before discovering revision exhaustion tears the publication. Treating close
request as destruction prevents cancellation or deferral; treating forced destruction as a request
permits an impossible rejection. Xilem's default exit-on-close hook is not a neutral multi-view
policy. A close value must not own native deletion, cancellation fan-out, callback execution, or
event-loop exit.

Telorgon-specific decision and tests:
One non-cloneable `ViewState` owns one identity, the existing independent lifecycle owner, and one
nonzero revision. It validates lifecycle and revision advancement before committing, captures
private-field immutable before/after snapshots, advances once for a real change, and preserves the
exact publication for redundant or failed observations. `CloseRequest` and
`CloseRequestDecision` form the cancellable path; `ForcedDestruction` and its phase form a separate
unanswerable fact. Both cite the exact observed view revision and perform no action. Portable tests
cover coherent snapshots, revision continuity and exhaustion, error atomicity, independent axes,
close-path type separation, thread-transferable values, and direct/root exports. No reference
source was copied.

### 6.16 Platform view metrics implementation audit

Concern:
Publishing coherent physical/logical geometry, scale, safe/avoid regions, output transform, color,
HDR, and renderability without accepting malformed native values, losing coordinate-space meaning,
or creating a second revision stream that can tear from the enclosing view snapshot.

Reference source, revision, and symbols reviewed:

- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`: `FlutterWindowMetricsEvent` physical
  width/height, pixel ratio, physical view insets, display ID, view ID, and size constraints plus
  `FlutterEngineDisplay` physical extent and device pixel ratio;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: physical window size and scale-factor construction,
  `WinitWindowEvent::{Resized,ScaleFactorChanged}`, `WindowEvent::{Resize,Rescale}`, and rendering
  with the current physical size/scale; and
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/backends/winit/event_loop.rs`: separate `WindowEvent::Resized` and
  `WindowEvent::ScaleFactorChanged` translation plus logical pointer/touch conversion using the
  current runtime-window scale.

Invariants extracted:
Physical extent and scale are host facts; logical extent is their coherent derived view-space fact.
Scale and every floating geometry component must be finite, and scale must be positive. Each inset
or rectangle retains its source coordinate space. Display orientation/transform, color encoding,
and HDR are neutral facts rather than backend enums. Zero width or height is a legitimate current
state but is not renderable. A metrics revision used for coordinate conversion must identify the
same publication carried by the enclosing view snapshot.

Failure cases and rejected approaches:
Signed physical dimensions admit impossible negative pixels. Arbitrary unvalidated floats admit
NaN, infinity, negative insets, or inverted regions. Clamping zero to one hides minimized or
unconfigured state and can trigger invalid acquisition. Untyped rectangles silently mix display,
physical-view, and logical-view coordinates. Importing renderer color-space types reverses the
neutral dependency. Unbounded avoid lists permit host-controlled allocation growth. Independently
committing metrics before discovering view-revision exhaustion tears observed truth. Cloning the
revision owner permits divergent publications.

Telorgon-specific decision and tests:
`ViewMetrics` derives logical extent and a named uniform logical-to-physical transform from
validated unsigned physical extent and scale. Safe insets are restricted to view logical/physical
spaces and fit their cited extent; typed nonempty avoid regions retain any declared source space and
are capped. Neutral display transform, orientation, color, and HDR values remain backend-free.
`ViewMetricsState` retains immutable values behind one non-cloneable revision owner. `ViewState`
preflights metrics and enclosing revisions before publishing either, and its immutable snapshot
retains the exact metrics revision. Portable tests cover malformed scale/geometry, derived overflow,
fractional logical extent, zero renderability, inset and avoidance limits, display facts, redundant
updates, both exhaustion paths, atomicity, and direct/root exports. No reference source was copied.

### 6.17 Platform event-envelope implementation audit

Concern:
Retaining view identity, strict host ordering, exact coordinate-conversion provenance, typed event
meaning, and loss evidence without moving native translation, coalescing policy, queue ownership,
or runtime dispatch into the neutral platform crate.

Reference source, revision, and symbols reviewed:

- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`: `FlutterPointerEvent` view identity,
  source-clock timestamp, physical coordinates, device/phase/button facts, and synthesized
  `FlutterKeyEvent` timestamp behavior;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: `handle_window_event`, per-window lookup,
  `event_reducer.reduce`, and separate native scale, resize, close, IME, and focus dispatch; and
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/platform.rs`: the typed non-exhaustive `WindowEvent` vocabulary, logical pointer
  positions, and separate scale, resize, close, and activation values.

Invariants extracted:
An event is scoped to the exact view generation and retains ordering evidence independently of its
payload. Source and receipt times remain distinct. Any coordinate converted into a view-relative
space cites the exact metrics publication used. A collapsed run retains the newest complete stamp
and a nonzero count of replaced older events. Payload typing must not make the neutral platform
crate depend on input, text, accessibility, service, or runtime packages.

Failure cases and rejected approaches:
Using only a timestamp cannot order equal-resolution events. Storing only a native window ID makes
stale generation reuse possible. Reading the latest scale during dispatch loses conversion
provenance. A boolean `coalesced` value loses how much input was collapsed. Keeping the first stamp
misrepresents latency and ordering. Letting the value type merge events would embed compatibility,
barrier, and dispatch policy in a data package. A monotelorgon payload enum would reverse focused
package dependencies.

Telorgon-specific decision and tests:
`PlatformEvent<T>` is a generic immutable envelope over `ViewId`, the newest `EventStamp`, explicit
`MetricsCitation`, `CoalescingMetadata`, and `T`. `CollapsedEventCount` is nonzero and excludes the
retained newest event. Payload mapping preserves all platform evidence. Constructors only record
adapter-produced facts and offer no merging operation. Portable unit, direct-path, and umbrella
tests cover unconverted and revision-citing events, newest source/receipt retention, collapsed
counts, typed payload access, and evidence-preserving mapping. No reference source was copied.

### 6.18 Structured platform-error implementation audit

Concern:
Preserving portable failure classification and useful causality for later terminal request outcomes
without leaking native messages, user content, paths, protocol identifiers, handles, pointers, or
file descriptors into diagnostics or immutable state.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/api.rs`: `PlatformError::{NoPlatform,NoEventLoopProvider,Unsupported,Other,
  OtherError}`, its display behavior, and boxed source support; and
- Telorgon's existing `crates/telorgon/src/shell/error.rs`: closed `ShellErrorKind`, static diagnostic context,
  structured rejection mapping, and payload-free `ShellError`.

Invariants extracted:
Portable behavior branches on a closed kind rather than diagnostic text. Error context describes an
operation, not native or user-provided data. A preserved cause must itself be sanitized and typed.
The public value remains immutable, thread-transferable, and suitable for exactly one later request
completion.

Failure cases and rejected approaches:
An unrestricted `String` or arbitrary boxed native error can retain secrets and makes portable code
parse display text. Native integer codes are not stable portable categories and may expose protocol
details. Erasing every cause loses useful failure structure. Treating denied, unsupported,
cancelled, or stale as failure kinds would duplicate the terminal `RequestOutcome` state machine.

Telorgon-specific decision and tests:
Eight closed `PlatformErrorKind` values classify post-admission host failures.
`PlatformErrorSource` retains only another kind and author-written `&'static str`; `PlatformError`
adds its own kind/context plus at most one sanitized source and implements the standard error chain.
No constructor accepts dynamic text or native payloads. Portable unit, direct-path, strict-lint,
rustdoc, and umbrella tests cover structural branching, every kind, sanitized causality, diagnostic
rendering, compactness, and thread transfer. No reference source was copied.

### 6.19 Terminal platform-request implementation audit

Concern:
Separating immediate validation/admission from one later identity-bound typed completion without
optimistically changing observed state, delivering the same completion twice, or making the neutral
value package execute requests and callbacks.

Reference source, revision, and symbols reviewed:

- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`:
  `FlutterPlatformMessageResponseHandle`, the requirement to answer every response handle, and the
  prohibition on sending multiple responses through one handle; and
- Telorgon's existing `crates/telorgon/src/shell/request/result.rs`: `ShellRequestResult`, its distinction
  between immediate `Accepted(AcceptedRequestId)` and rejection, and the rule that admission never
  becomes optimistic host truth.

Invariants extracted:
An immediate validation error has no admitted identity or later completion obligation. One admitted
request carries one opaque identity and one expected applied-value type. Completion is terminal and
distinguishes applied, denied, unsupported, cancelled, stale, and structured failure. Applied means
the operation completed; only a later revisioned snapshot reports current platform truth.

Failure cases and rejected approaches:
Returning applied data directly from admission conflates enqueueing with completion. A cloneable
completion token permits accidental double delivery. Dropping request identity prevents correlation.
Using `PlatformErrorKind` for denied, unsupported, cancelled, or stale duplicates the terminal state
machine. Letting the record invoke cancellation or callbacks adds service execution and reentrancy
to a neutral value module.

Telorgon-specific decision and tests:
`RequestAdmission<T, E>` returns either a request-specific immediate error or one non-cloneable
`AdmittedRequest<T>`. Consuming that token produces one non-cloneable `RequestCompletion<T>` bound
to the same `RequestId` and one `RequestOutcome<T>`. Applied mapping preserves identity and every
other terminal classification. Unit, direct-path, strict-lint, rustdoc, and umbrella tests cover
admission separation, all terminal families, sanitized failure, identity preservation, typed
mapping, and a compile-fail attempt to complete one token twice. No reference source was copied.

### 6.20 Injected monotonic-clock implementation audit

Concern:
Giving managed, embedded, and deterministic hosts one narrow monotonic sampling boundary without
silently selecting wall time, creating an ambient process clock, comparing unrelated domains, or
allowing a regressed observation to become accepted scheduler/input time.

Reference source, revision, and symbols reviewed:

- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/embedder/embedder.h`: `FlutterEngineGetCurrentTime`, the
  same-domain requirements on `FlutterPointerEvent.timestamp` and `FlutterKeyEvent.timestamp`, and
  synthesized key-event timestamp caveats;
- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/platform.rs` and `internal/core/animations.rs`:
  `Platform::duration_since_start`, its host override, standard-library fallback, and monotonicity
  assertion; and
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: event-loop-owned `Instant::now` sampling and elapsed
  frame-time derivation.

Invariants extracted:
The host selects the clock and all values from one source instance belong to one monotonic domain.
Equal consecutive samples are valid at coarse resolution. A regression is a typed boundary failure
and must not replace the last accepted observation. Embedded and deterministic hosts can own local,
manually controlled sources without a thread-transfer requirement.

Failure cases and rejected approaches:
Calling `std::time::Instant::now` inside the neutral crate hides the injected host domain and makes
deterministic testing impossible. Wall time can jump. A global fallback crosses independent hosts
and lifetimes. Requiring `Send + Sync` rejects valid owner-thread sources. Silently clamping a
regression hides a broken clock contract, while updating retained state before validation loses
atomicity.

Telorgon-specific decision and tests:
Object-safe `MonotonicClock::now(&mut self)` is the only source boundary.
`MonotonicClockState<C>` is non-cloneable, binds one source/domain assumption, retains the last
accepted sample, accepts ties, and returns `MonotonicClockError { previous, observed }` on regression
without committing it. It supplies source borrowing for explicit deterministic control but no
clock implementation, sleep, timer, scheduler, callback, thread, event loop, or global. Unit,
direct-path, strict-lint, rustdoc, and umbrella tests cover advancement, ties, regression atomicity,
manual control, and trait-object use. No reference source was copied.

### 6.21 Post-turn scheduling-decision implementation audit

Concern:
Publishing enough deterministic work/redraw/deadline/wake information for managed and embedded
hosts without moving Winit control-flow selection, rendering, clock reads, queues, or continuous
polling policy into the neutral platform package.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/backends/winit/event_loop.rs`: `about_to_wait`, active-animation redraw requests,
  `duration_until_next_timer_update`, `ControlFlow::Wait`, timed waits, and explicit polling mode;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: batching `need_redraw` identities before native
  `request_redraw` calls; and
- Telorgon's existing `crates/telorgon/src/runtime/scheduler.rs`: renderer/event-loop-independent per-view
  frame demand and optional caller-owned monotonic deadline.

Invariants extracted:
Remaining update, layout, semantics, and scene work are independent facts. Redraw demand names exact
view generations and equivalent discovery order produces an equivalent publication. Deadlines are
already-computed instants in the injected domain. Pending host wake and pending service completion
remain distinct. The managed/embedded adapter, not the record, maps these facts to redraw and wait
behavior.

Failure cases and rejected approaches:
Embedding Winit `ControlFlow` reverses the adapter dependency. A boolean redraw flag loses multi-view
identity. An unbounded view list admits externally amplified allocation. Retaining duplicates causes
redundant host calls. Choosing the latest deadline can oversleep earlier work. Treating absence as
zero changes the clock domain. Calling redraw, polling, sleeping, or sampling time while building the
record turns data publication into hidden host execution.

Telorgon-specific decision and tests:
`PostTurnSchedule` retains `RemainingWork`, a pre-copy bounded sorted unique `Arc<[ViewId]>`, an
optional `MonotonicInstant`, and `PendingHostFacts`. Policy-free merge ORs facts, performs a sorted
set union, chooses the earliest deadline, and rejects overflow without truncation. Unit, direct-path,
strict-lint, rustdoc, and umbrella tests cover independent facts, normalization, generation-aware
lookup, bound errors, merge, deadlines, empty values, and thread transfer. No reference source was
copied.

Operational follow-through in the managed Winit application host preserves those decisions. Native
input is coalesced and processed before `PostTurnSchedule` publication; clean input does not add a
redraw view; one per-view latch suppresses duplicate `request_redraw` calls; exact timer deadlines
map through the adapter; and occluded, suspended, and live-resize-throttled views retain demand
without busy polling. Startup, resize, expose, recovery, and unsolicited operating-system paint
callbacks remain explicit force-present reasons. The implementation rejects per-pointer-event
redraw calls, continuous `Poll`, and treating input delivery as paint demand. Portable tests cover a
10,000-move collapse, ordered input fences, clean turns, merged redraw reasons, request latching,
deadline shortening, and lifecycle presentation forcing; native visual qualification remains
explicitly user-run.

### 6.22 Typed platform-service registry implementation audit

Concern:
Letting independently developed narrow platform-service families install host-owned handles without
a monotelorgon platform trait, a global command enum, ambiguous type erasure, owner-thread breakage,
or implicit native/no-op fallback construction.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/platform.rs`: the broad `Platform` trait combining window creation, event-loop
  control, clock, clipboard, logging, and URI behavior plus default unsupported/no-op methods; and
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: runner-owned `Box<dyn ClipboardProvider>` and the Linux
  `NopClipboardContext` fallback path.

Invariants extracted:
One narrow service family has one exact type-level identity and chooses its own handle
representation. The registry borrows rather than clones handles and preserves owner-thread values.
Absent lookup is explicit. Duplicate, replacement, and removal paths deterministically preserve or
return ownership. Adding a service family changes neither a shared command enum nor registry code.

Failure cases and rejected approaches:
A single platform trait couples unrelated services and forces broad implementations. Keying only by
handle `TypeId` aliases two services that intentionally use the same trait-object representation.
String keys admit collisions and display-string branching. Implicit no-op/native construction makes
absence look available. Requiring `Send + Sync` rejects valid event-loop-owned `Rc` handles.
Cloning on lookup obscures ownership and can prolong leases. Exposing erased iteration invites
untyped invocation policy.

Telorgon-specific decision and tests:
Each `ServiceKey` has one associated handle, while the registry erases a private
`StoredService<Key>` and keys by the concrete key `TypeId`. `ServiceLookup` reports available or
typed not-registered status. Registration, replacement, and removal return rejected/displaced
ownership without cloning. The registry exposes count/contains but no erased iteration or invocation.
Unit, direct-path, strict-lint, rustdoc, and umbrella tests cover absence, duplicate atomicity,
identical handle representations under distinct keys, deterministic ownership transfer,
owner-thread `Rc<Cell<_>>`, and handle-redacted debug output. No reference source was copied.

### 6.23 Typed window-service implementation audit

Concern:
Describing per-view title, state, logical size constraints, attention, and close operations without
letting a neutral service mutate Winit objects, publish optimistic view truth, or choose application
exit policy.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/backends/winit/winitwindowadapter.rs`: `WindowOrNone` title/fullscreen/maximize/minimize
  and size-constraint setters plus the property-update path; and
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `xilem/src/window_options.rs`: `ReactiveWindowAttrs` and
  `rebuild_reactive_window_attributes`, and in `xilem/src/driver.rs`: `close_window`, `run_logic`,
  and `on_close_requested`.

Invariants extracted:
Native mutation belongs to the adapter, state/constraint changes are independently capability
discoverable, and close intention remains separate from arbitrary process exit. A successful
operation receipt is not a replacement for later host-observed view state. Constraints use the
view's logical coordinate space and malformed or contradictory bounds fail before admission.

Failure cases and rejected approaches:
A single window-command enum couples unrelated operations and capabilities. Retaining Winit handles
or invoking setters in the neutral crate reverses the adapter dependency. Unbounded titles permit
amplified allocation and unsafe diagnostics. Treating admitted state/fullscreen/close requests as
current snapshot truth creates optimistic divergence. Closing one view must not imply process exit.

Telorgon-specific decision and tests:
`WindowService` has separate object-safe admission methods and a per-view/per-operation capability
query. Bounded `WindowTitle` redacts content from debug output; `WindowSizeConstraints` validates
finite positive ordered logical extents. Each operation has a typed request and applied receipt, and
the owner-local `WindowServiceKey` retains `Rc<dyn WindowService>`. Unit, direct-path, strict-lint,
rustdoc, and umbrella tests cover capability, bounds, generations, admission/completion, registry
lookup, and redaction. No reference source was copied.

### 6.24 Bounded data-transfer implementation audit

Concern:
Preserving exact multi-format offer identity and bounded asynchronous reads without storing native
offers or content in neutral metadata, coercing files/images/HTML into text, or allocating an
untrusted advertised size before policy validation.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/data_transfer.rs`: `DataTransferInner`, `DataTransfer`, and its custom debug output;
- Zed revision `f4178619acd0d47ea1f76a2025c42962c6d6638c` in
  `crates/gpui_linux/src/linux/wayland/clipboard.rs`: `ReceiveData`, `DataOffer<T>`,
  `add_mime_type`, and byte/text/image reads; and
- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `packages/flutter/lib/src/services/clipboard.dart`: `ClipboardData`, `Clipboard::getData`, and
  `Clipboard::setData`.

Invariants extracted:
One offer generation advertises an exact ordered set of formats and aligned size knowledge. Reads
select one advertised representation, carry a hard caller bound, and may report bounded streaming
progress without exposing bytes. Replacing an offer invalidates a read prepared against the prior
generation. Source and trust are independent metadata used before allocation or decoding.

Failure cases and rejected approaches:
Plain-text-only convenience APIs lose rich format identity. Whole-vector reads before checking an
untrusted size can amplify memory use. Silent text/image/path coercion changes meaning. Debugging
format identifiers or content can leak sensitive clipboard/drag data. A native protocol offer,
filesystem path, callback, queue, or executor in the neutral crate gives it execution ownership.

Telorgon-specific decision and tests:
`DataFormat`, `DataOfferDescriptor`, and `DataFormatReadRequest` validate bounded exact formats,
aligned hints, offer generations, hard read limits, and bounded buffered/streamed modes.
`DataReadProgress` and `DataReadCompletion` are content-free and identity-bearing.
`DataTransferService` only advertises bounded capability, admits a linear read, and accepts explicit
cancellation. Unit, direct-path, strict-lint, rustdoc, and umbrella tests cover formats, trust,
bounds, stale generations, admission, cancellation, and redaction. No reference source was copied.

### 6.25 Typed clipboard-service implementation audit

Concern:
Modeling system and selection clipboards, capability, ownership changes, publication, and clearing
without collapsing everything to synchronous text or making a no-op/in-process fallback look like
the platform clipboard.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/platform.rs`: the `Clipboard` enum and platform clipboard methods;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: runner-owned `Box<dyn ClipboardProvider>`, synchronous
  get/set paths, and Linux `NopClipboardContext`; and
- Egui revision `fd54387eac03f57ca772a8fb590ceaadf780f31c` in
  `crates/egui-winit/src/clipboard.rs`: adapter selection among arboard, Smithay, and an in-process
  string fallback plus direct text/image paths.

Invariants extracted:
System and selection clipboards are separate scopes with independent availability. Read and write
permission can differ. Current content is represented by payload-free offer metadata and a
monotonic per-scope snapshot identity; ownership changes publish a newer snapshot. Publication and
clearing cite optional expected identity and complete asynchronously through typed request tokens.

Failure cases and rejected approaches:
Returning empty text for absence fabricates content. A no-op or in-process fallback misreports
system ownership. Synchronous content getters bypass the bounded data-transfer protocol. One shared
permission bit loses read/write distinctions. Logging offer formats or content risks disclosure.
Treating admission as the new current snapshot creates optimistic state.

Telorgon-specific decision and tests:
`ClipboardService` separately queries per-kind capability, reports current snapshot status, and
admits typed publish or clear requests. Capabilities retain exact bounded formats and separate
read/write permissions. Revisioned snapshots and changes carry descriptors but no bytes; unavailable
and failed status remain explicit. Unit, direct-path, strict-lint, rustdoc, and umbrella tests cover
scope separation, capability, snapshot monotonicity, optimistic identity, admission/completion,
absence, registry lookup, and redaction. No reference source was copied.

### 6.26 Revisioned text-input-service implementation audit

Concern:
Binding the canonical revisioned text session to native IME and virtual-keyboard adapters without
duplicating editing state, admitting malformed geometry or secure surrounding plaintext, accepting
native index conventions as portable offsets, or letting service admission apply edits.

Reference source, revision, and symbols reviewed:

- Slint revision `69ecb713f5c62d1b6fe986ff822a57f22152b4d9` in
  `internal/core/window.rs`: `InputMethodRequest`, `InputMethodProperties`, and
  `WindowAdapterInternal::input_method_request`, plus `internal/core/items/text.rs` enable/update
  request sites and preedit handling;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry_winit/src/event_loop_runner.rs`: Winit IME event conversion and `StartIme`, `EndIme`, and
  `ImeMoved` signal application, and `masonry_core/src/passes/update.rs`: focus-change IME reset and
  enable/disable state transitions; and
- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `engine/src/flutter/shell/platform/linux/fl_text_input_handler.cc` and its test: preedit callbacks,
  surrounding-text retrieval/deletion, and GTK input-context conversion paths.

Invariants extracted:
Opening, updating, and closing input-method state are distinct session transitions. The active
editable supplies view-coordinate candidate/caret geometry, configuration, selection, composition,
and bounded surrounding text. Native callbacks must be converted into the portable text model's
index convention before runtime delivery. Losing focus/session ownership disables or resets the
native input method so partial composition cannot migrate to another editable. Platform application
of a synchronization request is not application of a text edit.

Failure cases and rejected approaches:
Copying text storage into a platform service creates two editing authorities. A Winit-only
preedit/commit enum cannot represent revisioned mobile selection, composition, surrounding-text, or
editor-action behavior. Accepting UTF-16/native ranges in the neutral envelope bypasses canonical
boundary validation. Unbounded surrounding text permits amplified native transfer. Supplying
plaintext for secure entry leaks secrets, while formatting canonical edit deltas can leak inserted
or preedit content. Treating admitted synchronization as current native state creates optimistic
truth.

Telorgon-specific decision and tests:
`telorgon-platform` consumes the existing `telorgon-text` protocol types directly.
`TextInputSyncRequest` adds one `ViewId` and validates the 64-KiB hard bound, secure redaction,
active UTF-8 boundary, and finite nonnegative view geometry before admission.
`TextInputDeltaEvent` wraps an already-converted `TextSessionDelta` with view identity and
content-redacted diagnostics; the owning `TextInputSession` still validates session generation and
revision before editing. `TextInputService` only advertises capability and returns a linear typed
synchronization token through its owner-local registry handle. Unit, direct-path, strict-lint,
rustdoc, and umbrella tests cover limits, secure/plain diagnostic redaction, range/geometry errors,
delta identity, admission, completion, and explicit absent/stale view behavior. No reference source
was copied.

### 6.27 Constraint-aware retained glyph-layout implementation audit

Concern:
Giving intrinsic layout and paint one authoritative shaped-text result, while keeping measurement
free of atlas mutation and preventing cached atlas coordinates from surviving an atlas reset.

Reference source, revision, and symbols reviewed:

- Flutter revision `51fd9afadf309ba5337320bd3653f5345c156cb9` in
  `packages/flutter/lib/src/rendering/paragraph.dart`: `RenderParagraph::computeDryLayout` and
  `performLayout`, and in `packages/flutter/lib/src/painting/text_painter.dart`: the cached paragraph
  layout and `TextPainter::paint` paths;
- Xilem/Masonry revision `ce7b04d2ba2d9d7a8c364f2ab109e2083121e144` in
  `masonry/src/widgets/label.rs`: constraint-specific layout construction, measurement, layout, and
  paint selection; and
- React Native revision `2d427ba77bbf17bc487e25bef4d011097ba4fff5` in
  `packages/react-native/ReactCommon/react/renderer/components/text/ParagraphShadowNode.cpp`: the
  constraint-keyed prepared paragraph-layout cache path.

Invariants extracted:
Text geometry depends on content, typography, scale, and layout constraints. Intrinsic measurement
and constrained measurement are distinct cache entries. Paint consumes the same constrained shaped
geometry selected by layout instead of deriving placement from glyph ink bounds or a character-count
estimate. Glyph raster/atlas placement is a later concern; clearing an atlas invalidates placements,
not reusable shaping geometry.

Failure cases and rejected approaches:
Estimating width as character count times font size fails for proportional fonts, kerning, ligatures,
Unicode shaping, and fallback. Measuring from raster ink bounds discards intentional side bearings.
Shaping independently in layout and scene compilation can disagree and doubles work. Populating the
atlas during measurement couples CPU layout to paint resources. Retaining glyph coordinates across
an atlas clear publishes stale texture locations. A hidden layout-owned text cache would also split
font/cache authority from the application runtime.

Telorgon-specific decision and tests:
`telorgon-text` caches atlas-independent `ShapedText` geometry by complete constraints and lazily adds
atlas glyphs to the retained run during preparation. Atlas generations change only when placements
are invalidated; scene compilation rebuilds glyph instances when that generation changes.
`LayoutEngine::update` explicitly receives the application-owned `RetainedTextSystem`, measures real
unbounded intrinsic advances, then records the exact content-box constrained entry that paint reuses.
Focused tests prove atlas-free measurement, shaped-run reuse, constraint-specific wrapping, real
shrink width, shaped-advance alignment, and no scene-time reshaping. No source was copied. No GPU API
specification was applicable because this change concerns CPU shaping/cache ownership rather than a
graphics-API contract.

## 7. Licensing and provenance

The adjacent tree contains projects under different licenses, including permissive and copyleft
licenses. Source availability does not make code interchangeable.

- Default to studying behavior and independently implementing Telorgon's design.
- Do not paste functions, shader source, comments, test vectors, or distinctive structure into
  Telorgon without explicit provenance and compatibility review.
- Do not remove or bypass notices from any material explicitly approved for reuse.
- Treat Zed and any other mixed-license source as design reference only unless the exact file's
  license and proposed reuse are reviewed.
- Record external inspiration when it materially shaped an algorithm or test, even when no code was
  copied.

This guide is an engineering safeguard, not a license determination. Escalate uncertain reuse rather
than guessing.
