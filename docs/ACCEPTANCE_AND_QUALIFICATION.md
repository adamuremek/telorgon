# Telorgon Acceptance and Qualification Contract

## Status and authority

This is a **target contract** produced by planning Gate 6. It defines the evidence required to
accept implementation slices, call a backend or platform profile operational, and call a declared
profile production-qualified. It does not claim that the current repository has this test
infrastructure or that any current subsystem is production-qualified. Current capability status
remains authoritative in [Implementation status](IMPLEMENTATION_STATUS.md).

This document controls shared conformance suites, hardware-test behavior, validation policy, visual
comparison, performance evidence, device matrices, waivers, and qualification reports. More focused
documents still control the contract being tested:

- [GPU ownership and synchronization](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md) controls lifetimes,
  completion, presentation, hosted recording, and recovery;
- [Scene-to-GPU ABI and shaders](SCENE_GPU_ABI_AND_SHADERS.md) controls retained records, byte
  layouts, uploads, shaders, color, and batching;
- [Platform implementation order](PLATFORM_IMPLEMENTATION_ORDER.md) controls profile order and
  platform package isolation;
- [Platform integration contract](PLATFORM_INTEGRATION_CONTRACT.md) controls lifecycle axes, input
  and IME mapping, accessibility, services, data transfer, native handles, and embedded-host
  non-ownership; and
- [Performance](PERFORMANCE.md) describes current evidence and performance principles, while this
  document controls exact qualification evidence and reporting.

The suite is a Telorgon project conformance suite. It is not the Vulkan Conformance Test Suite and
does not authorize a Khronos trademark or driver-conformance claim. Telorgon qualification runs on
drivers that the profile identifies as Vulkan conformant, then tests Telorgon's use of them.

## 1. Decisions fixed by Gate 6

1. There is no single `tests passed` bit. Evidence is reported by layer, capability, backend,
   operating profile, platform, device, and driver.
2. Portable tests never initialize a graphics device merely because `cargo test --workspace` ran.
3. A developer hardware run may explicitly skip because a required device or tool is absent. A
   qualification run treats the same absence as a failed required matrix cell.
4. A trace/no-op backend validates plans and lifetimes but cannot prove shader execution, native
   synchronization, image correctness, presentation, or GPU performance.
5. A real offscreen GPU test proves execution and readback but not managed presentation, hosted
   scheduling, platform lifecycle, accessibility, or shell interoperability.
6. Image acceptance combines exact structural/color checks with fixture-scoped edge tolerances.
   One whole-image perceptual score may not hide a localized incorrect pixel region.
7. Portable performance gates prefer exact work, allocation, upload, barrier, and submission
   invariants. Hardware time budgets live in versioned profiles tied to declared machines.
8. Validation errors, device loss, hidden software fallback, hidden submission, readback, or
   device-wide idle are not converted into success by a screenshot.
9. Production support is a reproducible report over a declared matrix, not an inference from one
   developer machine.
10. New backends run the same scene, ownership, hosted, visual, and performance suites before their
    backend-specific extras. Backend-specific tests cannot replace shared tests.

## 2. Evidence layers and result vocabulary

### 2.1 Evidence layers

| Layer | Name | Proves | Does not prove |
| --- | --- | --- | --- |
| E0 | Static and package | Formatting, compile targets, features, dependency direction, licenses, generated-file freshness, documentation links | Runtime behavior |
| E1 | Unit and property | Pure algorithms, state machines, generational IDs, deltas, math, deterministic failure handling | Native API use |
| E2 | Trace conformance | Resource uses, pass dependencies, target contracts, ownership events, deterministic lowering, counter integrity | GPU execution or pixels |
| E3 | Shader and ABI | Host offsets, shader reflection, artifact hashes, compiler/validator acceptance, corrupt-bundle rejection | Driver execution |
| E4 | Real GPU offscreen | Device creation, allocation, command recording, submission, shader execution, copy-to-readback, pixel output | WSI or host integration |
| E5 | Managed presentation | Acquire/render/submit/present, resize and surface recovery in a Telorgon-owned presentation profile | Hosted frame ownership |
| E6 | Hosted render area | Host-owned device/frame/target, command-only recording, subregions, load/preserve rules, no hidden submit/wait | Native window lifecycle |
| E7 | Platform integration | Lifecycle, input, services, accessibility, packaging fixture, multiple views/windows | Shell external-image interop unless declared |
| E8 | Shell interoperability | Real external image import, GPU acquire/release, damage, multi-output composition, no CPU copy fallback | Protocol implementation by Telorgon |
| E9 | Production qualification | Declared device/driver matrix, failure/recovery, performance budgets, report and waiver review | Unlisted systems or capabilities |

An E4 pass cannot be used as shorthand for E5–E9. A higher layer includes its required lower-layer
results, but reports them independently so the failing boundary remains visible.

### 2.2 Per-cell results

Every test cell has one of these machine-readable outcomes:

| Outcome | Meaning |
| --- | --- |
| `pass` | The test executed on the recorded path and met its assertions. |
| `fail` | The test executed or was required to initialize, and did not meet the contract. |
| `skip` | A developer run deliberately did not execute the cell; never counts toward qualification. |
| `unsupported` | The profile declares an optional capability unavailable and the probe agrees; not a pass for that capability. |
| `waived` | A specific, time-bounded, reviewed exception applies; the original failure remains in the report. |

`not-run` may appear in an incomplete report but is never a completed qualification outcome. Native,
lowered, fallback, unsupported, and untested capability paths remain separate metadata from test
outcomes.

### 2.3 Acceptance levels

- **Slice accepted:** all lower evidence required by that implementation slice passes in developer
  and required CI profiles. This allows more implementation work; it is not a support claim.
- **Operational profile:** the intended real integration works with automated E0–E4 evidence plus
  the specific E5, E6, E7, or E8 layer that defines the profile.
- **Production-qualified profile:** all required E0–E9 cells pass on the declared matrix and the
  signed report has no expired waiver, required skip, missing device, or unreviewed regression.
- **Production-qualified with waivers:** permitted only for explicitly non-safety, non-corruption,
  non-ownership limitations. User-facing support material must display the limitation and expiry.

Memory safety, synchronization correctness, resource lifetime, data corruption, hangs, device-loss
recovery, security boundaries, hidden fallback, and host-ownership violations are non-waivable.

## 3. Test packages and file ownership

Gate 6 fixes this target layout; the crates remain planned until created through the migration
ledger:

```text
crates/
  telorgon-renderer-trace/
    src/
      lib.rs
      device.rs
      resources.rs
      recorder.rs
      validator.rs
      report.rs
  telorgon-backend-conformance/
    src/
      lib.rs
      capability.rs
      device_selector.rs
      fixture.rs
      image_compare.rs
      operation_counters.rs
      qualification_report.rs
      validation.rs
      waiver.rs
      suites/
        scene.rs
        plan.rs
        resource.rs
        offscreen.rs
        visual.rs
        managed.rs
        hosted.rs
        recovery.rs
        external_image.rs
        platform.rs
        performance.rs
    tests/
      portable.rs
      trace.rs
      compile_contracts.rs
      vulkan_hardware.rs
      metal_hardware.rs
      platform_profiles.rs
tests/
  fixtures/
    scenes/
    images/
    fonts/
    shaders/
    platform/
qualification/
  schema/
    profile.schema.json
    report.schema.json
    waiver.schema.json
  profiles/
  baselines/
  waivers/
```

Focused E1 tests stay beside their owning source. `telorgon-backend-conformance` uses public or
explicit test-support contracts; it does not reach into private backend fields merely to make a test
pass. The trace backend implements the common backend test seam but never presents itself as an
image-producing backend.

Generated reports, actual images, heatmaps, native validation logs, and captures are build artifacts,
not casually committed source. Approved qualification summaries and baselines are versioned; raw
artifacts are archived by release and linked by content hash.

### 3.1 Development-only dependencies

The implementation work order pins reviewed versions in workspace development dependencies:

- `proptest` for generational-table, delta, range, and state-machine properties;
- `trybuild`, or isolated Cargo fixture packages where target-specific linker behavior makes
  `trybuild` unsuitable, for compile-pass/compile-fail ownership contracts;
- `serde`, `toml`, and `serde_json` for profiles and reports;
- `sha2` for fixture, artifact, and report hashes;
- one PNG codec with color conversion disabled for lossless artifact I/O; and
- Criterion or an equivalent statistically reported harness for portable CPU microbenchmarks.

No async runtime, browser, window system, or telemetry service is added merely to run portable
tests. Test-only dependencies must not enter ordinary public dependency graphs.

## 4. Run classes and hardware skip policy

### 4.1 Portable workspace run

The ordinary workspace run covers E0–E3 where the required shader tools are installed. It must not
open windows, start background services, enumerate presentation surfaces, or initialize a physical
GPU as a side effect. Shader artifact freshness may use checked-in manifests so platforms without
the shader compiler can still verify hashes; regeneration remains a separate tool-enabled job.

### 4.2 Developer hardware run

Hardware tests are separately selected by backend and profile. They may return `skip` when no
matching physical device, driver, validation layer, display integration, or host fixture exists. The
harness prints and serializes the exact missing prerequisite. It never returns `pass` after an early
`None`, `return`, or caught initialization failure.

### 4.3 Qualification run

Qualification is selected by a profile file and `TELORGON_TEST_MODE=qualification`. The harness
requires `TELORGON_QUALIFICATION_PROFILE` to name the versioned profile. In this mode:

- every required device and integration cell must be selected unambiguously;
- a missing loader, device, validation layer, display integration, host fixture, or platform service
  is `fail`, not `skip`;
- unexpected fallback or adapter substitution is `fail`;
- reports are written even after individual failures so the matrix remains auditable; and
- the final command fails if any required cell is failed, skipped, unsupported, not-run, or covered
  by an invalid/expired waiver.

Backend test filtering uses stable suite/case IDs recorded in the report. Cargo substring filters
are a developer convenience and are not a qualification profile.

### 4.4 Isolation

Real GPU cases run in isolated test processes when they can trigger device loss, validation aborts,
or driver failure. A timeout kills only that test process and records `fail`; it does not leave a
resident helper. Tests do not run production applications or servers. A managed-presentation test
may create a short-lived test window inside its foreground test process where the platform requires
one.

## 5. Portable, trace, ABI, and shader matrix

### 5.1 Portable scene, runtime, and domain properties

Required E1 cases cover:

- generational slot allocation, removal, reuse, exhaustion, and stale-ID rejection;
- monotonic epochs, atomic delta application, duplicate/reordered/gapped delta rejection, and full
  snapshot recovery;
- coalescing adjacent dirty ranges without merging across untouched data or overflow;
- painter order under insertion, removal, clip/layer nesting, and adjacent batching;
- stable spatial/clip/primitive IDs during unrelated edits;
- invalid parent, cyclic spatial graph, non-finite geometry, invalid resource reference, and bounds
  rejection; and
- deterministic plan and diagnostic output from the same snapshot/capability set.

Property tests produce a minimized reproducible seed on failure. Regression seeds become named
fixtures when they expose a distinct bug class.

Gate 7 extends E1 with portable component/runtime cases for owner and generation validation,
dependency registration and invalidation, atomic action transactions, explicit keyed
reconciliation, deterministic child-first teardown, scoped task cancellation/stale-result rejection,
and revisioned UTF-8 text edits, selections, compositions, and native-offset conversions. The
complete case list is controlled by
[Authoring and component runtime](AUTHORING_AND_COMPONENT_RUNTIME.md#16-required-acceptance-cases);
these cases use the same result vocabulary, seed capture, reports, and release gates defined here.

Gate 8 extends E0–E3 with portable control and domain cases for activation/cancellation, focus and
selection, composite keyboard/directional navigation, controlled values/controllers, density and
adaptive targets, semantics, overlays, text editing/undo, keyed virtualization, application/shell
dependency isolation, shell request authority, protected-data redaction, and external-content traces.
The complete case list is controlled by
[Application and shell primitives](APPLICATION_AND_SHELL_PRIMITIVES.md#14-required-acceptance-cases).

### 5.2 Trace backend

Required E2 cases validate:

- create/use/update/destroy ordering and generational handle use;
- semantic resource use lowering into compatible stages, access types, layouts, and pass edges;
- attachment load, clear, preserve, resolve, discard, and subregion contracts;
- read-after-write, write-after-read, write-after-write, alias, and host/GPU visibility hazards;
- completion pins, deferred destruction, frame-slot reuse, and presenter retirement;
- hosted recording intervals with no submit, wait, present, allocation outside host policy, or
  command use after the host interval closes;
- external acquire before first use and release after last use;
- target origin/extent/sample/format/color compatibility;
- counter agreement with emitted trace operations; and
- error paths that leave no partly committed scene or leaked live object.

Failure-injection fixtures include stale deltas, invalid use declarations, unsupported capability
sets, allocation-budget exhaustion, descriptor exhaustion, out-of-date/surface-lost events, injected
device loss, host-contract misuse, and invalid external synchronization.

### 5.3 Compile contracts

Compile-pass/compile-fail fixtures prove that:

- acquired images and hosted recording intervals cannot outlive their owners;
- frame/target tokens are not clonable or reusable after consumption;
- owned and borrowed native-handle wrappers expose the intended `Send`/`Sync` behavior only;
- safe portable APIs cannot construct unvalidated native imports; and
- platform, shell, UI-domain, shader-build, and concrete backend dependencies remain in their
  permitted packages.

Compiler diagnostic text is not the primary assertion unless Telorgon owns that diagnostic. The main
assertion is pass versus fail plus the responsible source span/fixture purpose.

### 5.4 ABI and shader artifacts

Required E3 cases cover every layout and artifact obligation in Gate 4:

- compile-time size, alignment, offset, endian-independent word, and enum/flag assertions;
- host declaration versus reflection manifest comparison for every set, binding, stage, record, and
  specialization constant;
- deterministic clean regeneration and bundle-content hashes;
- SPIR-V validation for Vulkan and native compiler validation for each later artifact target;
- optimized and debug artifact variants where production builds differ;
- missing, duplicated, optimized-out, type-mismatched, version-mismatched, truncated, and corrupt
  bundle rejection before pipeline creation; and
- CPU math goldens for transforms, clipping, coverage, sRGB transfer, straight-to-premultiplied
  conversion, opacity, blending, and target encoding.

## 6. Shared real-renderer conformance matrix

Every real backend runs the following E4 suite against offscreen targets before any presentation
claim.

### 6.1 Primitive and image correctness

Fixtures cover:

- boxes with zero/nonzero borders, independent corner radii, degenerate extents, and fractional
  transforms;
- glyph masks, color glyph images, atlas edges, multiple pages, missing glyphs, and opacity;
- opaque, straight-alpha, premultiplied-alpha, linear, and sRGB image inputs;
- analytic rectangle/rounded clips, mask clips, nested clips, empty clips, and transformed clips;
- local/spatial/view transforms, negative scale, rotation, and large translated coordinates;
- nested opacity/layers and required blend modes;
- target formats/encodings declared by the core baseline, including explicit fallback results; and
- painter-order overlap cases that would fail if nonadjacent draws were globally sorted.

Paths and materials enter the core suite when their contracts are frozen; until then a backend may
report them only as optional capability suites.

### 6.2 Target and frame behavior

Fixtures cover:

- clear, preserve/load, discard, and damage-limited rendering;
- nonzero-origin subregions with untouched sentinel pixels outside the area;
- fixed and resized targets, zero-size suspension, scale changes, and format/sample changes;
- direct target and cached-offscreen composition paths;
- multiple independent views sharing one device without state leakage; and
- color and alpha results after real GPU copy-to-readback, never a CPU renderer substituted for the
  backend under test.

### 6.3 Resource behavior

Tests assert:

- partial buffer and texture uploads touch only declared ranges/rectangles;
- growth preserves live records and does not invalidate logical IDs;
- staging blocks, descriptors/bindings, transient targets, and command allocators are reused only
  after completion;
- glyph-atlas growth does not rewrite unchanged glyph records;
- pipeline and sampler caches distinguish all compatibility inputs;
- exhaustion returns a typed error or performs a declared bounded growth/fallback; and
- destruction occurs after the last real GPU use without device-wide idle in an ordinary frame.

### 6.4 Recovery and negative paths

Backend test seams must support deterministic fault injection above the native API even when a
driver cannot reliably produce the fault. Required cases include allocation failure, pipeline
failure, target invalidation, surface out-of-date/lost, device loss, malformed host input, and
corrupt shader bundle. Tests prove the state transition, error classification, retained snapshot
recovery, resource cleanup, and absence of an infinite retry loop.

A driver-induced loss case is also required on qualification systems where the vendor toolchain
offers a supported mechanism. Synthetic injection and a driver-induced event are reported
separately.

## 7. Visual conformance and golden policy

### 7.1 Canonical comparison space

Goldens are compared as canonical 32-bit-float **linear premultiplied RGBA** samples after explicit
decode of the test target. PNG encoding bytes, display color management, and alpha-less channel
garbage are not compared as if they were renderer semantics. Fixtures declare source encodings,
target encoding, alpha mode, scale, and background.

The software renderer may help generate an initial expected result only after its corresponding
math/primitive behavior is independently reviewed. Once approved, the fixture is versioned. A GPU
backend is never accepted by comparing it only to another unqualified backend.

### 7.2 Fixture manifest

Each visual case has a manifest containing:

- stable case ID and scene hash;
- dimensions, scale, sample count, target format, encoding, alpha mode, and clear/load behavior;
- checked-in font/image hashes and deterministic shaping/raster settings;
- exact-region and analytic-edge masks;
- per-region tolerance and rationale;
- required capabilities and permitted declared fallback;
- expected diagnostic/counter bounds; and
- golden version and approving change.

System fonts, locale-dependent fallback, clock values, random animation time, and driver-selected
assets are forbidden in deterministic goldens.

### 7.3 Tolerances

For normalized 8-bit SDR fixtures, exact interior regions permit at most `1/255` absolute error per
linear-premultiplied channel. An explicitly marked analytic/text edge region may permit at most
`4/255` per channel and `1/255` mean absolute error across that region. Pixels outside those masks
use exact-interior rules. Float/HDR fixtures define format-appropriate absolute and relative bounds
in their manifest before the test is enabled.

Any looser fixture must identify the primitive, driver variation, affected region, evidence, owner,
and review expiry. A global perceptual threshold cannot replace these bounds. A perceptual metric may
be recorded as additional diagnostic evidence.

Failures retain the expected image, actual image, absolute-error image, false-color heatmap, maximum
error and location, per-channel mean, failed-pixel count, adapter/driver identity, and fixture hash.

### 7.4 Golden updates

CI and qualification jobs never update goldens. Regeneration is an explicit reviewed operation that
shows old/new/diff artifacts and states whether the scene, intended output, toolchain, or tolerance
changed. A backend-specific golden is allowed only when the backend takes a declared, user-visible
native or fallback path; it may not mask an unexplained driver difference.

## 8. Vulkan validation and capture evidence

Every Vulkan E4–E8 correctness run enables `VK_LAYER_KHRONOS_validation` core validation and
synchronization validation through current layer settings. Qualification records the Vulkan loader,
SDK/layer version, enabled settings, API version, extensions/features, driver, device IDs, queue
families, and all debug messages.

The default required rule is zero unwaived validation errors and zero unwaived validation warnings.
Best-practices messages are recorded separately because some describe performance policy rather than
invalid use. A warning exception names its message ID or VUID, exact device/driver scope, reason,
owner, issue, evidence, and expiry. Filtering a message before it reaches the report is forbidden.

GPU-assisted validation runs as a separate stress configuration for descriptor/buffer access and
shader-time errors. It is not enabled simultaneously with every ordinary case because it instruments
execution, changes limits, and has material overhead. A GPU-assisted run never supplies performance
numbers. If the selected device cannot meet GPU-assisted prerequisites, the required profile cell
fails or uses an explicitly approved alternative device; it does not silently disappear.

Native captures and vendor tools are corroborating diagnostics. Tests compare Telorgon counters to
captured submissions, passes, draws, copies, and barriers for representative fixtures, but no binary
capture format is the sole long-term acceptance oracle.

## 9. Managed and hosted acceptance

### 9.1 Managed presentation

Each managed profile proves:

- surface creation, capability selection, acquire, record, submit, present, and bounded frame reuse;
- several frames of distinct visible output rather than clear-only presentation;
- resize, minimize/zero-size suspension, scale/format changes, out-of-date, surface loss, and
  recreation without stale acquired-token use;
- multiple windows/views sharing one device where the profile promises it;
- redraw scheduling that becomes idle after work completes; and
- orderly shutdown without presenting, submitting, or destroying in-use objects after teardown.

A test that only creates a window or swapchain is bring-up evidence, not managed presentation
acceptance.

### 9.2 Hosted render areas

Hosted suites use an independent fixture host and prove:

- borrowed device/queue/native objects remain host-owned;
- Telorgon records only inside the host's declared command interval;
- host-provided target origin, extent, format, encoding, sample count, initial state/use, load policy,
  and final state/use are honored;
- Telorgon performs no queue submit, present, device idle, queue idle, hidden transfer submission,
  command-pool reset, or thread creation outside the host contract;
- same-queue and host-declared multi-queue configurations obey completion ownership;
- unchanged views return without command allocation/recording;
- several Telorgon views can be composed with host work in one frame; and
- host rejection, target invalidation, device loss, and shutdown return typed outcomes without
  taking ownership.

Native-call interception or a backend audit layer verifies forbidden operations in addition to
Telorgon's own counters.

## 10. Platform and shell matrices

### 10.1 Platform milestone coverage

The P1–P8 sequence remains fixed by Gate 5. Each profile adds these required layers:

| Profile | Required evidence beyond E0–E4 |
| --- | --- |
| P1 Windows Vulkan | E5 managed presentation; desktop pointer/keyboard, resize/scale, clipboard and accessibility smoke; packaging fixture |
| P2 hosted Vulkan | E6 command-only host integration, subregions, multi-view, host ownership and recovery |
| P3 Linux Vulkan | E5 and E7 separately for Wayland and X11; feature graph and runtime path recorded independently |
| P4 Linux shell host | E8 protocol-neutral fixture plus at least one real external-image/synchronization mechanism supplied by an external protocol/policy host |
| P5 Metal/macOS | Shared E1–E7 suites through direct Metal; no Vulkan-portability result substitutes for direct Metal |
| P6 mobile foundation | Lifecycle/input/service state machines and platform-neutral touch, IME, safe-area, low-memory, suspend/resume fixtures |
| P7 Android Vulkan | NativeActivity renderer/packaging bring-up is reported separately; operational GameActivity or qualified alternate bridge requires shared Vulkan E4/E5/E7 plus Activity/surface recreation, touch/IME, accessibility, packaging and device matrix |
| P8 iOS Metal | Shared Metal E4/E5/E7 plus drawable absence, lifecycle, touch/IME, accessibility, safe-area, packaging and device matrix |

The full platform assertions are fixed by
[Gate 9](PLATFORM_INTEGRATION_CONTRACT.md#18-acceptance-contract). E7 records each service as
operational, partial, unsupported, denied, or untested and cannot claim blanket platform
qualification. Applicable profiles must cover independent lifecycle axes and surface generations,
revisioned metrics, complete input-unit/key mapping, text/IME range conversion, accessibility
activation/deltas/actions, multi-format data offers, service request outcomes, hidden-thread checks,
and secure-data redaction. Embedded profiles additionally prove that no window, event loop,
presenter, queue, or worker is created.

### 10.2 Shell interoperability

P4 starts with a protocol-neutral fake host so protocol policy cannot contaminate renderer tests.
Production shell qualification additionally requires a real host adapter and proves:

- native external image import without CPU pixel readback or software upload fallback;
- format/modifier or equivalent capability negotiation;
- acquire wait before sampling and release signal after the final GPU read;
- damage-limited composition, transforms, clips, opacity, and overlapping surfaces;
- import replacement, client disappearance, invalid handles, timeout, and device/output loss;
- multi-output rendering with independent scale/transform/damage; and
- release of imported resources only after both GPU completion and host/protocol permission.

Wayland, X11, KMS/DRM, or another protocol provider remains outside Telorgon. The report names the
external provider and exact interop mechanism rather than claiming Telorgon implements the protocol.

## 11. Performance and interference acceptance

### 11.1 Structural invariants

After warm-up, the shared instrumentation must prove:

| Scenario | Required invariant |
| --- | --- |
| Unchanged view | Zero UI update, layout, scene delta, upload bytes, command recording, submission, allocation, readback, or wait |
| One box property in a warmed 10,000-node scene | One primitive-slot patch, unchanged paint order, no buffer growth, no unrelated record rewrite, at most 256 uploaded bytes and one coalesced buffer copy |
| One spatial-root transform | One 32-byte spatial-record change; implementation alignment may copy at most 256 bytes; no child primitive rewrite or layout |
| One glyph insertion | Upload bounded to the declared dirty atlas rectangle; no unchanged glyph/primitive rewrite |
| Atlas/buffer growth | Live logical IDs remain stable; old storage retires only after completion; no device idle |
| Hosted unchanged view | No command allocation, recording, submission request, hidden worker, or target transition |
| Ordinary changed frame | No CPU image readback, per-primitive native draw, per-primitive heap allocation, or hidden transfer submission |
| One controlled component value | Only its dependent behavior/style/semantic properties update; no remount or unrelated measure/scene work |
| One keyed virtual item insertion | Work is bounded to affected materialized/selection/semantic ranges; unrelated item identities remain stable |
| One client-surface damage update | Only affected external-content/scene records and damage update; no CPU readback or unrelated shell subtree work |

If the final ABI changes a record size, Gate 4 and this table change together with measured evidence.
Counters come from the actual runtime/backend path and are cross-checked against trace/native capture
on representative cases.

### 11.2 Hardware timing and memory budgets

Portable code does not promise one universal millisecond value across unrelated hardware. Each
production profile defines versioned budgets for named workloads on a named qualification class:

- CPU update/layout/plan/record p50, p95, and p99;
- available GPU duration p50, p95, and p99;
- frame misses under the profile's refresh/pacing policy;
- steady/high-water CPU and GPU memory;
- allocations, upload bytes, passes, barriers, batches, draws, dispatches, and submissions;
- warm-up duration and pipeline/atlas cache behavior; and
- direct versus cached-offscreen hosted paths.

Measurements use optimized artifacts, fixed workloads, fixed resolution/scale/format, declared
power mode, controlled warm-up, enough samples for stable percentiles, and no validation, capture,
or GPU-assisted instrumentation. Results are compared only with an approved baseline on the same
machine class and configuration.

A default regression review is triggered by more than 10% p95 CPU/GPU time or more than 5% retained
memory, upload, allocation, pass, draw, or submission growth after repeated runs. The profile may set
stricter release limits. Noise is investigated and rerun; it is not waived by deleting samples.

## 12. Production device and driver matrix

A profile file names exact operating-system versions, target triples, API/backend versions, driver
branches, devices, display integration, and required capabilities. At minimum, first production
qualification aims for these independent categories when that platform is released:

| Platform profile | Minimum matrix categories |
| --- | --- |
| Windows Vulkan | AMD discrete, NVIDIA discrete, and Intel integrated/Arc paths; lowest and current supported Windows bands |
| Linux Vulkan | AMD Mesa, Intel Mesa, and NVIDIA paths; Wayland and X11 cells remain separate where supported |
| Hosted Vulkan | At least two host scheduling fixtures: host-owned single-queue composition and host-declared multi-queue/completion ownership |
| macOS Metal | At least two supported Apple GPU generations and lowest/current supported macOS bands |
| Android Vulkan | Qualcomm/Adreno and Arm/Mali classes, at least two supported Android API bands, phone and tablet/form-factor coverage where promised |
| iOS Metal | At least two supported Apple SoC generations, lowest/current supported iOS bands, phone and tablet where promised |
| Linux shell | Each declared import/synchronization mechanism across every driver family claimed by that shell profile |

If hardware access prevents a cell, the profile is incomplete or deliberately narrower; the report
does not generalize beyond the tested categories. Vendor/console profiles are defined inside the
authorized SDK environment and use the same report schema without exposing confidential identifiers.

Qualification uses a conformant driver/API combination appropriate to the claimed version. A vendor's
CTS status is recorded as environment metadata, but Vulkan CTS does not replace Telorgon's own renderer,
host, visual, lifecycle, recovery, or performance suites.

## 13. Profile, report, and waiver records

### 13.1 Qualification profile

A versioned profile records:

- profile ID/version, required evidence layers, suite/case selectors, and expected capabilities;
- target triple, OS/platform bands, backend/API and shader-bundle versions;
- permitted device/driver selectors and forbidden substitutions;
- output/target configuration, host fixture, external-image mechanism, and platform services;
- visual fixture set and tolerance-manifest hashes;
- workload/baseline IDs and timing/memory/count budgets;
- validation settings and required auxiliary configurations; and
- required matrix cells and applicable approved waiver IDs.

### 13.2 Qualification report

The immutable report records:

- profile and schema versions, source revision, dirty-tree state, Rust/compiler/build profile, and
  dependency lock hash;
- machine, OS, target, backend/API, loader/runtime, validation/tool, adapter/device, driver, queue,
  display, resolution, color, present, and power metadata;
- capability path results and reasons;
- every test ID with evidence layer, outcome, duration, artifact hashes, messages, and waiver ID;
- validation messages and settings;
- visual metrics/artifact references;
- performance p50/p95/p99, memory/high-water, structural counters, and baseline comparisons;
- pass/fail/skip/unsupported/waived/not-run totals by layer and matrix cell;
- recovery/fault-injection results, known limitations, and unsupported behavior; and
- generator version, timestamp, reviewer/approver identity, and report digest.

Dirty-tree reports are useful engineering evidence but cannot qualify a release artifact.

### 13.3 Waivers

A waiver is its own reviewed file. It contains a stable ID, exact test/message/VUID, profile and
device/driver scope, observed behavior, risk classification, reason, linked issue, compensating
evidence, owner, approval, creation date, expiry date or release, and removal test. Broad regular
expressions, blanket driver-family suppression, and permanent `ignore` annotations are invalid.

Expired waivers fail qualification. Updating a driver, backend path, shader bundle, or affected
contract forces revalidation of the waiver. The raw failure remains visible in reports and logs.

## 14. Required automation and release gates

The eventual automation is split so failures identify their boundary:

1. E0 formatting, build-profile/dependency, docs/link, generated-file, and license jobs;
2. E1 unit/property and compile-contract jobs on required target triples;
3. E2 trace conformance and failure-injection jobs;
4. E3 shader regeneration/validation/reflection jobs;
5. backend-specific E4 offscreen GPU/visual/validation jobs;
6. E5 managed, E6 hosted, E7 platform, and E8 shell jobs on their declared machines; and
7. E9 performance and release qualification jobs on controlled machines.

Required jobs may be scheduled or manually dispatched when scarce hardware is involved, but their
absence blocks the corresponding production claim. A pull request can merge under an explicitly
documented development policy without every production device run; a release cannot claim a profile
without a current complete report.

## 15. Gate 6 reference and specification audit

```text
Concern:
Separate portable correctness, no-GPU validation, real GPU execution, pixels, presentation, hosted
ownership, platform behavior, shell interop, and production performance without allowing a skipped
or modeled path to masquerade as hardware acceptance.

Telorgon files/contracts affected:
ACCEPTANCE_AND_QUALIFICATION.md; PERFORMANCE.md; IMPLEMENTATION_STATUS.md; target
telorgon-renderer-trace and telorgon-backend-conformance packages; all Gate 3–5 acceptance sections.

Reference revisions, paths, and symbols inspected:
wgpu d99c241a3b9dcc0f6674d990d007d79e94d39862 — docs/testing.md test taxonomy;
tests/src TestParameters, capability/expectation handling, AdapterReport and image comparison;
tests/tests/wgpu-compile lifetime failures; wgpu-validation no-op tests; wgpu-gpu multi-adapter real
GPU tests and regression fixtures.
Flutter 51fd9afadf309ba5337320bd3653f5345c156cb9 — impeller/golden_tests README, GoldenTests,
GoldenDigest and backend/GPU dimensions; impeller testing screenshotters and renderer/backend unit
tests; shell/platform/embedder/tests/embedder_vk_unittests.cc native-proc interception and submission
proof.

Official specifications/documentation checked:
Khronos Vulkan validation overview and VK_LAYER_KHRONOS_validation documentation; synchronization
and GPU-assisted validation settings; Vulkan CTS repository and Khronos conformant-product records.

Invariants extracted:
Use the cheapest truthful layer; keep compile, no-GPU validation, real-GPU and visual suites
separate; express capability prerequisites; run real tests across adapters; identify GPU/backend in
golden evidence; intercept native calls when hosted ownership is part of the contract; retain
machine-readable artifacts and failures.

Failure/recovery cases extracted:
Missing prerequisite mistaken for pass, known driver failure hidden by a global skip, lifetime misuse
that only compile-fail tests expose, output variation without adapter identity, validation errors
that do not change pixels, host submission that internal counters omit, and memory initialization or
device-loss paths that ordinary success cases do not exercise.

Approaches rejected and why:
One workspace test command initializing GPUs; one screenshot as renderer acceptance; silent skip in
qualification; system-font goldens; auto-updated goldens; one permissive whole-image threshold;
validation and performance measured in the same run; fixed universal millisecond promises; Vulkan
CTS treated as Telorgon conformance; permanent broad warning suppressions.

Telorgon-specific decision:
E0–E9 evidence layers, explicit five-state outcomes, separate developer and qualification modes,
canonical linear-premultiplied visual comparison with region masks, structural performance
invariants, profile-driven timing/device matrices, expiring narrow waivers, and immutable reports.

Tests/diagnostics derived:
The matrices and report fields in sections 5–14, including trace validation, compile contracts,
artifact reflection, real offscreen pixels, managed/hosted call ownership, external synchronization,
native validation logs, counter/capture comparison, fault injection, golden heatmaps, and controlled
percentile baselines.

Known gaps requiring hardware/vendor validation:
Exact shipping OS/driver/device lists, final Metal debug/validation tooling, platform-service APIs,
real host engines, external-image mechanisms, HDR tolerances, supported refresh/power policies,
console tooling, and numeric workload budgets for each production profile.
```

Primary official references:

- [Khronos Vulkan validation overview](https://docs.vulkan.org/guide/latest/validation_overview.html)
- [Khronos validation-layer configuration](https://github.com/KhronosGroup/Vulkan-ValidationLayers/blob/main/docs/khronos_validation_layer.md)
- [Khronos GPU-assisted validation](https://github.com/KhronosGroup/Vulkan-ValidationLayers/blob/main/docs/gpu_validation.md)
- [Khronos Vulkan CTS source and instructions](https://github.com/KhronosGroup/VK-GL-CTS/blob/main/external/vulkancts/README.md)
- [Khronos Vulkan conformant products](https://www.khronos.org/conformance/adopters/conformant-products/vulkan)

The adjacent projects were inspected read-only. Their code, test vectors, and distinctive test
structure were not copied into Telorgon.

## 16. Gate completion criteria

Gate 6 is complete when:

1. evidence layers and per-cell outcomes cannot confuse modeled, skipped, offscreen, presented,
   hosted, platform, shell, or production evidence;
2. portable versus developer-hardware versus qualification behavior is explicit;
3. shared scene, trace, ABI/shader, renderer, visual, resource, recovery, managed, hosted, platform,
   shell, and performance matrices are fixed;
4. golden tolerances, validation handling, hardware matrices, reports, and waivers have enforceable
   rules;
5. target test package/file ownership is assigned without adding GPU or platform dependencies to
   portable crates;
6. reference revisions and official sources are recorded; and
7. all active architecture/planning documents link to this authority without claiming the planned
   suites already exist.

Gates 7 and 8 add component/runtime/control/domain correctness, usability, accessibility, and shell
cases to this matrix. Gate 9 adds the platform contract and exact cases without inventing a second
qualification system. The numbered architecture-gate sequence is now closed; later implementation
work must continue to use this evidence/result/report model.
