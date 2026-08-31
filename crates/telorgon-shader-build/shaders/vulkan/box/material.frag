#version 450
struct GpuClip { vec4 view_bounds; vec4 local_rect; vec4 local_from_view_0; vec4 local_from_view_1; vec4 radii; vec4 mask_uv_from_view_0; vec4 mask_uv_from_view_1; uvec4 mode_mask_flags; };
struct GpuMaterialInstance { vec4 rect; uvec4 params_spatial_clip; float opacity; uint material_variant; uint flags; uint reserved; uvec4 resource_range_reserved; };
layout(set=1,binding=1,std430) readonly buffer ClipBlock { GpuClip values[]; } clips;
layout(set=2,binding=0,std430) readonly buffer MaterialBlock { GpuMaterialInstance values[]; } materials;
layout(set=2,binding=1,std430) readonly buffer ParameterBlock { uint values[]; } parameters;
layout(location=0) noperspective in vec2 unit_position;layout(location=1) noperspective in vec2 view_position;layout(location=2) flat in uint instance_slot;layout(location=0) out vec4 output_color;
vec4 unpack_srgba(uint p){return vec4(float(p&255u),float((p>>8u)&255u),float((p>>16u)&255u),float((p>>24u)&255u))/255.0;}
vec3 srgb_decode(vec3 v){bvec3 low=lessThanEqual(v,vec3(.04045));return mix(pow((v+.055)/1.055,vec3(2.4)),v/12.92,low);}
bool rounded(vec2 p,vec2 size,vec4 r){float radius=p.x<size.x*.5?(p.y<size.y*.5?r.x:r.w):(p.y<size.y*.5?r.y:r.z);vec2 q=min(p,size-p);if(q.x>=radius||q.y>=radius)return true;vec2 d=q-vec2(radius);return dot(d,d)<=radius*radius;}
bool clipped(uint slot,vec2 p){if(slot==0xffffffffu)return false;GpuClip c=clips.values[slot];vec2 local=p-c.view_bounds.xy;if(any(lessThan(local,vec2(0)))||any(greaterThan(local,c.view_bounds.zw)))return true;return c.mode_mask_flags.x==2u&&!rounded(local,c.view_bounds.zw,c.radii);}
void main(){GpuMaterialInstance item=materials.values[instance_slot];if(clipped(item.params_spatial_clip.w,view_position))discard;float t=item.material_variant==1u?unit_position.x:(item.material_variant==2u?unit_position.y:0.0);vec4 first=unpack_srgba(parameters.values[item.params_spatial_clip.x]);vec4 second=unpack_srgba(parameters.values[item.params_spatial_clip.x+1u]);vec4 color=mix(first,second,t);float a=color.a*clamp(item.opacity,0,1);output_color=vec4(srgb_decode(color.rgb)*a,a);}
