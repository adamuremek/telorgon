#version 450

layout(push_constant) uniform QuadPushConstants {
    vec4 rect_px;
    vec4 color;
    vec4 params;
    vec4 radii;
} pc;

layout(location = 0) out vec4 v_color;
layout(location = 1) out vec2 v_local_uv;

void main() {
    vec2 local_positions[6] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0)
    );

    vec2 local = local_positions[gl_VertexIndex];
    vec2 pixel = pc.rect_px.xy + local * pc.rect_px.zw;
    vec2 extent = max(pc.params.xy, vec2(1.0));
    vec2 ndc = vec2((pixel.x / extent.x) * 2.0 - 1.0, 1.0 - (pixel.y / extent.y) * 2.0);

    gl_Position = vec4(ndc, 0.0, 1.0);
    v_color = pc.color;
    v_local_uv = local;
}
