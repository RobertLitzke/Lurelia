// ---- particles (src/fx, render/particles.rs) --------------------------------
// Built on scene.wgsl for its lighting and fog. One instance per particle,
// six vertices each. Solid shapes (chips, leaves) are drawn first, writing
// depth, and lit per pixel like a sign board; soft ones (puffs, streaks) are
// drawn after the water, blended far to near, and lit once per particle so
// a big puff costs one shadow tap rather than one per pixel it covers.

struct Particle {
    pos: vec4<f32>,    // xyz, w = radius (m)
    vel: vec4<f32>,    // xyz m/s, w = spin (radians)
    color: vec4<f32>,  // linear rgb (a multiply on the material's colour where it has one), a = opacity
    info: vec4<u32>,   // x = shape, y = material (0 none), z = seed, w = 1 gives its own light
};
@group(2) @binding(0) var<storage, read> parts: array<Particle>;

const P_PUFF: u32 = 0u;
const P_CHIP: u32 = 1u;
const P_STREAK: u32 = 2u;
const P_LEAF: u32 = 3u;

struct PartOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) normal: vec3<f32>,
    // x = shape, y = 1 for its own light
    @location(4) @interpolate(flat) kind: vec2<u32>,
    // The light on a soft particle, worked out once here.
    @location(5) @interpolate(flat) lit: vec3<f32>,
};

@vertex
fn vs_particle(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> PartOut {
    let corner = quad_corner(vi);
    let p = parts[ii];
    let shape = p.info.x;
    let pos = p.pos.xyz;
    var size = p.pos.w;
    let to_cam_v = g.cam_pos.xyz - pos;
    let dist = max(length(to_cam_v), 1e-3);
    let f = to_cam_v / dist;
    let fb = billboard(f);
    var right = fb.right;
    var up = fb.up;
    var normal = f;
    let spin = p.vel.w;
    if (shape == P_CHIP) {
        // A square turning in the view's plane, never under about a pixel.
        let c = cos(spin);
        let s = sin(spin);
        let r = right * c + up * s;
        up = up * c - right * s;
        right = r;
        size = max(size, dist * 0.0010);
        // Lit as if facing partly up, so grit on the ground isn't black.
        normal = normalize(f + vec3<f32>(0.0, 0.8, 0.0));
    } else if (shape == P_LEAF) {
        // Tumbling: its face swings about an axis fixed by its seed.
        let h = f32(p.info.z & 1023u) / 1023.0 * 6.2831853;
        let axis = vec3<f32>(cos(h), 0.0, sin(h));
        let side = vec3<f32>(-axis.z, 0.0, axis.x);
        let n = normalize(vec3<f32>(0.0, 1.0, 0.0) * cos(spin) + side * sin(spin));
        right = axis;
        up = normalize(cross(n, axis));
        normal = select(-n, n, dot(n, f) > 0.0);
        size = max(size, dist * 0.0010);
    } else if (shape == P_STREAK) {
        // Stretched along its flight as it shows on screen.
        let v = p.vel.xyz;
        let speed = length(v);
        let along = select(up, v / max(speed, 1e-4), speed > 0.05);
        var w = cross(along, f);
        if (dot(w, w) < 1e-6) {
            w = right;
        }
        right = normalize(w) * max(size, dist * 0.0012);
        up = along * (size + speed * 0.022);
        size = 1.0;
    } else {
        // A puff never grows past half its distance, so one at your feet
        // can't fill the screen.
        size = min(size, dist * 0.5);
    }
    // The material's own colour, from the smallest mip of its side.
    var rgb = p.color.rgb;
    if (p.info.y != 0u) {
        rgb *= textureSampleLevel(tex, samp, vec2<f32>(0.5), i32(mats[p.info.y].y), 16.0).rgb;
    }
    var out: PartOut;
    out.corner = corner;
    out.color = vec4<f32>(rgb, p.color.a);
    out.normal = normal;
    out.kind = vec2<u32>(shape, p.info.w);
    let world = pos + (right * corner.x + up * corner.y) * size;
    out.world = world;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    // Soft particles: sun and sky at the particle's middle, once.
    out.lit = vec3<f32>(1.0);
    if (shape == P_PUFF || shape == P_STREAK) {
        let sh = shadow_factor(pos, vec3<f32>(0.0, 1.0, 0.0));
        // About what a lit surface gets: a puff is matter, not a light.
        out.lit = g.sun_color.rgb * g.sun_color.w * (0.2 + 0.6 * sh) * 0.5 + g.sky_ambient.rgb * 0.9;
    }
    return out;
}

// Chips and leaves: solid, cut to their outline, writing depth.
@fragment
fn fs_particle_solid(in: PartOut) -> @location(0) vec4<f32> {
    let c = in.corner;
    if (in.kind.x == P_LEAF) {
        // An oval drawn to a point at each end.
        let w = 0.55 * (1.0 - c.y * c.y * 0.6);
        if (abs(c.x) > w || abs(c.y) > 0.98) {
            discard;
        }
    } else if (max(abs(c.x), abs(c.y)) > 0.85) {
        discard;
    }
    // Mottled a little, so a pile of grit isn't one flat colour.
    let n = 0.88 + 0.24 * f32((hash3u(vec3<i32>(i32(c.x * 3.0), i32(c.y * 3.0), i32(in.kind.x))) & 255u)) / 255.0;
    let col = light(in.color.rgb * n, in.world, normalize(in.normal), 2.6, 0.0);
    return vec4<f32>(apply_fog(col, in.world), 1.0);
}

// Puffs and streaks: blended, soft to the edge.
@fragment
fn fs_particle_soft(in: PartOut) -> @location(0) vec4<f32> {
    let d = dot(in.corner, in.corner);
    if (d > 1.0) {
        discard;
    }
    var a: f32;
    if (in.kind.x == P_STREAK) {
        a = in.color.a * (1.0 - abs(in.corner.x)) * (1.0 - in.corner.y * in.corner.y);
    } else {
        a = in.color.a * (exp(-d * 3.0) - 0.05) / 0.95;
    }
    var col = in.color.rgb * in.lit;
    if (in.kind.y == 1u) {
        // Its own light: sparks are as bright as their colour says.
        col = in.color.rgb;
    }
    return vec4<f32>(apply_fog(col, in.world), max(a, 0.0));
}
