use bytemuck::{Pod, Zeroable};

#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuView {
    pub clip_from_view_0: [f32; 4],
    pub clip_from_view_1: [f32; 4],
    pub clip_from_view_2: [f32; 4],
    pub clip_from_view_3: [f32; 4],
    pub view_size_scale: [f32; 4],
    pub target_size_origin: [f32; 4],
    pub render_size_inverse: [f32; 4],
    pub epoch_flags: [u32; 4],
}
