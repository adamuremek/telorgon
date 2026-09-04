# Wayland solid resize preview

Status: implemented with CPU regression and compile-time verification; interactive Linux visual and
performance qualification remains user-run. This replaces the earlier stretched live-client preview.

## Behavior and configuration

Interactive resize shows an opaque dark slate content veil (`#262a30`) while the composed server frame
and veil follow the pointer. This is a solid fill, not a blur or a scaled screenshot. The client image,
including client-drawn decoration, popups, and subsurfaces, is hidden until the final redraw is ready.
Server-drawn title text and controls remain ordinary composed chrome. The first version switches
directly; it does not add a fade, blur pass, readback, image resampling, or intermediate render target.

Set `LinuxDesktopConfig::resize_preview_color` to an opaque `ColorRgba8` and pass the configuration via
the desktop declaration's `.linux(config)` method. Non-opaque colors fail configuration validation,
because transparency would expose the old content or unpainted bands this policy is intended to hide.

On press, the compositor sends the resizing state with the client's committed window extent. Motion
only changes desired geometry; it does not issue intermediate size requests. Release sends the final
size without the resizing state. The veil persists until an image publication acknowledges that final
configure or a later superseding configure. Cell/aspect-constrained clients may choose a smaller legal
extent; final geometry reconciles around the fixed opposite edges. A new resize supersedes an older
pending final transaction. Unmap removes its preview and pending requests; maximize/fullscreen reset
the transaction. A nonresponding client can leave its veil visible, but does not block input, other
clients, closing the window, or a subsequent resize. No timeout pretends that client content is ready.

## Work and ownership

- One retained one-unit solid scene supplies all veil placements. Changing preview bounds changes
  placement/damage only; it uploads no client texture and rebuilds no fill scene. Composed chrome can
  still require layout/scene updates. Vulkan and software consume the same neutral placements.
- A veiled tree retains its image scenes and queued pixel deltas without drawing or uploading them.
  Source and destination image extents remain equal; XDG margins and visible clips are separate.
- New SHM reads/conversions during an active drag enter the existing bounded, replaceable per-surface
  mailbox. Replacement retires an unused request; the latest request retains its buffer until copied
  or superseded. Already submitted work is allowed to finish. Paused requests cannot prevent unrelated
  surfaces from entering the worker, and the bounded queue scan never spins on paused entries.
- The first publication replacing an unread request takes a full copy, even for same-sized buffers
  with partial damage. Otherwise earlier deferred damage would be lost. Replacing a request with the
  same `wl_buffer` must not release it before the replacement read finishes. Explicit release remains
  per commit, while ordinary buffer release still respects other pending reads.
- Client frame callbacks are withheld during the active drag. After release, callbacks for accepted
  hidden intermediate revisions may resume without a new KMS flip, so callback-paced clients can
  reach their final redraw. Callback-only wakes leave presentation feedback pending; an actual
  displayed frame later reports its own revision and discards superseded hidden revisions. Each
  scanout slot carries the window/drag-image revisions in its rendered frame, not the latest client cache at flip
  completion. Future revisions stay queued for their own presentation. Damage-free commits can
  explicitly reuse delivered pixels while advancing revision/callback progress without a texture
  upload. The final image and preview removal enter the
  same desktop frame; no SHM callback alone removes a preview early.
  The separate cursor-plane callback/presentation path is unchanged by this window-resize policy.
- DMA-BUF acquire/release and materialization rules are unchanged. Its client resize requests are
  likewise reduced and hidden content does not draw, but this does not promise zero DMA-BUF import
  work for clients that keep committing independently of frame callbacks. Likewise, Telorgon cannot
  prevent independent client-side animation or work. Performance savings are not hardware-measured.

The profiler's existing `compositor.shm.copy_bytes` records actual reads. The new
`compositor.resize.shm_copy_deferred` instant records requests paused by an active resize; it is not an
estimate of saved bytes. Expect no new drag-triggered SHM reads after preexisting work drains.

## Reference and protocol audit

Concern: hide intermediate resize content and reduce producer work without blocking the Wayland
owner, losing skipped damage, releasing a leased buffer, or fabricating presentation feedback.

Read-only references inspected (paths relative to `../other-rendering-libs`):

- Android platform/base `1cdfff555f4a21f71ccc978290e2e212e2f8b168`,
  `base/libs/WindowManager/Shell/src/com/android/wm/shell/windowdecor/FluidResizeTaskPositioner.java`,
  `VeiledResizeTaskPositioner.java`, and `ResizeVeil.kt`: compare per-motion application bounds with
  veil-only movement, final bounds submission, fixed geometry, and a color-surface preview.
- Zed `f4178619acd0d47ea1f76a2025c42962c6d6638c`,
  `zed/crates/gpui_linux/src/linux/wayland/window.rs`: `handle_xdg_surface_event`, `frame`,
  `set_size_and_scale`, and `resize`; client configure acknowledgement, vblank resize throttling,
  geometry, viewport, renderer resize, and callback-driven progress are separate operations.

Official protocol contracts checked: the XDG-shell XML's `xdg_surface.set_window_geometry`,
`ack_configure`, and `xdg_toplevel.resize/configure/state.resizing`
([XML rendering](https://wayland.app/protocols/xdg-shell)); core
[`wl_surface.frame` and `wl_buffer.release`](https://wayland.freedesktop.org/docs/html/apa.html);
and presentation-time's
[`presented` versus `discarded`](https://wayland.app/protocols/presentation-time).
No graphics API ownership/barrier contract changes. No reference source or public abstraction copied.

Extracted invariants: desired frame geometry is not committed client image geometry; only the client
can redraw; callback pacing is not presentation proof; hidden and superseded work must retain/release
the same buffer ownership as visible work. Rejected alternatives: stretched snapshots distort text;
live clipping/padding exposes dead space; continuous configure/copy loops pay for unseen intermediate
sizes; synchronous waiting blocks unrelated clients; CPU blur/readback diverges between SHM and
GPU-backed content; early buffer release permits mutation during the delayed read.

## Verification and manual qualification

CPU tests cover native coordinate/clip mapping, shadow exclusion, all eight anchored edges and
cell-snapped final sizes, paused mailbox fairness/resume, placement-only solid previews, hidden image
retention, one final image publication, monotonic solid-scene epochs, opaque-color validation, final
configure serial/ack supersession, and hidden versus visible feedback with future revisions retained.
Source-boundary tests include the new geometry/state policy; Linux test compilation checks native
owner integration. No compositor, GUI, server, or GPU-presenting test is run by the agent.

Verification for this change:

- `cargo test -p telorgon --lib --quiet`: 916 passed.
- `cargo test -p telorgon --lib desktop_wayland --features embedded-vulkan,profiler --quiet`:
  25 passed.
- `cargo check -p telorgon --tests --target aarch64-unknown-linux-gnu --no-default-features
  --features desktop-wayland-linux,embedded-vulkan,profiler`: passed.
- `cargo build -p telorgon --lib --release --target aarch64-unknown-linux-gnu --no-default-features
  --features desktop-wayland-linux,embedded-vulkan,profiler`: passed.
- Formatting, whitespace, and changed-document relative links checked. Existing platform-dependent
  dead-code warnings remain; these results are not hardware performance or visual evidence.

User-run checks: resize a terminal aggressively from all eight edges; hold still during a drag and
release; repeat before the previous redraw finishes; try a slow client, no-motion click/release,
client-side decorations, popups/subsurfaces, partial-damage SHM animation, and a DMA-BUF client.
Check that chrome remains usable, the veil covers all content, the final content is sharp, and no
stale patches or background gaps appear. Compare actual SHM-copy counters during a held drag and
after release. Repeat with the software backend and the target Raspberry Pi/Mesa, AMD, Intel, and
NVIDIA hardware as available; build success alone does not qualify that matrix.
