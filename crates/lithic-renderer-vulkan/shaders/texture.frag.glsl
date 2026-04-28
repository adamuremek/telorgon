#version 450

layout(set = 0, binding = 0) uniform sampler2D surface_texture;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 sample_color = texture(surface_texture, v_uv);
    float alpha = sample_color.a * v_color.a;
    out_color = vec4(sample_color.rgb * alpha, alpha);
}
