#version 450
layout(set=0,binding=0,std140) uniform ViewBlock { vec4 clip_from_view_0; vec4 clip_from_view_1; vec4 clip_from_view_2; vec4 clip_from_view_3; vec4 view_size_scale; vec4 target_size_origin; vec4 render_size_inverse; uvec4 epoch_flags; vec4 placement_clip_rects[2]; vec4 placement_clip_radii[2]; } view_data;
float placement_coverage(){
    float amount=1.0;
    for(int i=0;i<2;i++){
        vec4 rect=view_data.placement_clip_rects[i];
        if(rect.z<0.0)continue;
        bool inverted=(view_data.epoch_flags.w&(1u<<uint(i+1)))!=0u;
        if(rect.z<=0.0||rect.w<=0.0){if(inverted)continue;return 0.0;}
        vec2 p=gl_FragCoord.xy-rect.xy;
        vec2 half_size=rect.zw*.5;
        vec4 radii=view_data.placement_clip_radii[i];
        float radius=p.x<half_size.x?(p.y<half_size.y?radii.x:radii.w):(p.y<half_size.y?radii.y:radii.z);
        radius=clamp(radius,0.0,min(half_size.x,half_size.y));
        vec2 q=abs(p-half_size)-(half_size-vec2(radius));
        float d=length(max(q,vec2(0)))+min(max(q.x,q.y),0.0)-radius;
        float coverage=clamp(.5-d,0.0,1.0);
        amount=min(amount,inverted?1.0-coverage:coverage);
    }
    return amount;
}


struct GpuClip { vec4 view_bounds; vec4 local_rect; vec4 local_from_view_0; vec4 local_from_view_1; vec4 radii; vec4 mask_uv_from_view_0; vec4 mask_uv_from_view_1; uvec4 mode_mask_flags; };
struct GpuBoxInstance {
    vec4 rect; vec4 radii; vec4 border_widths;
    uvec4 fill_border_t_r_b; uvec4 border_l_spatial_clip_flags;
    float opacity; uint reserved_0; uint reserved_1; uint reserved_2;
    vec4 outline; vec4 shadow_0; vec4 shadow_1; uvec4 outline_shadow_colors;
};
layout(set=1,binding=1,std430) readonly buffer ClipBlock { GpuClip values[]; } clips;
layout(set=2,binding=0,std430) readonly buffer BoxBlock { GpuBoxInstance values[]; } boxes;
layout(location=0) noperspective in vec2 local_position;
layout(location=1) noperspective in vec2 view_position;
layout(location=2) flat in uint instance_slot;
layout(location=0) out vec4 output_color;

vec4 unpack_srgba(uint p){return vec4(float(p&255u),float((p>>8u)&255u),float((p>>16u)&255u),float((p>>24u)&255u))/255.0;}
vec3 srgb_decode(vec3 v){bvec3 low=lessThanEqual(v,vec3(.04045));return mix(pow((v+.055)/1.055,vec3(2.4)),v/12.92,low);}
bool rounded(vec2 p,vec2 size,vec4 r){float radius=p.x<size.x*.5?(p.y<size.y*.5?r.x:r.w):(p.y<size.y*.5?r.y:r.z);vec2 q=min(p,size-p);if(q.x>=radius||q.y>=radius)return true;vec2 d=q-vec2(radius);return dot(d,d)<=radius*radius;}
bool clipped(uint slot,vec2 p){if(slot==0xffffffffu)return false;GpuClip c=clips.values[slot];vec2 local=p-c.view_bounds.xy;if(any(lessThan(local,vec2(0)))||any(greaterThan(local,c.view_bounds.zw)))return true;return c.mode_mask_flags.x==2u&&!rounded(local,c.view_bounds.zw,c.radii);}

float rounded_distance(vec2 p,vec2 size,vec4 radii){
    if(any(lessThanEqual(size,vec2(0))))return 1e6;
    float radius=p.x<size.x*.5?(p.y<size.y*.5?radii.x:radii.w):(p.y<size.y*.5?radii.y:radii.z);
    radius=clamp(radius,0.0,min(size.x,size.y)*.5);
    vec2 half_size=size*.5;
    vec2 q=abs(p-half_size)-(half_size-vec2(radius));
    return length(max(q,vec2(0)))+min(max(q.x,q.y),0.0)-radius;
}
float coverage(vec2 p,vec2 size,vec4 radii){float d=rounded_distance(p,size,radii);return clamp(.5-d/max(fwidth(d),1e-4),0.0,1.0);}
vec4 premul(uint packed,float amount,float opacity){vec4 c=unpack_srgba(packed);float a=c.a*amount*clamp(opacity,0.0,1.0);return vec4(srgb_decode(c.rgb)*a,a);}
vec4 over(vec4 destination,vec4 source){return source+destination*(1.0-source.a);}

float shadow_coverage(vec2 p,vec2 size,vec4 radii,vec4 shadow){
    float spread=shadow.w;
    vec2 shadow_p=p-shadow.xy+vec2(spread);
    vec2 shadow_size=size+vec2(spread*2.0);
    float distance=rounded_distance(shadow_p,shadow_size,max(radii+vec4(spread),vec4(0)));
    float blur=max(0.0,shadow.z);
    float aa=max(fwidth(distance),1e-4);
    return blur<=1e-6?clamp(.5-distance/aa,0.0,1.0):clamp(.5-distance/(blur*2.0+aa),0.0,1.0);
}

uint border_color(GpuBoxInstance item,vec2 p){
    vec4 widths=item.border_widths;
    vec4 distance=vec4(p.y,item.rect.z-p.x,item.rect.w-p.y,p.x);
    vec4 ratio=vec4(
        widths.x>0.0?distance.x/widths.x:1e20,
        widths.y>0.0?distance.y/widths.y:1e20,
        widths.z>0.0?distance.z/widths.z:1e20,
        widths.w>0.0?distance.w/widths.w:1e20);
    if(ratio.x<=min(min(ratio.y,ratio.z),ratio.w))return item.fill_border_t_r_b.y;
    if(ratio.y<=min(ratio.z,ratio.w))return item.fill_border_t_r_b.z;
    if(ratio.z<=ratio.w)return item.fill_border_t_r_b.w;
    return item.border_l_spatial_clip_flags.x;
}

void main(){float placement_amount=placement_coverage();if(placement_amount<=0.0)discard;
    GpuBoxInstance item=boxes.values[instance_slot];
    if(clipped(item.border_l_spatial_clip_flags.z,view_position))discard;
    vec2 size=item.rect.zw;
    vec2 p=local_position;
    vec4 result=vec4(0);
    uint shadow_count=item.outline_shadow_colors.w;
    if(shadow_count>1u)result=over(result,premul(item.outline_shadow_colors.z,shadow_coverage(p,size,item.radii,item.shadow_1),item.opacity));
    if(shadow_count>0u)result=over(result,premul(item.outline_shadow_colors.y,shadow_coverage(p,size,item.radii,item.shadow_0),item.opacity));

    float outline_width=max(0.0,item.outline.x);
    if(outline_width>0.0){
        float offset=item.outline.y;
        float outer_amount=offset+outline_width;
        float outer=coverage(p+vec2(outer_amount),size+vec2(outer_amount*2.0),max(item.radii+vec4(outer_amount),vec4(0)));
        float inner=coverage(p+vec2(offset),size+vec2(offset*2.0),max(item.radii+vec4(offset),vec4(0)));
        result=over(result,premul(item.outline_shadow_colors.x,clamp(outer-inner,0.0,1.0),item.opacity));
    }

    float outer=coverage(p,size,item.radii);
    vec4 widths=max(item.border_widths,vec4(0));
    vec2 inner_origin=vec2(widths.w,widths.x);
    vec2 inner_size=max(vec2(0),size-vec2(widths.w+widths.y,widths.x+widths.z));
    vec4 inner_radii=max(vec4(0),item.radii-vec4(max(widths.x,widths.w),max(widths.x,widths.y),max(widths.z,widths.y),max(widths.z,widths.w)));
    float inner=min(outer,coverage(p-inner_origin,inner_size,inner_radii));
    uint flags=item.border_l_spatial_clip_flags.w;
    if((flags&1u)!=0u)result=over(result,premul(item.fill_border_t_r_b.x,inner,item.opacity));
    float ring=clamp(outer-inner,0.0,1.0);
    if((flags&2u)!=0u&&ring>0.0)result=over(result,premul(border_color(item,p),ring,item.opacity));
    if(result.a<=0.0)discard;
    output_color=(result)*placement_amount;
}
