#version 450

layout(push_constant) uniform QuadPushConstants {
    vec4 rect_px;
    vec4 color;
    vec4 params;
    vec4 radii;
} pc;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_local_uv;
layout(location = 0) out vec4 out_color;

bool outside_corner(vec2 pixel, vec2 center, float radius) {
    return radius > 0.0 && distance(pixel, center) > radius;
}

void main() {
    vec2 rect_size = max(pc.rect_px.zw, vec2(1.0));
    vec2 pixel = v_local_uv * rect_size;
    float limit = min(rect_size.x, rect_size.y) * 0.5;
    float tl = clamp(pc.radii.x, 0.0, limit);
    float tr = clamp(pc.radii.y, 0.0, limit);
    float br = clamp(pc.radii.z, 0.0, limit);
    float bl = clamp(pc.radii.w, 0.0, limit);

    if (pixel.x < tl && pixel.y < tl && outside_corner(pixel, vec2(tl, tl), tl)) {
        discard;
    }
    if (pixel.x > rect_size.x - tr && pixel.y < tr && outside_corner(pixel, vec2(rect_size.x - tr, tr), tr)) {
        discard;
    }
    if (pixel.x > rect_size.x - br && pixel.y > rect_size.y - br && outside_corner(pixel, vec2(rect_size.x - br, rect_size.y - br), br)) {
        discard;
    }
    if (pixel.x < bl && pixel.y > rect_size.y - bl && outside_corner(pixel, vec2(bl, rect_size.y - bl), bl)) {
        discard;
    }

    out_color = v_color;
}
