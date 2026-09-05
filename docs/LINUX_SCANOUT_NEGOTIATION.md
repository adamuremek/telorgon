# Linux scanout negotiation

Status: implemented startup behavior, pending Linux hardware qualification.

The Wayland host now prepares the selected renderer before allocating primary-plane
buffers. This fixes the observed NVIDIA rejection of XRGB8888 + LINEAR +
SCANOUT|RENDERING, while retaining a distinct CPU scanout path.

## Startup and policy

The owner queries compatible CRTC/primary-plane combinations and checked IN_FORMATS
blobs. The existing connected-output/preferred-mode policy remains in effect.
Vulkan matches the DRM device using VK_EXT_physical_device_drm or verified PCI
identity; adapter enumeration order and performance scores do not establish
sharing compatibility. Exact color-attachment DMA-BUF import capabilities are
queried once per selected Vulkan device and output extent, then intersected with
each plane's format/modifier tuples. XRGB8888 and ARGB8888 are the supported
primary formats. Single-memory-plane layouts are required by the existing Vulkan
importer. Explicit layout information and KMS modifier support are required for
this Vulkan path.

GBM chooses a layout from the compatible set with SCANOUT|RENDERING. Returned
metadata is checked before import. A legacy GBM allocation is also eligible if it
returns an explicit modifier in that same negotiated set. The software path
tries explicit LINEAR + SCANOUT, then legacy SCANOUT|LINEAR, then capability-gated
mapped DRM dumb buffers. CPU candidates are mapped, initialized and unmapped
before acceptance; mapping uses the correct READ_WRITE transfer flags so partial
updates preserve untouched pixels on staging-backed drivers. No CPU path adds a
GPU rendering requirement. Missing IN_FORMATS means legacy capability rather than
an implicit claim of LINEAR support. Unknown layout is never passed to Vulkan's
explicit import API.

Each attempt allocates and validates one full-output-sized buffer first, including
Vulkan import or CPU access, framebuffer creation and an atomic TEST_ONLY modeset
request. That buffer becomes the first of the three frame slots. Remaining slots
are created and checked before the path is published. An imported but rejected
modifier is removed from the candidate set after the partial pool is destroyed.
There are at most 64 pool attempts per renderer policy. Permission/seat errors,
resource exhaustion and device loss stop the search; they are not disguised as
modifier incompatibility. Failure reports retain the stage, tuple and native
error. Success identifies the renderer, device, layout and slot count.

`Vulkan` and `Software` remain strict selections. `Auto` can abandon the complete
Vulkan transaction and build fresh CPU buffers; its fallback is logged. Client
DMA-BUF advertisement is derived from the successfully selected renderer. Neither
negotiation nor repeated capability queries run in the frame loop. Hardware
cursor selection remains independent with composited fallback.

## Ownership and ABI

Trial and prepared owners destroy imported Vulkan targets and KMS framebuffer
references before their BOs. Each failed pool is fully released before the next
attempt. The Vulkan completion worker is retired before its renderer's scenes and
targets. Existing page-flip retirement remains the prerequisite for buffer reuse.
Framebuffer cleanup reports native errors rather than panicking during rollback.

The KMS invalid-modifier constant now matches Linux's 0x00ffffffffffffff and the
Vulkan importer's value. GBM write transfer is 2, not the read flag 1; retained CPU
updates use READ_WRITE (3). Native allocation errors are captured immediately.
GBM modifiers2 and libdrm dumb-buffer convenience entrypoints are resolved only
when used, so an unavailable optional entrypoint becomes a capability failure
rather than a process-loader failure. This does not remove the framework's other
native library requirements.

## Reference-source audit

The checkout's adjacent other-rendering-libs directory was unavailable. In lieu
of a fictitious local review, the following independent sources were downloaded
and inspected read-only on 2026-09-05. No source code was copied into Telorgon.

- [Smithay allocator](https://github.com/Smithay/smithay/blob/master/src/backend/allocator/gbm.rs):
  `GbmBuffer::from_bo_with_node`, `GbmAllocator::create_buffer_with_flags`, export
  and import paths. Downloaded snapshot SHA-256:
  `cf510131d2a0e283df87b3e7d1e677cf9c2d1325475abd56ddc1f547196143a7`.
- [wlroots allocator](https://github.com/swaywm/wlroots/blob/master/render/allocator/gbm.c):
  `create_buffer`, `export_gbm_bo`, `buffer_destroy`, `allocator_destroy`.
  Downloaded snapshot SHA-256:
  `73624cf88d5ceece978906766304329d4fa2b3281e08009f1e55b223c059c40e`.

Extracted invariants: usage flags and explicit modifiers are independent
constraints; implicit layout cannot masquerade as LINEAR; exported memory-plane
metadata must describe the allocation actually returned; partial exports and
buffers must retire before their backing allocator/device. Telorgon additionally
requires exact Vulkan/KMS negotiation and validates each candidate with KMS.

Rejected alternatives: vendor-name quirks, forcing a numerical modifier, stripping
RENDERING from Vulkan allocations, claiming allocation success proves import or
display support, reusing failed GPU buffers for Auto's CPU fallback, and repeatedly
probing in the frame loop. A universal cross-GPU copy abstraction is outside this
change; a matching adapter is required for direct Vulkan scanout.

Primary specifications and ABI checks:

- [Linux buffer exchange](https://docs.kernel.org/userspace-api/dma-buf-alloc-exchange.html):
  producer/consumer intersections, exact metadata, import validation and the
  permitted legacy-to-explicit upgrade when the allocator reports a valid layout.
- [Vulkan DRM identity](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceDrmPropertiesEXT.html):
  primary/render node identity properties.
- [Vulkan modifier extension](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_image_drm_format_modifier.html):
  exact image-usage queries and explicit imported plane layouts.
- Installed `gbm.h`, `xf86drmMode.h`, `drm_mode.h` and `drm_fourcc.h`: native function
  signatures, transfer flags, invalid modifier, and IN_FORMATS byte layout.
  A compile-only C static-assert check against these headers passed.

## Validation and remaining qualification

Portable tests cover NVIDIA-style tiled-only render targets versus linear CPU
buffers, linear-capable renderers, disjoint/missing capabilities, truncated or
malformed modifier blobs, bitmask offsets above 63, ABI constants, mapped row
padding and damage, strict renderer policies, partial-pool cleanup, bounded
modifier rejection, error classification and existing page-flip buffer retirement.
Compile checks cover the Linux desktop feature combination and the software-only
application build. Native hardware applications were not launched.

A user-run qualification matrix still needs NVIDIA, AMD and Intel, both renderer
policies, first display and repeated page flips, hybrid-device identity, memory
pressure and software mapping. This change does not implement hotplug/session
reconstruction, multi-output policy, multi-plane/YUV scanout, cross-GPU copies,
HDR or a guarantee that unsupported hardware can render. Reconstruction code,
when added, must run this transaction again for its new device/output generation.
