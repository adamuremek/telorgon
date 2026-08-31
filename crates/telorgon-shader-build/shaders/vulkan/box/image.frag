#version 450
struct GpuClip { vec4 view_bounds; vec4 local_rect; vec4 local_from_view_0; vec4 local_from_view_1; vec4 radii; vec4 mask_uv_from_view_0; vec4 mask_uv_from_view_1; uvec4 mode_mask_flags; };
struct GpuImageInstance { vec4 rect; vec4 uv_normalized; uvec4 tint_spatial_clip_texture; float opacity; uint sampler_key; uint flags; uint reserved; };
layout(set=1,binding=1,std430) readonly buffer ClipBlock { GpuClip values[]; } clips;
layout(set=2,binding=0,std430) readonly buffer ImageBlock { GpuImageInstance values[]; } images;
layout(set=3,binding=0) uniform sampler2D source_texture;
layout(location=0) noperspective in vec2 uv;layout(location=1) noperspective in vec2 view_position;layout(location=2) flat in uint instance_slot;layout(location=0) out vec4 output_color;
vec4 unpack_srgba(uint p){return vec4(float(p&255u),float((p>>8u)&255u),float((p>>16u)&255u),float((p>>24u)&255u))/255.0;}
vec3 srgb_decode(vec3 v){bvec3 low=lessThanEqual(v,vec3(.04045));return mix(pow((v+.055)/1.055,vec3(2.4)),v/12.92,low);}
bool rounded(vec2 p,vec2 size,vec4 r){float radius=p.x<size.x*.5?(p.y<size.y*.5?r.x:r.w):(p.y<size.y*.5?r.y:r.z);vec2 q=min(p,size-p);if(q.x>=radius||q.y>=radius)return true;vec2 d=q-vec2(radius);return dot(d,d)<=radius*radius;}
bool clipped(uint slot,vec2 p){if(slot==0xffffffffu)return false;GpuClip c=clips.values[slot];vec2 local=p-c.view_bounds.xy;if(any(lessThan(local,vec2(0)))||any(greaterThan(local,c.view_bounds.zw)))return true;return c.mode_mask_flags.x==2u&&!rounded(local,c.view_bounds.zw,c.radii);}
void main(){GpuImageInstance item=images.values[instance_slot];if(clipped(item.tint_spatial_clip_texture.z,view_position))discard;vec4 sampled=texture(source_texture,uv);if(item.flags==2u)sampled.a=1.0;vec4 tint=unpack_srgba(item.tint_spatial_clip_texture.x);vec3 tint_linear=srgb_decode(tint.rgb);float opacity=clamp(item.opacity,0,1);float a=sampled.a*tint.a*opacity;vec3 rgb=item.flags==1u?sampled.rgb*tint_linear*tint.a*opacity:sampled.rgb*tint_linear*a;output_color=vec4(rgb,a);}
