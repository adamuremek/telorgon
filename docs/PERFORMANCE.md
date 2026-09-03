# Telorgon Performance and Qualification

## Status

This document separates current portable CPU evidence from target GPU, embedding, shell, and
platform qualification. Capability status is maintained in
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md). Backend-specific requirements are defined in
[RENDER_BACKEND_ARCHITECTURE.md](RENDER_BACKEND_ARCHITECTURE.md), and the immediate GPU work is
ordered in [VULKAN_IMPLEMENTATION_PLAN.md](VULKAN_IMPLEMENTATION_PLAN.md). Gate 4 defines which
scene ranges, GPU bytes, copies, descriptor updates, barriers, batches, and draws the first backend
must measure in [SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md#15-acceptance-tests).
Gate 5 fixes the order in which Windows, hosted, Linux, shell, Metal/macOS, and mobile profiles begin
collecting that evidence in [PLATFORM_IMPLEMENTATION_ORDER.md](PLATFORM_IMPLEMENTATION_ORDER.md).
Gate 6 is the authority for evidence layers, exact structural invariants, hardware-test behavior,
visual tolerances, timing/device profiles, waivers, and production reports in
[ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md). This document retains the current
evidence inventory and performance principles; it does not define a competing qualification system.
Gate 7 fixes component/state/reconciliation/task/text structural invariants in
[AUTHORING_AND_COMPONENT_RUNTIME.md](AUTHORING_AND_COMPONENT_RUNTIME.md#15-diagnostics-and-performance-invariants).
Gate 8 fixes control/controller/overlay/virtualization/adaptive/shell structural invariants in
[APPLICATION_AND_SHELL_PRIMITIVES.md](APPLICATION_AND_SHELL_PRIMITIVES.md#13-diagnostics-and-performance-invariants).
Gate 9 fixes platform event/service/IME/accessibility/data-transfer/external-import counters and
hidden-work prohibitions in
[PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md#17-diagnostics-performance-and-security).

The existing ignored developer-hardware E4 test executes Vulkan device creation, rendering,
submission, and readback. Its Slice 4 extension now records actual retained-scene upload bytes,
buffer copies/allocations, descriptor writes, batches, and draws. The developer-hardware run passed
with one 96-byte/one-copy property update and zero allocations, followed by a warmed zero-upload,
zero-allocation frame and zero validation errors. Portable tests independently prove the exact
one-record dirty update in a 10,000-box retained scene.

## Performance principles

- Idle UI performs no update, scene compilation, rendering, or submission work.
- Work scales with affected state rather than total mounted UI wherever dependencies permit.
- Structure, layout, spatial, text, semantics, paint, and compositing have separate invalidation.
- Resources and caches have explicit budgets and observable high-water marks.
- Embedded operation does not hide device waits, submissions, worker threads, or process-global
  state from the host.
- Portable gates measure counts and bytes; hardware timings always name the device and environment.
- A modeled counter cannot satisfy an operational backend gate.

## Current portable CPU gates

Current unit tests exercise:

- stale node and surface generation rejection;
- sparse-set insertion and swap removal;
- mount-once application behavior;
- coalesced property transactions;
- allocation-free warmed property updates;
- paint-only state changes that do not dirty layout;
- spatial-only scrolling after initial layout;
- bounded virtual-collection queries;
- retained text-run and glyph-atlas reuse;
- scene damage and range deltas;
- one changed visual visiting one node in a 10,000-node scene;
- 10,001 simple mounted nodes within the current 5 MiB CPU-scene test budget;
- adjacent compatible draw items classified as one logical batch;
- idle application frames returning without software submission;
- consecutive native pointer moves collapsing to one routed event per owner turn, with ordered
  button events preserving their fence;
- clean pointer turns leaving frame demand clear instead of requesting a native redraw;
- one outstanding native redraw request per managed view, with merged redraw reasons and explicit
  lifecycle presentation forcing;
- ordinary managed redraw demand capped at the current monitor refresh interval, with late frames
  scheduling from their actual start instead of issuing catch-up work;
- retained software framebuffer identity and damage-aware direct multi-scene rasterization;
- renderer-neutral desktop damage, hidden-revision retention, and repeated placement of one retained
  scene without duplicating its resources; and
- source-boundary checks that reject backend types in neutral desktop modules or cross-references
  between the Linux Vulkan and software assemblies; and
- bounded scene-delta queue coalescing.

These gates validate CPU algorithms and software behavior only.
The current mount-once root and property tests do not yet prove Gate 7's reusable component scopes,
dependency-tracked reads, typed action routing, explicit keyed reconciliation, scoped tasks, or
revisioned editable-text runtime.
They also do not prove Gate 8 activation cancellation, composite focus/selection, semantic control
behavior, adaptive identity, controller/overlay bounds, domain isolation, or shell request authority.

## Current Vulkan renderer checks

Portable and compile-only tests in `telorgon-renderer-vulkan` currently verify:

- geometric device-buffer growth and full-live-mirror rewrite planning on growth;
- adjacent dirty-range coalescing and exact GPU-record byte accounting;
- one changed box property in 10,000 boxes queues one 96-byte record and leaves paint order intact;
- adjacent-only mixed-pipeline painter-order batching, explicit blend modes, and measured batch/draw
  counts;
- reusable per-frame command pools, command buffers, descriptor pools/sets, and mapped staging;
- aligned multi-scene view records in one shared frame-staging stream;
- generation-safe descriptor reuse and completion-pinned buffer retirement;
- typed staging and device-local budget exhaustion;
- versioned R8 atlas and native RGBA/BGRA image uploads, regional staging writes, in-place updates
  for idle sampled images, completion-safe on-GPU preservation into reusable copy-on-write images,
  stable per-draw texture descriptors, full-image-free regional CPU retention with older-delta
  preservation, dense spatial/clip IDs, and analytic rounded clips;
- deterministic software rendering of the same ordered box/glyph/image/material scene; and
- passing developer-hardware E4 retained-resource and managed Windows E5 recovery harnesses.

The E4 hardware test creates a Vulkan instance/device, allocates real memory, records copies,
barriers, draws and readback, executes shaders, submits the queue, and checks pixels under validation.
The retained-resource assertions passed with a bounded one-record copy and a warmed frame with no
scene upload or buffer allocation. The post-Slice5 E5 regression also passed six frames, resize,
suspend/resume, surface replacement, shutdown, and zero validation errors. These are developer
evidence, not timing results or production qualification.

Slice 5 adds two ignored developer-hardware readback checks. One renders a deliberately mixed
box/glyph/image/material/clip/spatial scene and compares it with the software reference. The other
compiles the current Gallery scene and compares the full readback with documented mean/large-error
tolerances. The Gallery run passed with 1.506 mean channel error, a 0.0034 large-error ratio, and zero
validation errors. The focused mixed-scene run passed four draws with a maximum channel error of 3
and zero validation errors. Slice 5's E4 hardware evidence is accepted.
Together with the post-Slice5 E5 regression, Slice 5 hardware qualification is complete.

Explicit foreground hardware commands are:

```powershell
$env:TELORGON_TEST_MODE = "developer-hardware"
cargo test -p telorgon-renderer-vulkan --test vulkan_hardware -- --ignored --nocapture
cargo test -p telorgon-presenter-vulkan-wsi --test windows_managed_vulkan -- --ignored --nocapture
```

The final command opens the short-lived managed qualification window; it must remain user-run.

## Portable verification

Verification commands that do not start a GUI application are:

```powershell
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Required diagnostics

Portable diagnostics should report:

- mounted and visited nodes;
- mounted/updated/unmounted component scopes and routed/dropped actions;
- state reads/writes, dependency edges, and invalidation fan-out;
- keyed inserts/removes/moves, lifecycle closures, and identity-preserving moves;
- live/started/completed/cancelled tasks, stale results, timer deadlines, and queue high-water marks;
- text revisions/edits, range-conversion failures, shaped ranges, and cache reuse;
- activations/cancellations by source, focus/highlight/selection changes, controller and overlay
  lifetimes, target-size violations, adaptive slot moves, and virtual materialization ranges;
- shell snapshots/requests/results, protected redactions, external-surface revisions/damage, and
  client-surface placeholders;
- platform event and completion queue depth/high-water, event-to-dispatch latency, redraw reasons,
  native pointer-move/coalescing counts, clean input turns, issued/suppressed redraw requests,
  presentation idle/submitted outcomes, service outcomes/latency, stale generations/revisions, IME
  resyncs, semantics delta size, transfer bytes, external import/release latency, and unretired
  leases;
- evaluated bindings and committed properties;
- measured, arranged, spatially updated, and hit-indexed nodes;
- shaped text runs, glyph-cache hits/misses, atlas pages, and upload bytes;
- scene instances patched, damage area, delta bytes, and queue coalescing;
- CPU allocations and retained memory high-water marks; and
- selected capability fallbacks.

Operational GPU backends additionally report real:

- device allocations and resident bytes;
- staging and device upload bytes;
- command buffers/lists and submissions;
- barriers, passes, batches, draws, and dispatches;
- pipeline creation and cache behavior;
- descriptor/binding allocations and writes;
- intermediate targets and resolves;
- available GPU timestamps; and
- surface/device loss and recovery events.

Unavailable metrics are reported as unavailable rather than estimated from CPU model state.

## Embedded-host interference gates

After warm-up, an unchanged embedded `UiView` must:

- make `needs_update` allocation-free;
- perform no UI update, scene compilation, upload, command recording, or submission;
- create no threads or background tasks unless the host enabled them;
- perform no readback, device-wide idle, implicit queue wait, or independent submission;
- leave host target contents and resource states untouched when not asked to record; and
- remain within host-configurable CPU/GPU cache budgets.

When changed, Telorgon reports the CPU time, allocations, uploads, passes, barriers, draws, dispatches,
and damage attributable to each view. Direct-to-target and cached-offscreen modes are measured
separately.

## GPU qualification gates

The list below is a summary. The required E4–E9 test cases, outcome vocabulary, device matrix, and
report fields are controlled by
[Acceptance and qualification](ACCEPTANCE_AND_QUALIFICATION.md#6-shared-real-renderer-conformance-matrix).

A real backend is not production-qualified until tests on real hardware demonstrate:

- shader execution and visible output through the backend;
- owned presentation where supported;
- hosted command-only recording into host-provided targets;
- persistent resource reuse without ordinary-frame object churn;
- correct resource transitions and synchronization under validation/debug tooling;
- software/backend visual conformance within documented tolerances;
- resize, suspend/resume, target loss, device loss, and recovery;
- multi-view shared-device rendering;
- bounded upload and transient-target behavior;
- no ordinary CPU readback; and
- truthful counters matched against native GPU capture or validation evidence.

Shell/compositor qualification additionally requires real external-image import, GPU-side acquire
and release synchronization, multi-output behavior, and no hidden readback fallback.

## Timing reports

Gate 6 requires versioned per-machine-class budgets and p50/p95/p99 results; it deliberately rejects
a universal threshold across unrelated hardware. This section lists the environmental metadata that
must accompany those numbers.

Hardware-dependent reports identify:

- CPU, GPU, operating system, backend, driver, and backend version;
- build profile and compiler;
- output resolution, scale, format, color space, sample count, and present mode;
- power mode and frame pacing;
- scene/workload definition;
- warm-up and sample methodology; and
- native, lowered, fallback, unsupported, and untested capability paths.

A universal millisecond promise is not substituted for this evidence.
Portable work/count invariants and the default repeated-run regression-review thresholds are in
[Gate 6's performance section](ACCEPTANCE_AND_QUALIFICATION.md#11-performance-and-interference-acceptance).
