# Platform Implementation Order

Status: **Gate 5 — accepted implementation sequence**

This document fixes the order in which Telorgon becomes operational on managed desktop, hosted,
shell/compositor, Metal, and mobile platforms. It prevents “cross-platform” from becoming an
unbounded first milestone and prevents a successful window on one operating system from being
reported as complete platform support.

This is an implementation contract, not a current-support claim. Current availability remains in
[Implementation status](IMPLEMENTATION_STATUS.md). Gate 6's detailed evidence, platform/shell
matrix, hardware policy, device profiles, and reports are fixed in
[Acceptance and qualification](ACCEPTANCE_AND_QUALIFICATION.md). Gate 9's complete platform
lifecycle/service/capability API is fixed in
[Platform integration contract](PLATFORM_INTEGRATION_CONTRACT.md); Gate 5 fixes which profiles that
API must serve and the order in which they are implemented.

The renderer boundaries from Gates 1–4 remain authoritative:

- [File and API implementation blueprint](IMPLEMENTATION_BLUEPRINT.md);
- [GPU ownership and synchronization](GPU_OWNERSHIP_AND_SYNCHRONIZATION.md); and
- [Scene-to-GPU ABI and shader contract](SCENE_GPU_ABI_AND_SHADERS.md).

## 1. Decisions fixed by this gate

1. The first operational managed GPU platform is **Windows x86-64 with direct Vulkan**.
2. Host-provided Vulkan render-area recording follows the first owned Windows Vulkan path and is a
   first-class product milestone, not an optional cleanup task.
3. The second managed desktop platform is **Linux x86-64 with Vulkan**. Wayland and X11 are separate
   qualification profiles even when Winit supplies both integrations.
4. The protocol-neutral Linux Vulkan host contract remains the reusable shell/compositor boundary.
   The first first-party implementation of that contract is Telorgon's Linux-only Wayland server,
   compositor policy, input/session owner, and atomic KMS host.
5. **Metal is the second direct graphics backend.** It unlocks macOS and iOS and is the first
   materially different API used to validate the backend abstraction. MoltenVK is not the definition
   of Apple support.
6. The first managed Apple desktop profile is **macOS arm64 with direct Metal**.
7. Mobile foundation work is shared first. The first managed mobile profile is **Android arm64 with
   Vulkan**; the second is **iOS arm64 with Metal**.
8. Windows D3D12, Intel macOS, additional Linux architectures, private consoles, and Web are later
   profiles. None blocks the first desktop, embedded, shell, or mobile sets.
9. Winit is a managed-host implementation dependency, not Telorgon's platform-neutral API. Hosted and
   shell profiles do not acquire a Winit dependency merely because managed desktop uses it.
10. No managed profile silently falls back from a requested GPU backend to the software renderer.
    Software/headless remains an explicit profile.
11. A platform claim names its build target, lifecycle/window path, renderer/presenter, input/service
    coverage, packaging path, and evidence tier.
12. Implementations may work in parallel only after their declared dependencies below are complete;
    parallel work must not invent duplicate platform, presenter, or renderer owners.

## 2. What a platform claim means

Telorgon tracks these axes separately:

| Axis | Question |
|---|---|
| Build | Does the selected Cargo feature/target compile with a reviewed dependency graph? |
| Lifecycle | Can views be created, suspended, resumed, resized, closed, and recovered correctly? |
| Presentation | Does the intended GPU backend acquire, render, submit, and present real images? |
| Hosted mode | Can a host provide devices, frame contexts, targets, and submission policy? |
| Input/services | Which pointer, keyboard, touch, IME, clipboard, accessibility, safe-area, and system services are operational? |
| Packaging | Can a user build and launch the artifact through the platform's normal development/package workflow? |
| Qualification | Which Gate 6 evidence layers and correctness, recovery, device, driver, performance, and accessibility cells passed? |

Terms are used as follows:

- **Compile target:** only the build/package boundary compiles.
- **Bring-up:** a development fixture reaches a real platform/renderer path; manual success is not a
  support promise.
- **Operational profile:** the intended lifecycle and rendering functions execute through a real
  integration with automated evidence.
- **Production-qualified profile:** the profile passes its complete Gate 6 matrix on declared
  systems and has a documented packaging/recovery policy and immutable qualification report.

“Windows support,” “mobile support,” or “Wayland support” without these axes is not an acceptable
status statement. A Winit feature existing is evidence that an adapter can be attempted, not evidence
that Telorgon has implemented or qualified it.

## 3. Dependency order

```text
Gates 1–4 implemented
        |
        v
P1 Windows Vulkan managed
        |
        +------------------+
        |                  |
        v                  v
P2 Vulkan hosted      P3 Linux Vulkan managed
        |                  |
        +---------+--------+
                  |
                  v
        P4 Linux shell/compositor host

P2 hosted + Gate 4 conformance seam
                  |
                  v
        P5 Metal + macOS arm64
                  |
                  v
        P6 shared mobile foundation
             /                 \
            v                   v
P7 Android Vulkan arm64   P8 iOS Metal arm64
```

P3 may begin in parallel with late P2 work after the presenter and device contracts stop changing.
P5 may begin in parallel with P4 after hosted Vulkan and the Gate 4 traceable plan are stable. P7 and
P8 may proceed in parallel after P6 and their renderer prerequisites, but each retains independent
qualification.

This order is about implementation dependencies. It does not lower the product priority of easy UI
authoring, shells, or embedding.

## 4. Package ownership

| Package | Platform responsibility | Forbidden responsibility |
|---|---|---|
| `telorgon-platform` | Backend-neutral lifecycle, view, event, service, deadline, and capability contracts | Winit, native window handles, Vulkan, Metal, or application component state |
| `telorgon-platform-winit` | Managed Winit event-loop/window adapter and event translation | Renderer device, swapchain, UI runtime semantics, or hosted-mode API |
| `telorgon-presenter-vulkan-wsi` | Raw-handle Vulkan surface, swapchain, acquire, submit, and present | Input, component lifecycle, shell policy, or Vulkan rendering internals |
| `telorgon-renderer-vulkan` | Vulkan device, scenes, targets, commands, resources, and hosted interop | Winit, application event loop, or protocol implementation |
| `telorgon-app` | Easy managed application builder and selected platform/presenter/runtime assembly | Owning the platform-neutral runtime implementation or backend internals |
| `telorgon-embed` | Windowless host-driven device/view/render-area assembly | Event-loop, swapchain, presentation, or independent queue submission |
| `telorgon-shell` | Protocol-neutral surface/output/workspace models, capabilities, authority-specific requests/results, and host transport | Wayland/X11 protocol objects, policy engine, UI component implementation, or native import implementation |
| `telorgon-renderer-metal` | Future direct Metal device, resources, commands, shaders, and hosted interop | AppKit/UIKit/Winit lifecycle |
| `telorgon-presenter-metal-winit` | Future managed Winit/CAMetalLayer drawable acquisition and presentation | Metal render planning or platform services |
| `telorgon-platform-android` | Android-specific services not correctly represented by the neutral/Winit layer | Vulkan command recording or Activity ownership hidden from the managed host |
| `telorgon-platform-apple` | macOS/iOS service bridges not correctly represented by the neutral/Winit layer | Metal renderer ownership or cross-platform UI semantics |

`telorgon-platform-android` and `telorgon-platform-apple` are created only when the corresponding
milestone needs a real native service. A platform-specific function must not be placed in
`telorgon-platform-winit` merely because Winit exposed the first event that triggered it.

The intended dependency direction is:

```text
telorgon-core / telorgon-input
       |                  |
       v                  v
telorgon-runtime       telorgon-platform <--- telorgon-platform-winit

telorgon-runtime -----------+
telorgon-platform ----------+---> telorgon-app managed assembly
telorgon-platform-winit ----+          ^
selected presenter ------------------+---- selected renderer

telorgon-runtime + telorgon-platform service values + selected renderer
                              |
                              v
                         telorgon-embed
                    (no Winit or presenter dependency)
```

## 5. Shared managed-host lifecycle

The first Winit host implements Gate 9's independent view lifetime, application activity,
visibility, native-surface generation, and presenter-state axes. The earlier single combined
`Declared -> WindowLive -> PresentationReady -> Suspended` sketch is superseded: a hidden view may
still own a valid surface, an active application may contain an occluded view, and presentation can
be unavailable without destroying runtime state. Repeated notifications are idempotent and each
view changes independently.

### 5.1 Winit callback rules

- Create windows and presentation surfaces only after the first `ApplicationHandler::resumed`.
- Translate `window_event` into size/scale, focus, input, IME, close, and redraw operations for the
  addressed view. Events for one window never mutate another view's input/focus state.
- Render only for `WindowEvent::RedrawRequested` or an equivalent explicit host callback.
- `about_to_wait` may commit pending platform commands, call `request_redraw`, and select `Wait` or
  `WaitUntil` from the earliest Telorgon deadline. It must not render merely because the loop woke.
- `suspended` retires presentation resources through Gate 3 rules and preserves the runtime state.
  On Android the native surface is dropped before the callback returns.
- `memory_warning` releases eligible caches and reports the before/after memory counters; it cannot
  destroy resources referenced by in-flight work.
- `exiting` performs explicit shutdown. It must not rely on process termination to make GPU
  destruction safe.
- `ControlFlow::Poll` is not the default. Continuous polling is allowed only for an explicit
  continuously animating profile and remains observable in diagnostics.

Zero physical extent is an explicit not-renderable metrics state, not necessarily application
`Suspended` and not a request to create a 1×1 swapchain. Scale and resize changes atomically update
the revisioned physical/logical metrics, input mapping, layout, target metadata, and damage before
the next render.

No Telorgon package starts an event-loop, rendering, or maintenance thread in the background. The
managed `run` call owns the foreground loop; an embedding host owns its threads.

## 6. P1 — Windows x86-64 managed Vulkan

Initial target: `x86_64-pc-windows-msvc` with `winit 0.30.13`, `raw-window-handle 0.6`,
`ash-window 0.13`, and the Gate 1–4 Vulkan stack.

Implementation order:

1. Complete real offscreen Vulkan scene rendering and readback.
2. Create `telorgon-presenter-vulkan-wsi` and obtain the required Win32 Vulkan surface extensions
   through `ash-window`; do not write a second Win32 handle extractor.
3. Implement the shared managed lifecycle and a one-window `telorgon-app` Vulkan assembly.
4. Handle zero extent, resize, DPI changes, occlusion/minimize, suboptimal/out-of-date swapchains,
   surface loss, and explicit shutdown.
5. Translate mouse, wheel, keyboard, text/IME, focus, and close events into neutral input/view events.
6. Add a second-window fixture sharing a compatible `VulkanDevice` while retaining independent
   scenes, presentation surfaces, focus, scale, damage, and close behavior.
7. Switch the default Windows GPU application profile only after Gate 6's P1 acceptance suite passes.

The Vulkan loader is dynamically loaded. The Vulkan SDK is a development/validation dependency, not
a shipped application runtime dependency. Failure to find a suitable loader/adapter returns a
structured startup error naming the missing requirement. It does not invoke `SoftwareRenderer`.

P1 is operational when a Telorgon-authored scene renders and presents through real Vulkan, the
managed lifecycle survives its required recovery cases, two windows share one device correctly,
and native operation counters/validation evidence agree. The detailed P1 cases are in
[Gate 6's platform matrix](ACCEPTANCE_AND_QUALIFICATION.md#101-platform-milestone-coverage).

## 7. P2 — host-provided Vulkan render areas

P2 is deliberately ahead of broad desktop services because embedding/game-engine use is a primary
Telorgon entry point.

Required behavior:

- `embedded-vulkan` builds without `winit`, `ash-window`, Softbuffer, or a window-system feature.
- A host can create one shared `UiDevice` from a validated owned or borrowed Vulkan context and then
  create several independent `UiView` values.
- `prepare` performs runtime/layout/scene work once; each `record` call targets a host-provided area.
- Command-only mode records into the host's active command buffer/render-graph interval and neither
  begins/ends/resets/submits it nor presents.
- Target format, extent, origin, region, load/store, initial/final uses, completion domain, thread,
  and allocator rules are validated rather than inferred.
- Several views can record into separate targets or regions in one host frame without sharing UI
  state or performing independent submission.
- Unchanged views add no update/upload/record work unless the host explicitly requires target redraw.
- Shutdown consumes the Gate 3 hosted receipts and proves that borrowed resources outlive all uses.

The first fixture runs on Windows Vulkan because P1 provides known device evidence. The second runs
on Linux Vulkan as soon as P3 device bring-up is available. Hosted support is a renderer/host claim,
not a Winit platform claim.

P2 exits only when a test host composites multiple views with real GPU commands and Telorgon owns no
event loop, swapchain, frame pacing, or submission.

## 8. P3 — Linux x86-64 managed Vulkan

Initial target: `x86_64-unknown-linux-gnu`. The Linux managed adapter uses Winit and the existing
Vulkan presenter, but treats its two initial window-system paths separately:

- `application-vulkan-linux-wayland` enables the Winit Wayland path without X11;
- `application-vulkan-linux-x11` enables the Winit X11 path without Wayland; and
- a convenience dual build may enable both only after the two narrow profiles compile and pass.

Runtime selection between enabled Winit integrations is Winit/window-system policy. Telorgon does not
open a Wayland display, bind globals, create X11 windows, or interpret either protocol directly.
`telorgon-platform`, `telorgon-runtime`, `telorgon-shell`, and renderer-neutral packages depend on neither
Wayland nor X11 crates.

P3 reuses P1 lifecycle and presenter code. Platform-specific modules may handle activation tokens,
desktop settings, IME differences, clipboard selection, and packaging, but may not fork the runtime
or renderer. Fractional scaling, compositor-driven resize, missing/late configure, zero extent,
occlusion, surface loss, and environment-specific Vulkan surface support are explicit test cases.

P3 is operational only when both narrow profiles pass their declared Gate 6 lifecycle, presentation,
input, and recovery suites. Passing Wayland does not imply X11, and the reverse is also true.

Implementation status: the shared raw-handle Vulkan presenter cross-compiles for Linux, and a
Linux-target fixture verifies that Wayland, Xlib, and XCB display handles select distinct required
Vulkan surface extensions without connecting to a display. No Linux swapchain, lifecycle, input, or
recovery run has been accepted; both Wayland and X11 qualification are deliberately deferred.

## 9. P4 — Linux shell/compositor host

P4 makes Telorgon useful for building a desktop shell without making Telorgon a display protocol
implementation. It depends on hosted Vulkan, Linux target/output evidence, the external-image
contract, and the
[Gate 8 shell primitive/component contract](APPLICATION_AND_SHELL_PRIMITIVES.md#9-shell-domain).

Delivery has two proofs:

1. A protocol-neutral fixture supplies stable surface trees, owned test images, damage, transforms,
   output targets, and explicit completion. It proves shell UI and output planning without claiming
   zero-copy client composition.
2. A Linux Vulkan interop fixture imports a real externally owned image and acquire synchronization,
   samples/composites it without CPU readback, and returns valid release synchronization.

The protocol/policy host owns Wayland or another protocol stack, surface validation, client
lifecycle, roles, focus/activation policy, buffer-type negotiation, output scheduling, and system
commands. Such a host may be Telorgon's focused Linux-only compositor packages, an adjacent example,
a private integration, or a separately maintained package. Wayland types and dependencies never
enter `telorgon-shell`, `telorgon-scene`, UI components, or the general renderer contract.

The first real interop profile uses Gate 9's typed Linux DMA-BUF/external-FD import lease when
supported, including explicit DRM format/modifier/plane metadata, consuming FD ownership, acquire
wait, and release-after-final-read rules. An adapter reports unsupported when a zero-copy path
cannot be established; it never downloads the client buffer to the CPU.

P4 proves one output first, then multiple outputs sharing a device. Shell visuals developed earlier
against mock surfaces remain modeled until these host/interop proofs pass.

## 10. P5 — direct Metal and macOS arm64

Initial target: `aarch64-apple-darwin`. Metal is implemented directly and in the same order as
Vulkan:

1. select and document a narrow Rust/Objective-C binding stack through a dedicated backend gate;
2. render the Gate 4 box/image/text conformance scene offscreen with real Metal resources and
   packaged Metal shader artifacts;
3. implement hosted `MTLDevice`/command-buffer/texture recording without submission ownership;
4. implement `telorgon-presenter-metal-winit` over a platform view and `CAMetalLayer` drawable; and
5. assemble managed macOS lifecycle, input, scale, menus/IME/accessibility service adapters, and
   packaging evidence.

The common scene and plan remain unchanged. Metal maps bindings, resource uses, render encoders,
completion, and target encoding to native mechanisms. Backend-specific shader artifacts may use MSL
and metallib; they obey the logical interfaces and visual semantics from Gate 4, not necessarily
Vulkan's byte-for-byte descriptor organization.

Telorgon acquires drawables only when a frame is ready, retains them for the minimum valid interval,
and allows the host/presenter to own commit/presentation policy. The presenter handles drawable
unavailability as an explicit acquire outcome.

The Metal implementation is the evidence required before extracting or stabilizing a broad
`telorgon-rhi`. If Metal exposes a Vulkan-shaped type, update the common contract instead of adding a
misleading adapter shim.

`x86_64-apple-darwin` is a later compatibility profile. It does not block the arm64 Apple baseline.

## 11. P6 — shared mobile foundation

Before claiming Android or iOS support, shared packages must represent:

- idempotent active/inactive/suspended/resumed/terminating lifecycle;
- native view/surface loss independent from application-state loss;
- monotonic frame callbacks and demand-driven scheduling;
- multi-pointer touch with stable contact identity, pressure/type where available, cancellation,
  gesture arbitration, and coordinate transforms;
- density, orientation, safe area/insets, occlusion, and appearance changes;
- editable text/IME session state, selection, composition, virtual keyboard, and dismissal;
- memory pressure and cache-trimming policy;
- application focus, pause/background rules, and state restoration hooks; and
- platform accessibility bridge inputs.

[Gate 7](AUTHORING_AND_COMPONENT_RUNTIME.md) owns component state, reconciliation, scoped tasks,
and the platform-neutral editable-text session;
[Gate 8](APPLICATION_AND_SHELL_PRIMITIVES.md) owns cross-input control/adaptive behavior; and
[Gate 9](PLATFORM_INTEGRATION_CONTRACT.md) owns the exact lifecycle/service/capability interfaces.
P6 integrates those contracts; it does not fork a mobile UI runtime. Desktop touch uses the same
neutral multi-pointer values.

A mobile renderer proof without touch, lifecycle, scale/insets, and IME behavior is renderer
bring-up, not an operational mobile GUI profile.

## 12. P7 — Android arm64 Vulkan

Initial target: `aarch64-linux-android` with the Vulkan backend. Managed bring-up uses Winit 0.30's
Android integration.

Dependency rules:

- do not add a separately versioned `android-activity` dependency beside Winit; use Winit's
  `winit::platform::android::activity` re-export;
- the first Rust-only renderer/packaging bring-up profile uses Winit's `android-native-activity`
  feature and is not called an operational GUI profile;
- the first operational managed GUI profile uses GameActivity because the initial Winit/AccessKit
  accessibility path supports that integration, unless a separately qualified native-view bridge
  proves equivalent lifecycle, IME, and accessibility behavior; never enable both Activity glue
  features in one artifact; and
- hosted Android engines use `telorgon-embed` plus Vulkan interop and do not pull in either managed
  Activity feature.

Create the window/Vulkan surface only after `resumed` supplies a valid native surface. On
`suspended`, stop acquisition and drop the presenter surface before the callback returns while
retaining recoverable runtime/scene/device state according to policy. Handle redundant lifecycle
events, orientation/pre-transform, density/insets, touch cancellation, keyboard/IME, low-memory,
and application background/foreground transitions.

The NativeActivity sample is renderer/packaging evidence, not the final GUI or product packaging
promise. GameActivity or the qualified alternate bridge, store metadata, SDK/minimum-device policy,
native IME/accessibility/service bridges, and device matrix are operational/qualification work. The
renderer must query Android surface capabilities rather than assuming desktop formats, transforms,
or present behavior.

## 13. P8 — iOS arm64 Metal

Initial target: `aarch64-apple-ios` using the already operational Metal backend.

Managed bring-up may embed Winit as a static Rust library in an Xcode-owned application shell, but
Winit does not define Telorgon's iOS service coverage. UIKit lifecycle, native text input, safe areas,
accessibility, application state, memory warnings, and packaging/signing remain explicit Apple
platform responsibilities.

Windows/views are created after the active/resumed lifecycle point. Presentation uses a
`CAMetalLayer`/drawable path and releases scarce drawables promptly. A hosted iOS engine may provide
its own `MTLDevice`, command buffer, texture/render target, UIView/layer, and services without Winit
or Telorgon-owned presentation.

iOS operational status requires the shared mobile suite plus iOS-specific lifecycle, Metal,
touch/IME, accessibility, rotation/insets, memory-pressure, and packaging evidence. macOS Metal
success does not imply iOS success.

## 14. Later profiles

These do not enter the critical path above:

- **Windows D3D12:** useful as a native Windows backend and another binding/synchronization check,
  but Vulkan already unlocks the first Windows profile.
- **Intel macOS:** a compatibility target after arm64 macOS evidence and dependency/toolchain review.
- **Additional Linux architectures/libc environments:** qualify only with actual loader, window
  system, packaging, and device evidence.
- **Private consoles:** hosted-first vendor packages using offline vendor shader tools and the shared
  conformance harness. No console support claim exists without an SDK/backend/hardware run.
- **Web:** requires a separate WebGPU/Web platform plan; it is not implied by Winit's Web target.
- **Portability adapters:** wgpu or another layer may be an optional backend, never the direct Vulkan
  reference or a substitute for Metal portability evidence.

## 15. Cargo and feature profiles

The umbrella exposes curated profiles rather than one feature that enables every platform:

| Profile | Includes | Explicitly excludes |
|---|---|---|
| `application-vulkan-windows` | app/runtime, Winit Windows adapter, Vulkan presenter/backend | Linux WSI, mobile glue, software fallback |
| `embedded-vulkan` | runtime/embed, Vulkan backend/interop | Winit, WSI presenter, Softbuffer |
| `application-vulkan-linux-wayland` | app/runtime, Winit Wayland, Vulkan presenter/backend | X11, mobile glue, software fallback |
| `application-vulkan-linux-x11` | app/runtime, Winit X11, Vulkan presenter/backend | Wayland, mobile glue, software fallback |
| `shell-vulkan-linux` | shell/runtime/embed, Vulkan external interop | Winit managed host unless an example explicitly adds it; protocol stack |
| `application-metal-macos` | app/runtime, Winit macOS adapter, Metal presenter/backend | Vulkan/MoltenVK, iOS glue |
| `embedded-metal` | runtime/embed, Metal backend/interop | Winit and CAMetalLayer ownership |
| `application-vulkan-android-native-bringup` | app/runtime, Winit NativeActivity, Vulkan presenter/backend | operational GUI claim, GameActivity glue, software fallback |
| `application-vulkan-android-game` | app/runtime, Winit GameActivity, Android IME/accessibility/services, Vulkan presenter/backend | NativeActivity glue, software fallback |
| `application-metal-ios` | app/runtime, iOS/Apple services, Metal presenter/backend | Vulkan/MoltenVK, Android glue |
| `headless` | runtime and software reference backend | native window, WSI, GPU backend |

Exact final Cargo feature spelling may be shortened before stabilization, but the dependency
isolation represented by these rows is normative. CI uses narrow profiles; an “everything” build is
not the only boundary check.

`RendererPreference::Auto` may select only among compiled, operational GPU backends for the active
profile. Selecting software requires the explicit software/headless profile or an explicitly named
software entry point.

## 16. Required source layout

The authoritative platform, Winit translation, accessibility-adapter, service-adapter,
conformance, `telorgon-app`, and `telorgon-embed` file blueprint is
[Gate 9 section 15](PLATFORM_INTEGRATION_CONTRACT.md#15-package-and-file-blueprint). Presenter/backend
file layouts remain those fixed by Gates 1–4. In particular, `application_handler.rs` dispatches;
it does not become a monotelorgon runtime, renderer, input system, service registry, or presenter.

## 17. Work-package boundaries

Each pull request/work order handles one bounded outcome:

1. neutral platform values and compile-only dependency tests;
2. Winit lifecycle/window registry without renderer changes;
3. one presenter surface/acquire/present path without UI feature additions;
4. managed assembly using existing runtime/backend contracts;
5. one input/service family and its conformance fixtures;
6. one hosted-device/frame/target integration path;
7. one platform feature profile and dependency-graph test;
8. one shell external-image/synchronization mechanism; or
9. one platform qualification report.

A work order must name target/profile, owner packages, adjacent references inspected, official API
sections, tests, unsupported behavior, and documentation status changes. It must not start a second
platform to make the first patch appear cross-platform.

## 18. Gate 5 reference audit

```text
Concern:
Order managed desktop, hosted, shell, cross-API, and mobile work without coupling the runtime to a
window system or confusing window creation with platform qualification.

Telorgon files/contracts affected:
PROJECT_SCOPE_AND_ARCHITECTURE.md platform/entry points/delivery stages; IMPLEMENTATION_BLUEPRINT.md;
MIGRATION_PLAN.md; Vulkan presenter/host order; target telorgon-platform, telorgon-platform-winit,
telorgon-app, telorgon-embed, shell, Vulkan, Metal, and presenter packages.

Reference revisions, paths, and symbols inspected:
Slint 69ecb713f5c62d1b6fe986ff822a57f22152b4d9 — internal/core/platform.rs Platform,
WindowAdapter, event-loop proxy, timer deadlines, neutral WindowEvent; internal/backends/winit/lib.rs
WinitCompatibleRenderer suspend/resume and ApplicationHandler forwarding;
winitwindowadapter.rs scale/resize/zero-size behavior.
Flutter 51fd9afadf309ba5337320bd3653f5345c156cb9 — shell/platform/embedder/embedder.h Vulkan/Metal
renderer configs, host-owned device/queue/image lifetimes, task runners, window metrics, pointer,
vsync, shutdown, multi-view compositor backing stores and caching; embedder tests for Vulkan/Metal
and compositor paths.

Official specifications/documentation checked:
Winit 0.30.13 ApplicationHandler lifecycle/RedrawRequested guidance, platform feature matrix, Android
activity glue, and iOS integration; Vulkan WSI surface/swapchain capability/acquire/present rules;
Android NativeActivity/GameActivity lifecycle/packaging guidance; Apple CAMetalLayer/drawable
acquisition and presentation guidance.

Invariants extracted:
The platform adapter and renderer are replaceable boundaries; windows/surfaces are lifecycle-owned;
redundant suspend/resume is normal; redraw is explicit and deadline-driven; host task/thread rules
are declared; hosted devices and images outlive engine use; multi-view backing stores have explicit
create/collect/present ownership; platform feature coverage is uneven and must be reported per axis.

Failure/recovery cases extracted:
Zero-size/minimized targets, surface invalidation on mobile suspend, repeated lifecycle callbacks,
late resize/scale, drawable unavailable, swapchain out-of-date/surface loss, host shutdown racing
posted tasks, backing-store collection, low-memory warnings, and unavailable platform features.

Approaches rejected and why:
One all-platform initial milestone; Winit types in the neutral runtime; renderer-owned event loops;
hosted mode after every managed platform; Linux Wayland code in Telorgon core; MoltenVK as Apple
portability proof; Android/iOS renderer screenshots as full mobile support; silent software fallback;
extracting a broad RHI before Metal evidence.

Telorgon-specific decision:
Windows Vulkan -> hosted Vulkan -> Linux Vulkan -> Linux protocol-neutral shell, with direct Metal
and macOS after hosted Vulkan, then shared mobile foundation -> Android Vulkan and iOS Metal.
Managed Winit, presenter, renderer, embed, services, and shell/protocol policy keep separate owners.

Tests/diagnostics derived:
Narrow Cargo profile checks; idempotent lifecycle state tests; redraw/deadline traces; multi-window
shared-device tests; owned and hosted GPU integration; separate Linux Wayland/X11 runs; shell
external-image/release-sync proof; mobile suspend/surface recreation and low-memory tests; per-axis
support reports. The complete mapping is in
[Gate 6](ACCEPTANCE_AND_QUALIFICATION.md#10-platform-and-shell-matrices).

Known gaps requiring hardware/vendor validation:
Exact supported OS versions, device/driver distributions, packaging/signing, native accessibility
and IME bridges, Android Activity profile qualification, Apple Metal binding/tool choices, external
image mechanisms, HDR/color management, console SDKs, and production thresholds.
```

No adjacent source code was copied. The references informed lifetime, boundary, sequencing, and
test decisions.

Primary sources:

- [Winit 0.30.13 `ApplicationHandler`](https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html)
- [Winit platform feature matrix](https://docs.rs/crate/winit/0.30.13/source/FEATURES.md)
- [Winit Android integration](https://docs.rs/winit/0.30.13/winit/platform/android/)
- [Winit iOS integration](https://docs.rs/winit/0.30.13/winit/platform/ios/)
- [Vulkan WSI specification](https://docs.vulkan.org/spec/latest/chapters/VK_KHR_surface/wsi.html)
- [Android GameActivity setup](https://developer.android.com/games/agdk/game-activity/get-started)
- [Android NativeActivity](https://developer.android.com/reference/android/app/NativeActivity)
- [Apple Metal onscreen presentation](https://developer.apple.com/documentation/metal/onscreen-presentation)
- [Apple CAMetalLayer](https://developer.apple.com/documentation/quartzcore/cametallayer)

## 19. Gate completion criteria

Gate 5 is complete when:

- first operational, desktop-set, shell, cross-API, and mobile platform orders are explicit;
- managed presentation and hosted rendering have separate package/dependency paths;
- Windows, Linux Wayland, Linux X11, macOS, Android, and iOS have named backend/profile roles;
- Wayland/protocol ownership remains outside portable core/shell/scene contracts and is isolated in
  the Linux-only server/compositor packages;
- Winit is contained to managed-platform/presenter packages;
- Apple support requires direct Metal and mobile support requires shared lifecycle/input/services;
- every milestone has dependencies and an exit boundary;
- later platforms are deferred without being claimed unsupported forever; and
- all active planning/status documents link to this contract without retaining undecided-platform
  placeholders.
