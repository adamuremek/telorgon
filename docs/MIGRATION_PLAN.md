# Telorgon Current-to-Target Migration Plan

> Packaging-consolidation status: the crate-disposition portions of this historical migration plan
> are superseded by the current single-package decision. Framework owners are modules of
> `telorgon`, `telorgon-macros` remains the required procedural-macro companion, and
> `telorgon-shader-build` remains unpublished tooling. Type, ownership, API, and deletion rationale
> below remain useful historical context. [Code layout](CODE_LAYOUT.md) and
> [Cargo publishing](PUBLISHING.md) define the current paths and registry boundaries.

## Status and authority

This document completes implementation-planning Gate 2. It maps the workspace that exists on
2026-08-20 to the target package architecture without treating the migration as a clean-room
rewrite. It is authoritative for:

- the disposition of every current crate;
- the destination of every current public type and top-level public function;
- source-file moves, compatibility bridges, and deliberate breaking changes;
- the order in which old implementations may be removed; and
- the conditions that allow a compatibility path to be deleted.

The immediate rendering interfaces and Slice 1–3 file ownership remain controlled by
[IMPLEMENTATION_BLUEPRINT.md](IMPLEMENTATION_BLUEPRINT.md). This migration plan controls how the
rest of the workspace reaches the broader package graph in
[PROJECT_SCOPE_AND_ARCHITECTURE.md](PROJECT_SCOPE_AND_ARCHITECTURE.md). Gate 3 native lifetime and
completion rules are fixed in
[GPU_OWNERSHIP_AND_SYNCHRONIZATION.md](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md). Gate 4 scene storage,
delta, GPU transfer-record, batching, upload, and shader moves are fixed in
[SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md). Gate 5 fixes managed/hosted platform,
shell/compositor, Metal, and mobile sequencing in
[PLATFORM_IMPLEMENTATION_ORDER.md](PLATFORM_IMPLEMENTATION_ORDER.md). Gate 6 fixes migration and
backend acceptance evidence, compile contracts, test package ownership, and qualification reporting
in [ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md). Gate 7 fixes component/state,
action, reconciliation, task, and text-runtime ownership plus the exact current-type migration in
[AUTHORING_AND_COMPONENT_RUNTIME.md](AUTHORING_AND_COMPONENT_RUNTIME.md).
Gate 8 fixes the domain cut, accessible behavior, catalog tiers, shell host boundary, source owners,
and Epoch E order in
[APPLICATION_AND_SHELL_PRIMITIVES.md](APPLICATION_AND_SHELL_PRIMITIVES.md).
Gate 9 fixes the view/lifecycle/event/service types, Winit translation, native IME/accessibility,
managed/embedded ownership, data transfer, native handles, exact platform source owners, and Epoch D
platform slices in [PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md).

This is a target migration contract, not a claim that the moves have occurred. Current availability
is still controlled by [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## 1. Migration rules

### 1.1 Outcome labels

The public API ledger uses these labels:

| Label | Meaning |
| --- | --- |
| **Keep** | Preserve the type's responsibility and normally its name in the same package. |
| **Move** | Move one implementation to a lower or more focused package; re-export only where dependency direction permits it. |
| **Redesign** | Preserve the use case, but replace an API whose ownership or semantics conflict with the target architecture. |
| **Remove** | Delete an obsolete model or public surface; do not provide a misleading compatibility implementation. |
| **Tool** | Keep the type inside a first-party tool, outside the framework API and umbrella prelude. |

### 1.2 One implementation at every step

A move is performed by moving the implementation and, when safe, re-exporting it from the old
owner. Codex must not copy a type into a new crate and maintain two versions. A bridge cannot create
a reverse dependency merely to preserve an old import path.

### 1.3 Compatibility is subordinate to truthful semantics

Telorgon is currently a `0.1.0` workspace and has no declared stable public API. Low-cost source
compatibility is still valuable, but these changes are intentionally breaking:

- `AppRuntime<App, R>` becomes renderer-free `AppRuntime<App>`;
- the `Renderer` trait is replaced by the backend/scene/frame/target contract;
- modeled `VulkanRenderer` and `VulkanStats` are removed rather than deprecated as Vulkan APIs;
- numeric `RenderTargetId` is replaced by backend-typed targets;
- presentation and mandatory readback leave the baseline renderer contract;
- raw `u64` external synchronization and external-texture handles are removed; and
- the current `telorgon-scene` package changes from UI-node storage to retained render-scene storage.

These breaks happen in named migration epochs and update all workspace consumers in the same change.
No `legacy` feature may silently retain the false Vulkan facade or a second renderer-owned runtime.

### 1.4 Bridges have deletion conditions

Every compatibility bridge below names a deletion condition. Passing time or reaching a version
number is not enough. The replacement must be operational, workspace examples must use it, and a
compile fixture must cover the intended public path before the bridge is removed.

### 1.5 Vulkan work is not blocked on the complete package split

Only the seams required by Slices 1–3 are performed before the first real Vulkan path. Component,
domain, accessibility, mobile, shell, trace, and second-backend packages are not prerequisites for
offscreen Vulkan rendering or owned Vulkan presentation.

## 2. Ordered migration epochs

### Epoch A: Baseline and mechanical source separation

1. Record the current public-type inventory and package dependency graph.
2. Add compile fixtures for the umbrella, managed app, headless software path, scene compiler, and
   first-party tools.
3. Split monotelorgon crate roots into cohesive files without changing behavior or public paths.
4. Keep every moved definition single-sourced through `pub use`.

Exit: behavior tests are unchanged, current examples compile, and crate roots primarily declare
modules and exports.

### Epoch B: Renderer/runtime seam — implementation Slice 1

Apply [IMPLEMENTATION_BLUEPRINT.md](IMPLEMENTATION_BLUEPRINT.md):

1. replace `Renderer` with the initial internal `RenderBackend` contract;
2. make `AppRuntime<App>` renderer-free;
3. move software presentation orchestration into the software native host;
4. remove the modeled Vulkan implementation and its software readback delegation; and
5. stop the umbrella from claiming modeled Vulkan types are operational.

Exit: UI preparation and backend execution are separately testable; software reference behavior
still works through an explicit host; no code path named Vulkan invokes the software renderer.

### Epoch C: Operational Vulkan — implementation Slices 2 and 3

1. replace the Vulkan model with the real modular `ash` implementation;
2. add offline `telorgon-shader-build` artifacts;
3. add the initial `telorgon-presenter-vulkan-winit`, then split it into
   `telorgon-presenter-vulkan-wsi`, `telorgon-presenter-dxgi`, and
   `telorgon-bridge-vulkan-dxgi` once the working Windows compositor path justifies those ownership
   boundaries; and
4. retain the software managed path as an explicit reference profile until Vulkan acceptance passes.

Gate 5 applies the first owned-presentation exit to Windows x86-64. Linux and mobile presentation do
not enter Epoch C merely because Winit can compile for those targets.

Exit: real offscreen Vulkan and Windows owned Vulkan presentation satisfy their slice exit criteria.

### Epoch D: Core semantic package split

1. create `telorgon-runtime`, `telorgon-input`, `telorgon-accessibility`, and
   `telorgon-renderer-software`;
2. move renderer-free runtime types out of the managed-host package using Gate 7's ordered component,
   nongeneric UI storage, typed action, task, and text slices;
3. perform the `telorgon-scene` namespace transition described in section 6;
4. create the Gate 9 neutral platform spine and deterministic conformance host;
5. extract the Winit view registry, event queue/translation, scheduling, and scoped handle access to
   `telorgon-platform-winit`;
6. create `telorgon-accessibility` and the selected AccessKit adapter packages without leaking native
   adapter dependencies into neutral crates;
7. create `telorgon-embed` with no Winit, presenter, queue, event-loop, or thread ownership;
8. reduce `telorgon-app` to managed application assembly and preserve only documented compatibility
   re-exports; and
9. complete Gate 5 hosted Vulkan before broadening optional managed-platform services.

Exit: runtime, input, retained scene, software rendering, managed presentation, and embedding build
as focused packages with no concrete backend dependency in the core UI/runtime graph.

### Epoch E: Public UI domains and shell split

After Gates 7 and 8 freeze the component and primitive behavior contracts, execute
[Gate 8's ordered slices](APPLICATION_AND_SHELL_PRIMITIVES.md#12-ordered-implementation-slices):

1. create the application primitive/component sibling packages;
2. create the shell primitive/component sibling packages;
3. replace `telorgon-compositor` with protocol-neutral `telorgon-shell` data and host contracts;
4. move basic controls out of the low-level `UiBuilder`; and
5. split application and shell theme namespaces without duplicating the theme engine.

Exit: application-only and shell-only profiles compile independently and neither domain depends on
the other.

### Epoch F: Conformance, RHI extraction, and cleanup

1. add `telorgon-renderer-trace` and `telorgon-backend-conformance` using Gate 6's exact evidence layers,
   package/file boundaries, shared suites, fixture rules, and report schemas;
2. extract a focused `telorgon-rhi` only after Vulkan plus the trace or a materially different
   backend prove a genuinely common contract;
3. move first-party applications into `telorgon-tools-*` packages;
4. replace flat umbrella exports with profiles and domain preludes; and
5. delete every bridge whose exit condition has passed.

Exit: the target dependency graph is enforced and no legacy feature selects obsolete ownership.

## 3. Current crate disposition

| Current crate | Disposition | Target responsibility |
| --- | --- | --- |
| `telorgon-core` | Keep and narrow | Geometry, color, neutral identifiers, time/value types, and shared small errors. Linux/Wayland conversion leaves core. |
| `telorgon-scene` | Repurpose in Epoch D | Current UI-node arena moves to `telorgon-ui`; package name becomes the retained render scene currently located in `telorgon-render::scene`. |
| `telorgon-ui` | Keep and split internally | Mounted UI storage, typed properties, foundation atoms, semantic records, and low-level mounting. Basic application controls leave after their domain packages exist. |
| `telorgon-layout` | Keep and split internally | Canonical incremental geometry, spatial/clip results, hit indices, focus order support, and virtualization. |
| `telorgon-text` | Keep and split internally | Shaping, retained runs, glyph data, atlas deltas, and later editing/IME-neutral text state. |
| `telorgon-theme` | Keep and split internally | Source parsing, compilation, identifiers, runtime scopes, domain namespaces, and diagnostics. |
| `telorgon-render` | Keep but narrow | Scene compilation and render planning; initial backend contract until a later RHI extraction. It stops owning retained scene storage and software rasterization. |
| `telorgon-material` | Keep and correct | Logical material/effect descriptions and fallback policy. Shader artifacts move to the shared offline build path; target pooling moves to render/backend execution. |
| `telorgon-renderer-vulkan` | Replace in place | Real direct Vulkan backend. The current CPU model is deleted, not archived as a second implementation. |
| `telorgon-app` | Keep but narrow | Managed application convenience host only. Runtime, input, platform traits, software renderer, and embedding move to focused packages. |
| `telorgon-compositor` | Retired | Protocol-neutral surface data and host contracts live in `telorgon-shell`; chrome lives in shell components; fake synchronization is removed. |
| `telorgon-ui-gallery` | Retired | The bundled gallery application and tool-local fixtures were removed; component behavior remains covered in the owning library packages. |
| `telorgon-theme-studio` | Retired | The bundled authoring application was removed; preview scopes and replacement remain library APIs. |
| `telorgon-theme-build` | Retired | The command wrapper was removed; source compilation and archive encoding remain in `telorgon-theme`. |
| `telorgon-theme-create` | Retired | The template-generating command wrapper was removed. |
| `telorgon` | Rebuild as umbrella | Curated features, `application` and `shell` preludes, managed `run`, embedded namespace, and explicit low-level modules; no independent runtime logic. |

## 4. Public API migration ledger

The inventory contains 160 public type declarations, one top-level public function, four top-level
public constants, and two public shader modules. Public methods and associated constants migrate
with their owning type unless an exception is named below.

### 4.1 `telorgon-core` — 11 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep** | `ColorRgba8`, `EdgeInsets`, `PointF`, `PointI`, `RectF`, `RectI`, `SizeF`, `SizeI`, `Transform2D` | Remain in `telorgon-core`, split into `color`, `geometry`, and later coordinate/color-convention modules. |
| **Move/Redesign** | `BinaryState` | Move to `telorgon-input` and become the neutral button/key state. Linux-value and Wayland-value conversions move to platform/protocol adapters. A deprecated `BinaryState` alias may remain after the final name is chosen. |
| **Remove/Replace** | `InputEvent` | The three-variant Linux-shaped event is replaced by the unified host-input vocabulary in `telorgon-input`. Do not preserve `from_linux_value`/`to_wayland` in core. |

Epoch D also adds neutral typed identifiers to `telorgon-core::id`: `UiNodeId`, `ImageId`,
`MaterialId`, `StyleId`, and `ThemeScopeId`. These definitions move from existing crates so UI,
scene, material, and theme packages can share identity without dependency cycles.

### 4.2 Current `telorgon-scene` — 7 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Move** | `NodeId` | Definition moves to `telorgon-core::id::UiNodeId`; `NodeId` is a deprecated alias during the internal migration. The target retained-scene package may re-export the alias without depending on UI. |
| **Move** | `Children`, `DirtyFlags`, `NodeArena`, `NodeCore`, `SparseSet`, `SubtreeRange` | Move once into `telorgon-ui::storage`. They remain low-level/expert APIs but leave the umbrella prelude. Direct `telorgon_scene::NodeArena`-style imports are an intentional Epoch D break because preserving them would make retained scene depend on UI. |

### 4.3 `telorgon-ui` — 42 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep** | `Background`, `Border`, `BorderSide`, `BoxSizing`, `BoxStyle`, `CornerRadii`, `Flow`, `LayoutStyle`, `Overflow`, `Shadow`, `ShadowList`, `SizeRule`, `SizeRule2D` | Shared foundation atoms remain in focused `telorgon-ui` style/layout modules. Domain primitives compose them. |
| **Move** | `ImageId`, `MaterialId`, `StyleId`, `ThemeScopeId` | Definitions move to `telorgon-core::id`; `telorgon-scene`, `telorgon-material`, and `telorgon-theme` re-export their owned identifiers. `telorgon-ui` keeps temporary deprecated re-exports only while existing properties use them. |
| **Keep** | `StringId` | Remains the mounted-UI string-interner identifier. It is not a process-global string or asset ID. |
| **Keep/Refine** | `StateBits`, `MountedUi`, `NodeKind`, `TextStyle`, `TextVisual`, `ImageVisual`, `Interaction`, `SemanticNode`, `SemanticRole`, `UiRoot`, `UiMemoryReport`, `UiDiagnostics` | Remain low-level UI storage/record types. `MountedUi<A>` becomes nongeneric; `Interaction` splits resolved node interaction from runtime listener/action routes; editing visuals reference text snapshots/runs rather than interned revision strings. Semantic processing and platform deltas live in `telorgon-accessibility`; semantic inputs remain mounted UI data. |
| **Redesign** | `UiBuilder`, `UiTransaction`, `TransactionResult`, `Property`, `PropertyValue`, `NodeBlueprint`, `TextHandle`, `ControlHandle`, `ScrollHandle` | Preserve mount-once/property patching. Builder mechanics become `telorgon-ui::MountWriter` wrapped by `telorgon-runtime::Ui<A>`; runtime transactions own atomic state/structure/action/task commit; `Property<T>` remains advanced and typed; `PropertyValue`/mount descriptors become private; broad handles become Gate 8's focused `TextController`, `ScrollController`, `SelectionModel`, overlay/navigation controllers, or focused advanced refs according to responsibility. |
| **Move/Redesign** | `EventPhase`, `UiEvent`, `UiEventKind` | Nongeneric neutral routing phases/events move to `telorgon-input`; text edit payloads live in `telorgon-text`; typed moved actions and private generational routes live in `telorgon-runtime`. Temporary `telorgon-ui` re-exports are allowed because UI may depend on the lower input vocabulary, not vice versa. |

`UiNodeId` remains the public spelling in new APIs. `NodeId` exists only as a migration alias.

### 4.4 `telorgon-layout` — 6 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep** | `ClipId`, `ComputedLayout`, `LayoutDiagnostics`, `LayoutEngine`, `SpatialId`, `VirtualCollection` | Remain in `telorgon-layout`. Their files split by model, engine, spatial/clip, hit testing, diagnostics, and virtualization. Backend scene code consumes canonical layout output instead of recalculating it. |

`ClipId` and `SpatialId` stay logical identifiers; they are not native GPU handles.

### 4.5 `telorgon-render` — 28 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Move/Refine** | `BoxInstance`, `DamageRegion`, `DenseInstances`, `DirtyRanges`, `GlyphInstance`, `ImageInstance`, `MaterialInstance`, `PrimitiveKind`, `RangePatch`, `RenderClip`, `RenderScene`, `RenderSceneDelta`, `RenderSpatialNode` | Move after Slice 3 into the repurposed `telorgon-scene` under the [Gate 4 scene contract](SCENE_GPU_ABI_AND_SHADERS.md#3-scene-native-records). Replace UI-authoring dependencies with scene-native records, `DenseInstances` swap removal with typed generational slot tables, translation/scale with full affine transforms, and normalized glyph UVs with page-plus-texel rectangles. `telorgon-render` re-exports moved definitions for one migration epoch. Storage helpers remain expert APIs, not umbrella-prelude items. |
| **Redesign/Move** | `DrawItem` | Painter-order identity moves to `telorgon-scene` as `ScenePaintItem`; pipeline/batch/target selection is removed and produced by `telorgon-render` planning. |
| **Keep/Redesign** | `BlendMode`, `BatchKey`, `PipelineKind` | Stay in `telorgon-render` as planner policy/output and become the backend-neutral blend selection, `PipelineKey`, and `BindingKey` fixed by Gate 4. Only adjacent compatible paint items merge. |
| **Keep** | `CompileStats`, `SceneCompiler` | Remain in `telorgon-render`; compiler inputs change to the new UI/runtime packages through mechanical import updates. |
| **Keep/Refine** | `RenderError`, `RenderResult` | Remain the initial internal rendering error/result in Slices 1–3. Gate 3 removes surface/presentation state from the core error and assigns device recovery to the host. A later `telorgon-rhi` may own lower-level errors only after extraction is justified. |
| **Remove/Replace** | `PresentRequest`, `PresentStats`, `RenderTargetId`, `Renderer` | Replaced in Slice 1 by `RenderRequest`, backend-typed targets, `RenderBackend`, and presenter-owned presentation statistics. No compatibility implementation preserves the old combined contract. |
| **Redesign** | `ReadbackRequest`, `RenderedFrame` | Become the optional readback types `ReadbackRequest` and `ReadbackImage`. They are not presentation results and are not required by `RenderBackend`. |
| **Move** | `SoftwareRenderer` | Temporarily remains during Slices 1–3, then moves once to `telorgon-renderer-software`. The umbrella may re-export it under an explicit software/headless feature; `telorgon-render` cannot re-export it afterward without creating a cycle. |

### 4.6 `telorgon-renderer-vulkan` — 3 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Redesign** | `VulkanConfig` | Keep the name for real backend policy, but replace model-only fields with validation, adapter, capability, memory-budget, and owned/hosted policy. |
| **Remove/Replace** | `VulkanRenderer` | Remove the CPU model. Replace it with `VulkanInstance`, `VulkanDevice`, `VulkanScene`, a sealed lifetime-bearing `VulkanFrameContext`, owned/hosted recording frames, `VulkanRecordedFrame`, `VulkanTarget`, and opaque completion/receipt types. Do not create a type alias. |
| **Remove/Replace** | `VulkanStats` | Replace predicted values with `VulkanDiagnostics` plus actual scene-update/render/presentation statistics. Do not preserve estimates under a deprecated name. |

The public `shaders` byte constants also leave the API. Generated shader bundles are private
validated artifacts loaded by the Vulkan shader module.

### 4.7 `telorgon-app` — 14 types, four listener constants, and `run_native`

| Outcome | Current public types/function | Destination and rule |
| --- | --- | --- |
| **Move/Redesign** | `Application`, `AppContext`, `AppEvent`, `Command`, `TimerId` | The normal root becomes a Gate 7 `Component`; a compatibility application adapter moves to `telorgon-runtime::application`. `AppContext` splits into create/update/application contexts; neutral events move to input/text; scheduling and Gate 9 service commands split; timers enqueue typed actions. `telorgon-app` temporarily re-exports the curated surface. |
| **Redesign/Move** | `AppRuntime`, `FrameDiagnostics`, `FrameScheduler`, `SceneDeltaQueue` | Slice 1 removes the renderer generic. Gate 7 moves the component/UI-node portion to renderer-free `telorgon-runtime::ViewRuntime` and the scheduler there; view/render diagnostics remain with managed/embed assembly; `SceneDeltaQueue` moves to render/host transport because the component runtime does not own renderer consumption. |
| **Move/Replace** | `PlatformInput` | Move to `telorgon-input` and become the host-input entry type after pointer/touch/pen/IME requirements are fixed. `telorgon-app` only translates Winit events. |
| **Move/Redesign** | `LISTEN_POINTER`, `LISTEN_ACTION`, `LISTEN_KEY`, `LISTEN_FOCUS` | Replace raw `u16` masks with a typed `EventInterest` value in `telorgon-input`. Deprecated constant aliases may exist only while current mounted listeners migrate. |
| **Redesign/Move** | `HeadlessRuntime` | Preserve a temporary software convenience in Slices 1–3. Final headless assembly belongs under `telorgon-embed::headless` with explicit `telorgon-renderer-software`; it does not belong to the managed-window package. |
| **Keep/Refine** | `WindowConfig`, `AppError`, `AppResult` | `WindowConfig` becomes the managed `WindowOptions` builder; errors stay in `telorgon-app` while retaining typed runtime/platform/presenter sources instead of flattening them to strings. |
| **Keep as bridge** | `run_native` | Becomes a deprecated wrapper over the default managed `telorgon::run`. Remove after all first-party examples use `run` and the platform/profile matrix has a compile fixture for the default. |

The current native host's physical-as-logical positions, sentinel cursor-leave coordinates, fixed
line-scroll multiplier, numeric-only key mapping, elapsed-millisecond timestamps, unconditional
close-to-exit behavior, ignored clipboard commands, and combined Winit/Softbuffer ownership are not
compatibility semantics. Gate 9 replaces them with revisioned metrics/stamps, lossless neutral
input, request outcomes, close policy, services, and separate platform/presenter owners.

### 4.8 `telorgon-compositor` — 7 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Move/Redesign** | `SurfaceId`, `SurfaceState`, `SurfaceWorld` | `SurfaceId` moves to `telorgon-shell`; mutable state/world become immutable revisioned `ClientSurfaceSnapshot` values plus host snapshot transport. They describe protocol-neutral host truth; they do not own Wayland, discovery, or compositor policy. |
| **Move/Redesign** | `ExternalTextureId` | Replace with a typed logical `ExternalImageId` in the scene/shell contract and backend-specific import descriptors in interop packages. The numeric handle is not a native texture. |
| **Remove/Replace** | `ExternalSync` | Delete the three-`u64` model. Gate 3 defines opaque owned/host completion domains and linear hosted receipts; Gates 4/9 define typed external-image acquire/release contracts. No raw semaphore compatibility path is allowed. |
| **Remove/Replace** | `Compositor` | Replace with `telorgon-shell` snapshot/request transport, output/client-surface primitives, and ordinary shell components. Telorgon still does not implement display protocols, discovery, authorization, or window policy. |
| **Move/Redesign** | `ChromeAction` | Splits into component-local typed actions and authority-specific `SurfaceRequest` values validated by the host. It is not a global compositor action enum and never optimistically commits host state. |

The `telorgon-compositor` package is removed only after shell fixtures compile against `telorgon-shell`
and Vulkan external-image import has an explicit capability result. Its current model never becomes
a fallback that copies client surfaces through the CPU.

### 4.9 `telorgon-text` — 16 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep/Refine** | `AtlasGlyph`, `AtlasPageUpdate`, `GlyphAtlas`, `GlyphAtlasView`, `PreparedText`, `RetainedTextRequest`, `RetainedTextRun`, `RetainedTextSystem`, `TextCacheStats`, `TextError`, `TextLayoutRequest`, `TextResult`, `TextRunId`, `TextRunKey` | Remain in `telorgon-text`, split into error, style, shaping, retained runs, glyph, atlas, and cache. Gate 7 adds opaque revisioned buffer/snapshot/range/edit/composition/navigation/session modules; CPU atlas views become expert software/test APIs. |
| **Redesign name** | `FontTextRenderer` | Rename to `TextEngine` because it coordinates shaping/raster/glyph caching but does not render a Telorgon scene. A deprecated alias is allowed while call sites migrate. |
| **Redesign name** | `TextStyle` | Rename the text-package value to `ResolvedTextStyle` so it does not collide with the authoring `telorgon-ui::TextStyle`. A deprecated alias is safe inside `telorgon-text`. |

### 4.10 `telorgon-theme` — 11 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep** | `CompiledStyle`, `CompiledTheme`, `PaintSource`, `PaintStyle`, `StyleSource`, `ThemeDiagnostic`, `ThemeError`, `ThemeFormat`, `ThemeSource`, `ThemeResult` | Remain in `telorgon-theme`, split into source, compiler, compiled model, diagnostics, archive, and resolver modules. `StyleId`/`ThemeScopeId` definitions come from core and are re-exported by theme. |
| **Redesign** | `ThemeRuntime` | Evolves into one engine with distinct application, shell, and preview namespaces plus typed Gate 8 component style contracts. Keep an alias only if resolver semantics remain identical; do not encode the application domain as hard-coded scope zero in the final contract. |

### 4.11 `telorgon-material` — 5 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Keep/Redesign** | `MaterialContract`, `MaterialLibrary`, `MaterialPass`, `MaterialPassKind` | Remain logical material descriptions in `telorgon-material`. Gate 4 replaces fixed blur-specific pass wiring with shader-bundle/capability/fallback descriptions. |
| **Remove** | `TargetPool` | The unused integer bookkeeping model was deleted. Concrete backends own real transient-resource allocation. |

Public material shader byte constants are removed. The material crate refers to logical shader
bundle IDs; `telorgon-shader-build` creates backend artifacts.

### 4.12 First-party tool crates — 10 types

| Outcome | Current public types | Destination and rule |
| --- | --- | --- |
| **Remove** | `DocumentError`, `StudioAction`, `ThemeDocument`, `ThemeStudio` | Removed with the bundled Theme Studio. Runtime theme APIs remain in `telorgon-theme`. |
| **Remove** | `DebugOverlay`, `GalleryAction`, `GalleryApp`, `PerformanceSnapshot`, `PreviewState`, `Specimen` | Removed with the bundled gallery. Public components remain in their application packages. |

The build/create tools currently declare no public library types. Their binary names may remain
stable even when package names gain the `telorgon-tools-` prefix.

### 4.13 `telorgon` umbrella exports

The current umbrella flatly re-exports most low-level types and always depends on application,
software, compositor, material, and modeled Vulkan crates. The target umbrella instead provides:

```text
telorgon::application::prelude   Default application primitives/components and lifecycle
telorgon::shell::prelude         Shell primitives/components and host-neutral commands
telorgon::embed                  Host-driven multi-view and render-area entry points
telorgon::run                    Default managed application convenience
telorgon::{core, ui, layout, text, theme, scene, render}
telorgon::renderer_vulkan        Explicit backend-implementer/native integration surface
```

Flat re-exports may remain deprecated only when they do not force an unselected domain/backend into
the dependency graph. `VulkanRenderer`, `VulkanStats`, `Renderer`, combined presentation types, and
raw compositor synchronization are removed immediately at their breaking epoch.

## 5. Source-file move and deletion ledger

| Current source | Action | Final owner or deletion condition |
| --- | --- | --- |
| `crates/telorgon/src/core/input.rs` | Move neutral state; delete platform conversions | `telorgon-input` event/pointer/keyboard/route modules; Gate 8 adds shared activation, focus, composite, gesture, and shortcut transition modules. Linux/Wayland conversions live only in adapters. |
| `crates/telorgon/src/core/lib.rs` | Keep export-only | Add focused `id.rs`, time/value/error modules only when used. |
| `crates/telorgon/src/scene/arena.rs` | Move once | `crates/telorgon/src/ui/storage/arena.rs`; `UiNodeId` definition comes from core. |
| `crates/telorgon/src/scene/sparse_set.rs` | Move once | `crates/telorgon/src/ui/storage/sparse_set.rs`. |
| `crates/telorgon/src/scene/lib.rs` | Repurpose | Becomes retained-scene exports after the two moves above and the `telorgon-render::scene` migration. |
| `crates/telorgon/src/ui/lib.rs` | Mechanically split, then Gate 7/8 narrow | `node`, `style`, `layout_style`, `visual`, `interaction_state`, `semantics`, `foundation`, `external_content`, `overlay`, `storage`, `property`, `mount_writer`, `diagnostics`, and temporary event bridge modules; action storage, public blueprints, and broad transactions leave. |
| `crates/telorgon/src/layout/lib.rs` | Mechanically split, then Gate 8 extend | `model`, `engine`, `measure`, `arrange`, `spatial`, `hit_test`, `virtualization`, `scroll`, `virtual_range`, `popup_placement`, and `diagnostics`. |
| `crates/telorgon/src/render/renderer.rs` | Delete in Slice 1 | Replaced by Blueprint `backend`, `error`, `request`, `stats`, `target`, and `readback` modules. |
| `crates/telorgon/src/render/scene.rs` | Keep through Slice 3, then replace using Gate 4 | Split scene-native records, typed tables, snapshots, deltas, paint items, spatial/clip records, and logical resources under target `telorgon-scene`; keep UI-to-scene conversion and planning in `telorgon-render`; retain temporary planner/compiler-facing re-exports. |
| `crates/telorgon/src/render/software.rs` | Keep through Slice 3, then move | `telorgon-renderer-software`; remove direct `telorgon-render` export when all workspace consumers migrate. |
| `crates/telorgon/src/render/compiler.rs` | Keep and later split | Remains `telorgon-render`; split only by cohesive compile stages after the scene ABI is fixed. |
| `crates/telorgon/src/renderer_vulkan/lib.rs` | Replace, not rename | Delete CPU model after Slice 1 replacement modules exist; do not preserve it as `model.rs`. |
| `telorgon-renderer-vulkan/build.rs` | Delete | Ordinary builds embed validated generated artifacts and do not discover system shader compilers. |
| `crates/telorgon/src/application_host/lib.rs` | Split in Slice 1 | Use the exact Blueprint files; runtime files move to `telorgon-runtime` in Epoch D. |
| `crates/telorgon/src/application_host/native.rs` | Split and delete | Follow Gate 9's current-to-target ledger: Winit registry/translation/scheduling to `telorgon-platform-winit`, software presentation to the explicit software presenter/assembly, runtime state to `telorgon-runtime`, and managed selection to focused `telorgon-app` files. |
| `telorgon-compositor/src/lib.rs` | Decompose, then delete crate | Stable IDs and immutable snapshot concepts to `telorgon-shell`; output/client-surface placement to shell primitives; chrome to shell components; mutable registry owner and raw sync deleted. |
| `crates/telorgon/src/text/lib.rs` | Mechanically split, then extend under Gates 7/8 | `error`, `style`, `shaping`, `glyph`, `atlas`, `retained`, `cache`, then `buffer`, `snapshot`, `range`, `edit`, `composition`, `navigation`, neutral `session`, `editor_state`, `edit_history`, and `transform`. |
| `crates/telorgon/src/theme/lib.rs` | Mechanically split | `source`, `compiler`, `compiled`, `resolver`, `scope`, `archive`, `diagnostics`, `error`. |
| `crates/telorgon/src/material/lib.rs` | Split and correct | `id`, `contract`, `library`, `fallback`; target-pool implementation leaves. |
| `telorgon-material/build.rs` | Delete | Material artifacts are produced by `telorgon-shader-build`, never ad-hoc host compiler discovery. |
| Gallery/Studio monotelorgon libraries | Split as tools evolve | Application, specimens/editor, diagnostics, state, and fixtures remain tool-only. |
| Umbrella `telorgon/src/lib.rs` | Replace flat root | Feature-gated namespaces and curated preludes; crate root remains exports/documentation only. |

Mechanical splitting must be done before unrelated behavior is added to a large file, but it need
not all occur in one change. Each split must preserve tests and use `pub use` from a single owner.

## 6. The `telorgon-scene` namespace transition

The existing package name conflicts with the target architecture and requires one controlled
breaking epoch:

1. Add `UiNodeId` and the other neutral logical IDs to `telorgon-core::id`.
2. Move `NodeArena`, `NodeCore`, `DirtyFlags`, `SparseSet`, `Children`, and `SubtreeRange` into
   `telorgon-ui::storage` and update all workspace imports.
3. Keep a deprecated `NodeId = UiNodeId` alias in core and the umbrella; do not make the target
   scene package depend on UI to preserve old arena paths.
4. Apply [Gate 4's scene-native/GPU-record split](SCENE_GPU_ABI_AND_SHADERS.md), then move retained render-scene storage out of
   `telorgon-render::scene` into `telorgon-scene`. Preserve rendered behavior and delta identity, but do
   not preserve public fields that would make retained scene depend on UI authoring types.
5. Make `telorgon-render` depend on `telorgon-scene` and temporarily re-export moved render-scene types.
6. Update Vulkan, software, compositor/shell, benchmarks, and tests to the new canonical paths.
7. Remove render re-exports after all first-party consumers and compatibility fixtures use
   `telorgon-scene` directly.

This order avoids a package cycle and ensures that the package named scene owns the scene presented
to render backends rather than the private storage of the UI tree.

## 7. New package creation ledger

| New package | Created when | Initial source/contract |
| --- | --- | --- |
| `telorgon-gpu-abi` | Before Slice 2 shader/pipeline work | Gate 4's exact versioned POD transfer records, packed-color helpers, and compile-time size/alignment/offset assertions; no renderer, allocation, scene ownership, or graphics-API types. |
| `telorgon-shader-build` | Slice 2 | New offline compiler/manifest tool from the Blueprint; replaces renderer/material build scripts. |
| `telorgon-presentation` | Presentation separation | Neutral revisioned metrics, lifecycle, acquire/present, linear-frame, and completion-stage contracts; no graphics API or window-system dependency. |
| `telorgon-presenter-vulkan-wsi` | Slice 3 extraction | Vulkan surface/swapchain/acquire/present owner; no UI/runtime or DXGI code. |
| `telorgon-presenter-dxgi` | Windows presentation extraction | Windows-only D3D11/DXGI native device, HWND swapchain, copy, resize, present, and fence-wait owner; no Vulkan dependency. |
| `telorgon-bridge-vulkan-dxgi` | Windows presentation extraction | Windows-only shared-image, Vulkan import, keyed-mutex, imported-timeline, adapter-validation, and dual-API retirement owner. |
| `telorgon-presenter-softbuffer` | Epoch D extraction | Native software surface, framebuffer transfer, damage, present, suspend, and shutdown owner; no runtime/scene interpretation. |
| `telorgon-presenter-vulkan-winit` | Retired after compatibility transition | Removed after internal imports moved to the owning WSI and Vulkan/DXGI bridge packages. |
| `telorgon-runtime` | Epoch D | Gate 7 mount-once components, owner-scoped state/reads, atomic transactions, typed actions, keyed reconciliation, tasks, scheduling, and one renderer-free mounted view runtime. |
| `telorgon-input` | Epoch D | Gate 7 nongeneric neutral event, pointer/touch/pen, key/modifier, route, propagation, and default-response values; no platform conversions or component actions. |
| `telorgon-accessibility` | Epoch D | Gate 9 semantic tree, revisioned snapshots/deltas, actions, text ranges, and imported attachment model; no platform API. |
| `telorgon-accessibility-accesskit` | Epoch D | Neutral semantics-to-AccessKit mapping; no window/event-loop ownership. |
| `telorgon-accessibility-accesskit-winit` | Epoch D | Per-view Winit AccessKit activation, event-first processing, callback queueing, and action routing. |
| `telorgon-renderer-software` | Epoch D | Single move of `SoftwareRenderer`; deterministic reference/headless path. |
| `telorgon-platform` | Epoch D | Gate 9 IDs/stamps, independent lifecycle snapshots, revisioned metrics, scheduling, request outcomes, capabilities, and narrow service traits. |
| `telorgon-platform-winit` | Epoch D | Winit lifecycle/event translation extracted from managed app; the assembly selects a presenter, but this package owns no renderer. |
| `telorgon-platform-conformance` | Epoch D | Fake clock/services and deterministic lifecycle, input, text, accessibility, and data-transfer fixtures. |
| `telorgon-platform-clipboard-arboard` | Epoch D desktop services | Optional Tier-A text/image bridge; never the neutral clipboard/data-offer model. |
| `telorgon-platform-windows` / `-linux` | Gate 5 P1/P3, when needed | Native desktop services Winit cannot represent; no runtime or renderer ownership. |
| `telorgon-embed` | Epoch D | New `UiHost`/multi-view/render-area assembly over runtime and backend contracts. |
| `telorgon-primitives-application` | Epoch E | Gate 8 application regions, HUD/viewport/world anchors, and render-target/video content; no shell dependency. |
| `telorgon-components-application` | Epoch E | Gate 8 Tier A accessible controls first, then explicitly supported Tier B/C application/tool/game components. |
| `telorgon-shell` | Epoch E | Gate 8 protocol-neutral IDs, immutable snapshots, capabilities, authority-specific requests/results, and host transport; no protocol or policy implementation. |
| `telorgon-primitives-shell` | Epoch E | Gate 8 authorized output/layer/client-surface/input-geometry primitives over `telorgon-shell`; no application dependency. |
| `telorgon-components-shell` | Epoch E | Gate 8 chrome/workspace/panel/launcher/status/notification/secure components over shell primitives. |
| `telorgon-renderer-metal` | Gate 5 P5 | Direct Metal backend created after hosted Vulkan proves the common seam; no AppKit/UIKit/Winit ownership. |
| `telorgon-presenter-metal-winit` | Gate 5 P5 | Managed macOS/iOS Winit/CAMetalLayer drawable owner; separate from Metal command execution. |
| `telorgon-platform-apple` | Gate 5 P5/P8, when needed | Native macOS/iOS services that the neutral and Winit layers cannot correctly provide. |
| `telorgon-platform-android` | Gate 5 P7, when needed | Android lifecycle/services beyond the neutral Winit event layer; no Vulkan execution. |
| `telorgon-renderer-trace` | Epoch F | Non-rendering resource, usage, lifetime, and plan validator. |
| `telorgon-backend-conformance` | Epoch F | Shared backend compile, lifetime, offscreen, hosted, recovery, and visual suites. |
| `telorgon-rhi` | Epoch F, conditionally | Extract only proven backend-common contracts after Vulkan plus trace or another explicit API. |
| `telorgon-tools-*` | Epoch F or when touched | Renamed first-party tools; binary names may stay stable. |

Creating an empty placeholder crate does not advance the migration. A package is added when it has
one usable contract, its focused tests, and a dependency-boundary check.

## 8. Compatibility bridge ledger

| Bridge | Allowed implementation | Delete when |
| --- | --- | --- |
| `run_native` | Removed; use `telorgon::run` | Completed after the default managed profile and first-party components compiled. |
| Lifecycle types re-exported by `telorgon-app` | `pub use telorgon_runtime::*` for the curated application surface | Umbrella/domain preludes are stable and direct app users have a documented migration path. |
| Input events re-exported by app/UI | Re-export lower `telorgon-input` types only | Platform adapters and runtime use canonical input paths. |
| Core logical IDs re-exported by owner crates | Re-export the same core definitions | New public examples use owner/core paths and no compatibility fixture requires old UI paths. |
| Render-scene types re-exported by `telorgon-render` | Re-export the same `telorgon-scene` definitions | All first-party backends and tools import the scene package directly. |
| `FontTextRenderer`/resolved `TextStyle` old names | Removed; use `TextEngine`/`ResolvedTextStyle` | Completed after all first-party call sites migrated. |
| `ThemeRuntime` old name | Removed; use `ThemeRegistry` | Completed after application/shell/preview domain tests migrated. |
| Software renderer umbrella export | Feature-gated re-export from `telorgon-renderer-software` | It may remain permanently under an explicit `headless`/`software` namespace; it must not be enabled by Vulkan profiles. |
| Tool binary names | Cargo `[[bin]]` names independent of package names | May remain permanently if they are the desired user-facing commands. |

No bridge is allowed for `VulkanRenderer`, `VulkanStats`, raw `ExternalSync`, `RenderTargetId`, the
old `Renderer` trait, or the renderer generic on `AppRuntime`.

## 9. Dependency direction after Epoch D

Rows lower in this table may depend only on the listed packages from the same or earlier rows. The
table does not require every allowed edge.

| Layer | Packages | Allowed direct foundations |
| --- | --- | --- |
| Neutral foundation | `telorgon-core`, `telorgon-gpu-abi` | Standard library and deliberately selected value/POD-layout dependencies only. The two packages need not depend on each other. |
| UI foundations | `telorgon-ui`, `telorgon-input`, `telorgon-text` | `telorgon-core`; temporary UI input re-exports may add `telorgon-ui -> telorgon-input`. |
| UI processors | `telorgon-layout`, `telorgon-accessibility`, `telorgon-theme` | Core plus the specific UI/text/input records they process; no runtime, platform, or renderer. |
| Render data | `telorgon-scene`, `telorgon-material` | Core and explicitly approved scene-resource value contracts; no UI authoring, domain component, runtime, platform, or backend package. |
| Planning | `telorgon-render` | Core, retained scene, GPU ABI, material, and compiler-facing UI/layout/text inputs; no concrete backend or presenter. |
| Runtime | `telorgon-runtime` | Core plus UI/input/text foundations for components, state, bindings, transactions, actions, reconciliation, tasks, scheduling, and mounted-view invalidations; no layout/render planning, concrete backend, platform, WSI, or domain component package. |
| Backends | `telorgon-renderer-software`, `telorgon-renderer-vulkan`, future renderers | Render/scene contracts and backend-specific dependencies; never application or shell components. |
| Platform/presentation | `telorgon-platform`, `telorgon-platform-winit`, `telorgon-presentation`, `telorgon-presenter-vulkan-wsi`, `telorgon-presenter-dxgi`, `telorgon-bridge-vulkan-dxgi`, `telorgon-presenter-softbuffer` | Platform-neutral traits plus explicitly selected, typed presenter/bridge assemblies; WSI and native presentation remain absent from offscreen profiles. |
| Domain models | `telorgon-shell` | Core plus deliberately approved opaque logical external-content/synchronization references from lower contracts; protocol/native/backend types and application components are forbidden. |
| Domain primitives | `telorgon-primitives-application`, `telorgon-primitives-shell` | Runtime/UI foundations; shell primitives may additionally depend on `telorgon-shell`; application primitives never do. |
| Domain components | `telorgon-components-application`, `telorgon-components-shell` | Their matching primitive package and shared foundations; shell components may additionally use `telorgon-shell`; cross-domain edges are forbidden. |
| Hosts | `telorgon-app`, `telorgon-embed` | Runtime plus required UI processors/render planning and explicitly selected platform/backend packages; they remain siblings. |
| Convenience/tools | `telorgon`, `telorgon-tools-*` | Feature-selected packages only; the umbrella adds no implementation logic. |

Normative rules are:

- `telorgon-core`, UI, layout, input, text, accessibility, theme, and runtime do not depend on a
  concrete renderer or platform;
- retained `telorgon-scene` does not depend on UI domain components or shell policy;
- renderers consume scene/render contracts but core packages never re-export concrete backends;
- managed app and embed are siblings;
- application and shell primitive/component packages are siblings; and
- WSI/presentation packages do not enter offscreen or command-only dependency profiles.

Gate 4 fixes the direction as UI/layout/text inputs -> `telorgon-render` compiler -> `telorgon-scene`,
with `telorgon-render` planning consuming `telorgon-scene` and `telorgon-gpu-abi`. Scene and GPU ABI
packages never depend on UI authoring or a concrete backend. A package cycle is never accepted as a
temporary migration shortcut.

## 10. Migration validation

Every migration work package must perform the checks applicable to its scope:

1. `cargo check` the directly changed packages and their reverse workspace dependents.
2. Run unit tests for moved implementations; a file move must not reduce test count or coverage.
3. Compile, but do not launch, the gallery, Theme Studio, managed example, and tool binaries.
4. Compile independent umbrella profiles so application, embedded, shell, headless, and Vulkan
   features do not pull unselected domains/backends.
5. Run a public-API inventory comparison and account for every removal in this document.
6. Add compile fixtures for supported bridges and compile-fail fixtures for forbidden dependency
   direction or invalid ownership where Rust can express it.
7. Run architecture checks for cycles, concrete backend leakage, and unexpected Winit/Vulkan/SDK
   dependencies in core packages.
8. Update `IMPLEMENTATION_STATUS.md` only when operational behavior changes, not for a file move.
9. Record each applicable result as pass/fail/skip/unsupported/waived under the Gate 6 evidence
   layer; a portable or trace result cannot stand in for required hardware or platform evidence.

Hardware-presenting examples and GUI applications remain user-run. CI/agent validation is compile,
unit, headless, offscreen, and explicitly configured hardware-test work only. Developer hardware
skips and qualification failures follow
[Gate 6's hardware policy](ACCEPTANCE_AND_QUALIFICATION.md#4-run-classes-and-hardware-skip-policy).

## 11. Gate 2 reference and specification audit

Gate 2 reuses the two-implementation ownership comparison recorded in the Gate 1 audit:

- `../other-rendering-libs/wgpu/wgpu-hal/src/lib.rs` for separated device, queue, surface, command,
  acquire, and presentation contracts;
- Flutter Impeller Vulkan context, command queue, and command-pool sources for owned/embedder
  contexts and completion-safe command resource reuse; and
- Slint core/renderer/backend separation for keeping platform selection outside mounted UI storage.

The lasting Vulkan-related removals were cross-checked against primary Khronos documentation:

- the [Vulkan WSI specification](https://docs.vulkan.org/spec/latest/chapters/VK_KHR_surface/wsi.html)
  requires use of a presentable image to be bounded by acquisition and release through presentation,
  and requires host synchronization for swapchains, acquire primitives, and normally queues;
- the [Vulkan command-buffer lifecycle](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html)
  forbids resetting or modifying pending command buffers; and
- the [Vulkan external-memory and synchronization guide](https://docs.vulkan.org/guide/latest/extensions/external.html)
  distinguishes typed imported/exported handle kinds and payload ownership.

Consequences for migration:

- WSI stays in the presenter instead of a universal renderer `present` method;
- frame/command resources cannot be represented as freely reusable numeric model slots;
- external synchronization cannot be three context-free `u64` values; and
- old modeled types are removed where a compatibility alias would imply safety or behavior that
  does not exist.

No adjacent source is copied or added as a dependency.

## 12. Gate completion criteria

Gate 2 is complete when:

- all 16 current workspace crates have a recorded disposition;
- all 160 current public type declarations, four listener constants, two shader modules, and
  `run_native` have a destination or removal rule;
- every source implementation scheduled to move has one owner and a stated ordering;
- every compatibility bridge has a deletion condition;
- every deliberate source break is named rather than discovered during implementation;
- the `telorgon-scene` namespace conflict has a cycle-free transition;
- the migration preserves the GPU-first Slice 1–3 priority; and
- this document is linked from the canonical documentation index and current code-layout guide.
