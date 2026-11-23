// Shared by every scene shader. Prepended to each module at load time.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    shadow_mats: array<mat4x4<f32>, 5>, // cascade 0, then older/newer of cascades 1 and 2
    cam_pos: vec4<f32>,      // xyz, time in seconds
    sun_dir: vec4<f32>,      // xyz toward the sun, w = shadows on
    sun_color: vec4<f32>,    // rgb, intensity
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    ground_color: vec4<f32>,
    fog: vec4<f32>,          // density, sea level, camera underwater, world size
    splits: vec4<f32>,       // far edge of each shadow cascade, metres of view depth
    screen: vec4<f32>,       // width, height, 1/width, 1/height
    cam_fwd: vec4<f32>,
    shadow_texel: vec4<f32>, // world size of one shadow texel, per cascade; w = filter spread (1 = as built)
    box_min: vec4<f32>,      // highlighted voxel box, w = visible
    box_max: vec4<f32>,
    style: vec4<f32>,        // x = 1 for the calm style
    sky_ambient: vec4<f32>,  // sky fill colour for hemi_ambient
    glow: vec4<f32>,         // light the air scatters near the sun; w = how much
    air_sun: vec4<f32>,      // toward the real sun (the key light may be the moon); w = night
    mist: vec4<f32>,         // biome mist scale, mist everywhere, mist height (m), 1 = map set
    shadow_layers: vec4<f32>, // layers of the older/newer maps of cascades 1 and 2
    shadow_blend: vec4<f32>,  // x, y: how far cascades 1 and 2 have blended to their newer map
    water: array<vec4<f32>, 6>, // world shallow, deep, then two alternates; w = 1 where set
    lamps: vec4<f32>,        // x = how many lamps are in the lamp buffer, y = 1 while the polish map has marks
    // The weather (world/weather.rs): x = cover added to the cloud deck,
    // y = rain, z = snow, w = the wind's multiplier. All zero and w = 1 on a
    // fair day, which is the sky every one of these was tuned under.
    weather: vec4<f32>,
    // The season (world/season.rs): xyz = the multiply on every green thing,
    // w = how far the grass has gone over to straw. 1, 1, 1, 0 in summer,
    // which is the year every one of these was tuned in.
    season: vec4<f32>,
    // The northern lights (render/aurora.rs): x = brightness, y = how far
    // overhead, z = the night's layout. Zero on most worlds.
    aurora: vec4<f32>,
    // Under water (docs/UNDERWATER.md): x = the water surface over the
    // camera (the sea's when it is dry), y = visibility in metres, z = how
    // fast sunlight fades per metre of depth, w = 1 under inland water.
    under: vec4<f32>,
    // x = snow lying on whatever faces up (tree tops, ledges, roofs), 0 to 1;
    // y = 1 for a map floating in space (world.json `space`).
    snow: vec4<f32>,
    // The EDIT page's platform: x, z of its middle, half its side with the
    // bank, w = 1 while it is laid (FrameParams::studio).
    studio: vec4<f32>,
    // What the weather has left on the ground (world/damp.rs): x = wet,
    // y = standing water, z = frost in shade, w = frost where the sun has
    // been. All zero on dry ground, which is what the looks were tuned on.
    damp: vec4<f32>,
    // Lightning (world/lightning.rs): x, z where it comes to ground, its
    // seed, and how bright it is now. w = 0 between strikes.
    bolt: vec4<f32>,
    // The tide (world/tide.rs): x = how far the open sea has fallen from
    // high water, y = the range, z = high water, w = 1 with a tide. `fog.y`
    // is the open sea's surface now.
    tide: vec4<f32>,
    // The surf (world/surf.rs): x = breaker height on a fair day (m), y =
    // wave length (m), z = period (s), w = 1 with surf.
    surf: vec4<f32>,
    // The swell (world/surf.rs): xy = the way it runs (x, z), z = crest to
    // trough on a fair day (m; 0 for none), w = the longest wave's length.
    swell: vec4<f32>,
};

// What the season does to the colour of a leaf: its own green multiply,
// which is 1 in summer and leaves it exactly alone. A leaf does not go over
// to straw -- it turns or it falls, and both of those are voxels, not colour.
fn seasoned(c: vec3<f32>) -> vec3<f32> {
    return c * g.season.xyz;
}

// What it does to grass, which does go over: as far toward dry straw as the
// year has gone. Straw is the colour with its green pulled down toward its
// red and a good deal of the blue out, which turns a meadow to hay without
// washing it to grey.
fn strawed(c: vec3<f32>) -> vec3<f32> {
    let tinted = c * g.season.xyz;
    let lum = dot(tinted, vec3<f32>(0.33, 0.5, 0.17));
    let straw = vec3<f32>(max(tinted.r, lum * 1.15), lum * 0.92, lum * 0.5);
    return mix(tinted, straw, clamp(g.season.w, 0.0, 1.0));
}

// A point light placed by the world (render/lamps.rs). All the engine's own
// light comes from the sky, so these are the only thing that lights a roofed
// room; they cast no shadow of their own.
struct Lamp {
    pos: vec4<f32>,          // xyz, w = reach in metres
    color: vec4<f32>,        // linear rgb, w = strength at the lamp
    dir: vec4<f32>,          // a spot's direction, w = cos of its half-angle; w = -2 lights all round
};

// Where world point p falls in shadow map `m`: uv, depth, and whether inside.
struct ShadowPos {
    uv: vec2<f32>,
    z: f32,
    inside: bool,
};

fn shadow_pos(m: mat4x4<f32>, p: vec3<f32>) -> ShadowPos {
    let lp = m * vec4<f32>(p, 1.0);
    var out: ShadowPos;
    out.uv = lp.xy * vec2<f32>(0.5, -0.5) + 0.5;
    out.z = lp.z;
    out.inside = all(out.uv >= vec2<f32>(0.0)) && all(out.uv <= vec2<f32>(1.0)) && lp.z <= 1.0;
    return out;
}

// Cascade ci's older and newer map (matrix index, layer) and the blend; for
// cascade 0 both are the one map.
fn cascade_maps(ci: u32) -> vec4<f32> {
    if (ci == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let k = ci - 1u;
    return vec4<f32>(g.shadow_layers[2u * k], g.shadow_layers[2u * k + 1u], select(g.shadow_blend.x, g.shadow_blend.y, k == 1u), f32(2u * k + 1u));
}

struct EntityInst {
    pos_yaw: vec4<f32>,   // world position of the model pivot, yaw
    info: vec4<f32>,      // model voxel size (m), pivot in model voxels
    tint: vec4<f32>,      // rgb multiplier, w = flash
    rot: vec4<f32>,       // quaternion applied before the yaw; zero = none
    // Added after the multiply, so a coat can repaint and not only darken:
    // a dark brown elk cannot be made pastel by multiplying. w flags the
    // shadow and glass (models.rs `Instance::shift`).
    shift: vec4<f32>,
    // A soft body's flesh (anim/flesh.rs): [0] this part's joint from the
    // body's origin (creature frame, m) and its own weight, [1] the jiggle
    // (m) and its parent's weight, [2] sag, floor, height and half length.
    // All zero on anything rigid.
    flesh: array<vec4<f32>, 3>,
    // x: the glow's gain less one (a coat's `glow`, models.rs `Instance::glow`).
    glow: vec4<f32>,
};

// How far a point of a soft body moves, `p` from the body's origin (bottom
// centre) in the creature frame, metres. Must match anim/flesh.rs `field`.
fn flesh_field(p: vec3<f32>, f: array<vec4<f32>, 3>) -> vec3<f32> {
    let sag = f[2].x;
    let floor = f[2].y;
    let hgt = max(f[2].z, 0.001);
    let len = max(f[2].w, 0.001);
    let up = max(p.y - floor, 0.0);
    let h = min(up / hgt, 1.0);
    let s = sag * 0.34;
    // The top settles most, and what it loses bulges out low on the flanks.
    let low = 1.0 - h;
    let spread = s * (1.6 * low * low + 0.25) * smoothstep(0.0, 0.08, up);
    let along = clamp(p.z / len, -1.0, 1.0);
    var d = vec3<f32>(p.x * spread, -s * up * h, p.z * spread * 0.2);
    // Loose flesh lags the frame: the top and the hind end most, the belly
    // on the ground not at all.
    d += f[1].xyz * h * h * (1.0 + 0.4 * along);
    return d;
}

// The flesh offset for a vertex `l` metres from its part's joint.
fn flesh_offset(l: vec3<f32>, f: array<vec4<f32>, 3>) -> vec3<f32> {
    let w = f[0].w;
    let wp = f[1].w;
    if (w == 0.0 && wp == 0.0) {
        return vec3<f32>(0.0);
    }
    // A part goes with its parent's flesh at the joint, and with its own
    // past it, so a soft head on a soft body never parts at the neck.
    var d = vec3<f32>(0.0);
    if (w != 0.0) {
        d += w * flesh_field(l + f[0].xyz, f);
    }
    if (wp != w) {
        d += (wp - w) * flesh_field(f[0].xyz, f);
    }
    return d;
}

// A material's declared colour field (`drift` in its library entry), as the
// multiply that takes its own texture to each end. Rows 256.. of `mats` hold
// the calm style's pair and rows 512.. the classic one; z is 0 where the
// material declared none and the style's own ramp stands instead.
// Returns (lo, hi, declared).
struct Drift {
    lo: vec3<f32>,
    hi: vec3<f32>,
    on: bool,
};

fn unpack_mul(v: u32) -> vec3<f32> {
    return vec3<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u)) * (2.0 / 255.0);
}

fn drift_of(mat: u32, style: u32) -> Drift {
    let row = mats[256u + style * 256u + mat];
    var d: Drift;
    d.on = row.z != 0u;
    d.lo = unpack_mul(row.x);
    d.hi = unpack_mul(row.y);
    return d;
}

// Rotate v by quaternion q (a zero quaternion leaves v alone).
fn qrot(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> quads: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> chunks: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> mats: array<vec4<u32>>;
@group(0) @binding(4) var tex: texture_2d_array<f32>;
@group(0) @binding(5) var samp: sampler;
@group(0) @binding(6) var heightmap: texture_2d<f32>;
@group(0) @binding(7) var<storage, read> palette: array<vec4<f32>>;
@group(0) @binding(8) var<storage, read> equads: array<vec4<u32>>;
@group(0) @binding(9) var<storage, read> einst: array<EntityInst>;
// r = biome mist (0..1), g = the smoothed ground it lies on (m); see Renderer::set_mist.
@group(0) @binding(10) var mistmap: texture_2d<f32>;
// The look's gradient strips (render/grade.rs): row 0 by distance, row 1 by
// luminance. Sampled with `samp`; both rows are clamped in `strip`.
@group(0) @binding(13) var gradetex: texture_2d<f32>;
// The grade's knobs, in the order render::grade::Grade::uniform writes them.
struct GradeU {
    a: vec4<f32>, // ramp reach (m), how much the ramp is held to the horizon's value, haze from ramp, haze density
    b: vec4<f32>, // sky from ramp, sky reach, shade from ramp, sun-side warmth
    c: vec4<f32>, // sun glow, crown shading, exposure, tint strength
    d: vec4<f32>, // gradient map, saturation, contrast, vignette
    e: vec4<f32>, // time-of-day tint, ramp curve (1 eased, 0 straight), distance slabs, spare
};
@group(0) @binding(14) var<uniform> gd: GradeU;
// The land's own skyline (render/horizon.rs): one layer per compass
// direction holding the sine of the skyline's elevation, then a last layer
// holding how much of the sky the cell can see.
@group(0) @binding(18) var horizonmap: texture_2d_array<f32>;
@group(0) @binding(19) var<storage, read> lamplist: array<Lamp>;

// What the lamps near this point add to it. Distance falls off smoothly to
// nothing at the reach, and a surface facing away still takes a little, so a
// wall under a lamp is dim rather than black ("wrap" lighting, which stands
// in for the bounce this engine does not trace).
fn lamp_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    let count = u32(g.lamps.x);
    for (var i = 0u; i < count; i = i + 1u) {
        let l = lamplist[i];
        let d = l.pos.xyz - world;
        let dist = length(d);
        if (dist >= l.pos.w) {
            continue;
        }
        let k = 1.0 - dist / l.pos.w;
        let fall = k * k;
        let ndl = dot(n, d / max(dist, 1e-4));
        // Half-Lambert: 1 straight at the lamp, 0.25 at right angles, a
        // little under 0 behind.
        let wrap = max(ndl * 0.5 + 0.5, 0.0);
        // A spot lights only inside its cone, fading over its outer edge.
        var cone = 1.0;
        if (l.dir.w > -1.5) {
            let c = dot(-d / max(dist, 1e-4), l.dir.xyz);
            cone = smoothstep(l.dir.w, mix(l.dir.w, 1.0, 0.35), c);
        }
        acc = acc + l.color.rgb * (l.color.w * fall * wrap * wrap * cone);
    }
    return acc;
}

struct QuadVert {
    local: vec3<f32>,   // position in the chunk's (or model's) voxel units
    normal: vec3<f32>,
    face: u32,
    mat: u32,
    ao: f32,
    sloped: u32,
    rounded: u32,       // a rounded tree quad (corners moved in 3D)
    nominal: vec3<f32>, // the corner before slope offsets
    blend_mat: u32,     // the neighbouring ground material, 0 for none
    blend: f32,         // its share at this corner, 0 to 0.75
};

// Height offset of one corner of a sloped quad, in voxels (signed 4-bit quarters).
fn slope_offset(o: u32, k: u32) -> f32 {
    return f32(bitcast<i32>(((o >> (4u * k)) & 15u) << 28u) >> 28u) / 4.0;
}

fn dir_normal(d: u32) -> vec3<f32> {
    switch d {
        case 0u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 2u: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 3u: { return vec3<f32>(0.0, -1.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(0.0, 0.0, -1.0); }
    }
}

fn axis(a: u32) -> vec3<f32> {
    switch a {
        case 0u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1u: { return vec3<f32>(0.0, 1.0, 0.0); }
        default: { return vec3<f32>(0.0, 0.0, 1.0); }
    }
}

// Must match mesh.rs DIRS.
fn dir_u(d: u32) -> u32 {
    var t = array<u32, 6>(1u, 2u, 2u, 0u, 0u, 1u);
    return t[d];
}
fn dir_v(d: u32) -> u32 {
    var t = array<u32, 6>(2u, 1u, 0u, 2u, 1u, 0u);
    return t[d];
}

// Quad layout: see mesh.rs.
// Bits [start, start + n) of the 64-bit number hi:lo, n < 32.
fn bits64(lo: u32, hi: u32, start: u32, n: u32) -> u32 {
    var v: u32;
    if (start >= 32u) {
        v = hi >> (start - 32u);
    } else if (start + n <= 32u) {
        v = lo >> start;
    } else {
        v = (lo >> start) | (hi << (32u - start));
    }
    return v & ((1u << n) - 1u);
}

fn sext(v: u32, n: u32) -> f32 {
    return f32(bitcast<i32>(v << (32u - n)) >> (32u - n));
}

// Corner k's offset in voxels for a rounded (tree) quad: see mesh.rs.
fn round_offset(q: vec4<u32>, k: u32) -> vec3<f32> {
    let b = 16u * k;
    return vec3<f32>(sext(bits64(q.w, q.z, b, 5u), 5u), sext(bits64(q.w, q.z, b + 5u, 6u), 6u), sext(bits64(q.w, q.z, b + 11u, 5u), 5u)) / 16.0;
}

// Whether a quad splits along its other diagonal: the one that keeps
// occlusion gradients symmetric. Sloped quads all split the same way, so a
// hillside facets evenly instead of in zigzags.
fn quad_flip(q: vec4<u32>) -> bool {
    let rounded = (q.x >> Q_ROUND_BIT) & 1u;
    let o = select(q.z & 65535u, 0u, rounded == 1u);
    let sloped = o != 0u || rounded == 1u;
    let aoall = (q.y >> Q_AO_SHIFT) & 255u;
    return ((aoall & 3u) + ((aoall >> 4u) & 3u)) < (((aoall >> 2u) & 3u) + ((aoall >> 6u) & 3u)) && !sloped;
}

// The corner (0..3) vertex c (0..5) of a six-vertex quad is drawn at.
fn quad_vertex_corner(q: vec4<u32>, c: u32) -> u32 {
    var ta = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    var tb = array<u32, 6>(1u, 2u, 3u, 1u, 3u, 0u);
    return select(ta[c], tb[c], quad_flip(q));
}

// How far corner k of a sloped creature part's quad follows the parent bone
// (anim/slope.rs), 0 to 1. Only rounded quads without a smoothed normal keep
// weights in their size fields; everything else reads 0.
fn seam_weight(q: vec4<u32>, k: u32) -> f32 {
    if (((q.x >> Q_ROUND_BIT) & 1u) == 0u || ((q.x >> Q_NORMAL_BIT) & 3u) != 0u) {
        return 0.0;
    }
    return f32((q.x >> (Q_SEAM_SHIFT + 2u * k)) & 3u) / 3.0;
}

// A part's vertex in the creature frame (metres from the creature's pivot
// before the yaw, as `rot` leaves it): its own rotation, or for the corners
// of a sloped part stretched to its parent, partly the parent's (glow.yzw,
// w kept positive: anim push_parts). Then the soft body's flesh.
fn entity_local(q: vec4<u32>, k: u32, local: vec3<f32>, inst: EntityInst) -> vec3<f32> {
    let v = (local - inst.info.yzw) * inst.info.x;
    var r = qrot(inst.rot, v);
    let w = seam_weight(q, k);
    if (w > 0.0) {
        let pq = vec4<f32>(inst.glow.yzw, sqrt(max(1.0 - dot(inst.glow.yzw, inst.glow.yzw), 0.0)));
        r = mix(r, qrot(pq, v), w);
    }
    return r + flesh_offset(r, inst.flesh);
}

// `entity_local` turned by the yaw and placed in the world.
fn entity_world(q: vec4<u32>, k: u32, local: vec3<f32>, inst: EntityInst) -> vec3<f32> {
    let l = entity_local(q, k, local, inst);
    let c = cos(inst.pos_yaw.w);
    let sn = sin(inst.pos_yaw.w);
    return inst.pos_yaw.xyz + vec3<f32>(l.x * c + l.z * sn, l.y, -l.x * sn + l.z * c);
}

// Vertex c (0..3) of a quad drawn indexed as (0, 1, 2) (0, 2, 3): turning the
// corners by one gives the other diagonal, (1, 2, 3) (1, 3, 0).
fn decode_quad4(q: vec4<u32>, c: u32) -> QuadVert {
    return decode_corner(q, select(c, (c + 1u) & 3u, quad_flip(q)));
}

fn decode_corner(q: vec4<u32>, corner: u32) -> QuadVert {
    let p = vec3<f32>(f32(q.x & 31u), f32((q.x >> Q_Y_SHIFT) & 31u), f32((q.x >> Q_Z_SHIFT) & 31u));
    let rounded = (q.x >> Q_ROUND_BIT) & 1u;
    let o = select(q.z & 65535u, 0u, rounded == 1u);
    let sloped = select(0u, 1u, o != 0u || rounded == 1u);
    // A rounded quad's size fields hold its normal (mesh/quad.rs): it is 1x1,
    // or 2x2 for a leaf block.
    let block = (q.x >> Q_BLOCK_BIT) & 1u;
    let w = select(f32(((q.x >> Q_W_SHIFT) & 31u) + 1u), f32(1u + block), rounded == 1u);
    let h = select(f32(((q.x >> Q_H_SHIFT) & 31u) + 1u), f32(1u + block), rounded == 1u);
    let mat = q.y & 255u;
    var ho = array<f32, 4>(slope_offset(o, 0u), slope_offset(o, 1u), slope_offset(o, 2u), slope_offset(o, 3u));
    let d = (q.x >> Q_DIR_SHIFT) & 7u;
    let aoall = (q.y >> Q_AO_SHIFT) & 255u;
    var aos = array<u32, 4>(aoall & 3u, (aoall >> 2u) & 3u, (aoall >> 4u) & 3u, (aoall >> 6u) & 3u);
    let cu = select(0.0, 1.0, corner == 1u || corner == 2u);
    let cv = select(0.0, 1.0, corner >= 2u);
    let n = dir_normal(d);
    let positive = (d & 1u) == 0u;
    let base = p + select(vec3<f32>(0.0), n, positive);
    var out: QuadVert;
    out.nominal = base + axis(dir_u(d)) * (cu * w) + axis(dir_v(d)) * (cv * h);
    // Grow flat quads a few thousandths of a voxel past their edges. Where a
    // merged quad's edge meets the corners of several small ones (a
    // T-junction), rounding leaves pinholes along the seam that show the sky.
    // A flat quad grows within its own plane, so the overlap is exact and far
    // below a pixel. Sloped quads are 1x1 and never have such an edge.
    let grow = select(0.004, 0.0, sloped == 1u);
    let spread = axis(dir_u(d)) * (cu * 2.0 - 1.0) + axis(dir_v(d)) * (cv * 2.0 - 1.0);
    var shift = vec3<f32>(0.0, ho[corner], 0.0);
    if (rounded == 1u) {
        // A leaf block's offsets are in quarters of a voxel, not sixteenths.
        shift = round_offset(q, corner) * f32(1u + 3u * block);
    }
    out.local = out.nominal + spread * grow + shift;
    out.normal = n;
    out.face = d;
    out.mat = mat;
    out.ao = f32(aos[corner]);
    out.sloped = sloped;
    out.rounded = rounded;
    out.blend_mat = select((q.z >> Q_BLEND_SHIFT) & 255u, 0u, rounded == 1u);
    // A rounded quad's word2 is all corner offsets, so it carries no blend.
    out.blend = select(f32((q.z >> (24u + 2u * corner)) & 3u) * 0.25, 0.0, rounded == 1u);
    return out;
}

// The smoothed normal a rounded quad is lit by, if it has one (Q_NORMAL_BIT;
// octahedral, 5 bits a component, in the size fields), else zero.
fn quad_normal(q: vec4<u32>) -> vec3<f32> {
    if (((q.x >> Q_ROUND_BIT) & 1u) == 0u || ((q.x >> Q_NORMAL_BIT) & 1u) == 0u) {
        return vec3<f32>(0.0);
    }
    let o = vec2<f32>(f32((q.x >> Q_W_SHIFT) & 31u), f32((q.x >> Q_H_SHIFT) & 31u)) / 31.0 * 2.0 - 1.0;
    var n = vec3<f32>(o.x, 1.0 - abs(o.x) - abs(o.y), o.y);
    if (n.y < 0.0) {
        n = vec3<f32>((1.0 - abs(o.y)) * sign(o.x), n.y, (1.0 - abs(o.x)) * sign(o.y));
    }
    return normalize(n);
}

fn hash3u(p: vec3<i32>) -> u32 {
    var h = u32(p.x) * 0x8da6b343u ^ u32(p.y) * 0xd8163841u ^ u32(p.z) * 0xcb1ab31fu;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

// A hash of a strike's seed and a step down its channel, -0.5 to 0.5.
fn bolt_jag(seed: f32, k: f32) -> f32 {
    return fract(sin(seed * 12.9898 + k * 78.233) * 43758.5453) - 0.5;
}

// Where the channel is `k` steps of 14 down from the cloud base: a lean from
// the top that straightens toward the ground, and a jag at every step.
fn bolt_point(k: f32) -> vec3<f32> {
    let seed = g.bolt.z;
    let ground = g.fog.y;
    let base = ground + 560.0;
    let t = k / 14.0;
    let lean = vec2<f32>(bolt_jag(seed, 91.0), bolt_jag(seed, 37.0)) * 260.0 * (1.0 - t);
    let jag = vec2<f32>(bolt_jag(seed, k), bolt_jag(seed, k + 50.0)) * 44.0 * select(1.0, 0.0, k >= 14.0);
    return vec3<f32>(g.bolt.x + lean.x + jag.x, mix(base, ground, t), g.bolt.y + lean.y + jag.y);
}

// How far the ray from the camera along `dir` passes from segment a-b, as an
// angle, and how far along the ray the nearest point is.
fn ray_seg(o: vec3<f32>, dir: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> vec2<f32> {
    let ab = b - a;
    let ao = a - o;
    let dd = dot(dir, dir);
    let de = dot(dir, ab);
    let ee = dot(ab, ab);
    let den = max(dd * ee - de * de, 1e-4);
    let u = clamp((dot(ao, dir) * de - dot(ao, ab) * dd) / den, 0.0, 1.0);
    let q = a + ab * u;
    let s = max(dot(q - o, dir), 1.0);
    return vec2<f32>(length(q - o - dir * s) / s, s);
}

// The light of a lightning channel seen along `dir` from the camera: a white
// core about a metre across, a violet halo round it, and the cloud base lit
// over the strike. Part of the sky colour, so every reflection of the sky
// shows it too.
fn bolt_light(dir: vec3<f32>) -> vec3<f32> {
    let f = g.bolt.w;
    if (f < 0.002) {
        return vec3<f32>(0.0);
    }
    let o = g.cam_pos.xyz;
    var near = 10.0;
    var a = bolt_point(0.0);
    for (var k = 1; k <= 14; k++) {
        let b = bolt_point(f32(k));
        let r = ray_seg(o, dir, a, b);
        near = min(near, r.x / (0.0009 + 1.1 / r.y));
        a = b;
    }
    // One branch, off the fourth step, going its own way for a while.
    var c = bolt_point(4.0);
    let side = vec3<f32>(bolt_jag(g.bolt.z, 7.0), -0.8, bolt_jag(g.bolt.z, 8.0)) * vec3<f32>(90.0, 60.0, 90.0);
    for (var k = 1; k <= 4; k++) {
        let b = c + side + vec3<f32>(bolt_jag(g.bolt.z, f32(k) + 20.0), 0.0, bolt_jag(g.bolt.z, f32(k) + 30.0)) * 30.0;
        let r = ray_seg(o, dir, c, b);
        near = min(near, r.x / (0.0007 + 0.6 / r.y));
        c = b;
    }
    let core = exp(-near * near);
    let halo = exp(-near * 0.08) * 0.18;
    // The deck over the strike lights up with it.
    let top = normalize(bolt_point(0.0) - o);
    let cloud = pow(max(dot(dir, top), 0.0), 18.0) * smoothstep(-0.02, 0.1, dir.y) * 0.8;
    return vec3<f32>(0.78, 0.82, 1.0) * f * (core * 9.0 + halo + cloud);
}

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    let up = max(dir.y, 0.0);
    // The calm sky turns blue sooner above the horizon. At night the glow at
    // the horizon spreads much wider, there being no sun to concentrate it:
    // kept as tight as the daytime one, it left a bright rim a degree or two
    // deep that distant water mirrored into a pale slab lying along the
    // skyline.
    let fall = mix(select(0.55, 0.42, g.style.x > 0.5), 0.8, g.air_sun.w);
    var c = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, pow(up, fall));
    if (dir.y < 0.0) {
        // Below the horizon the sky falls away at the same rate it does
        // above it, and cools. Held at the horizon's own colour instead, it
        // was brighter than the sky a degree above it, and since the haze in
        // front of distant land reads this colour, the land's edge cut a
        // pale band right across the view on a clear night.
        let down = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, pow(-dir.y, 0.42));
        c = down * mix(vec3<f32>(1.0), vec3<f32>(0.82, 0.86, 0.92), min(-dir.y * 3.0, 1.0));
    }
    let s = max(dot(dir, sun), 0.0);
    // A tight halo round the key light (sun or moon), and a broad glow round
    // the real sun that stays after it sets: gold, then rose, then violet,
    // lying along the horizon on the sunset side.
    // The moon's halo is tighter than the sun's.
    c += g.sun_color.rgb * mix(pow(s, 48.0) * 0.45, pow(s, 400.0) * 0.5 + pow(s, 40.0) * 0.06, g.air_sun.w);
    let st = max(dot(dir, g.air_sun.xyz), 0.0);
    let band = pow(1.0 - min(abs(dir.y), 1.0), 6.0);
    c += g.glow.rgb * (pow(st, 8.0) * 0.3 + pow(st, 3.0) * 0.3 * g.glow.w + pow(st, 2.0) * band * 0.35 * smoothstep(0.35, -0.05, g.air_sun.y));
    if (g.style.x > 0.5) {
        // Palette studies: the low sky takes the far end of the fog ramp, so
        // distant ridges dissolve into it; a broad warm glow round the sun.
        if (gd.b.x > 0.0) {
            let low = 1.0 - smoothstep(0.0, max(gd.b.y, 0.01), dir.y);
            c = mix(c, flat_ramp(1.0), low * gd.b.x);
        }
        if (gd.c.x > 0.0) {
            c += flat_glow(s);
        }
        if (gd.b.w > 0.0) {
            // The low sky warms toward the sun too, so haze and sky agree.
            c *= mix(vec3<f32>(1.0), flat_sunside(dir), 1.0 - smoothstep(0.0, 0.35, dir.y));
        }
    }
    return c + bolt_light(dir);
}

// Mist at p, before the fall-off with height: x = density at the ground
// (per metre), y = the smoothed ground it lies on. Biome banks from the mist
// map plus a thin mist everywhere at dawn and dusk.
fn mist_sample(p: vec3<f32>) -> vec2<f32> {
    let uv = clamp(p.xz / g.fog.w, vec2<f32>(0.002), vec2<f32>(0.998));
    let m = textureSampleLevel(mistmap, samp, uv, 0.0);
    // Slow drifting swells, so a bank breathes rather than sitting still.
    let t = g.cam_pos.w;
    let swell = 0.75 + 0.25 * sin(p.x * 0.031 + t * 0.07) * sin(p.z * 0.027 - t * 0.05);
    return vec2<f32>((m.r * g.mist.x * g.mist.w * swell + g.mist.y) * 0.028, m.g);
}

// How much mist lies between the camera and `world`. Density and ground are
// averaged from three samples along the ray (Simpson weights); the fall-off
// with height is integrated exactly, since sampling it at three points
// overstated the mist on every upward ray and greyed the whole sky.
fn mist_amount(world: vec3<f32>, dist: f32) -> f32 {
    if (g.mist.y + g.mist.x * g.mist.w < 0.0005) {
        return 0.0;
    }
    let a = mist_sample(g.cam_pos.xyz);
    let b = mist_sample(mix(g.cam_pos.xyz, world, 0.5));
    let c = mist_sample(world);
    let m = (a + 4.0 * b + c) / 6.0;
    // Heights above the mist's ground in units of its height; in a hollow
    // below the smoothed ground the mist is simply at full density.
    let h0 = max(g.cam_pos.y - m.y, 0.0) / g.mist.z;
    let h1 = max(world.y - m.y, 0.0) / g.mist.z;
    let x = h1 - h0;
    var f = exp(-h0);
    if (abs(x) > 0.001) {
        f = (exp(-h0) - exp(-h1)) / x;
    }
    return 1.0 - exp(-m.x * dist * f);
}

fn mist_color(dir: vec3<f32>) -> vec3<f32> {
    let st = max(dot(dir, g.air_sun.xyz), 0.0);
    // Sunlit mist is bright, nearly white; by night it takes the sky's violet.
    return g.sky_ambient.rgb * 0.75 + g.sun_color.rgb * g.sun_color.w * 0.16 + g.glow.rgb * g.glow.w * (pow(st, 5.0) * 1.3 + 0.1);
}

// Haze near the sun picks up its glow.
fn inscatter(dir: vec3<f32>) -> vec3<f32> {
    let st = max(dot(dir, g.air_sun.xyz), 0.0);
    return g.glow.rgb * g.glow.w * (pow(st, 5.0) * 0.55 + pow(st, 24.0) * 0.5);
}

// ---- Under water (docs/UNDERWATER.md) ---------------------------------------

// The surface the light under the water comes down through: the lake or
// river the camera is in, else the sea.
fn water_top() -> f32 {
    return select(g.fog.y, g.under.x, g.under.w > 0.5);
}

// How far the tide draws down river water standing at `level` that it may
// draw down by at most `ebb` (world/tide.rs `Tide::river_fall`, the same):
// all of the sea's fall at the sea's own level, none a full range above it.
fn river_fall(level: f32, ebb: f32) -> f32 {
    if (g.tide.w < 0.5 || ebb <= 0.0) {
        return 0.0;
    }
    let taper = max(1.0 - max(level - g.tide.z, 0.0) / g.tide.y, 0.0);
    return min(g.tide.x * taper, ebb);
}

// How wet the tide has left ground at height `y`, 0 to 1: soaked just above
// the water, still dark up to the line high water reached. Nothing without a
// tide, and nothing at high water but the swash at the edge.
fn tide_wet(y: f32) -> f32 {
    if (g.tide.w < 0.5) {
        return 0.0;
    }
    let above = y - g.fog.y;
    let under_high = smoothstep(g.tide.z + 0.12, g.tide.z - 0.05, y);
    return under_high * mix(0.85, 1.0, smoothstep(0.6, 0.0, above)) * step(-0.3, above);
}

// The world's water colours (world.json `water`), or the engine's own
// (WATER_SHALLOW and WATER_DEEP, from render/mod.rs).
fn uw_shallow() -> vec3<f32> {
    return select(WATER_SHALLOW, g.water[0].rgb, g.water[0].w > 0.5);
}

fn uw_deep() -> vec3<f32> {
    return select(WATER_DEEP, g.water[1].rgb, g.water[1].w > 0.5);
}

// The colour of the water itself round a swimmer: the bright shallow colour
// near the top, the deep one by twenty metres down, dark at night.
fn underwater_color() -> vec3<f32> {
    let d = max(g.under.x - g.cam_pos.y, 0.0);
    let c = mix(uw_shallow() * 0.42, uw_deep() * 1.5, smoothstep(1.0, 22.0, d));
    return c * (1.0 - 0.88 * g.air_sun.w);
}

fn uw_noise(p: vec2<f32>, seed: i32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let ii = vec2<i32>(i);
    let a = f32(hash3u(vec3<i32>(ii.x, ii.y, seed)) & 1023u) / 1023.0;
    let b = f32(hash3u(vec3<i32>(ii.x + 1, ii.y, seed)) & 1023u) / 1023.0;
    let c = f32(hash3u(vec3<i32>(ii.x, ii.y + 1, seed)) & 1023u) / 1023.0;
    let d = f32(hash3u(vec3<i32>(ii.x + 1, ii.y + 1, seed)) & 1023u) / 1023.0;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Where a point under the water was lit from: the spot on the surface the
// sunbeam through it came down from.
fn uw_entry(p: vec3<f32>, top: f32) -> vec2<f32> {
    let sun = g.sun_dir.xyz;
    return p.xz + sun.xz / max(sun.y, 0.2) * (top - p.y);
}

// Caustics: the bright net of light the ripples focus on whatever lies under
// them, 0 (none) to about 1.5, fading with depth. Two layers of ridged
// noise drifting different ways; where both ridges cross it is brightest.
fn caustics(p: vec3<f32>) -> f32 {
    let top = water_top();
    let d = top - p.y;
    if (d <= 0.0 || g.sun_dir.y < 0.05) {
        return 0.0;
    }
    let q0 = uw_entry(p, top);
    // Turned off the grid, so the value noise's cells do not line up square.
    let q = vec2<f32>(q0.x * 0.866 - q0.y * 0.5, q0.x * 0.5 + q0.y * 0.866);
    let t = g.cam_pos.w;
    let a = 1.0 - abs(uw_noise(q * 1.7 + vec2<f32>(t * 0.33, t * 0.21), 41) * 2.0 - 1.0);
    let b = 1.0 - abs(uw_noise(vec2<f32>(q.x * 0.34 - q.y * 0.94, q.x * 0.94 + q.y * 0.34) * 2.3 + vec2<f32>(-t * 0.27, t * 0.35), 43) * 2.0 - 1.0);
    let c = pow(a, 10.0) + pow(b, 10.0) + 2.0 * pow(a * b, 6.0);
    return c * exp(-d * 0.11) * smoothstep(0.0, 0.4, d);
}

// Shafts of sun hanging in the water along the view to `world`: light the
// water scatters toward the eye, brighter where a shaft (a patch of the
// surface the waves are focusing) is overhead.
fn water_shafts(world: vec3<f32>, dist: f32) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    if (sun.y < 0.05) {
        return vec3<f32>(0.0);
    }
    let top = water_top();
    let dir = (world - g.cam_pos.xyz) / max(dist, 0.001);
    let reach = min(dist, 32.0);
    let t = g.cam_pos.w;
    var s = 0.0;
    for (var i = 0; i < 6; i++) {
        let p = g.cam_pos.xyz + dir * reach * (f32(i) + 0.5) / 6.0;
        let d = top - p.y;
        if (d <= 0.0) {
            continue;
        }
        let q = uw_entry(p, top);
        let n = uw_noise(q * 0.11 + vec2<f32>(t * 0.03, t * 0.019), 47);
        s += smoothstep(0.52, 0.82, n) * exp(-d * 0.07);
    }
    return g.sun_color.rgb * g.sun_color.w * (s / 6.0) * (reach / 32.0) * 0.5 * uw_shallow();
}

// What the water takes out of a colour between a lit point and an eye under
// the water, on top of the haze: red goes first, then green.
fn water_tint(dist: f32) -> vec3<f32> {
    return exp(-dist / max(g.under.y, 1.0) * vec3<f32>(0.75, 0.16, 0.05));
}

// ---- The look's colour ramps (calm style; see render/grade.rs) -------------
// Every amount below comes from the grade uniform, so the LOOK page changes
// them while the game runs; 0 turns one off.

fn flat_lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// A sample from gradient strip row `row` at `t` (0..1), with the half-texel
// offset that keeps the ends on the first and last samples.
fn strip(t: f32, row: f32) -> vec3<f32> {
    let n = f32(textureDimensions(gradetex).x);
    let u = (clamp(t, 0.0, 1.0) * (n - 1.0) + 0.5) / n;
    let v = (row + 0.5) / f32(textureDimensions(gradetex).y);
    return textureSampleLevel(gradetex, samp, vec2<f32>(u, v), 0.0).rgb;
}

// The art-directed distance ramp: t = 0 near, 1 far. `gd.a.y` decides how
// much of its own brightness it keeps: at 1 each stop is rescaled to the
// horizon's value, so only its hue survives (the old behaviour); at 0 the
// strip sets the value too, which is what makes ridges read as bands.
// A stop on the distance strip, rescaled toward `lum` so the strip supplies
// hue and the sky supplies brightness (gd.a.y decides how much).
fn flat_ramp_lum(t: f32, lum: f32) -> vec3<f32> {
    let r = strip(t, 0.0);
    let held = r * (lum / max(flat_lum(r), 0.001));
    return mix(r, held, clamp(gd.a.y, 0.0, 1.0));
}

fn flat_ramp(t: f32) -> vec3<f32> {
    return flat_ramp_lum(t, flat_lum(g.sky_horizon.rgb));
}

// Where on the ramp a point `dist` metres away sits. Straight by distance,
// or eased so the strip is spent near the camera (gd.e.y).
fn flat_ramp_t(dist: f32) -> f32 {
    let r = max(gd.a.x, 1.0);
    return mix(clamp(dist / r, 0.0, 1.0), 1.0 - exp(-dist / r), clamp(gd.e.y, 0.0, 1.0));
}

// The distance the haze is worked out at. With slabs on, everything inside
// one slab is treated as being at its middle, so a whole ridge takes one
// colour and one thickness of haze and reads as a flat card.
fn flat_dist(dist: f32) -> f32 {
    if (gd.e.z < 1.5) {
        return dist;
    }
    let step = max(gd.a.x, 1.0) / gd.e.z;
    return (floor(dist / step) + 0.5) * step;
}

// Haze warmer looking into the sun, cooler looking away.
fn flat_sunside(dir: vec3<f32>) -> vec3<f32> {
    let k = gd.b.w;
    let w = smoothstep(-0.6, 0.95, dot(dir, g.air_sun.xyz));
    return mix(vec3<f32>(1.0 - 0.6 * k, 1.0, 1.0 + 0.6 * k), vec3<f32>(1.0 + k, 1.0 + 0.2 * k, 1.0 - k), w);
}

// A broad warm glow round the sun, in the sky and in the haze.
fn flat_glow(st: f32) -> vec3<f32> {
    let warm = g.sun_color.rgb * vec3<f32>(1.0, 0.82, 0.58);
    return warm * (pow(st, 8.0) * 0.28 + pow(st, 3.0) * 0.07) * gd.c.x;
}

// Leaves and needles darker toward the ground and lit toward the crown top,
// by height above the ground in the heightmap. k = 0 leaves them alone.
fn flat_tree(world: vec3<f32>) -> f32 {
    let n = vec2<f32>(textureDimensions(heightmap));
    let ip = vec2<i32>(clamp(world.xz / g.fog.w * n, vec2<f32>(0.0), n - 1.0));
    let h = world.y - textureLoad(heightmap, ip, 0).r;
    let k = gd.c.y;
    return mix(1.0 - 0.42 * k, 1.0 + 0.1 * k, smoothstep(8.0, 30.0, h));
}

// Shadowed and indirect light tinted by the ramp at that distance.
fn flat_shade(ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let t = flat_ramp_t(flat_dist(length(world - g.cam_pos.xyz)));
    let r = flat_ramp(t);
    let rc = r * (flat_lum(ambient) / max(flat_lum(r), 0.001));
    let k = gd.b.z * (0.5 + 0.5 * t);
    return mix(ambient, rc, k);
}

// How thick the air between the camera and `world` is, as a multiple of the
// plain distance. Haze thins with height over a scale of gd.e.w metres, and
// this integrates that along the ray exactly, so a valley floor lies under
// more air than the ridge behind it and the two separate. Sampling the
// falloff at the ends instead overstated it on every upward ray, the way the
// mist's own integral avoids.
fn air_depth(world: vec3<f32>) -> f32 {
    let scale = gd.e.w;
    if (scale < 1.0) {
        return 1.0;
    }
    let h0 = max(g.cam_pos.y - g.fog.y, 0.0) / scale;
    let h1 = max(world.y - g.fog.y, 0.0) / scale;
    let x = h1 - h0;
    if (abs(x) < 0.001) {
        return exp(-h0);
    }
    return (exp(-h0) - exp(-h1)) / x;
}

// Haze between the camera and `world`: rgb = its colour, a = how much of
// the view it replaces.
fn haze(world: vec3<f32>) -> vec4<f32> {
    let d = world - g.cam_pos.xyz;
    let dist = length(d);
    if (g.fog.z > 0.5) {
        let a = 1.0 - exp(-dist / max(g.under.y, 1.0));
        return vec4<f32>(underwater_color() + water_shafts(world, dist), a);
    }
    let dir = d / max(dist, 0.001);
    if (g.style.x > 0.5) {
        // Aerial perspective that starts sooner, is thicker low down near the sea
        // and leans toward a warm haze, so ridges read as layered silhouettes.
        // Distance measured in air rather than in metres: everything the
        // ramp and the haze do then follows the thickness, and DISTANCE
        // SLABS cut the view along the land instead of straight across it.
        let dq = flat_dist(dist * air_depth(world));
        let f = 1.0 - exp(-pow(dq * g.fog.x, 1.4));
        // The haze takes the sky it stands in front of, leaning warm. Held to
        // the horizon's own colour instead, it came out brighter than the sky
        // just above it, and the land's edge cut a hard bright band across
        // the view: at night the sky falls away within a degree of the
        // horizon, and the haze stayed where the horizon was.
        let sky = sky_color(dir);
        var c = mix(sky, sky * vec3<f32>(1.02, 0.96, 0.84), 0.4);
        var fa = f;
        if (gd.a.z > 0.0) {
            // The haze colour comes from the ramp by distance, at the
            // brightness of the sky in the same direction.
            c = mix(c, flat_ramp_lum(flat_ramp_t(dq), flat_lum(sky)), gd.a.z);
            // A ramped haze is thick: denser, so the ramp has room to show.
            fa = 1.0 - exp(-pow(dq * g.fog.x * max(gd.a.w, 0.01), 1.25));
        }
        if (gd.b.w > 0.0) {
            c *= flat_sunside(dir);
        }
        if (gd.c.x > 0.0) {
            c += flat_glow(max(dot(dir, g.sun_dir.xyz), 0.0)) * 0.6;
        }
        return vec4<f32>(c + inscatter(dir), clamp(fa, 0.0, 1.0));
    }
    let f = 1.0 - exp(-pow(dist * air_depth(world) * g.fog.x, 1.6));
    return vec4<f32>(sky_color(dir) + inscatter(dir), clamp(f, 0.0, 1.0));
}

// Mist toward `world` (0..1); none underwater. Smooth enough over a quad or
// a sprite to work out per vertex (fog_mist) and interpolate.
fn fog_mist(world: vec3<f32>) -> f32 {
    if (g.fog.z > 0.5) {
        return 0.0;
    }
    return mist_amount(world, length(world - g.cam_pos.xyz));
}

// Haze, then `mist` of mist, over `col`.
fn apply_fog_m(col: vec3<f32>, world: vec3<f32>, mist: f32) -> vec3<f32> {
    // In space there is no air to see through.
    if (g.snow.y > 0.5) {
        return col;
    }
    let h = haze(world);
    let out = mix(col, h.rgb, h.a);
    if (mist <= 0.0) {
        return out;
    }
    return mix(out, mist_color(normalize(world - g.cam_pos.xyz)), mist);
}

fn apply_fog(col: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    return apply_fog_m(col, world, fog_mist(world));
}

// All of the fog as col * a + rgb, for working it out per vertex.
fn fog_terms(world: vec3<f32>) -> vec4<f32> {
    if (g.snow.y > 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let h = haze(world);
    let m = fog_mist(world);
    let mc = mist_color(normalize(world - g.cam_pos.xyz));
    return vec4<f32>(h.rgb * h.a * (1.0 - m) + mc * m, (1.0 - h.a) * (1.0 - m));
}

// Shadow on leaves when the sun is low: from late afternoon into dusk they
// take only half of it, so a lit wood keeps its glow instead of going flat
// and dark (the canopy shades itself heavily at a grazing sun).
fn leaf_shadow(sh: f32) -> f32 {
    let low = 1.0 - smoothstep(0.05, 0.3, g.sun_dir.y);
    return mix(sh, 1.0, 0.5 * low);
}

fn hemi_ambient(n: vec3<f32>) -> vec3<f32> {
    let t = n.y * 0.5 + 0.5;
    let sky_amb = g.sky_ambient.rgb * 0.55;
    return mix(g.ground_color.rgb, sky_amb, t);
}

// ---- billboards ---------------------------------------------------------------
// One quad per instance, six vertices as two triangles. Spray, motes,
// particles, sign plates and the sea plane all take their corners from here.

// Vertex `vi`'s corner, each coordinate -1 or 1.
fn quad_corner(vi: u32) -> vec2<f32> {
    var cu = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var cv = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec2<f32>(cu[vi % 6u], cv[vi % 6u]);
}

struct Facing {
    right: vec3<f32>,
    up: vec3<f32>,
};

// The axes of a billboard whose unit direction to the camera is `f`: square
// to the view, and upright unless it is seen from straight above or below.
fn billboard(f: vec3<f32>) -> Facing {
    var right = cross(vec3<f32>(0.0, 1.0, 0.0), f);
    if (dot(right, right) < 1e-6) {
        right = vec3<f32>(1.0, 0.0, 0.0);
    }
    right = normalize(right);
    return Facing(right, cross(f, right));
}

// The swell at `xz` (world/surf.rs `swell`, the same; here for the sea in
// scene.wgsl and the kelp canopy riding it in foliage.wgsl): x = how far it lifts
// the sea, yz = its slope (d/dx, d/dz). A few long waves running in from the
// open sea, gone in shallow water and in sheltered water (`open`, from
// `shore_at`); a wave shorter than a few `spacing`s (the mesh's, or a
// pixel's) is left out, not aliased.
fn swell(xz: vec2<f32>, t: f32, depth: f32, spacing: f32, open: f32) -> vec3<f32> {
    // Most of the mesh is too coarse to hold even the longest wave: nothing
    // to read there.
    if (spacing >= g.swell.w * 0.25) {
        return vec3<f32>(0.0);
    }
    let a = g.swell.z * 0.5 * clamp(g.weather.w, 0.5, 2.0) * open * smoothstep(0.4, 4.0, depth);
    if (a <= 0.0) {
        return vec3<f32>(0.0);
    }
    var ls = array<f32, 3>(1.0, 0.63, 0.41);
    var turn = array<f32, 3>(0.0, 0.47, -0.66);
    var share = array<f32, 3>(0.5, 0.3, 0.2);
    var y = 0.0;
    var slope = vec2<f32>(0.0);
    for (var i = 0; i < 3; i++) {
        let len = g.swell.w * ls[i];
        let keep = 1.0 - smoothstep(len * 0.12, len * 0.25, spacing);
        if (keep <= 0.0) {
            continue;
        }
        let k = 6.2831853 / len;
        let w = sqrt(9.81 * k);
        let sn = sin(turn[i]);
        let cs = cos(turn[i]);
        let d = vec2<f32>(g.swell.x * cs - g.swell.y * sn, g.swell.x * sn + g.swell.y * cs);
        let phase = k * dot(d, xz) - w * t + f32(i) * 1.7;
        let amp = a * share[i] * keep;
        y += amp * cos(phase);
        slope -= d * (amp * k * sin(phase));
    }
    return vec3<f32>(y, slope);
}
