// ---- prints on the ground (creatures/sign.rs, render/signs.rs) --------------
// Built on common.wgsl alone. One instance per print, six vertices each, a
// flat quad lying on the ground along the heading. The fragment draws the
// foot's shape procedurally and returns a multiplier on what the frame
// already holds (the pipeline blends by multiplying), so a print takes the
// ground's own colour and light. Bright snow is past white before the grade,
// so its dent has to be deep to show at all: a dent in snow goes blue-grey, one in mud
// dark, one in sand a soft shade with a raised rim.

struct GpuSign {
    a: vec4<f32>, // xyz on the drawn ground, w = length (m)
    b: vec4<f32>, // heading, wear 0..1, foot, bed
};
@group(2) @binding(0) var<storage, read> signs: array<GpuSign>;

const F_HOOF: u32 = 0u;
const F_PAW: u32 = 1u;
const F_BEAR: u32 = 2u;
const F_BIRD: u32 = 3u;
const F_BOOT: u32 = 4u;

const SIGN_FADE_NEAR: f32 = 30.0;
const SIGN_FADE_FAR: f32 = 45.0;

struct SignOut {
    @builtin(position) clip: vec4<f32>,
    // Where in the print, in print lengths: y heel (-0.5) to toe (0.5).
    @location(0) q: vec2<f32>,
    @location(1) world: vec3<f32>,
    // x = foot, y = bed
    @location(2) @interpolate(flat) kind: vec2<u32>,
    @location(3) @interpolate(flat) wear: f32,
};

@vertex
fn vs_sign(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> SignOut {
    let s = signs[ii];
    let c = quad_corner(vi) * 0.75;
    let yaw = s.b.x;
    // Forward is (-sin, -cos), as the creatures face; across is its right.
    let fwd = vec3<f32>(-sin(yaw), 0.0, -cos(yaw));
    let across = vec3<f32>(-fwd.z, 0.0, fwd.x);
    var world = s.a.xyz + (across * c.x + fwd * c.y) * s.a.w + vec3<f32>(0.0, 0.02, 0.0);
    // Pulled a little toward the eye, so a flat print laid on a slope of
    // sloped voxels is not half buried in it.
    let to_eye = g.cam_pos.xyz - world;
    let pulled = world + normalize(to_eye) * min(0.12, length(to_eye) * 0.05);
    var out: SignOut;
    out.clip = g.view_proj * vec4<f32>(pulled, 1.0);
    out.q = c;
    out.world = world;
    out.kind = vec2<u32>(u32(s.b.z + 0.5), u32(s.b.w + 0.5));
    out.wear = s.b.y;
    // Too far to see: off the screen.
    if (length(to_eye) > SIGN_FADE_FAR + 2.0) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    return out;
}

// 1 inside the ellipse at `c` with radii `r`, falling to 0 over `soft`.
fn blob(q: vec2<f32>, c: vec2<f32>, r: vec2<f32>, soft: f32) -> f32 {
    let e = length((q - c) / r);
    return 1.0 - smoothstep(1.0 - soft, 1.0 + soft, e);
}

// 1 along the segment a-b, `w` wide.
fn line(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>, w: f32, soft: f32) -> f32 {
    let ab = b - a;
    let t = clamp(dot(q - a, ab) / dot(ab, ab), 0.0, 1.0);
    let d = length(q - a - ab * t) / w;
    return 1.0 - smoothstep(1.0 - soft, 1.0 + soft, d);
}

// The foot's shape, 0 outside to 1 pressed in. `grow` swells it a little,
// for the rim pushed up round it.
fn foot_shape(q: vec2<f32>, foot: u32, soft: f32, grow: f32) -> f32 {
    let k = 1.0 + grow;
    if (foot == F_HOOF) {
        // Two slots, a little apart and closing toward the tip.
        let l = blob(q, vec2<f32>(-0.12, 0.0), vec2<f32>(0.1, 0.44) * k, soft);
        let r = blob(q, vec2<f32>(0.12, 0.0), vec2<f32>(0.1, 0.44) * k, soft);
        return max(l, r);
    }
    if (foot == F_PAW) {
        var m = blob(q, vec2<f32>(0.0, -0.2), vec2<f32>(0.24, 0.2) * k, soft);
        m = max(m, blob(q, vec2<f32>(-0.24, 0.12), vec2<f32>(0.09) * k, soft));
        m = max(m, blob(q, vec2<f32>(-0.09, 0.3), vec2<f32>(0.09) * k, soft));
        m = max(m, blob(q, vec2<f32>(0.09, 0.3), vec2<f32>(0.09) * k, soft));
        m = max(m, blob(q, vec2<f32>(0.24, 0.12), vec2<f32>(0.09) * k, soft));
        return m;
    }
    if (foot == F_BEAR) {
        var m = blob(q, vec2<f32>(0.0, -0.1), vec2<f32>(0.36, 0.3) * k, soft);
        for (var i = 0; i < 5; i++) {
            let x = (f32(i) - 2.0) * 0.16;
            m = max(m, blob(q, vec2<f32>(x, 0.32 - abs(x) * 0.25), vec2<f32>(0.075) * k, soft));
        }
        return m;
    }
    if (foot == F_BIRD) {
        let heel = vec2<f32>(0.0, -0.22);
        let w = 0.045 * k;
        var m = line(q, heel, vec2<f32>(-0.33, 0.3), w, soft);
        m = max(m, line(q, heel, vec2<f32>(0.0, 0.42), w, soft));
        m = max(m, line(q, heel, vec2<f32>(0.33, 0.3), w, soft));
        m = max(m, line(q, heel, vec2<f32>(0.0, -0.44), w, soft));
        return m;
    }
    // The boot: heel and sole, with lugs across the sole.
    var m = max(blob(q, vec2<f32>(0.0, -0.3), vec2<f32>(0.15, 0.17) * k, soft), blob(q, vec2<f32>(0.0, 0.12), vec2<f32>(0.19, 0.33) * k, soft));
    if (grow == 0.0) {
        // The lugs of the sole, across it.
        m *= mix(0.55, 1.0, step(0.45, fract(q.y * 13.0)));
    }
    return m;
}

@fragment
fn fs_sign(in: SignOut) -> @location(0) vec4<f32> {
    let foot = in.kind.x;
    let bed = in.kind.y;
    let wear = clamp(in.wear, 0.0, 1.0);
    // Wear blurs the edges and takes the depth out of it.
    let soft = 0.08 + 0.55 * wear;
    let m = foot_shape(in.q, foot, soft, 0.0);
    let rim = clamp(foot_shape(in.q, foot, soft, 0.28) - m, 0.0, 1.0);
    var dent = vec3<f32>(0.5, 0.46, 0.42);
    var lip = 1.0;
    if (bed == 0u) {
        // Snow: the inside of a print is in its own blue shadow, and the
        // snow it pushed aside stands a little proud of the rest.
        dent = vec3<f32>(0.36, 0.44, 0.66);
        lip = 1.1;
    } else if (bed == 1u) {
        // Mud: dark and wet at the bottom.
        dent = vec3<f32>(0.36, 0.32, 0.29);
        lip = 1.04;
    } else if (bed == 2u) {
        dent = vec3<f32>(0.58, 0.53, 0.48);
        lip = 1.08;
    }
    let fresh = pow(1.0 - wear, 0.7);
    // Deepest in the middle of each pad and toe, shallower toward the edge,
    // so it reads as a dent rather than a stamp.
    let core = foot_shape(in.q, foot, soft, -0.35);
    let floor = mix(min(dent * 1.3, vec3<f32>(1.0)), dent, core);
    var mult = mix(vec3<f32>(1.0), floor, m * fresh) * mix(1.0, lip, rim * fresh);
    // Fade with distance and into the fog, so a far print is not a dark dot
    // laid over the haze.
    let dist = length(in.world - g.cam_pos.xyz);
    let fade = (1.0 - smoothstep(SIGN_FADE_NEAR, SIGN_FADE_FAR, dist)) * fog_terms(in.world).a;
    mult = mix(vec3<f32>(1.0), mult, fade);
    return vec4<f32>(mult, 1.0);
}
