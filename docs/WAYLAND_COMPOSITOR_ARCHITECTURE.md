# Telorgon Wayland Compositor Architecture

## Document role

This document describes the **current implementation** of Telorgon's Linux-only Wayland desktop
runtime and records the remaining qualification work. It is not a claim that every protocol in
`wayland-protocols` is implemented or that the compositor is production-qualified.

The implementation deliberately uses the official C ABIs and protocol XML. It does not use Winit,
Smithay, wlroots, an X11 compatibility host, or a Rust compositor framework.

## Public assembly

`Application::desktop_environment` is the only process entrypoint for this mode. Compositor-owned
pixels are normal Telorgon `Component` values:

```rust,ignore
Application::desktop_environment("Telorgon")
    .linux(LinuxDesktopConfig::default())
    .renderer(Renderer::Auto)
    .assets(assets::bundle())
    .app_icon(app_icons())
    .cursor_theme(assets::cursors::DEFAULT)
    .compositor(
        Compositor::new()
            .window_frame(|model| MyWindowFrame { model })
            .policy(MyCompositorPolicy),
    )
    .shell_widget(ShellWidget::new("panel").content(MyPanel))
    .run()?;
```

The frame factory receives a fresh `WindowChromeModel` for each server-decorated toplevel and is
rendered at that window's outer extent. The model carries title, activation, state, capabilities,
and application-icon metadata. Visuals are normal Telorgon composition using `BoxDecoration` plus
explicit title, icon, drag, resize, content-slot, and action roles. Final retained layout defines
the client-content offset and hit regions; no fixed control placement is imposed by the host.

The typed project asset bundle is shared by frames, normal shell composition, pointer themes, and
the desktop fallback `AppIconProfile`. Client-provided `xdg_toplevel_icon_v1` name/buffer snapshots
override that fallback. A permitted client-provided `wl_pointer` cursor surface overrides the
configured pointer theme, and a client-side xdg-decoration request suppresses Telorgon's frame.
See [Custom windows, assets, icons, and pointers](CUSTOM_WINDOWS_ASSETS_AND_POINTERS.md) for the
complete authoring API.

`LinuxDesktopConfig` selects the DRM device, seat, optional Wayland socket name, output scale,
frame dimensions, and pointer extent. The umbrella exposes this mode through the
`desktop-wayland-linux` Cargo feature; it remains target plumbing rather than a renderer-selection
API or a second process entrypoint. The mode has no Windows or macOS implementation.
Top/right/bottom/left `ShellWidget::reserve_space` declarations reduce the maximized work area;
floating widgets remain overlays and fullscreen windows continue to use the complete output.

## Ownership and dependency layers

```text
official wayland.xml + wayland-protocols XML
                    |
                    v
telorgon-wayland-server
  XML schema validation, native wl_interface descriptors,
  libwayland-server display/global/resource/event-loop ABI
                    |
                    v
telorgon-compositor-wayland
  clients, resources, double-buffered surfaces, roles, xdg-shell,
  seats, outputs, SHM, DMA-BUF descriptors, explicit sync, events
              /                         \
             v                           v
telorgon-compositor-render          telorgon-platform-linux
  SHM -> Telorgon images            libseat, libinput, XKB, keymap FD
  DMA-BUF -> Vulkan leases                 |
             \                           /
              v                         v
             telorgon-app desktop_wayland owner thread
             Telorgon composition + scene/render orchestration
                              |
                              v
             telorgon-presenter-vulkan-kms
             libdrm atomic KMS + GBM scanout buffers
```

No layer above relies on generated Rust protocol packages. `telorgon-wayland-server` parses the
installed official XML, constructs `wl_interface`/`wl_message` descriptors with stable storage,
and passes decoded requests to Telorgon-owned state. `libwayland-server` remains the mature transport,
resource, client, socket, and event-loop implementation.

## Protocol source and advertisement rules

`ProtocolCatalog::load_desktop` reads `/usr/share/wayland/wayland.xml` and the pinned paths under
`/usr/share/wayland-protocols` by default. Parsing is bounded and rejects malformed XML, missing
interfaces, versions older than the profile, duplicate interface names, invalid signatures, and
unbounded schema counts. Native descriptors retain their strings and type arrays until every
global/resource using them is destroyed.

Loading an XML schema does **not** advertise its globals. A global is created only where
`NativeCompositor` has a dispatcher. Request and event `since` versions are checked against each
resource's negotiated version. The machine-readable profile is
[`protocols/telorgon-wayland-profile.toml`](../protocols/telorgon-wayland-profile.toml).

| Global | Maximum advertised | Availability |
| --- | ---: | --- |
| `wl_compositor` | 6 | Always |
| `wl_shm` | 1 | Always; ARGB8888/XRGB8888 plus accepted Telorgon formats |
| `wl_subcompositor` | 1 | Always |
| `wl_data_device_manager` | 3 | Always; selection plus pointer/touch drag-and-drop with MIME FD and action negotiation |
| `xdg_wm_base` | 7 | Always |
| `zxdg_decoration_manager_v1` | 1 | Always |
| `wp_cursor_shape_manager_v1` | 1 | Always |
| `xdg_toplevel_icon_manager_v1` | 1 | Always; commit-synchronized named or square SHM icon snapshots |
| `wp_fractional_scale_manager_v1` | 1 | Always |
| `wp_viewporter` | 1 | Always; commit-synchronized source crop and destination scale |
| `wp_presentation` | 2 | Always; commit-scoped monotonic feedback after KMS commit |
| `xdg_activation_v1` | 1 | Always; fresh-input-authorized, opaque, one-shot tokens |
| `ext_session_lock_manager_v1` | 1 | Always; blank-first secure KMS transition and input isolation |
| `zwp_relative_pointer_manager_v1` | 1 | Always; focused accelerated and unaccelerated deltas |
| `zwp_pointer_constraints_v1` | 1 | Always; focused pointer lock and region confinement |
| `zwp_idle_inhibit_manager_v1` | 1 | Always; scoped inhibitors exposed to shell power policy |
| `wl_output` | 4 | Per configured KMS output |
| `wl_seat` | 9 | Per configured libseat seat |
| `zwp_linux_dmabuf_v1` | 3 | Only after exact Vulkan importable format/modifier tuples are supplied |
| `zwp_linux_explicit_synchronization_v1` | 2 | Only when the selected render/present path accepts acquire and release fences |

Linux-dmabuf feedback v4/v5 and DRM syncobj are not advertised. Their XML metadata is retained as
an implementation roadmap, not as a support claim.

## Surface and shell behavior

The compositor enforces per-client object limits and ownership, permanent surface roles, xdg
configure/ack ordering, buffer-before-configure rejection, bounded pending configures, SHM pool
bounds, DMA-BUF plane/tuple validation, and client-scoped single-use input serials. Surface state is
double-buffered. Damage, opaque/input regions, scale, transform, offset, frame callbacks, buffer
attachments, and subsurface relationships flow through the commit model.

Implemented shell roles are xdg-toplevel, xdg-popup, wl-subsurface, session-lock, drag icon, and
pointer cursor. Toplevel icons accept bounded square SHM buffers and optional desktop icon names,
become immutable when assigned, and latch to the target surface's next commit. Popups retain
positioner anchor, gravity, offset, and constraint flags and receive
configure/repositioned events. Synchronized subsurfaces cache their state until the parent commit.
Server-side decoration is the default; explicit client-side negotiation is respected. The managed
policy loop maintains explicit stacking, keyboard activation state, titlebar and client-requested
move grabs, border/corner and client-requested resize grabs, maximize/restore, fullscreen/restore,
minimize, and close events. Multi-workspace policy and output-aware placement remain shell policy
extensions rather than protocol-server concerns.

Frame callbacks and presentation-time feedback are associated with the exact commit revision and
emitted after a successful KMS commit rather than at `wl_surface.commit`. Presentation feedback uses
`CLOCK_MONOTONIC`; the blocking presenter reports no hardware timing flags until page-flip timestamp
proof is integrated.
Copied SHM buffers are released after the compositor has taken its copy. Explicit-sync acquire FDs
are attached to a specific surface revision and release objects are completed only by the render
path that consumed that revision.

## Input, output, and session

`telorgon-platform-linux` owns narrow bindings to libseat, libinput, and xkbcommon. The runtime opens
the requested seat, obtains the DRM FD through libseat, publishes one `wl_seat`, creates an
NUL-terminated XKB keymap in a memfd, and delivers pointer motion/focus/buttons/axes plus keyboard
focus/keys/modifiers and slot-stable touch down/motion/up/frame/cancel streams. Pointer and touch
coordinates are transformed into surface-local coordinates.

Relative-pointer and pointer-constraints v1 are native-dispatched. A constraint is unique per
pointer/surface pair, follows pointer focus, implements persistent and one-shot lifetimes, and
emits the required locked/unlocked or confined/unconfined transitions. Locked pointers suppress
absolute motion while continuing relative deltas; confined pointers are clipped to the nearest
point in the surface-local constraint region.

XDG activation v1 issues cryptographically opaque, bounded, one-shot tokens. Tokens backed by a
fresh client-scoped input/focus serial are authorized; tokens without valid interaction metadata
are intentionally ineffective, as allowed by the protocol. Successful activation raises the
target in Telorgon's explicit stacking order and transfers keyboard focus. Tokens may be passed
between clients and unknown or consumed handles are ignored.

Session-lock v1 assigns permanent lock-surface roles, configures each output at its exact logical
size, rejects pre-configure/null/wrong-size commits, and isolates normal surfaces from both input
and rendering. A lock request first switches composition to opaque black plus lock surfaces. The
`locked` event is emitted only after that frame successfully completes Vulkan/software scanout and
the blocking atomic KMS commit. If the lock client dies after that event, Telorgon remains locked on
black; a later lock client may take responsibility for recovery. Unlock is accepted only from the
active lock object after `locked` was sent.

Core data-device v3 is native-dispatched. Only the keyboard-focused client may install a selection
using a fresh input serial; offers carry bounded validated MIME types, access is revoked on focus
loss, and `receive` forwards the supplied FD to the source. Replaced sources are cancelled.
Pointer- and touch-origin drag grabs validate the initiating serial, assign and render the drag-icon
role, hit-test client targets, issue per-data-device offers, negotiate v3 copy/move/ask actions,
preserve legacy copy behavior, and deliver enter/motion/leave/drop plus source completion or
cancellation. Entering secure session-lock mode cancels any active drag before isolating input.

The managed path currently publishes one connected KMS output and advertises pointer, keyboard, and
touch. Multi-output layout, hotplug, seat disable/re-enable KMS reconstruction, repeat scheduling,
input-method support, and touch-shape/orientation metadata remain qualification gaps.

Compose icon declarations are semantic and active. `window.close`, `window.maximize`, and
`window.minimize` are rendered into the matching server-decoration controls and define their hit
targets. Cursor-shape requests resolve through `cursor.default`, `cursor.pointer`, `cursor.text`,
`cursor.grab`, the resize-direction names, and the other CSS-compatible cursor semantic names; an
unconfigured shape falls back to the compositor's composed pointer component. The entire frame,
control, pointer, and cursor-shape visual path therefore remains ordinary Telorgon composition.

## Rendering and presentation

The operational managed path is entirely Telorgon-rendered:

1. The compositor copies a committed SHM buffer with checked offset/stride/extent arithmetic.
2. `telorgon-compositor-render` converts supported little-endian Wayland pixel formats to a retained
   Telorgon `ImageResource` with explicit alpha/color metadata, then applies the committed
   `wl_surface` transform/scale and viewporter crop/destination using bounded deterministic
   sampling.
3. Client images, composed policy, composed server frames, and composed shell widgets are rendered
   through `telorgon-renderer-software` into one RGBA output. The composed pointer is uploaded into
   three completion-retired ARGB8888 GBM cursor buffers when an atomic cursor plane is available;
   otherwise it is damage-composited into the primary output.
4. Three linear GBM scanout buffers receive the primary output. Primary and cursor state are
   submitted through one serialized libdrm atomic-commit scheduler to a connector-compatible CRTC.
   Cursor-only motion coalesces to the newest position and commits without repainting the primary
   plane; it does not use the legacy cursor ioctls or asynchronous page-flip flag.

`telorgon-compositor-render::DmaBufImporter` is the zero-copy Vulkan client-buffer bridge. It exposes
only exact single-plane format/modifier tuples queried from the selected `VulkanDevice`, validates
allocation bounds, consumes an optional acquire sync FD, creates a generation-scoped
external-image lease, and binds it into `VulkanScene`. The external-image path can export the
matching release requirement.

The managed KMS path also has an owned Vulkan scanout route. All three primary GBM buffers are imported with
their explicit modifier and row layout as Vulkan color targets. Telorgon renders the completed
desktop image into the selected target, transfers queue ownership back to
`VK_QUEUE_FAMILY_FOREIGN_EXT`, waits for GPU completion, and only then makes that frame eligible for
the serialized atomic KMS scheduler. `Renderer::Vulkan` requires this route. `Renderer::Auto`
attempts it on each supported
adapter and falls back to the mapped software scanout route; `Renderer::Software` selects the
mapped route directly. Client surfaces and all shell visuals remain Telorgon render resources in both
cases; the current managed Vulkan route uses one final retained-image upload after reference
software composition while direct per-surface GPU composition is the next optimization step.

The GBM/KMS bindings are Telorgon-owned and use the original libgbm/libdrm ABIs. Scanout allocation,
mapping, modifier-aware framebuffer creation, connector/encoder/CRTC/plane discovery, primary and
cursor-plane filtering, mode blobs, atomic test commits, initial modesets, nonblocking page-flip
events, primary/cursor buffer retirement, and mailbox scheduling are implemented. Direct scanout,
general overlays, color management, VRR, HDR, and hardware qualification are still open.

## FD, synchronization, and lifetime invariants

- Incoming protocol FDs become `OwnedFd` immediately; duplicated FDs have one documented owner.
- SHM reads use positional I/O and never mutate a client's shared file offset.
- A DMA-BUF buffer owns all plane FDs until duplicated into a generation-scoped renderer lease.
- Explicit acquire/release state is keyed by `(surface, commit revision)`, not merely by surface.
- Vulkan explicit modifier imports use `VkSubresourceLayout::size == 0`, as required by the Vulkan
  specification, while allocation size is retained separately for bounds checking.
- A GBM buffer outlives its KMS framebuffer, and both outlive the atomic request that references it.
- A cursor buffer is never mapped while it is current or named by the one pending CRTC commit. A
  software fallback retains an active cursor framebuffer until an atomic plane-disable completion.
- Only one nonblocking atomic commit is outstanding per CRTC. Pointer events received while it is
  outstanding replace desired cursor position rather than enqueueing additional commits.
- A libseat-managed DRM FD remains owned by the seat; KMS receives a duplicate.
- Wayland globals are destroyed before bind contexts and native protocol descriptors.
- No callback is allowed to unwind across a C ABI boundary.

## Source audit

The implementation is based on the following primary specifications and source audits:

- [Wayland server API](https://wayland.freedesktop.org/docs/html/apc.html), [wire/XML rules](https://wayland.freedesktop.org/docs/book/Message_XML.html), and [core protocol](https://wayland.freedesktop.org/docs/html/apa.html) define transport/resource and core object behavior.
- [xdg-shell](https://wayland.app/protocols/xdg-shell), [xdg-decoration](https://wayland.app/protocols/xdg-decoration-unstable-v1), [xdg-toplevel-icon](https://wayland.app/protocols/xdg-toplevel-icon-v1), [xdg-activation](https://wayland.app/protocols/xdg-activation-v1), [session-lock](https://wayland.app/protocols/ext-session-lock-v1), [cursor-shape](https://wayland.app/protocols/cursor-shape-v1), [linux-dmabuf](https://wayland.app/protocols/linux-dmabuf-v1), and [explicit synchronization](https://wayland.app/protocols/linux-explicit-synchronization-unstable-v1) define the implemented extension contracts.
- [DRM KMS documentation](https://docs.kernel.org/gpu/drm-kms.html) and the official libdrm [`xf86drmMode.h`](https://cgit.freedesktop.org/drm/libdrm/tree/xf86drmMode.h) define atomic presentation and exact ABI layouts. The source audit caught the legacy coordinate fields that precede `possible_crtcs` in `drmModePlane`.
- The [Vulkan explicit DRM modifier structure](https://registry.khronos.org/vulkan/specs/latest/man/html/VkImageDrmFormatModifierExplicitCreateInfoEXT.html) requires each explicit plane layout's `size` to be zero.
- wgpu commit `d99c241a3b9dcc0f6674d990d007d79e94d39862` was inspected for DMA-BUF import capability and ownership invariants; Flutter commit `51fd9afadf309ba5337320bd3653f5345c156cb9` was inspected for sync-FD ownership and frame-slot reuse. Those projects are references only; no framework code or abstraction was copied.

### Atomic cursor-plane audit

```text
Concern:
Tear-free hardware-cursor positioning, atomic primary/cursor scheduling, and completion-safe cursor
framebuffer reuse.

Telorgon files/contracts affected:
crates/telorgon/src/application_host/desktop_wayland.rs;
crates/telorgon/src/presenter_vulkan_kms/{ffi.rs,kms.rs,model.rs}; this document.

Reference revisions, paths, and symbols inspected:
Android platform/base 1cdfff555f4a21f71ccc978290e2e212e2f8b168,
core/java/android/view/SurfaceControl.java Transaction, apply, setPosition, setBuffer, and fences;
Flutter 51fd9afadf309ba5337320bd3653f5345c156cb9,
engine/src/flutter/shell/platform/embedder/embedder_external_view_embedder.{h,cc} layer collection,
single present callback, and post-present recycling;
wlroots 0855cdacb2eeeff35849e2e9c4db0aa996d78d10, backend/drm/drm.c cursor desired-state
updates and atomic CRTC commit;
Smithay e3d461a057ba244d213a8498ec372b0799cca103,
src/backend/drm/compositor/mod.rs and src/backend/drm/surface/atomic.rs plane assignment, test-only
validation, atomic commit, and vblank completion.

Official specification sections checked:
Linux DRM KMS standard cursor-plane properties and legacy/atomic non-mixing rule; DRM client atomic,
universal-plane, and cursor-hotspot capabilities; DRM_MODE_PAGE_FLIP_EVENT,
DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_PAGE_FLIP_ASYNC, and test-only atomic commits; libdrm
drmModeAddFB2 and drmModeAtomicCommit declarations.

Invariants extracted:
All visible plane changes form one explicit atomic state; one CRTC commit is outstanding at a time;
new pointer events coalesce to newest desired state; nonblocking does not mean asynchronous scanout;
current or pending cursor buffers are immutable; replacement is reusable only after flip completion;
cursor-only commits do not imply client-surface presentation.

Failure and recovery cases extracted:
Missing cursor plane/ARGB8888/properties, unsupported modifier allocation, cursor image larger than
the reported hardware extent, rejected test or runtime commit, a cursor update arriving during an
outstanding primary flip, and software fallback while a cursor framebuffer remains active.

Approaches rejected and why:
Legacy drmModeMoveCursor/drmModeSetCursor2 is an untracked driver-dependent update beside the atomic
primary path; software-only cursors force primary damage on every move; DRM_MODE_PAGE_FLIP_ASYNC may
tear; one commit per raw input event ignores CRTC back-pressure and loses mailbox coalescing.

Telorgon-specific decision:
Auto-select an atomic ARGB8888 cursor plane, use three GBM/DRM framebuffer slots, append the newest
cursor generation to a primary or cursor-only vblank commit, and disable that plane atomically in
the same primary commit that introduces the software cursor fallback.

Tests/diagnostics derived:
Pure cursor-state tests cover in-flight motion coalescing, current/pending buffer exclusion, and
fallback retirement after disable completion. Linux feature and profiler feature cross-target
checks compile the complete path. Profiler events distinguish atomic cursor submit, scanout
completion latency, image-stage failure, and atomic-commit failure.

Known gaps requiring hardware or vendor validation:
No physical/vGPU matrix has yet qualified cursor-plane format/modifier restrictions, negative edge
coordinates, hotspot properties, visual tear behavior, latency, runtime fallback, or driver-specific
atomic commit behavior.
```

Smithay and wlroots were rejected as implementation dependencies because Telorgon owns protocol
state, policy, composition, and rendering. They remain useful external compatibility references.

## Verification and qualification boundary

Portable state tests cover surface commits, roles, ownership, serials, subsurface cycles, buffer
release tracking, SHM/DMA-BUF validation, xdg configure ordering, XML schema bounds, and KMS frame
slot reuse. Windows-hosted checks cover the non-Linux declarations; an
`x86_64-unknown-linux-gnu` compile check covers the complete Linux feature graph without starting
the compositor. Per repository policy, no server or application is launched by automated work.

The current path is **operational by integration and compile evidence, but not
production-qualified**. A Linux TTY/hardware run is still required for socket/client conformance,
atomic modesetting and page-flip behavior, libseat transitions, input devices, multiple GPUs,
multiple outputs, failure recovery, and performance. The largest remaining gaps are
multi-output/hotplug, direct per-surface Vulkan composition, page-flip timestamps,
input-method/text-input integration, and newer DMA-BUF feedback/syncobj protocol generations.
