# GPU Ownership, Frame, Synchronization, and Recovery Contract

## Status and authority

This document completes implementation-planning Gate 3. It is the target contract for resource
ownership, frame recording, submission, presentation, hosted rendering, completion, destruction,
readback, and recovery in Telorgon's first Vulkan implementation. It refines the API shapes in
[IMPLEMENTATION_BLUEPRINT.md](IMPLEMENTATION_BLUEPRINT.md) and the policy in
[VULKAN_IMPLEMENTATION_PLAN.md](VULKAN_IMPLEMENTATION_PLAN.md).

This is an implementation specification, not a claim about the current code. If an implementation
needs a different owner, synchronization direction, or lifetime relationship, update this document
before changing the code. Gate 4 adds scene, resource-use, upload, and shader ABI details in
[SCENE_GPU_ABI_AND_SHADERS.md](SCENE_GPU_ABI_AND_SHADERS.md); those details do not weaken the
ownership rules fixed here. Gate 6 maps these obligations to compile, trace, real-GPU, managed, and
hosted evidence in [ACCEPTANCE_AND_QUALIFICATION.md](ACCEPTANCE_AND_QUALIFICATION.md).
Gate 9 fixes borrowed native-window handles and typed external-resource import/release payloads in
[PLATFORM_INTEGRATION_CONTRACT.md](PLATFORM_INTEGRATION_CONTRACT.md#12-native-handles-and-external-gpu-resources);
those adapters must preserve every completion and destruction rule here.

## 1. Decisions fixed by Gate 3

1. Telorgon distinguishes render-submission completion, swapchain-image acquisition, and presentation
   completion. No one is evidence for either of the other two.
2. The first owned Vulkan backend uses one graphics submission timeline per logical device.
   Completion values are monotonic and are never compared across devices or completion domains.
3. Device operations take shared `&self` access and synchronize their internal queues, allocators,
   caches, and garbage lists. A mutable per-view scene still cannot be rendered concurrently.
4. Device frame slots and swapchain images are independent pools. A frame slot is reusable after its
   submission completes; a swapchain image is usable only between a successful acquire and its
   release through present or an explicit supported release operation.
5. The initial embedded Vulkan contract is command-only. Telorgon records into a host command buffer;
   it does not end, reset, submit, or present that command buffer.
6. Every submitted resource is kept alive by an in-flight resource set. Ordinary `Drop` never waits
   for a queue or device and never destroys a native object still referenced by pending work.
7. Swapchain acquire state is a linear token. The token, not a second mutable presenter call,
   performs present or discard. This makes double-acquire and reconfigure-while-acquired invalid in
   safe Rust.
8. Presentation wait semaphores are owned per swapchain image. They are not recycled merely because
   the render submission's frame fence or timeline value completed.
9. Readback records a real GPU copy in the same graphics frame for the initial backend and becomes
   readable only after that frame's completion is proven. It remains an explicit test/export/
   diagnostics capability.
10. No internal worker thread, hidden queue submission, normal-frame device idle, or CPU rendering
    fallback is permitted.

## 2. Vocabulary and ownership classes

Telorgon uses these ownership classes in native backend code:

| Class | Meaning | Destruction rule |
| --- | --- | --- |
| **Owned** | Telorgon created the native object and owns its allocation. | Telorgon destroys it after all recorded and submitted uses complete. |
| **Internally shared** | A private strong guard shares an owned parent or resource inside Telorgon. | Final guard loss schedules or performs safe destruction; no public raw-handle ownership is implied. |
| **Host-borrowed** | An embedding host owns the native object and grants temporary use under an `unsafe` contract. | Telorgon never destroys it. The host must outlive all Telorgon children and complete explicit shutdown. |
| **Imported** | A foreign resource is usable under a typed import and acquire/release contract. | Gate 4 fixes logical image metadata; Gate 9 fixes typed per-platform payload, linear ownership, acquire, release-after-final-read, and failure cleanup. It is never treated as an ordinary owned allocation. |

Terms used throughout this document:

- **frame slot**: device-owned reusable recording storage, descriptor arenas, staging ranges, and
  in-flight pins;
- **recording frame**: exclusive permission to record through one frame slot or host command buffer;
- **recorded frame**: a finished owned command buffer waiting to be submitted exactly once;
- **completion point**: opaque proof boundary for work in one completion domain;
- **resource pin**: a strong reference that prevents destruction while recorded or submitted work
  may use a resource;
- **swapchain generation**: one configured swapchain and its images, views, image-indexed
  presentation synchronization, and retirement state;
- **acquired token**: unique permission to use and then release one acquired swapchain image.

## 3. Native object ownership

| Object | Owner | Important lifetime rule |
| --- | --- | --- |
| Vulkan loader entry, instance, debug messenger | `VulkanInstance` | Instance children and debug messenger are destroyed before the instance. |
| Physical-device handle | `VulkanInstance`/selection report, non-owning | It is never destroyed by Telorgon. |
| Logical device, allocator, caches, completion timeline | `VulkanDevice` in owned mode | All Telorgon device children are destroyed before the logical device. |
| Host logical device and host queues | embedding host | Telorgon never destroys or waits them without the hosted contract explicitly allowing the operation. |
| Queue access authority | `VulkanDevice` | Every operation on the same native queue is externally synchronized by one shared lock, even if graphics and present roles alias. |
| Per-view retained GPU state | `VulkanScene` | Holds a private strong device guard and may be mutated by only one scene operation at a time. |
| Owned frame slots, command pools, command buffers | `VulkanDevice` | Reset/recycled only after the slot's submission completion is reached. |
| Host command pool and command buffer | embedding host | Telorgon records only within the declared interval and never begins, ends, resets, or submits it. |
| Owned buffers/images and allocations | `VulkanDevice` resource wrappers | Native handle is destroyed before its allocation is freed, after last use completes. |
| Surface, swapchain, swapchain image views | `VulkanWinitPresenter` | Swapchain images are borrowed from the swapchain; the presenter owns only their views and synchronization. |
| Swapchain images | presentation engine/swapchain | Telorgon does not allocate or destroy them individually. |
| DXGI factory, D3D11 device/context/fence, and HWND swapchain | `telorgon-presenter-dxgi` | They are created on the adapter whose LUID matches the Vulkan device and are released after bridge work completes. |
| D3D11 shared bridge texture | `VulkanDxgiBridge` | D3D11 owns the allocation; its NT handle is consumed to import dedicated Vulkan image memory. Both API views die only after the shared fence proves final use. |
| Imported Vulkan bridge image/memory/view | `VulkanDxgiBridge` | These are Vulkan children of the selected device but alias D3D11-owned storage. The Vulkan view/image/memory destruction order precedes dropping the D3D11 texture. |
| Shared D3D11 fence / Vulkan timeline semaphore payload | `VulkanDxgiBridge` | One fence payload crosses both APIs. Monotonic render-done/D3D-done pairs serialize access; the Win32 sharing handle is closed immediately after Vulkan import. |
| Offscreen image and view | backend resource owner | A `VulkanTarget` borrows them and cannot extend their lifetime. |
| Target descriptor | the frame/acquired token/host descriptor that created it | It is non-owning, generation-checked, and valid only for its declared recording lifetime. |
| Readback staging allocation | pending readback operation | It remains pinned until completion and mapping/invalidation are safe. |

`VulkanInstance`, `VulkanDevice`, and resource wrappers may use private `Arc`-like guards. Public
types do not expose those guards or imply that cloning a wrapper clones a native Vulkan object.

## 4. Common backend lifetime shape

The initial internal backend contract uses generic associated frame and target lifetimes:

```rust,ignore
pub trait RenderBackend {
    type Scene;
    type FrameContext<'frame>
    where
        Self: 'frame;
    type Target<'frame>
    where
        Self: 'frame;

    fn create_scene(&self) -> RenderResult<Self::Scene>;

    fn apply_scene_delta(
        &self,
        scene: &mut Self::Scene,
        delta: &RenderSceneDelta,
    ) -> RenderResult<SceneUpdateStats>;

    fn render<'frame>(
        &self,
        scene: &mut Self::Scene,
        frame: &mut Self::FrameContext<'frame>,
        target: &Self::Target<'frame>,
        request: &RenderRequest,
    ) -> RenderResult<RenderStats>;
}
```

The associated lifetime permits an owned backend frame, a borrowed host command buffer, an
offscreen target, or a target borrowed from an acquired presentation token without erasing native
validity into integers. `&self` is required because one device is shared by multiple UI views and
hosts; internal synchronization protects shared device services. `&mut Scene` preserves per-view
mutation exclusivity.

`render` only records. It never submits, presents, waits, maps readback memory, or invokes a software
renderer.

## 5. Device and queue contract

### 5.1 Owned construction

`VulkanInstance` owns loading and instance extensions. WSI packages request required instance
extensions without adding Winit or `ash-window` to the renderer package:

```rust,ignore
pub struct InstanceExtensionRequest<'a> { /* names plus provenance */ }
pub struct BorrowedVulkanSurface<'a> { /* narrow unsafe surface query view */ }
pub struct PresentationRequirement<'a> { /* surface plus required queue role */ }

impl VulkanInstance {
    pub fn load(
        config: &VulkanConfig,
        extensions: &[InstanceExtensionRequest<'_>],
    ) -> RenderResult<Self>;
}

impl VulkanDevice {
    pub fn create_owned(
        instance: VulkanInstance,
        config: &VulkanConfig,
        selection: &DeviceSelection,
        presentation: Option<PresentationRequirement<'_>>,
    ) -> RenderResult<Self>;
}
```

The safe public presenter assembles the unsafe surface-query value internally. A target application
does not construct raw Vulkan handles.

The initial selection policy prefers one queue family supporting graphics and presentation. If the
chosen adapter requires separate families, the initial swapchain uses concurrent sharing between
those families. An exclusive-sharing ownership-transfer path is a later measured optimization, not
an implicit assumption in Slice 3.

### 5.2 Queue synchronization

- One synchronization guard exists per unique native queue handle, not per semantic role.
- Graphics submission and presentation lock the relevant queue guards for the native calls only.
- Queue locks are never held while waiting on completion, compiling UI, or calling application code.
- The initial backend has one graphics submission stream. It does not add a transfer or compute
  submission stream until profiling and a later queue plan justify one; Gate 4 lowers its uploads on
  the graphics stream.
- Hosts may advance completion or perform maintenance explicitly; Telorgon starts no maintenance
  thread.

## 6. Completion domains

### 6.1 Owned completion

An owned Vulkan device creates one timeline semaphore for graphics submission progress. Every
successful submission signals a strictly increasing nonzero value. Value zero represents already
complete/no submitted work.

```rust,ignore
pub struct CompletionPoint { /* private device/domain identity and value */ }
pub struct SubmissionReceipt { /* completion plus measured submission data */ }

pub enum CompletionStatus {
    Pending,
    Complete,
    DeviceLost,
}

pub enum WaitPolicy {
    Poll,
    Timeout(Duration),
}

impl VulkanDevice {
    pub fn poll_completion(&self, point: &CompletionPoint)
        -> RenderResult<CompletionStatus>;
    pub fn wait_for(&self, point: &CompletionPoint, policy: WaitPolicy)
        -> RenderResult<CompletionStatus>;
    pub fn maintain(&self) -> RenderResult<MaintenanceStats>;
}
```

`CompletionPoint` fields are private. It deliberately has no cross-domain ordering implementation.
The device rejects a point from another device or completion domain. `maintain` reads progress,
recycles completed frame slots, resolves readbacks, and drains eligible deferred destruction; it
does not wait for future work.

### 6.2 Completion is not presentation completion

A submission completion point proves that queue commands and their resource references completed.
It does not prove that a presentation operation stopped waiting on a binary semaphore. Presenter
retirement therefore uses image reacquisition or an explicit presentation fence when supported; it
never recycles presentation synchronization from the frame timeline alone. Presentation IDs and
`vkWaitForPresentKHR` provide a separate, exact-frame display-progress proof for managed resize;
they do not replace the semaphore-reuse and retirement proof.

### 6.3 Hosted completion

A host creates one opaque, monotonic `HostCompletionDomain` for the queue schedule into which Telorgon
records. After the host submits a receipt, it binds that receipt to a nondecreasing completion value
and later advances the completed value:

```rust,ignore
pub struct HostCompletionDomain { /* private identity and monotonic state */ }
pub struct HostCompletionPoint { /* domain identity and value */ }
pub struct HostedFrameReceipt { /* pins and usage report; must be resolved */ }

impl HostCompletionDomain {
    pub fn new() -> Self;
    pub fn point(&self, submitted_value: u64) -> RenderResult<HostCompletionPoint>;
}

impl VulkanDevice {
    pub fn commit_hosted(
        &self,
        receipt: HostedFrameReceipt,
        point: HostCompletionPoint,
    ) -> RenderResult<()>;

    pub fn discard_hosted(&self, receipt: HostedFrameReceipt) -> RenderResult<()>;

    pub fn advance_host_completion(
        &self,
        domain: &HostCompletionDomain,
        completed_value: u64,
    ) -> RenderResult<MaintenanceStats>;
}
```

Submitted values are nonzero. Several receipts included in one host submission may share a point;
later committed points and completed values must never move backwards. The device rejects a receipt
from another device/domain or an invalid value. `discard_hosted` is legal only when the host did not
submit the recorded commands. Dropping an unresolved receipt records a host-contract violation and
moves its pins to a quarantine set; it never guesses that GPU work completed.

## 7. Owned frame state machine

Device frame slots are independent of presenters and swapchain image count. Configuration selects a
small bounded number and the backend reports the effective count. Each slot owns:

- a command pool and primary command buffer;
- per-frame descriptor arena(s);
- staging-ring ranges and transient allocations;
- the resource pins collected while recording; and
- the last owned completion point that protects reuse.

```text
Available
   | begin_owned_frame
   v
Recording --finish--> Recorded --submit exactly once--> InFlight
   |                     |                                  |
   +--abort/drop----------+--drop/abort safely--------------+
                                                             | completion reached
                                                             v
                                                          Available
```

The API shape is:

```rust,ignore
pub struct VulkanRecordingFrame<'device> { /* exclusive slot; !Send */ }
pub struct VulkanRecordedFrame { /* finished slot; submit exactly once */ }
pub struct VulkanFrameContext<'frame> { /* sealed validated recording adapter */ }
pub struct SubmissionSync<'sync> { /* private optional wait/signal leases */ }

pub enum BeginFrameOutcome<'device> {
    Ready(VulkanRecordingFrame<'device>),
    Busy,
}

impl SubmissionSync<'static> {
    pub fn none() -> Self;
}

impl VulkanDevice {
    pub fn begin_owned_frame(
        &self,
        wait: WaitPolicy,
    ) -> RenderResult<BeginFrameOutcome<'_>>;

    pub fn submit(
        &self,
        frame: VulkanRecordedFrame,
        sync: SubmissionSync<'_>,
    ) -> RenderResult<SubmissionReceipt>;
}

impl VulkanRecordingFrame<'_> {
    pub fn context_mut(&mut self) -> &mut VulkanFrameContext<'_>;
    pub fn finish(self) -> RenderResult<VulkanRecordedFrame>;
    pub fn abort(self) -> RenderResult<()>;
}
```

`SubmissionSync` has private fields. Ordinary offscreen callers can construct only `none()`; the
Vulkan presenter obtains a narrow backend-native synchronization packet through the sealed presenter
interop described below. This is not a general raw-semaphore API.

`begin_owned_frame` selects only an available or completed slot. `WaitPolicy::Poll` returns `Busy`
if none is ready; an explicit bounded wait may also finish as `Busy`. It never performs an unbounded
hidden wait.

A recording frame is bound to the creation thread and is not `Send`. Before submission, `abort` or
`Drop` may end/reset an otherwise valid unused command buffer and release pins. After successful
submission, ownership of the slot and pins has transferred to the in-flight tracker, so user code
cannot reset or drop the pending command buffer independently.

The presenter uses the same `VulkanRecordedFrame`; there is no presenter-specific command pool.

## 8. Target contract

`VulkanTarget<'frame>` is a non-owning validated view. Its private representation includes:

- device identity and resource/swapchain-generation identity;
- image and image-view identity;
- format, extent, sample count, and required usage flags;
- requested render area;
- initial and final semantic image state;
- queue-family context; and
- color-space and alpha-mode metadata.

The backend validates before recording that the target and frame belong to the same device and
completion domain, the region is in bounds, required attachment/sample/format features exist, and
the declared initial/final states can be honored. It returns `InvalidTarget` or `Unsupported`
instead of recording undefined behavior.

A target borrows exactly one of:

1. an owned offscreen image wrapper;
2. a live `AcquiredVulkanFrame` token; or
3. a host target descriptor and its recording interval.

It never owns or destroys the image/view. Native handles remain private except in the explicitly
unsafe interop module.

## 9. Presenter and swapchain ownership

### 9.1 State model

`VulkanWinitPresenter` owns the surface and at most one active swapchain generation. Its explicit
state is one of:

```text
Unconfigured -> Ready <-> Acquired
       |          |          |
       v          v          v
   Suspended  NeedsReconfigure
       ^          |
       +----------+

Any configured state -> SurfaceLost
Any operational state -> DeviceLost
Any state -> Shutdown
```

Zero drawable extent enters `Suspended`; no swapchain image is acquired and the host does not busy
loop. Resize records a pending extent and reconfigures only when no acquisition token is live.

### 9.2 Linear acquired token

```rust,ignore
pub enum AcquireOutcome<'presenter> {
    Ready(AcquiredVulkanFrame<'presenter>),
    Suspended,
    NotReady,
    NeedsReconfigure,
}

pub struct AcquiredVulkanFrame<'presenter> {
    /* private mutable presenter borrow, generation, image index, and acquire sync */
}

pub enum PresentDisposition {
    Presented,
    PresentedSuboptimal,
    NeedsReconfigure,
    SurfaceLost,
}

pub struct PresentOutcome {
    pub submission: SubmissionReceipt,
    pub disposition: PresentDisposition,
}

pub struct PresentError {
    /* typed kind/context plus optional SubmissionReceipt when submit succeeded */
}

impl PresentError {
    pub fn submission(&self) -> Option<&SubmissionReceipt>;
    pub fn into_submission(self) -> Option<SubmissionReceipt>;
}

impl VulkanWinitPresenter {
    pub fn acquire(
        &mut self,
        device: &VulkanDevice,
        frame: &VulkanRecordingFrame<'_>,
    ) -> Result<AcquireOutcome<'_>, PresentError>;

    pub fn resize(&mut self, extent: SizeI);
    pub fn suspend(&mut self) -> Result<(), PresentError>;
    pub fn resume(&mut self, extent: SizeI) -> Result<(), PresentError>;
}

impl AcquiredVulkanFrame<'_> {
    pub fn target(&self) -> VulkanTarget<'_>;

    pub fn submit_and_present(
        self,
        device: &VulkanDevice,
        frame: VulkanRecordedFrame,
    ) -> Result<PresentOutcome, PresentError>;

    pub fn discard(self, device: &VulkanDevice) -> Result<(), PresentError>;
}
```

The acquired token contains no backend frame slot, but records the slot identity supplied to
`acquire`; `submit_and_present` rejects a recorded frame from a different slot/device. Its mutable
presenter borrow prevents another acquire, resize application, reconfigure, or presenter destruction
through safe Rust while it is live. Consuming methods release that borrow after the image is
presented or explicitly abandoned.

Dropping a token without `submit_and_present` or `discard` marks the presenter `NeedsReconfigure`
and prevents silent image reuse. When swapchain maintenance is enabled it also attempts explicit
image release; the legacy path retires the whole generation instead of pretending the image became
available normally.

`discard` uses `VK_EXT_swapchain_maintenance1`/`VK_KHR_swapchain_maintenance1` image release when
available. Without an explicit image-release facility, discard retires the generation and reports
the legacy retirement path; it does not pretend the image became available normally.

Because the presenter and renderer are separate crates, `telorgon_renderer_vulkan::interop::presenter`
provides a narrow sealed SPI for an opaque frame-slot binding and presentation submission waits/
signals. Its fields and safe constructors are private, it validates device identity, and it is not
re-exported by the `telorgon` umbrella. Any constructor accepting native semaphore or queue data is
`unsafe` and documents the Vulkan lifetime and external-synchronization obligations. The SPI does
not expose general application queue submission.

### 9.3 Semaphore ownership

- Image-available/acquire semaphores may be indexed by device frame slot and are reused only after
  the submission that waited on them has completed.
- On the maintenance path, render-finished semaphores and presentation fences are indexed by
  acquire slot; a slot is reused only after both render submission and presentation complete.
- On the legacy path, render-finished semaphores waited by `vkQueuePresentKHR` are indexed by
  swapchain image. Reacquiring image `i` and satisfying that acquire's synchronization proves that
  the previous presentation of image `i` no longer needs its render-finished semaphore.
- If swapchain-maintenance presentation fences are available, the presenter uses them for explicit
  presentation completion and generation retirement and omits the acquire fence.
- If present ID/wait is available, each queued frame receives a monotonically increasing
  swapchain-local ID. Managed resize prefers polling that ID, while maintenance fences continue to
  own semaphore reuse and generation retirement.
- A frame timeline value or submission fence alone never authorizes reuse or destruction of a
  presentation-wait semaphore.

### 9.4 Managed redraw order

```text
prepare AppRuntime and drain scene deltas
  -> if idle, stop without acquiring
  -> begin/reserve an owned device frame
  -> acquire a swapchain image for that reserved frame slot
       -> on suspended/not-ready/reconfigure: abort the unused device frame
  -> borrow the acquired target
  -> record VulkanDevice::render
  -> finish the device frame
  -> acquired_token.submit_and_present(device, recorded_frame)
  -> device.maintain() without waiting
```

Reserving the device frame before acquisition prevents a scarce acquired image from being held
while waiting for reusable command storage. If recording fails, the acquired token is explicitly
discarded/recovered and the recording frame is aborted.

### 9.5 Windows DXGI bridge

When enabled and supported, `VulkanDxgiBridge` replaces only the Vulkan WSI presentation edge;
scene compilation, recording, and rendering remain owned by `VulkanDevice`. The presenter creates
a D3D11 device on the exact Vulkan adapter LUID, imports D3D11-owned shared textures into Vulkan,
and owns an HWND flip swapchain configured with `DXGI_SCALING_NONE`.
The D3D11 allocation uses the runtime-required NT-handle/keyed-mutex misc-flag pair; Telorgon uses
the keyed mutex as an explicit 0-to-1 Vulkan / 1-to-0 D3D11 ownership handoff and the separately
shared D3D11 fence/Vulkan timeline semaphore for queue ordering and retirement.

Each acquired bridge token exclusively borrows one imported image. Vulkan waits for the preceding
D3D-complete timeline value, acquires mutex key 0, renders, releases key 1, and signals
render-complete. D3D11 acquires key 1, waits for render-complete, copies to the current DXGI back
buffer, presents, signals D3D-complete, and releases key 0. The next Vulkan frame cannot
touch a bridge texture until that value is reached. Resize and suspension wait for the final D3D
value before destroying the Vulkan alias and D3D texture, release all temporary back-buffer COM
references before `ResizeBuffers`, and recreate exact-extent bridge images. No ordinary frame uses
CPU readback, CPU upload, a CPU fence wait, or Vulkan queue/device idle.

Default `WM_WINDOWPOSCHANGED` processing emits the nested `WM_SIZE`. The thread-affine Win32
subclass prepares an expanding exact buffer before the native target grows, so the old smaller
target clips already-current content. A contraction retains the old larger buffer for clipping and
posts one coalesced private HWND message; the modal queue delivers it after the complete native
sent-message transaction unwinds. Both paths repeat synchronization after the boundary, and the
bounded present plus `DwmFlush` then observe the committed extent.

This bridge does not treat `Present` success or its D3D-complete value as proof that DWM displayed
the frame. Until a DXGI display-completion primitive is integrated, managed resize retains the
separate bounded display-interval/DWM barrier.

## 10. Swapchain reconfiguration and retirement

Reconfiguration follows these rules:

1. Coalesce resize requests and stop acquisition at zero extent.
2. Require that no `AcquiredVulkanFrame` token is live.
3. Wait only for owned submissions that reference the old generation, never ordinary unrelated
   device work.
4. Create the replacement with `oldSwapchain` and move the old generation to a retirement list.
5. Destroy old views and synchronization only after both render use and presentation use are proven
   complete.

Interactive resize separates logical metrics from the physical swapchain. Exact native resize
generations publish revisioned logical extents to layout, hit testing, semantics, and scene
compilation at display cadence. `Started` and `Updating` render the newest logical scene through the
current swapchain with suboptimal reconfiguration deferred. `Ended` presents one final logical-size
preview when the old generation remains usable and then commits one replacement swapchain. Zero
extent suspends, while `OUT_OF_DATE` and surface loss may force early recovery. Projection and
scissor mapping use the same logical-to-target transform.

With present ID/wait, the managed resize barrier can identify the exact queued frame. With
swapchain-maintenance presentation fences, presentation-resource retirement remains explicit. On
Vulkan implementations without maintenance fences, per-image semaphore reuse relies on
reacquisition. Full generation retirement uses a
documented conservative legacy path during rare reconfigure/shutdown, may require queue/device idle
in that exceptional path, and emits a diagnostic. Because queue idle alone is not a formal proof
that the presentation engine released every waited semaphore, production qualification on such a
path requires validation for the target platform. Normal frames never call `vkQueueWaitIdle` or
`vkDeviceWaitIdle`.

`OUT_OF_DATE` and `SUBOPTIMAL` are presenter state outcomes. If the render submission succeeded but
present then reports an exceptional result, `PresentOutcome` or `PresentError` retains the
`SubmissionReceipt`; the submission and its resource lifetimes are not forgotten.

## 11. Hosted command-only rendering

### 11.1 Unsafe import boundary

The embedding host creates a hosted device with an unsafe descriptor that declares:

- instance, physical-device, logical-device, and queue handles;
- enabled extensions/features and device dispatch compatibility;
- queue-family and external-synchronization rules;
- allocator callbacks or the permitted allocation policy;
- host thread/command-buffer recording policy;
- the `HostCompletionDomain`; and
- the requirement that the native device outlive every Telorgon child and receipt.

The constructor is unsafe because Rust cannot verify foreign Vulkan object lifetime or queue
synchronization. After valid construction, ordinary scene update and render calls remain safe.

### 11.2 Hosted recording interval

```rust,ignore
pub struct HostedFrameDescriptor<'host> {
    /* borrowed recording primary command buffer, queue family, thread,
       target declarations, host completion domain, and recording state */
}

pub struct VulkanHostedFrame<'host> { /* validated command-only context; !Send */ }

impl VulkanDevice {
    pub unsafe fn begin_hosted_frame<'host>(
        &self,
        descriptor: HostedFrameDescriptor<'host>,
    ) -> RenderResult<VulkanHostedFrame<'host>>;
}

impl VulkanHostedFrame<'_> {
    pub fn context_and_target(
        &mut self,
    ) -> (&mut VulkanFrameContext<'_>, VulkanTarget<'_>);
    pub fn finish(self) -> RenderResult<HostedFrameReceipt>;
    pub fn abort(self) -> RenderResult<()>;
}
```

At entry, the command buffer must be recording as a primary command buffer and outside a legacy
render pass. Telorgon may begin/end dynamic rendering and record barriers/draw/copy commands only in
the declared interval. It does not begin/end/reset the host command buffer, submit a queue, signal a
host primitive, or present.

The descriptor supplies initial/final semantic resource states. The initial Vulkan synchronization2
mapping is fixed by [Gate 4](SCENE_GPU_ABI_AND_SHADERS.md#11-semantic-use-to-vulkan-synchronization).
A missing or impossible state is a host-contract error, not permission to guess.

`finish` moves all referenced resource pins and the recorded resource-usage report into the
`HostedFrameReceipt`. The host must commit or discard it as defined in section 6.3.

Hosted submission support, if later added, is a separate opt-in capability with explicit queue
authority. It does not change command-only behavior.

## 12. Resource pins and deferred destruction

Every command-recording helper resolves logical handles to private strong resource pins. A
successful owned submit moves those pins into an in-flight batch tagged with its completion point.
A hosted finish moves them into its unresolved receipt and then into the host completion domain on
commit.

When the last application-facing resource wrapper is dropped:

1. no wait is performed;
2. the resource's last-use completion point is computed from submitted use;
3. a typed destruction record enters the relevant completion-domain garbage queue; and
4. `maintain`, explicit waits, frame acquisition, and shutdown drain records whose proof is complete.

Destruction records encode dependency order. Views/samplers/descriptors and API handles are
destroyed before parent images/buffers and before allocator memory is freed. Pipeline objects are
destroyed before layouts; swapchain views and synchronization are destroyed before the swapchain;
all owned device children are destroyed before the device.

Recorded-but-never-submitted owned work releases pins during abort. Submitted work can never return
to that path. Unresolved hosted receipts are quarantined rather than prematurely destroyed.

## 13. Readback

The initial Vulkan backend records readback copies on the same graphics command buffer and queue as
the rendering that produces the target. There is no separate transfer context or hidden submission.

```rust,ignore
pub struct PendingReadback { /* staging allocation plus unbound/bound completion */ }

pub enum ReadbackStatus {
    AwaitingSubmission,
    Pending,
    Ready,
    DeviceLost,
}

pub trait RenderReadback<B: RenderBackend> {
    type Pending;

    fn record_readback<'frame>(
        &self,
        backend: &B,
        frame: &mut B::FrameContext<'frame>,
        target: &B::Target<'frame>,
        request: &ReadbackRequest,
    ) -> RenderResult<Self::Pending>;
}

impl VulkanDevice {
    pub fn readback_status(&self, pending: &PendingReadback)
        -> RenderResult<ReadbackStatus>;
    pub fn map_readback(&self, pending: PendingReadback)
        -> RenderResult<ReadbackImage>;
}
```

For an owned frame, submission binds the pending readback to the returned `CompletionPoint`. For a
hosted frame, committing the `HostedFrameReceipt` binds it to the declared host completion point.
The pending value and its frame share private binding state, so submission/host commit attaches the
correct point without caller handle plumbing. Mapping is rejected before `Ready`; non-coherent
memory is invalidated before CPU access. An explicit bounded-wait convenience may be used by tests
and export tools. Normal render and present paths never read pixels back.

## 14. Errors and recovery ownership

Core rendering errors are device/recording failures:

```rust,ignore
pub enum RenderErrorKind {
    Unsupported,
    InvalidTarget,
    InvalidScene,
    OutOfMemory,
    DeviceLost,
    HostContract,
    Internal,
}
```

Surface loss, out-of-date, suboptimal, acquisition timeout/not-ready, and presentation failure use
the presenter's typed result/error. They do not enter the backend-neutral render error merely
because the initial backend is Vulkan.

`VulkanDevice` has `Ready`, `Lost`, `ShuttingDown`, and `Destroyed` states. Any native device-lost
result atomically marks it `Lost`; new recording/submission is rejected and no automatic retry loop
runs. Device loss is permanent for that logical device.

Recovery is host orchestration:

1. preserve `AppRuntime` and its CPU retained scene;
2. stop use of the lost device and best-effort destroy its children under Vulkan's device-loss
   rules;
3. create a replacement instance/device/presenter as required;
4. create a replacement `VulkanScene`;
5. request `AppRuntime::scene_snapshot`, apply it to the new scene, and resume; and
6. publish diagnostics rather than silently selecting software rendering.

Surface loss is presenter-local first: recreate the surface and recheck adapter queue presentation
support. If the existing device cannot present to the replacement surface, the host performs the
device recovery sequence.

## 15. Shutdown order

Managed shutdown is explicit and ordered:

1. stop requesting frames and resolve every acquired token;
2. abort unsubmitted recording/recorded frames;
3. wait only for remaining owned submissions during final shutdown;
4. drop backend scenes and application-owned GPU resources;
5. retire/destroy presenter generations, surface, and presentation synchronization;
6. drain eligible deferred destruction and destroy device-owned children, allocator, timeline, and
   logical device;
7. destroy instance children and the instance; then
8. release the platform window.

Internal strong parent guards make accidental Rust drop order safe, but explicit shutdown reports
leaked acquisitions, unresolved host receipts, or quarantined resources.

Hosted mode requires `shutdown_hosted(completed_domains)` before the host destroys its Vulkan
device. Shutdown rejects outstanding receipts or incomplete resource pins. After device loss, a
force-abandon path may quarantine bookkeeping for best-effort teardown, but it never reports the
resources as having completed normally.

## 16. Threading contract

- `VulkanDevice` is `Send + Sync` through internal synchronization.
- `VulkanScene` may move between threads when its resource types permit, but one mutable scene
  operation at a time is required.
- `VulkanRecordingFrame`, `VulkanHostedFrame`, and presenter acquired tokens are `!Send` and must be
  used/dropped on their creation thread.
- Native queue calls are externally synchronized through the device queue guard.
- Presenter mutation remains on the platform event-loop thread.
- No background completion, upload, shader-compilation, or presentation thread is created by the
  backend.

## 17. Cross-API portability rules

The portable concepts are device, per-view scene, recording frame, typed target, recorded work,
completion domain/point, resource pins, presentation token, and hosted receipt. Vulkan-specific
objects remain behind the Vulkan backend or presenter.

Expected mappings are:

| Telorgon concept | Vulkan | Metal | Direct3D 12 |
| --- | --- | --- | --- |
| Owned completion point | timeline semaphore value | shared-event value or command-buffer completion adapter | fence value |
| Recording frame | command buffer from a completed command pool slot | command buffer/encoder interval | command list from a completed allocator slot |
| Deferred destruction | timeline-tagged garbage | completion-value-tagged retained objects | fence-tagged garbage |
| Acquired presentation token | swapchain image acquire/present | drawable lifetime | back-buffer index/present interval |
| Hosted recording | borrowed recording command buffer | borrowed command buffer/encoder contract | borrowed command list contract |

The common API does not expose Vulkan layouts, pipeline stages, queue-family indices, semaphores,
Metal encoders, D3D resource states, descriptor heaps, or console SDK handles. Native interop uses
backend-specific extension modules. A second backend may refine syntax, but it must preserve the
linear ownership and completion proofs rather than emulate Vulkan names.

## 18. Required diagnostics and tests derived from this gate

Implementation work packages must add diagnostics for:

- completion-domain/device mismatches and nonmonotonic host completion;
- frame-slot stalls and explicit wait duration;
- dropped acquired tokens and unresolved hosted receipts;
- live resource pins and deferred-destruction backlog;
- swapchain generation retirement and legacy presentation-retirement fallback;
- normal-path queue/device idle calls, which must remain zero; and
- transition to surface-lost or device-lost state.

[Gate 6's full matrix](ACCEPTANCE_AND_QUALIFICATION.md) places these cases across compile, property,
trace, real-GPU, managed, and hosted evidence. The ownership implementation includes double
acquisition/reconfigure borrowing, cross-device target rejection, frame reuse before completion,
abort-before-submit, hosted receipt commit/discard, nonmonotonic host completion, deferred
destruction, per-image presentation semaphore reuse, zero-extent suspension,
out-of-date-after-submit receipt retention, and shutdown with live objects. A trace pass cannot
replace the required native synchronization case.

## 19. Reference and specification audit

This gate compared two independent adjacent implementations:

- `../other-rendering-libs/wgpu/wgpu-hal/src/lib.rs`, its Vulkan queue implementation, and
  `wgpu-hal/src/vulkan/swapchain/native.rs` for explicit device/queue/surface ownership, one-live-
  acquisition rules, command-buffer liveness, timeline submission, and image-indexed presentation
  synchronization; and
- Flutter Impeller's Vulkan `command_queue_vk.cc`, `command_buffer_vk.h`, `command_pool_vk.cc`, and
  command-pool tests for moving tracked resources into a fence-protected submission, releasing them
  only on completion, and recycling thread command pools after completion.

The Windows bridge additionally compared wgpu's DX12 HWND/DirectComposition ownership and Vulkan
D3D11 shared-handle import paths with Zed's D3D11 HWND presenter. The adopted invariants are exact
adapter-LUID matching, flip-model HWND presentation, `DXGI_SCALING_NONE`, releasing back-buffer
references before resize, dedicated external-memory import, and one explicit cross-API fence
payload. DirectComposition is not used because its swapchain contract requires stretch scaling.

The decisions were cross-checked against primary Khronos material:

- [Vulkan WSI specification](https://docs.vulkan.org/spec/latest/chapters/VK_KHR_surface/wsi.html)
  for presentation-engine ownership and the acquire-to-release image interval;
- [Vulkan command-buffer lifecycle](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html) for
  pending-command-buffer and command-pool reset restrictions;
- [Vulkan synchronization specification](https://docs.vulkan.org/spec/latest/chapters/synchronization.html)
  for queue submission and timeline-semaphore ordering;
- [Vulkan object lifetime rules](https://docs.vulkan.org/spec/latest/chapters/fundamentals.html) for
  resources referenced by pending commands;
- [Vulkan device and queue specification](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html)
  for device loss and device-child destruction;
- [Khronos swapchain semaphore reuse guidance](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html)
  for the distinction between render completion and presentation-wait semaphore reuse; and
- [`vkDestroySwapchainKHR`](https://docs.vulkan.org/refpages/latest/refpages/source/vkDestroySwapchainKHR.html)
  and [`vkQueueWaitIdle`](https://docs.vulkan.org/refpages/latest/refpages/source/vkQueueWaitIdle.html)
  for retirement and the limited proof supplied by queue idle.

No adjacent implementation is copied or added as a dependency.

## 20. Gate completion criteria

Gate 3 is complete when:

- every instance/device/queue/frame/target/swapchain/resource owner is named;
- owned and host-borrowed native objects have different construction and destruction rules;
- frame-slot, acquire, render-submission, and presentation lifetimes are not conflated;
- the common backend signature can express borrowed frames and targets;
- owned frame transitions and hosted receipt resolution are linear;
- deferred destruction and readback are tied to explicit completion proof;
- resize, suspend, surface loss, device loss, and shutdown have named recovery owners;
- the earlier presenter borrow conflict is removed from the Blueprint;
- reference/spec findings are recorded; and
- this document is linked from the canonical documentation index and related implementation plans.
