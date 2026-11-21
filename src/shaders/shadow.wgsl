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
    // Tubes (mesh/lean.rs) cast from their real faces too: their square
    // faces are only the anchors they hang from, a staircase of squares
    // beside the stem, not its shape.
    let facet = (mats[qv.mat].w & M_FACET) != 0u;
    let tube = ((q.x >> Q_TUBE_BIT) & 1u) == 1u;
    let p = select(qv.local, qv.nominal, qv.rounded == 1u && !facet && !tube);
    return cascade.light_view_proj * vec4<f32>(ch.xyz + p * ch.w, 1.0);
}

struct ShadowEntityOut {
    @builtin(position) pos: vec4<f32>,
    // How much of the shadow is cast, 0 to 1.
    @location(0) fade: f32,
};

@vertex
fn vs_shadow_entity(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> ShadowEntityOut {
    var out: ShadowEntityOut;
    let q = equads[vi / 6u];
    let qv = decode_quad(q, vi % 6u);
    let inst = einst[ii];
    // shift.w 2: drawn but casting nothing (the first-person cut of the
    // player's body; ride.rs `stand_in` casts the whole one instead).
    // Below 0: cast thinned, -w of it (entities.rs `shadow_fade`).
    out.fade = select(1.0, -inst.shift.w, inst.shift.w < 0.0);
    if (inst.shift.w > 1.5 || out.fade <= 0.0) {
        out.pos = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }
    let s = inst.info.x;
    let l = qrot(inst.rot, (qv.local - inst.info.yzw) * s);
    let c = cos(inst.pos_yaw.w);
    let sn = sin(inst.pos_yaw.w);
    let world = inst.pos_yaw.xyz + vec3<f32>(l.x * c + l.z * sn, l.y, -l.x * sn + l.z * c);
    out.pos = cascade.light_view_proj * vec4<f32>(world, 1.0);
    return out;
}

// A thinned shadow: an ordered dither over the shadow map's texels, which the
// filtering where the shadow is read smooths into a lighter shade.
@fragment
fn fs_shadow_entity(in: ShadowEntityOut) {
    if (in.fade >= 1.0) {
        return;
    }
    let p = vec2<u32>(in.pos.xy) % vec2<u32>(4u, 4u);
    let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    if ((bayer[p.y * 4u + p.x] + 0.5) / 16.0 >= in.fade) {
        discard;
    }
}
