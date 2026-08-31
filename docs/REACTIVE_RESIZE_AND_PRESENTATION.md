# Reactive Resize and Windows Presentation

## Status

This document records the implemented managed-Windows resize contract and the hardware evidence
still required before it is production-qualified. `IMPLEMENTATION_STATUS.md` remains authoritative
for current availability, and `GPU_OWNERSHIP_AND_SYNCHRONIZATION.md` controls Vulkan lifetimes.

## Problem addressed

The affected Windows Vulkan path could spend about two seconds inside a nominally nonblocking
`vkAcquireNextImageKHR(..., timeout = 0, ...)` call during interactive resize. Recreating a
swapchain for intermediate extents amplified that driver/WSI behavior into two visible stalls: an
old stretched frame, an intermediate generation, and a later current-size frame.

The originating AMD/Windows system stopped reproducing the stalls after its GPU driver was
updated, without a corresponding renderer-side fix. That before/after report strongly localizes
the observed behavior to the display driver or Windows Vulkan WSI path. It is evidence for this
specific incident, not a rule that every slow acquire is a driver defect.

## Windows presenter selection

Managed Windows now prefers an HWND-bound DXGI presenter when
`VulkanConfig::enable_dxgi_presenter` is true (the Windows default) and the selected Vulkan adapter
exposes a valid LUID plus importable Win32 external memory and D3D12-fence semaphore handles. The
renderer remains Vulkan. A D3D11 device is opened on the exact same adapter, creates shared BGRA
textures, and owns a two-buffer flip-sequential `CreateSwapChainForHwnd` swapchain with
`DXGI_SCALING_NONE`. Vulkan renders directly into an imported shared texture; D3D11 performs one
GPU-to-GPU copy into the current DXGI back buffer and calls `Present1(1, 0)`. There is no CPU
readback or upload. NT-handle textures carry the required
`D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX` creation pair;
Vulkan acquires mutex key 0 and releases key 1, while D3D11 acquires key 1 around copy/present and
releases key 0. The shared D3D11 fence imported as a Vulkan timeline semaphore additionally orders
queue submission and retirement.

`DXGI_SCALING_NONE` is the reason for this path: if a client-size transition briefly outruns buffer
reconfiguration, DXGI clips or exposes the window background instead of stretching the old buffer
to the new client extent. `DefWindowProc` emits `WM_SIZE` while processing
`WM_WINDOWPOSCHANGED`. For contractions, the Win32 subclass forwards the nested notification
without touching the swapchain and posts one coalesced private HWND message. This preserves the old
larger buffer for DWM to clip during an inward-drag gap. Expansions use the inverse order: the exact
larger swapchain is rendered and presented before native processing exposes the larger DWM target,
where scaling-none clips it against the still-smaller old target. The modal queue then handles the
private message after the sent-message transaction unwinds, giving both directions a final exact
present and `DwmFlush` against the committed target. DirectComposition was not selected because
composition swapchains require
`DXGI_SCALING_STRETCH`, which preserves the failure mode under investigation. If the adapter or
external-handle profile is unavailable, the existing Vulkan WSI presenter remains the capability
fallback.

## Implemented contract

Logical view metrics and physical presentation remain independently revisioned. On managed
Windows, every exact `WM_SIZE` extent remains a bounded transaction. The preferred path is:

```text
exact client extent from WM_SIZE
  -> revisioned resize update
  -> if expanding, prepare/present the exact larger buffer before the native target expands
  -> runtime input/layout/hit-test/semantics for that extent
  -> scene delta carrying the logical extent
  -> Vulkan work snapshot carrying the metrics revision
  -> ResizeBuffers/create the scaling-none HWND swapchain for the matching extent
  -> render and clip into a same-adapter D3D11 texture imported by Vulkan
  -> Vulkan signals render-done on the imported shared fence
  -> return from default WM_WINDOWPOSCHANGED/WM_SIZE handling
  -> coalesced private HWND message applies the latest exact client extent
  -> D3D11 waits, copies to the current DXGI back buffer, and calls Present1(1, 0)
  -> conservatively wait one display interval for the resize barrier
  -> release the bounded WM_SIZE barrier
  -> DwmFlush before returning to the native sizing loop
```

`VulkanLiveResizeMode::Responsive` remains the default for the Vulkan WSI fallback. Neither
preferred DXGI presentation nor responsive Vulkan WSI intentionally renders a mismatched
scene/swapchain pair, never stages the final frame through an old swapchain, and does not return
from an interactive Windows size callback after merely enqueueing the matching frame. The wait is
bounded to 100 ms so a broken WSI cannot deadlock the UI thread; Vulkan work remains on the
presentation worker. The existing driver-workaround is retained as the explicit
`DeferredScaledPreview` mode: it keeps the old swapchain during the native transaction, presents
reflowed previews through it, and commits once after the final preview. The window system may
nonuniformly scale those compatibility previews.

The resize state machine is:

| Phase | Reactive state | Responsive surface state | Deferred fallback surface state |
| --- | --- | --- | --- |
| `Stable` | Apply the current metrics revision | Commit the requested extent | Commit the requested extent |
| `Started` | Apply and render the exact native extent | Commit, present, and synchronize that extent | Keep the current swapchain |
| `Updating` | Apply each changed `WM_SIZE` extent | Commit, present, and synchronize that extent | Keep the current swapchain |
| `Ended` | Produce the final scene | Commit without an old-surface preview | Preview once, then commit |
| `Cancelled` or zero extent | Retain bounded state | Suspend | Suspend |
| `OUT_OF_DATE`/surface loss | Preserve latest logical state | Force recovery | Force recovery |

The Windows host uses exact `WM_ENTERSIZEMOVE`/`WM_EXITSIZEMOVE` generations. During that native
modal loop, a thread-affine Win32 subclass forwards the current client extent directly to the
managed host on `WM_SIZE` and on a display-paced recovery timer tick. This is required because
posting another Winit user or paint event into the nested loop can defer the application callback
until mouse release. Exact `WM_SIZE` updates bypass normal refresh coalescing. After the runtime
lays out the view, the worker queues a matching present, and the presentation engine completes it,
the UI thread calls `DwmFlush` before allowing native resize processing to continue. Merely
returning successfully from `Present` or `vkQueuePresentKHR` does not release the barrier. The DXGI
bridge currently reports no exact display-completion token, so the worker conservatively waits one
display interval before the flush. On the Vulkan WSI fallback, when
`VK_KHR_present_id` and `VK_KHR_present_wait` are available, the matching frame carries a monotonic
swapchain-local ID and the worker polls that exact ID before releasing the barrier. Otherwise the
worker uses a maintenance presentation fence or conservatively waits one display interval before
the flush. The timer remains nonblocking and coalesced; it only
retries progress if a WSI implementation misses the bounded barrier. Resize and scene deltas are
submitted in one `PresentationSnapshot` carrying the metrics revision and scene epoch; the worker
rejects incoherent batches, and a stale target extent or deferred preview cannot satisfy the resize
barrier. The direct Win32 extent stream is the sole authority while the native transaction is
active. Parallel Winit `Resized` callbacks are ignored during that transaction, and delayed
callbacks are rejected afterward when their extent no longer matches the actual client size. The
release path re-reads the client extent before its final commit.

## Preview mapping

The retained scene extent is the logical view extent. The Vulkan viewport is the current physical
target region. One `ViewMapping` supplies both projection and logical-clip-to-target-scissor
conversion, including nonzero hosted target origins. Responsive managed resize keeps those extents
coherent, so the mapping does not introduce a transient nonuniform stretch. Only the explicit
`DeferredScaledPreview` compatibility mode uses the inverse mapping needed before the window system
scales an old swapchain image to the current client size.

Hit testing remains on the runtime thread and uses the exact resize event before later pointer
events are dispatched. Only the native size transaction waits; input and layout do not run on the
presentation worker.

## Presentation policy

- The same-adapter DXGI HWND presenter with `DXGI_SCALING_NONE`, two flip-sequential buffers, and
  `Present1(1, 0)` is the managed Windows default when its external-memory profile is available.
- FIFO is the Vulkan WSI fallback default while Windows resize behavior is qualified;
  `VulkanConfig::prefer_mailbox_present` can opt into MAILBOX with mandatory FIFO fallback.
- Telorgon optionally enables `VK_KHR_get_surface_capabilities2` and
  `VK_EXT_surface_maintenance1`. For the selected present mode, it requests
  `VK_PRESENT_SCALING_ONE_TO_ONE_BIT_EXT` with deterministic MIN gravity on each supported axis
  (CENTERED, then MAX, are compatibility fallbacks). If either extension, one-to-one scaling, or
  a valid gravity on either axis is unavailable, or the device did not enable the existing
  `swapchainMaintenance1` feature, swapchain creation uses the legacy path.
- Interactive Windows Vulkan resize uses exact extent/layout/present transactions by default;
  display-paced coalescing remains the recovery path and the policy on platforms without the
  Win32 size transaction.
- `VulkanConfig::live_resize_mode` can select `DeferredScaledPreview` for a known-unhealthy WSI
  implementation. Scaled preview is never inferred from a mismatched extent alone.
- Swapchains request `minImageCount + 1`, capped by the surface maximum.
- `SUBOPTIMAL` is deferred during preview; `OUT_OF_DATE` forces recovery.
- No live-resize path uses an infinite acquire timeout, `vkQueueWaitIdle`, or `vkDeviceWaitIdle`.

## Synchronization paths

The DXGI bridge imports one shared `ID3D11Fence` into Vulkan as a timeline semaphore with
`VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_D3D12_FENCE_BIT`. For each frame, Vulkan waits for the prior
D3D-copy completion value, acquires keyed-mutex key 0, renders, releases key 1, and signals a
render-complete value. D3D11 acquires key 1, waits for render completion, copies the shared texture
into the current back buffer, presents, signals the next D3D-complete value, and releases key 0.
This initially serializes bridge frames deliberately, ensuring
that neither API reuses a shared image while the other may access it. Resize waits only for the
last bridge value before releasing imported image views/memory and calling `ResizeBuffers`;
explicit shutdown additionally waits for the Vulkan presentation queue. Ordinary frames do not
use CPU fence waits or queue/device idle.

When `VK_KHR_present_id` and `VK_KHR_present_wait` are exposed and
`VulkanConfig::enable_present_wait` is true, every successful present carries a monotonic ID. The
Windows resize barrier prefers that exact presentation proof over all fallbacks. The worker polls
with a zero Vulkan timeout; the existing 100 ms native barrier remains the only blocking bound.
Presentation IDs do not replace maintenance fences used for semaphore reuse and generation
retirement.

One-to-one presentation scaling is a compositor-distortion fallback, not a completion primitive.
It prevents the presentation engine from nonuniformly stretching a temporarily mismatched
swapchain image when the Vulkan surface reports that policy as supported. It neither proves that a
frame reached the display nor makes a late frame arrive sooner, so the exact-present/fence/display-
interval barrier remains unchanged.

When `VK_EXT_swapchain_maintenance1` is exposed and
`VulkanConfig::enable_swapchain_maintenance1` is true:

- acquisition uses a binary semaphore without an acquire fence;
- acquire-slot-indexed present semaphores are reused only after their presentation fences signal;
- retired generations poll render receipts and presentation fences;
- abandoned acquired images are explicitly released; and
- failed presentation requests remain conservatively retained until fence proof or the bounded
  queue-idle fallback.

Without maintenance support, the existing acquire-fence/reacquisition retirement protocol remains
active. Queue idle is restricted to shutdown, surface replacement, or the bounded retirement limit.

## Unhealthy-WSI fallback

The worker measures every acquire attempt. Only `DeferredScaledPreview` arms the unhealthy-WSI
circuit breaker: if acquiring its old swapchain exceeds the current display interval, further
preview acquisition is suppressed for that generation. Input, reactive layout, and scene/mailbox
coalescing continue, and resize end clears the breaker before the final commit. Responsive commits
never arm or inherit this suppression; otherwise one slow intermediate acquire would deliberately
leave the compositor stretching that frame until mouse release, contradicting the responsive
contract.

Profiler evidence includes the selected DXGI path, `presenter.dxgi.scaling_none`, DXGI extent,
metrics revision, scene epoch, resize phase/generation, acquire duration,
zero-timeout breach count, the exact `vkAcquireNextImageKHR` dispatch interval, swapchain
generation/extent/image count, selected present mode, present-wait and maintenance availability,
one-to-one presentation-scaling selection, and raw Vulkan vendor/device/driver identifiers. A raw
zero-timeout dispatch lasting at least 100 ms emits
`presentation.vulkan_wsi.zero_timeout_acquire_stall`. The profiler explains that this is a likely
driver/WSI signature, recommends a driver update or rollback, and asks for a saved capture plus
Vulkan validation if the problem remains.

## Automated evidence

Unit tests cover the DXGI flip-sequential/scaling-none descriptor, monotonic paired cross-API fence
values and exhaustion, the required D3D11 NT-handle texture sharing flags,
responsive-by-default policy selection, exact resize phase transitions, selection
of the `WM_SIZE` barrier only for responsive commits, rejection of stale-target and staged-preview
completion signals, explicit deferred phase transitions, final revision retention, rejection of
scaled-preview staging and acquire-stall suppression for responsive commits, FIFO/MAILBOX
selection, capability gating of exact present wait, preference of presentation IDs over
maintenance fences, optional instance-extension filtering, capability/gravity selection for
one-to-one presentation scaling, bounded latest-scene mailbox behavior, explicit fallback preview
staging, deferred-only acquire-stall suppression, and logical-to-target scissor mapping for
mismatched extents and hosted origins. Feature-on compilation covers profiler instrumentation plus
the capability-gated maintenance, exact-present, and one-to-one scaling paths.

## Required hardware qualification

The DXGI path now has compile and portable descriptor/synchronization evidence plus one targeted
manual run on the affected Windows hardware. That run initially exposed invalid D3D sharing flags
and then a missing keyed-mutex ownership handoff; after both were corrected, the application
rendered through the DXGI route and the reporter confirmed the original stretch/squash behavior was
gone. A subsequent shrink-cycle report still exposed a large black region and localized an ordering
bug: the synchronous resize path ran from inside the nested `WM_SIZE`, before the outer
`WM_WINDOWPOSCHANGED` transaction returned from the lower/default procedure. Delaying every
direction fixed contraction but exposed the old smaller buffer during expansion. The corrected
pre-expansion/post-contraction synchronization still requires the next manual two-direction check. The
earlier targeted developer report passed the former deferred policy
after an AMD driver update: the previously reproducible multi-second acquire stalls no longer
occurred. The broader user-run Windows matrix must still
compare DXGI HWND against Vulkan WSI, responsive/deferred modes, FIFO/MAILBOX, exact-present/fallback completion,
maintenance/legacy synchronization, active drag/release, and repeated enlargement/shrink cycles.
Acceptance requires no sustained compositor
stretch on the responsive path, no multi-second acquire spikes, aligned hit testing and visuals,
bounded retirement, and zero validation errors. The implementation is not claimed to meet those
hardware criteria until a new report is recorded.

## Reference audit

Inspected read-only references:

- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/swapchain/mod.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/swapchain/native.rs`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/swapchain/khr/khr_swapchain_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/impeller/renderer/backend/vulkan/swapchain/khr/khr_swapchain_impl_vk.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/shell/platform/windows/flutter_windows_view.cc`
- `../other-rendering-libs/flutter/engine/src/flutter/shell/platform/windows/compositor_opengl.cc`
- `../other-rendering-libs/slint/internal/renderers/skia/wgpu_30_surface.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/dx12/dcomp.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/dx12/mod.rs`
- `../other-rendering-libs/wgpu/wgpu-hal/src/vulkan/device.rs`
- `../other-rendering-libs/zed/crates/gpui_windows/src/directx_renderer.rs`

Extracted invariants: desired logical size is separate from the current swapchain generation;
acquire/present synchronization owns explicit reusable slots; old swapchains remain alive until
render and presentation use complete; and ordinary UI resize reconfigures the presentation extent
rather than making old-surface stretching part of the layout contract. Flutter's Windows embedder
blocks its platform resize callback until a correctly sized frame is generated and presented, then
calls `DwmFlush` specifically to avoid the old surface being stretched over the new view. Telorgon
adopts that bounded transaction while retaining its dedicated Vulkan worker; it does not adopt
Flutter's unbounded GPU acquisition behavior. Slint reconfigures its surface from the physical
resize event.

The exact-present addition also compared the same wgpu and Flutter swapchain paths. Neither uses
`VK_KHR_present_wait`; wgpu explicitly records the lack of a portable present wait in its frame-
latency handling, while Flutter relies on FIFO plus its platform resize callback. The mechanism was
therefore cross-checked directly against the Khronos `VK_KHR_present_id` and
`VK_KHR_present_wait` specification. Adopted invariants are: enable both extensions and both feature
bits as one capability; assign strictly increasing nonzero IDs per swapchain; attach exactly one ID
per queued swapchain present; poll only the live generation; and retain maintenance fences for
presentation-resource lifetime even when an ID supplies the managed resize proof.

The one-to-one scaling addition rechecked the wgpu, Flutter, and Slint surface paths above. None
requests `VK_EXT_surface_maintenance1` presentation scaling, so no implementation code was copied.
The Khronos surface-maintenance contract is the authority: query scaling for the selected present
mode through `vkGetPhysicalDeviceSurfaceCapabilities2KHR`, request only reported scaling/gravity
bits, require the device's `swapchainMaintenance1` feature, and leave the swapchain create chain
unchanged when any required capability is absent.

The DXGI addition compared two independent Windows implementations. Zed's D3D11 renderer creates
an HWND flip swapchain with `DXGI_SCALING_NONE`, releases back-buffer references before
`ResizeBuffers`, and disables DXGI's Alt+Enter ownership. wgpu's DX12 paths establish the separate
HWND and DirectComposition ownership shapes, while its Vulkan external-image path establishes the
dedicated Win32 external-memory import pattern. Telorgon adopts the HWND scaling policy and same-
adapter imported-image lifetime; it does not copy either renderer architecture. The bridge was
cross-checked against Microsoft's
[`DXGI_SCALING`](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/ne-dxgi1_2-dxgi_scaling),
[`ID3D11DeviceContext4::Wait`](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_3/nf-d3d11_3-id3d11devicecontext4-wait),
and [`ID3D11Fence::CreateSharedHandle`](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_3/nf-d3d11_3-id3d11fence-createsharedhandle)
contracts plus the Khronos external-memory and external-semaphore rules.

Rejected alternatives: increasing the acquire timeout, busy retry, device/queue idle during live
resize, treating a worker thread alone as a cure for a blocked WSI call, blindly increasing frames
in flight, making scaled preview the default, and treating renderer crop/scissor as a way to
override native presentation scaling. A DirectComposition presenter was also rejected for this
experiment because `CreateSwapChainForComposition` requires `DXGI_SCALING_STRETCH`. The adopted UI-thread wait is limited to 100 ms and observes
worker progress; it performs no Vulkan work itself. Presentation one-to-one scaling/gravity remains
a capability-gated distortion fallback, not a substitute for generating layout at the exact view
extent or proving presentation completion. Exact present wait is not emulated when unsupported,
does not replace maintenance fences, and never introduces queue/device idle or an unbounded host
wait.
