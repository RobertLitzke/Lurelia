// Depth-only shadow pass, one cascade per draw.

struct Cascade {
    light_view_proj: mat4x4<f32>,
};
@group(1) @binding(0) var<uniform> cascade: Cascade;

@vertex
fn vs_shadow_terrain(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let q = quads[vi / 4u];
    let qv = decode_quad4(q, vi % 4u);
    let ch = chunks[q.y >> 16u];
    // Rounded trees cast the shadow of their square voxels: a performance
    // compromise (only the faces turned from the sun need drawing, about 40%
    // fewer shadow quads than the rounded shape). The rounded surface lies
    // within a fraction of a voxel of the square one, inside the shadow bias.
    // Faceted rock casts from its real facets: it moves up to half a voxel
    // and is large, so square shadows left lit streaks down shaded walls.
    let facet = (mats[qv.mat].w & M_FACET) != 0u;
    let p = select(qv.local, qv.nominal, qv.rounded == 1u && !facet);
    return cascade.light_view_proj * vec4<f32>(ch.xyz + p * ch.w, 1.0);
}

@vertex
fn vs_shadow_entity(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let q = equads[vi / 6u];
    let qv = decode_quad(q, vi % 6u);
    let inst = einst[ii];
    // shift.w 2: drawn but casting nothing (the first-person cut of the
    // player's body; ride.rs `stand_in` casts the whole one instead).
    if (inst.shift.w > 1.5) {
        return vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    let s = inst.info.x;
    let l = qrot(inst.rot, (qv.local - inst.info.yzw) * s);
    let c = cos(inst.pos_yaw.w);
    let sn = sin(inst.pos_yaw.w);
    let world = inst.pos_yaw.xyz + vec3<f32>(l.x * c + l.z * sn, l.y, -l.x * sn + l.z * c);
    return cascade.light_view_proj * vec4<f32>(world, 1.0);
}
