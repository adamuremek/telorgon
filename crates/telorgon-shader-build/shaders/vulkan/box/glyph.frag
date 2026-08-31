#version 450
struct GpuClip { vec4 view_bounds; vec4 local_rect; vec4 local_from_view_0; vec4 local_from_view_1; vec4 radii; vec4 mask_uv_from_view_0; vec4 mask_uv_from_view_1; uvec4 mode_mask_flags; };
struct GpuGlyphInstance { vec4 rect; vec4 uv_texels; uvec4 color_spatial_clip_page; float opacity; uint flags; uvec2 reserved; };
layout(set=1,binding=1,std430) readonly buffer ClipBlock { GpuClip values[]; } clips;
layout(set=2,binding=0,std430) readonly buffer GlyphBlock { GpuGlyphInstance values[]; } glyphs;
layout(set=3,binding=0) uniform sampler2D atlas_texture;
layout(location=0) noperspective in vec2 uv_texels;layout(location=1) noperspective in vec2 view_position;layout(location=2) flat in uint instance_slot;layout(location=0) out vec4 output_color;
vec4 unpack_srgba(uint p){return vec4(float(p&255u),float((p>>8u)&255u),float((p>>16u)&255u),float((p>>24u)&255u))/255.0;}
vec3 srgb_decode(vec3 v){bvec3 low=lessThanEqual(v,vec3(.04045));return mix(pow((v+.055)/1.055,vec3(2.4)),v/12.92,low);}
bool rounded(vec2 p,vec2 size,vec4 r){float radius=p.x<size.x*.5?(p.y<size.y*.5?r.x:r.w):(p.y<size.y*.5?r.y:r.z);vec2 q=min(p,size-p);if(q.x>=radius||q.y>=radius)return true;vec2 d=q-vec2(radius);return dot(d,d)<=radius*radius;}
bool clipped(uint slot,vec2 p){if(slot==0xffffffffu)return false;GpuClip c=clips.values[slot];vec2 local=p-c.view_bounds.xy;if(any(lessThan(local,vec2(0)))||any(greaterThan(local,c.view_bounds.zw)))return true;return c.mode_mask_flags.x==2u&&!rounded(local,c.view_bounds.zw,c.radii);}
void main(){GpuGlyphInstance item=glyphs.values[instance_slot];if(clipped(item.color_spatial_clip_page.z,view_position))discard;float coverage=texture(atlas_texture,uv_texels/vec2(textureSize(atlas_texture,0))).r;vec4 color=unpack_srgba(item.color_spatial_clip_page.x);float a=color.a*coverage*clamp(item.opacity,0,1);output_color=vec4(srgb_decode(color.rgb)*a,a);}
