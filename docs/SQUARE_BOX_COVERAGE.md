# Square box corner coverage

The Vulkan box shader now integrates the local rectangular pixel footprint for axis-aligned
boxes whose radii are all zero. Fully covered corner pixels remain opaque; fractional boundaries
retain fractional coverage. Rounded and rotated boxes retain the existing signed-distance path.
Derivatives are evaluated before the selection, and fill/border partitioning is unchanged.

The old `fwidth(rounded_distance(...))` estimate crosses different edges within a fragment quad
at a square corner. That estimate can widen the smoothing band and fade an otherwise fully
covered corner pixel. This is a coverage issue, not extra layout spacing or an implicit radius.

## Reference audit

The adjacent `../other-rendering-libs` checkout is unavailable. This bounded coverage correction
uses the upstream references and official specification below, as permitted by
[the reference guide](REFERENCE_IMPLEMENTATIONS.md):

- [Dear ImGui Vulkan fragment shader](https://github.com/ocornut/imgui/blob/master/backends/vulkan/glsl_shader.frag):
  ordinary geometry preserves the supplied color/texture alpha without an additional corner-distance fade.
- [egui WGPU shader](https://github.com/emilk/egui/blob/master/crates/egui-wgpu/src/egui.wgsl):
  fragment output preserves geometry/texture alpha; framebuffer color conversion does not invent corner coverage.
- [Khronos GLSL fwidth reference](https://github.com/KhronosGroup/OpenGL-Refpages/blob/main/gl4/fwidth.xml):
  `fwidth` sums absolute locally differenced derivatives. A nonlinear minimum/maximum distance
  around a corner is not the same as the footprint of the affine local coordinates.

Invariants: opaque, fully covered square pixels stay opaque; no UI geometry is enlarged;
rounded and transformed edges keep smoothing; fractional square edges keep coverage; derivatives
remain outside fragment-varying branches. No source was copied from these references.

Rejected alternatives: overlap neighboring controls; globally disable antialiasing; force a
nonzero radius; snap layout coordinates; change all rounded-distance smoothing. These conceal
or broaden the issue and can change intentional geometry or rounded rendering.

## Verification

The software regression draws three abutting controls and checks every pixel, including shared
corners, at integer scales 1, 2, and 3 and both pixel-origin parities. The existing ignored Vulkan
readback regression now uses truly abutting controls (its former 38-unit widths had a 40-unit
stride), and includes 300% alongside integer/fractional scales and square/rounded parent clips.
The Vulkan regression must be compiled only in automated work; running it requires the repository's
explicit developer hardware workflow. Shader generation validates SPIR-V and reflects the ABI.
Manual compositor verification remains necessary for the observed display artifact.
