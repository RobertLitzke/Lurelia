// Main pass: sky, terrain, entities, water and the highlight box.

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_samp: sampler_comparison;

// The cloud layer, as a field on a plane CLOUD_Y above the sea. The sky and
// the shadows on the ground read the same field at a world position, so the
// cloud you can see overhead is the one whose shadow crosses the hill below.
const CLOUD_Y: f32 = 900.0;
// How much of the sun a thick cloud takes.
const CLOUD_SHADE: f32 = 0.62;
const CLOUD_SHADOW_FREQ: f32 = 2.6;

fn cloud_fbm(p: vec2<f32>, octaves: i32, freq: f32) -> f32 {
    let drift = vec2<f32>(g.cam_pos.w * 2.0, g.cam_pos.w * 0.7);
    var sum = 0.0;
    var amp = 0.5;
    var q = (p + drift) * 0.0018 * freq;
    for (var i = 0; i < octaves; i++) {
        let iq = floor(q);
        let fq = q - iq;
        let u = fq * fq * (3.0 - 2.0 * fq);
        let a = f32(hash3u(vec3<i32>(i32(iq.x), i32(iq.y), i)) & 1023u) / 1023.0;
        let b = f32(hash3u(vec3<i32>(i32(iq.x) + 1, i32(iq.y), i)) & 1023u) / 1023.0;
        let c = f32(hash3u(vec3<i32>(i32(iq.x), i32(iq.y) + 1, i)) & 1023u) / 1023.0;
        let d = f32(hash3u(vec3<i32>(i32(iq.x) + 1, i32(iq.y) + 1, i)) & 1023u) / 1023.0;
        sum += mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * amp;
        amp *= 0.5;
        q = q * 2.03 + vec2<f32>(5.2, 1.3);
    }
    return sum;
}

// How much cloud covers the point `p` on the plane. `soft` widens the edge,
// for the shadow on the ground.
fn cloud_cover(p: vec2<f32>, soft: f32) -> f32 {
    let sum = cloud_fbm(p, 5, 1.0);
    var lo = select(0.52, 0.56, g.style.x > 0.5);
    var hi = select(0.78, 0.90, g.style.x > 0.5);
    // The weather moves both thresholds down the same field, so an overcast
    // day is the fair day's clouds spread until they meet rather than a
    // second deck: the shapes you could see at noon are still the ones
    // overhead when it closes in. Negative cover (the heat) scours it clear.
    let w = g.weather.x;
    lo = lo - w * 0.34;
    hi = hi - w * 0.40;
    return smoothstep(lo - soft, hi + soft, sum);
}

fn clouds(dir: vec3<f32>) -> f32 {
    if (dir.y <= 0.01) {
        return 0.0;
    }
    // Where the ray meets the cloud plane. The camera's own height is in it,
    // so the clouds have parallax and keep their places as you fly.
    let up = max(CLOUD_Y - g.cam_pos.y, 60.0);
    let p = g.cam_pos.xz + dir.xz / dir.y * up;
    return cloud_cover(p, 0.0) * smoothstep(0.01, 0.2, dir.y);
}

// How much sun reaches `world` through the clouds: 1 in the open, down to
// 1 - CLOUD_SHADE under a thick one. It reads the same field the sky draws,
// at the point where the sun's ray crosses the cloud plane, so the shadow
// belongs to a cloud you can look up and see. It costs no geometry and
// reaches the world's edge, so it is on whether or not the shadow maps are.
fn cloud_shadow(world: vec3<f32>) -> f32 {
    if (!CLOUD_SHADOW_ON) {
        return 1.0;
    }
    let s = g.sun_dir.xyz;
    // A low sun throws cloud shadows tens of kilometres sideways, far past
    // the clouds this layer stands for; let them go as it sets.
    let k = CLOUD_SHADE * smoothstep(0.10, 0.30, s.y) * (1.0 + 0.3 * max(g.weather.x, 0.0));
    if (k < 0.01) {
        return 1.0;
    }
    let p = world.xz + s.xz * ((CLOUD_Y - world.y) / max(s.y, 0.05));
    // CLOUD_SHADOW_FREQ: the one deliberate difference from the sky. At the
    // sky deck's own scale a cloud is half a kilometre across, so a world a
    // kilometre and a half wide lies under one of them and the shadow reads
    // as a flat dimming. The shadow field runs finer, as a lower broken deck
    // would, and breaks the land into patches you can watch travel.
    return 1.0 - smoothstep(0.50, 0.62, cloud_fbm(p + vec2<f32>(311.0, -97.0), 4, CLOUD_SHADOW_FREQ)) * k;
}

// How far above the ground the land's own shadow still holds. A bird or a
// glider well clear of the hills is in the open whatever the valley below is
// doing, and the baked skyline was measured at ground level.
const HORIZON_LIFT: f32 = 40.0;

// Where `world` falls on the baked maps, and how much of them applies there
// (0 well above the ground, 1 on it).
fn horizon_uv(world: vec3<f32>) -> vec3<f32> {
    let uv = clamp(world.xz / g.fog.w, vec2<f32>(0.001), vec2<f32>(0.999));
    let n = vec2<f32>(textureDimensions(heightmap));
    let ip = vec2<i32>(clamp(uv * n, vec2<f32>(0.0), n - 1.0));
    let ground = textureLoad(heightmap, ip, 0).r;
    let k = 1.0 - smoothstep(4.0, HORIZON_LIFT, world.y - ground);
    return vec3<f32>(uv, k);
}

// The sun against the land's own skyline: 1 in the open, 0 where a ridge
// stands between this point and the sun. The cascades do everything near,
// so this fades in as they fade out; with the shadow maps off it is all
// there is, and comes in sooner.
fn horizon_shadow(world: vec3<f32>) -> f32 {
    let s = g.sun_dir.xyz;
    if (!HORIZON_ON || s.y > 0.98) {
        return 1.0;
    }
    let near = select(60.0, g.splits.z * 0.85, g.sun_dir.w >= 0.5);
    let far = select(150.0, g.splits.z * 1.25, g.sun_dir.w >= 0.5);
    let reach = smoothstep(near, far, length(world - g.cam_pos.xyz));
    if (reach < 0.01) {
        return 1.0;
    }
    let h = horizon_uv(world);
    if (h.z < 0.01) {
        return 1.0;
    }
    // Azimuth 0 points along +Z and they run round through +X, as the bake
    // lays them out.
    let a = atan2(s.x, s.z) / 6.2831853 * f32(HORIZON_AZIMUTHS);
    let b = floor(a);
    let t = a - b;
    let k0 = i32(b - floor(b / f32(HORIZON_AZIMUTHS)) * f32(HORIZON_AZIMUTHS));
    let k1 = (k0 + 1) % HORIZON_AZIMUTHS;
    let sin_h = mix(
        textureSampleLevel(horizonmap, samp, h.xy, k0, 0.0).r,
        textureSampleLevel(horizonmap, samp, h.xy, k1, 0.0).r,
        t,
    );
    // A soft terminator: the bake is one sample every few metres, and a hard
    // edge off it would crawl as the sun moves.
    let lit = smoothstep(sin_h - 0.035, sin_h + 0.02, s.y);
    return mix(1.0, lit, reach * h.z);
}

// How much of the sky this point can see, from the same bake: 1 on open
// ground, less in a hollow or under a scarp. This is the large scale the
// per-voxel occlusion cannot reach, and unlike the shadow it holds at every
// distance, because a valley floor is darker whether you stand in it or look
// across at it.
fn sky_openness(world: vec3<f32>) -> f32 {
    if (!SKY_OCCLUSION_ON) {
        return 1.0;
    }
    let h = horizon_uv(world);
    if (h.z < 0.01) {
        return 1.0;
    }
    let open = textureSampleLevel(horizonmap, samp, h.xy, HORIZON_AZIMUTHS, 0.0).r;
    return mix(1.0, mix(1.0, open, SKY_OPENNESS), h.z);
}



// Rock casts no shadow. The mesher only makes a face where a voxel meets
// air, so the inside of a hill is not in the shadow maps at all, and a sun
// ray that travels through it is unoccluded: slanting in under the land, it
// lights the walls and floors of anything buried -- a cave, a cellar, an imported
// arena sunk under the plain -- in patches that crawl as the sun moves. The
// maps cannot fix this, because the geometry they would need is not meshed.
//
// So ask the land instead. A point below the land's own height field is
// inside the hill whatever the maps say, and the sun is not on it -- the ray
// may well climb out into the open a hundred metres away, but it got there
// through rock.
//
// The one thing that test gets wrong is the face of anything carved under the
// field: a mesa wall, a canyon side, the cut of a road are all open rock
// standing below the plateau the field remembers, and a flat depth test puts
// every cliff in Redrock in shadow. So look along the sun, over a few metres,
// for land lying lower than the point itself. A face has the floor beside it
// and keeps its sun; a room under a plain has the plain on every side and
// does not.
//
// What this costs is sun reaching into a cave mouth, where the voxels are
// open but the land lies over them. There is no bounce light in this engine,
// so that was the leak lighting it, and it is lit by its lamps now like every
// other roofed place. VOXEL_SUNK_SHADOW=0 for comparisons.
const SUNK_STEPS: i32 = 4;
const SUNK_REACH: f32 = 8.0;

// The land's height at one texel, without the bilinear filter: near enough
// for the look sideways, and a quarter of the taps.
fn land_tap(xz: vec2<f32>) -> f32 {
    let dims = vec2<f32>(textureDimensions(heightmap));
    let p = clamp(xz / g.fog.w * dims, vec2<f32>(0.0), dims - vec2<f32>(1.0));
    return textureLoad(heightmap, vec2<i32>(p), 0).r;
}

fn under_land(world: vec3<f32>) -> f32 {
    if (!SUNK_ON) {
        return 1.0;
    }
    // Above the land, which is almost every fragment: nothing more to do.
    var deep = ground_height(world.xz) - world.y;
    if (deep < 0.35) {
        return 1.0;
    }
    // The EDIT page's platform is cut down into the land with the sky open
    // over it, and the land's texels laid the sun across it in blocky
    // patches.
    if (g.studio.w > 0.5 && all(abs(world.xz - g.studio.xy) <= vec2<f32>(g.studio.z))) {
        return 1.0;
    }
    let flat = g.sun_dir.xz;
    let len = length(flat);
    if (len > 0.001) {
        let dir = flat / len;
        for (var i = 1; i <= SUNK_STEPS; i++) {
            let xz = world.xz + dir * (SUNK_REACH * f32(i) / f32(SUNK_STEPS));
            // Against the point's own height, not the ray's: a ray that only
            // gets out by climbing got out through rock.
            deep = min(deep, land_tap(xz) - world.y);
            if (deep < 0.35) {
                return 1.0;
            }
        }
    }
    return 1.0 - smoothstep(0.35, 1.75, deep);
}

// Everything that shades a point without being in a shadow map: the clouds
// overhead, the land it may be buried under, and past the last cascade the
// land's own horizon. All are a few instructions and reach the world's edge,
// so none of them follows the SHADOWS
// setting; turning the maps off costs you contact shadows, not the weather.
fn far_shadow(world: vec3<f32>) -> f32 {
    return cloud_shadow(world) * horizon_shadow(world) * under_land(world);
}

fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    return cascade_shadow(world, n) * far_shadow(world);
}

// The shadow maps alone: 1 wherever they do not reach.
fn cascade_shadow(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (g.sun_dir.w < 0.5) {
        return 1.0;
    }
    // Cascades are squares around the camera, so pick by distance, not view depth.
    let depth = length(world - g.cam_pos.xyz);
    var ci = 0u;
    if (depth > g.splits.x) { ci = 1u; }
    if (depth > g.splits.y) { ci = 2u; }
    if (depth > g.splits.z) { return 1.0; }
    let texel = g.shadow_texel[ci];
    // The calm style spreads the filter wider for softer shadow edges.
    // A world may spread it wider still (world.json sun.shadow_soft).
    let spread = select(1.0, 1.8, g.style.x > 0.5) * max(g.shadow_texel.w, 1.0);
    let inv = spread / SHADOW_SIZE;
    let ndl = clamp(dot(n, g.sun_dir.xyz), 0.02, 1.0);
    let tanl = min(sqrt(1.0 - ndl * ndl) / ndl, 4.0);
    // The legacy bias (VOXEL_SHADOW_BIAS=legacy): 0.24 m along the light,
    // 0.54 m saturated so voxel-scale relief cast no shadow, plus a texel
    // of slope (the map spans 600 m of depth). Surfaces that are in the map
    // themselves or cast from other geometry (SHADOW_VOXEL) keep the 0.24 m
    // in both styles: a voxel of clearance.
    var p = world + n * texel * 1.2;
    let flat_bias = select(select(0.0004, 0.0009, g.style.x > 0.5), 0.0004, shadow_kind == SHADOW_VOXEL && !SHADOW_BIAS_LEGACY);
    var bias = flat_bias + texel * tanl * max(g.shadow_texel.w, 1.0) / 600.0;
    if (shadow_kind == SHADOW_TIGHT && !SHADOW_BIAS_LEGACY) {
        // The map holds only faces turned from the sun (render back_dirs),
        // so a sunlit face is compared against the far side of its own
        // solid, at least a voxel behind it, and needs next to no bias to
        // stay out of its own shadow. A fixed bias along the light is
        // instead a slab of light under every caster: with 0.54 m of it the
        // top half metre of a post under a canopy, the foot of a wall under
        // its eaves and the ground beside a log all came out lit
        // (2026-11-21, Redrock). So the bias is a texel and a half of
        // depth, and the reach a grazing sun needs comes from pushing the
        // sample out along the normal instead, as far as the filter spreads
        // (sin of the angle off the sun times the filter's texels). Off the
        // normal, a point beside a caster moves away from the surface, not
        // toward the light, so contact shadows stay shut.
        let sinl = sqrt(1.0 - ndl * ndl);
        p = world + n * texel * (1.0 + 1.5 * spread * sinl);
        bias = texel * (1.5 + 0.5 * tanl) * spread / 600.0;
    }
    var s = 1.0;
    if (ci == 0u) {
        let sp = shadow_pos(g.shadow_mats[0], p);
        if (!sp.inside) {
            return 1.0;
        }
        var sum = 0.0;
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                sum += textureSampleCompareLevel(shadow_map, shadow_samp, sp.uv + vec2<f32>(f32(x), f32(y)) * inv, 0, sp.z - bias);
            }
        }
        s = sum / 9.0;
    } else {
        // Cascades 1 and 2 blend from their older map to their newer one as
        // the next is drawn (see render SLICES). Four filtered taps a map,
        // spread as wide as the nine of cascade 0.
        let cm = cascade_maps(ci);
        let mi = u32(cm.w);
        var sa = 1.0;
        var sb = 1.0;
        if (cm.z < 0.99) {
            sa = shadow_pcf4(g.shadow_mats[mi], i32(cm.x), p, inv * 1.5, bias);
        }
        if (cm.z > 0.01) {
            sb = shadow_pcf4(g.shadow_mats[mi + 1u], i32(cm.y), p, inv * 1.5, bias);
        }
        s = mix(sa, sb, cm.z);
    }
    // Fade out at the far edge of the last cascade.
    if (ci == 2u) {
        s = mix(s, 1.0, smoothstep(g.splits.z * 0.85, g.splits.z, depth));
    }
    return s;
}

fn shadow_pcf4(m: mat4x4<f32>, layer: i32, p: vec3<f32>, spread: f32, bias: f32) -> f32 {
    let sp = shadow_pos(m, p);
    if (!sp.inside) {
        return 1.0;
    }
    var sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let o = vec2<f32>(f32(i & 1) - 0.5, f32(i >> 1u) - 0.5) * spread;
        sum += textureSampleCompareLevel(shadow_map, shadow_samp, sp.uv + o, layer, sp.z - bias);
    }
    return sum * 0.25;
}

// Set by fs_terrain for foliage voxels before it calls light().
var<private> leafy: bool = false;
// How much shadow bias the surface being lit needs (cascade_shadow), set
// by fs_terrain and fs_entity before they call light().
// SHADOW_TIGHT: a voxel face turned toward the sun, or an entity's face:
// the shadow maps hold only faces turned from the sun (render back_dirs,
// and p_shadow_entity culls front faces), so it is compared against the
// far side of its own solid and needs almost no bias.
// SHADOW_VOXEL: a voxel of clearance (0.24 m) for terrain that is in the map
// itself or casts from other geometry: a face turned away that a smoothed
// normal still lights sits at its own depth in the map; rounded trees cast
// from their square voxels and faceted rock from facets that lean off them
// (both striped a cliff with the tight bias, 2026-11-21); a sloped quad
// where the sun grazes it leans off its voxel face; tubes are drawn along
// their own line, off their voxels.
// SHADOW_LEGACY: leaves, and everything not drawn as terrain or entities
// (foliage cards, particles, water, signs).
const SHADOW_LEGACY: u32 = 0u;
const SHADOW_TIGHT: u32 = 1u;
const SHADOW_VOXEL: u32 = 2u;
var<private> shadow_kind: u32 = SHADOW_LEGACY;

fn light(albedo: vec3<f32>, world: vec3<f32>, n: vec3<f32>, ao: f32, emissive: f32) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    let ndl = max(dot(n, sun), 0.0);
    var sh = 1.0;
    if (ndl > 0.0) {
        sh = shadow_factor(world, n);
        if (leafy) {
            sh = leaf_shadow(sh);
        }
    }
    let a = clamp(ao / 3.0, 0.0, 1.0);
    let calm = g.style.x > 0.5;
    let ao_amb = mix(select(0.38, 0.5, calm), 1.0, a * a * (3.0 - 2.0 * a));
    var lit = ndl * sh;
    var direct = g.sun_color.rgb * g.sun_color.w * lit * mix(select(0.75, 0.9, calm), 1.0, a);
    // The per-voxel occlusion is metres across; the baked one is hundreds,
    // and is what sets a valley floor below the ridge it lies under.
    var ambient = hemi_ambient(n) * ao_amb * sky_openness(world);
    if (calm && gd.b.z > 0.0) {
        ambient = flat_shade(ambient, world);
    }
    // Below the surface, sunlight fades with depth, the ripples net it with
    // caustics, and the ambient turns the water's colour.
    let under = water_top() - world.y;
    if (under > 0.0) {
        let k = exp(-under * g.under.z);
        direct *= k * vec3<f32>(0.75, 0.95, 1.0) * (0.72 + 0.9 * caustics(world));
        ambient = mix(underwater_color() * 2.2, ambient, k);
    }
    // Lamps are not shadowed and not occluded by the sky map, which is the
    // point of them: they are what lights anything with a roof over it.
    let lamp = lamp_light(world, n) * mix(0.55, 1.0, a);
    var col = albedo * (direct + ambient + lamp) + albedo * emissive;
    if (g.fog.z > 0.5) {
        col *= water_tint(length(world - g.cam_pos.xyz));
    }
    return col;
}

// The land's own normal, from the world height map, over a few metres: the
// slope a hillside has, rather than the slope of one voxel step in it.
fn land_normal(xz: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(heightmap));
    let d = max(g.fog.w / max(dims.x, 1.0), 1.0);
    let hx = ground_height(xz + vec2<f32>(d, 0.0)) - ground_height(xz - vec2<f32>(d, 0.0));
    let hz = ground_height(xz + vec2<f32>(0.0, d)) - ground_height(xz - vec2<f32>(0.0, d));
    return normalize(vec3<f32>(-hx, 2.0 * d, -hz));
}

// ---- terrain ---------------------------------------------------------------

// How far sloped ground's lighting normal leans back toward its face direction.
const SLOPE_SOFTEN: f32 = 0.5;

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) face_mat: vec2<u32>,
    @location(2) ao: f32,
    @location(3) @interpolate(flat) vscale: f32,
    @location(4) nominal: vec3<f32>,
    @location(5) @interpolate(flat) sloped: u32,
    @location(6) blend: f32,
    @location(7) @interpolate(flat) blend_mat: u32,
    // A sloped ground quad's own normal: the cross of its diagonals, the
    // average of its two triangles (zero for other quads).
    @location(8) @interpolate(flat) patch_n: vec3<f32>,
    // 1 for a quad drawn off its voxel face: a tube (mesh/lean.rs), drawn
    // along its stem's own line, or a rounded one (trees, faceted rock),
    // whose corners move. Its voxel face direction says little about which
    // way it faces, or where it casts from (shadow.wgsl).
    @location(9) @interpolate(flat) moved: u32,
};

@vertex
fn vs_terrain(@builtin(vertex_index) vi: u32) -> TerrainOut {
    let q = quads[vi / 4u];
    let qv = decode_quad4(q, vi % 4u);
    let ch = chunks[q.y >> 16u];
    let world = ch.xyz + qv.local * ch.w;
    var out: TerrainOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.face_mat = vec2<u32>(qv.face, qv.mat);
    out.ao = qv.ao;
    out.vscale = ch.w;
    out.nominal = ch.xyz + qv.nominal * ch.w;
    out.sloped = qv.sloped;
    out.blend = qv.blend;
    out.blend_mat = qv.blend_mat;
    out.patch_n = vec3<f32>(0.0);
    out.moved = ((q.x >> Q_TUBE_BIT) & 1u) | qv.rounded;
    let o = q.z & 65535u;
    let smooth_n = quad_normal(q);
    if (dot(smooth_n, smooth_n) > 0.5) {
        // Foliage lit by the crown's smoothed shape, not its facets; the
        // length 2 tells the fragment shader to use it as it is.
        out.patch_n = smooth_n * 2.0;
    } else if (qv.sloped == 1u && qv.rounded == 0u) {
        let d = qv.face;
        let u = axis(dir_u(d));
        let v = axis(dir_v(d));
        let up = vec3<f32>(0.0, 1.0, 0.0);
        let d1 = u + v + up * (slope_offset(o, 2u) - slope_offset(o, 0u));
        let d2 = v - u + up * (slope_offset(o, 3u) - slope_offset(o, 1u));
        let pn = normalize(cross(d1, d2));
        out.patch_n = select(-pn, pn, dot(pn, dir_normal(d)) > 0.0);
    } else if (qv.rounded == 1u && (mats[qv.mat].w & M_FACET) != 0u) {
        // Faceted rock: light the whole quad by its diagonals' normal, as
        // sloped ground is, so a wall reads as broad planes, not lumps.
        let c0 = decode_corner(q, 0u).local;
        let c1 = decode_corner(q, 1u).local;
        let c2 = decode_corner(q, 2u).local;
        let c3 = decode_corner(q, 3u).local;
        let pn = normalize(cross(c2 - c0, c3 - c1));
        out.patch_n = select(-pn, pn, dot(pn, dir_normal(qv.face)) > 0.0);
    }
    return out;
}

fn vnoise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let ii = vec2<i32>(i);
    let a = f32(hash3u(vec3<i32>(ii.x, ii.y, 7)) & 1023u) / 1023.0;
    let b = f32(hash3u(vec3<i32>(ii.x + 1, ii.y, 7)) & 1023u) / 1023.0;
    let c = f32(hash3u(vec3<i32>(ii.x, ii.y + 1, 7)) & 1023u) / 1023.0;
    let d = f32(hash3u(vec3<i32>(ii.x + 1, ii.y + 1, 7)) & 1023u) / 1023.0;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Broad clumps of sun and shade over a canopy, from a point in the world. A
// crown is lit in patches, not as one even green; without this every tree past
// the leaf cards' fade distance (LEAF_FADE) read as a smooth mound, and near
// ones as one flat colour under their cards. Leaf cards (foliage.wgsl) and the
// voxels behind them use the same field at the same strength, so a crown keeps
// its clumps as its cards fade away. VOXEL_DAPPLE=0 turns it off.
fn canopy_dapple(p: vec3<f32>) -> f32 {
    if (!DAPPLE_ON) {
        return 1.0;
    }
    // Sheared by height, so the clumps are three-dimensional and two crowns
    // one above the other do not share a pattern.
    let q = p.xz + vec2<f32>(p.y, -p.y) * 0.6;
    let n = vnoise2(q * 0.40) * 0.62 + vnoise2(q * 1.15 + vec2<f32>(7.3, 2.1)) * 0.38;
    // Two octaves averaged sit close to the middle, so stretch what is left
    // of the range: without this the clumps came out a few per cent deep.
    return 1.0 + clamp((n - 0.5) * 2.4, -0.5, 0.5) * 0.44;
}


// Calm style: broad fields of colour across the land, tens of metres across,
// in place of per-texel detail. Grass drifts between sage and ochre meadow.
fn calm_tint(albedo: vec3<f32>, mat: u32, world: vec3<f32>) -> vec3<f32> {
    let t = vnoise2(world.xz * 0.012) * 0.6 + vnoise2(world.xz * 0.045 + vec2<f32>(13.0, 7.0)) * 0.4;
    let f = mats[mat].w;
    let d = drift_of(mat, 0u);
    // A material that declares its own two colours uses them, whatever tint
    // class it is in; the ramps below are what every meadow and every canopy
    // shares when none is declared.
    if (d.on && (f & M_CALM_MEADOW) != 0u) {
        return albedo * mix(d.lo, d.hi, smoothstep(0.3, 0.7, t));
    }
    if ((f & M_CALM_MEADOW) != 0u) {
        return albedo * mix(vec3<f32>(0.86, 1.0, 0.84), vec3<f32>(1.3, 1.04, 0.58), smoothstep(0.3, 0.7, t));
    }
    var k = 1.0;
    if (gd.c.y > 0.0 && (f & M_FOLIAGE_FW) != 0u) {
        k = flat_tree(world);
    }
    if (d.on) {
        return albedo * mix(d.lo, d.hi, t) * k;
    }
    if ((f & M_CALM_LEAF) != 0u) {
        return albedo * mix(vec3<f32>(0.88, 1.0, 1.0), vec3<f32>(1.1, 1.06, 0.82), t) * k;
    }
    return albedo * (0.94 + 0.12 * t) * k;
}

// Classic style: large patches of drier and lusher growth, in one place so
// the ground (fs_terrain) and the grass standing on it (foliage.wgsl) read
// the same. A material's own declared pair stands in for the shared ramp.
fn classic_patch(albedo: vec3<f32>, mat: u32, world: vec3<f32>) -> vec3<f32> {
    let t = vnoise2(world.xz * 0.025) * 0.65 + vnoise2(world.xz * 0.11) * 0.35;
    let d = drift_of(mat, 1u);
    if (d.on) {
        return albedo * mix(d.lo, d.hi, t);
    }
    return albedo * mix(vec3<f32>(1.08, 1.02, 0.84), vec3<f32>(0.88, 0.98, 1.0), t);
}

// Grass understory: meadow ground near the camera is painted as the shaded
// lower layer of the grass (darker, cooler, with short blade strokes), so the
// ground between tufts reads as more grass and hides the slope facets. Fades
// out with the grass; the tuft bases shade toward the same colour (foliage.wgsl).
const UNDERSTORY: vec3<f32> = vec3<f32>(0.80, 0.82, 0.72);
const UNDER_FADE: vec2<f32> = vec2<f32>(30.0, 55.0);

// One dash in p's 12 cm cell: +/- brightness where p is on it, else 0.
fn blade_dash(p: vec2<f32>, layer: u32) -> f32 {
    let cell = 0.12;
    let c = floor(p / cell);
    let h = hash3u(vec3<i32>(i32(c.x), i32(c.y), 41 + i32(layer)));
    let r = vec4<f32>(f32(h & 255u), f32((h >> 8u) & 255u), f32((h >> 16u) & 255u), f32(h >> 24u)) / 255.0;
    let centre = (c + 0.5) * cell;
    let a = r.x * 3.14159;
    let d = vec2<f32>(cos(a), sin(a));
    let q = p - centre;
    let half_len = cell * (0.25 + 0.2 * r.y);
    let along = clamp(dot(q, d), -half_len, half_len);
    let off = length(q - d * along);
    let line = 1.0 - smoothstep(0.004, 0.009, off);
    // Half the dashes are lit blade tops, half are shadows between them.
    return line * select(-0.14, 0.12, r.z > 0.5) * step(0.25, r.w);
}

fn understory(albedo: vec3<f32>, world: vec3<f32>, footprint: f32) -> vec3<f32> {
    let dist = length(world - g.cam_pos.xyz);
    let k = 1.0 - smoothstep(UNDER_FADE.x, UNDER_FADE.y, dist);
    if (k <= 0.0) {
        return albedo;
    }
    // Strokes: two offset grids of 12 cm cells, each holding one short dash
    // (a fallen or bent blade) at its own angle, lighter or darker than the
    // ground. Dashes stay inside their cell, so one cell is read per grid.
    let detail = 1.0 - smoothstep(0.02, 0.07, footprint);
    var stroke = 0.0;
    if (detail > 0.0) {
        stroke = (blade_dash(world.xz, 0u) + blade_dash(world.xz + vec2<f32>(0.06, 0.04), 1u)) * detail;
    }
    let tone = UNDERSTORY * (1.0 + 0.09 * (vnoise2(world.xz * 1.3) - 0.5));
    let shade = tone * (1.0 + stroke);
    return albedo * mix(vec3<f32>(1.0), shade, k);
}

// Glass: each texel is a tiny facet tilted its own way, so as you move,
// single texels flash where one catches the sun. `soft` is the rime version
// (frost, ice): facets tilted far less and lit far wider, so the surface
// reads as a sheen with glitter in it rather than a field of flashing pixels.
// `fp` is the uv the pixel covers: once a pixel spans a good part of a facet
// the flash can no longer be drawn, only aliased, so it fades out instead.
fn glass_glints(world: vec3<f32>, n: vec3<f32>, uv: vec2<f32>, face: u32, fp: f32, soft: bool) -> vec3<f32> {
    let aa = 1.0 - smoothstep(0.2, 0.7, fp * 32.0);
    if (aa <= 0.0) {
        return vec3<f32>(0.0);
    }
    let cell = vec2<i32>(floor(uv * 32.0));
    let h = hash3u(vec3<i32>(cell.x, cell.y, i32(face) * 7919 + i32(floor(dot(world, n) * 4.0))));
    let j = vec3<f32>(f32(h & 1023u), f32((h >> 10u) & 1023u), f32((h >> 20u) & 1023u)) / 1023.0 - 0.5;
    let tilt = select(0.9, 0.3, soft);
    let sharp = select(900.0, 160.0, soft);
    let gain = select(9.0, 2.2, soft);
    let facet = normalize(n + j * tilt);
    let view = normalize(world - g.cam_pos.xyz);
    var s = pow(max(dot(reflect(view, facet), g.sun_dir.xyz), 0.0), sharp);
    if (soft) {
        // Round off the facet so one texel is not a hard bright square, and
        // hold most of the shine in the facets that already face the sun.
        let r = length(fract(uv * 32.0) - 0.5);
        s = s * smoothstep(0.55, 0.22, r);
    }
    if (s < 0.004) {
        return vec3<f32>(0.0);
    }
    return g.sun_color.rgb * g.sun_color.w * s * gain * aa * shadow_factor(world, n);
}

// Water: small points of sun glitter on the ripples, each living a fraction
// of a second. `cell` is the glint spacing in metres.
// Froth on open water (docs/UNDERWATER.md): streaks of foam drawn out
// downwind and broken into flecks, drifting, more of it on a windy day
// (`g.weather.w`). 0 to 1. What makes a surface read as water from
// underneath, and keeps a calm sea from looking like glass from above.
fn surface_froth(p: vec2<f32>, t: f32) -> f32 {
    let drift = vec2<f32>(0.22, 0.13) * t;
    // Stretched along the drift: long thin lines, not blobs.
    let along = vec2<f32>(p.x * 0.85 + p.y * 0.5, -p.x * 0.5 + p.y * 0.85);
    // The ridges of the noise, not its peaks: thin wind lines, not blots.
    let line = 1.0 - abs(vnoise2(vec2<f32>(along.x * 0.05, along.y * 0.17) + drift * 0.05) * 2.0 - 1.0);
    let fleck = vnoise2(p * 1.3 + drift);
    let fine = vnoise2(p * 3.4 - drift * 1.3);
    let wind = clamp(g.weather.w, 0.5, 2.5);
    let lo = 0.88 - 0.04 * (wind - 1.0);
    return smoothstep(lo, lo + 0.07, line) * smoothstep(0.42, 0.72, fleck * 0.65 + fine * 0.35);
}

fn water_glints(p: vec2<f32>, t: f32, view: vec3<f32>, n: vec3<f32>, cell: f32) -> f32 {
    let c = floor(p / cell);
    let ci = vec2<i32>(c);
    let phase = f32(hash3u(vec3<i32>(ci.x, ci.y, 3)) & 255u) / 255.0;
    let h = hash3u(vec3<i32>(ci.x, ci.y, i32(floor(t * 4.0 + phase))));
    let j = vec2<f32>(f32(h & 1023u), f32((h >> 10u) & 1023u)) / 1023.0 - 0.5;
    let facet = normalize(n + vec3<f32>(j.x, 0.0, j.y) * 0.6);
    let s = pow(max(dot(reflect(view, facet), g.sun_dir.xyz), 0.0), 1200.0);
    let spot = smoothstep(0.32, 0.05, length(fract(p / cell) - 0.5));
    return s * spot;
}

fn face_axes(face: u32) -> mat2x3<f32> {
    if (face < 2u) {
        return mat2x3<f32>(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, -1.0, 0.0));
    } else if (face < 4u) {
        return mat2x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    }
    return mat2x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, -1.0, 0.0));
}

// ---- what the weather leaves on the ground (world/damp.rs) -----------------

struct Damp {
    albedo: vec3<f32>,
    // How much the surface shines with a film of water, 0 to 1.
    sheen: f32,
    // Standing water over this point, 0 to 1.
    pool: f32,
    // Frost lying here, 0 to 1.
    frost: f32,
    // How much of `pool` is a sheet flood over pale flat ground (a salt
    // pan): an inch of clear water, so the bed shows bright through it.
    sheet: f32,
};

// Frost's own colour: the pale grey-blue of rime, not snow's white.
const RIME: vec3<f32> = vec3<f32>(0.78, 0.83, 0.9);

// Wet, standing water and frost on one terrain fragment, before it is lit.
// Wet darkens most what faces the sky. Puddles need ground that is level over
// several metres, lie a little thicker in hollows, and are shaped by a noise
// that grows them outward as the pools fill. Frost holds on where the sun has
// not reached since it rose: two amounts from the CPU, picked between by this
// point's own shadow, so a white edge retreats behind every shadow as the
// morning goes on. Nothing lies under the land (a cave, a cellar).
fn damp_ground(albedo: vec3<f32>, world: vec3<f32>, n: vec3<f32>, plant: bool, grassy: bool, floods: bool) -> Damp {
    var d = Damp(albedo, 0.0, 0.0, 0.0, 0.0);
    let open = under_land(world);
    if (open < 0.01) {
        return d;
    }
    let up = smoothstep(-0.2, 0.9, n.y);
    // Rain runs down a rock face or a trunk and wets it nearly as much as
    // the ground, in streaks; an overhang's underside stays drier. Leaves
    // and grass shed it, so their sides stay as they were.
    var wet = g.damp.x * open * mix(select(0.85, 0.4, plant), 1.0, up);
    // What the tide has left wet on its way out (world/tide.rs): every face,
    // a harbour wall's as much as the sand, up to the high-water line.
    wet = max(wet, tide_wet(world.y) * open * select(1.0, 0.6, plant));
    wet *= 1.0 - 0.6 * smoothstep(-0.2, -0.8, n.y);
    if (!plant && up < 0.99) {
        let streak = vnoise2(vec2<f32>((world.x + world.z) * 2.5, world.y * 0.35));
        wet = min(wet * mix(1.0, 0.7 + 0.55 * streak, 1.0 - up), 1.0);
    }
    if (wet > 0.005) {
        // A film of water stops the surface scattering white light back, so
        // it goes darker and deeper in colour, the darks most of all.
        let k = select(1.0, 0.55, plant);
        d.albedo = pow(max(albedo, vec3<f32>(0.0)), vec3<f32>(1.0 + 0.5 * wet * k)) * (1.0 - 0.25 * wet * k);
        d.sheen = wet * select(select(1.0, 0.2, plant), 0.0, leafy);
    }
    if (g.damp.y > 0.005 && !plant && n.y > 0.9) {
        let level = smoothstep(0.982, 0.997, land_normal(world.xz).y);
        let uv = clamp(world.xz / g.fog.w, vec2<f32>(0.002), vec2<f32>(0.998));
        let smooth_ground = textureSampleLevel(mistmap, samp, uv, 0.0).g;
        let hollow = 0.55 + 0.45 * smoothstep(0.6, -0.6, world.y - smooth_ground);
        let shape = vnoise2(world.xz * 0.45) * 0.65 + vnoise2(world.xz * 1.9 + vec2<f32>(7.3, 2.1)) * 0.35;
        let edge = 0.82 - 0.3 * g.damp.y * hollow;
        d.pool = level * open * smoothstep(edge, edge + 0.025, shape);
        // The ground round a pool is soaked darker than the rest, which is
        // what makes the water read as lying in it rather than painted on.
        let rim = level * open * smoothstep(edge - 0.07, edge, shape) * (1.0 - d.pool);
        d.albedo *= 1.0 - 0.35 * rim;
        if (floods) {
            // A pan floods from edge to edge on the first standing water and
            // stays a sheet until it is nearly gone, breaking up into
            // islands of crust only at the very end.
            let film = smoothstep(0.003, 0.05, g.damp.y);
            let islands = smoothstep(0.35, 0.65, shape + (1.0 - film) * 0.6);
            d.sheet = level * open * film * (1.0 - islands * (1.0 - film) * 0.9);
            d.pool = max(d.pool, d.sheet);
        }
    }
    let rime = max(g.damp.z, g.damp.w);
    if (rime > 0.01) {
        var lit = 0.0;
        if (g.air_sun.w < 0.5 && dot(n, g.sun_dir.xyz) > 0.0) {
            lit = shadow_factor(world, n);
        }
        let f = mix(g.damp.z, g.damp.w, lit) * open;
        let lie = vnoise2(world.xz * 3.1) * 0.6 + vnoise2(world.xz * 11.0 + vec2<f32>(2.7, 9.4)) * 0.4;
        let s = smoothstep(0.2, 0.8, n.y) * smoothstep(0.35, 0.7, lie + f * 0.6) * f * select(0.55, 0.85, grassy) * (1.0 - d.pool);
        d.albedo = mix(d.albedo, RIME, s);
        d.frost = s * lit;
    }
    return d;
}

// The shine on wet ground and standing water, and the glitter of frost in
// the sun, over a lit terrain fragment. A puddle is mostly the sky it
// reflects over the dark ground under it; rain falling roughens it.
fn damp_light(col: vec3<f32>, world: vec3<f32>, n: vec3<f32>, d: Damp, footprint: f32) -> vec3<f32> {
    var c = col;
    let v = normalize(g.cam_pos.xyz - world);
    var nn = normalize(mix(n, vec3<f32>(0.0, 1.0, 0.0), d.pool));
    if (d.pool > 0.0 && g.weather.y > 0.02) {
        let t = g.cam_pos.w;
        let r = vec2<f32>(vnoise2(world.xz * 7.0 + vec2<f32>(t * 3.1, 0.0)), vnoise2(world.xz * 7.0 + vec2<f32>(4.2, t * 2.7))) - 0.5;
        nn = normalize(nn + vec3<f32>(r.x, 0.0, r.y) * 0.18 * g.weather.y * d.pool);
    }
    // A wet film shows only a little of the sky; a pool is mostly sky, and
    // dimmer than the sky itself, since it also shows the dark bed under it.
    let shine = d.sheen * 0.1 + d.pool * mix(0.7, 0.95, d.sheet);
    if (shine > 0.0) {
        // A puddle shows its dark bed; a sheet of an inch over salt shows
        // the salt, a little greyed.
        c = mix(c, c * mix(0.35, 0.8, d.sheet), d.pool * 0.85);
        var r = reflect(-v, nn);
        r.y = abs(r.y);
        let fres = 0.02 + 0.98 * pow(1.0 - max(dot(nn, v), 0.0), 5.0);
        // The sky with its clouds in it: a pool holding only the sky's
        // gradient reads as a flat pale decal.
        var sky = sky_color(r);
        if (d.pool > 0.0) {
            let lum = dot(sky, vec3<f32>(0.2126, 0.7152, 0.0722));
            sky = mix(sky, vec3<f32>(lum * 1.15), clouds(r) * 0.8);
        }
        c += sky * fres * shine * sky_openness(world);
        let s = dot(r, g.sun_dir.xyz);
        if (s > 0.9) {
            let tight = mix(80.0, 1200.0, d.pool);
            let sh = shadow_factor(world, nn);
            c += g.sun_color.rgb * g.sun_color.w * pow(max(s, 0.0), tight) * fres * shine * sh * mix(2.0, 12.0, d.pool);
        }
    }
    if (d.frost > 0.0 && footprint < 0.05) {
        // Points of rime catching the sun: a sparse hashed grid of 3 cm
        // cells, faded out before they are smaller than a pixel.
        let h = hash3u(vec3<i32>(floor(world * 33.0)));
        let speck = select(0.0, 1.0, (h & 1023u) < 14u);
        c += g.sun_color.rgb * g.sun_color.w * speck * d.frost * 0.9 * (1.0 - smoothstep(0.02, 0.05, footprint));
    }
    return c;
}

@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4<f32> {
    let face = in.face_mat.x;
    var mat = in.face_mat.y;
    if (in.blend_mat != 0u) {
        // Two ground materials meet here. The share of the higher-numbered one
        // is the same field seen from either side of the border, so testing it
        // against one noisy threshold draws a single continuous, wavy edge.
        let other = in.blend_mat;
        let f = select(1.0 - in.blend, in.blend, other > mat);
        let th = 0.5 + (vnoise2(in.world.xz * 2.2) - 0.5) * 0.55 + (vnoise2(in.world.xz * 7.0 + vec2<f32>(3.1, 8.7)) - 0.5) * 0.25;
        mat = select(min(mat, other), max(mat, other), f > th);
    }
    let n0 = dir_normal(face);
    var ax = face_axes(face);
    // Wood lying along X or Z: turn the texture so its grain (v) runs along
    // the log. End faces keep the upright mapping.
    let gw = mats[mat].w;
    if ((gw & M_GRAIN_X) != 0u && face >= 2u) {
        // Top/bottom (u x, v z) and +-Z faces (u x, v -y): v along x.
        if (face < 4u) {
            ax = mat2x3<f32>(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0));
        } else {
            ax = mat2x3<f32>(vec3<f32>(0.0, -1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0));
        }
    } else if ((gw & M_GRAIN_Z) != 0u && face < 2u) {
        // +-X faces (u z, v -y): v along z. Top/bottom already run v along z.
        ax = mat2x3<f32>(vec3<f32>(0.0, -1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    }
    // Texture coordinates from world position: one 32-texel tile per metre.
    var uv = vec2<f32>(dot(in.world, ax[0]), dot(in.world, ax[1]));
    let dw_x = dpdx(in.world);
    let dw_y = dpdy(in.world);
    // A sloped quad is two flat triangles; take each one's own normal.
    var n = n0;
    if (in.sloped == 1u) {
        let fn_ = normalize(cross(dw_x, dw_y));
        n = select(-fn_, fn_, dot(fn_, n0) > 0.0);
    }
    // Light sloped ground by the whole quad's normal, tipped a little toward
    // the face direction: the two triangles of a quad otherwise light apart
    // and a hillside facing away from a low sun turns into dark sawtooth stripes.
    var n_light = n;
    if (dot(in.patch_n, in.patch_n) > 3.0) {
        n_light = normalize(in.patch_n);
    } else if (dot(in.patch_n, in.patch_n) > 0.5) {
        n_light = normalize(mix(in.patch_n, n0, SLOPE_SOFTEN));
    }
    // Gentle ground is a staircase: one voxel of rise over a long run is a
    // steep little facet in land that is nearly flat, and it catches the sun
    // as a bright diamond among its flat neighbours. Where the land's own
    // slope, read over several metres, is much gentler than the facet, light
    // by the land instead.
    if (dot(in.patch_n, in.patch_n) > 0.5) {
        let land = land_normal(in.world.xz);
        let gentle = smoothstep(0.93, 0.995, land.y);
        let steep = smoothstep(0.1, 0.5, 1.0 - dot(n_light, land));
        n_light = normalize(mix(n_light, land, gentle * steep * 0.85));
    }
    var duv_x = vec2<f32>(dot(dw_x, ax[0]), dot(dw_x, ax[1]));
    var duv_y = vec2<f32>(dot(dw_y, ax[0]), dot(dw_y, ax[1]));
    let m = mats[mat];
    // Leaves: every block wore the same tile the same way round, and on a
    // crown past the leaf cards' fade that repeat lined up into rows. Each
    // voxel turns and flips the tile by a hash of its own (one of eight ways
    // round, about the tile's centre), which breaks the rows and costs one
    // hash. The gradients turn with it, so the filtering is unchanged.
    if ((m.w & (M_FOLIAGE_FW | M_GROWS)) != 0u) {
        let tv = vec3<i32>(floor((in.nominal - n0 * 0.5 * in.vscale) / in.vscale));
        let th = hash3u(tv + vec3<i32>(9157, 311, 7703));
        let cell = floor(uv);
        var t = uv - cell - 0.5;
        if ((th & 1u) != 0u) {
            t = vec2<f32>(-t.y, t.x);
            duv_x = vec2<f32>(-duv_x.y, duv_x.x);
            duv_y = vec2<f32>(-duv_y.y, duv_y.x);
        }
        if ((th & 2u) != 0u) {
            t = -t;
            duv_x = -duv_x;
            duv_y = -duv_y;
        }
        if ((th & 4u) != 0u) {
            t.x = -t.x;
            duv_x.x = -duv_x.x;
            duv_y.x = -duv_y.x;
        }
        uv = cell + 0.5 + t;
    }
    var layer = m.y;
    if ((m.w & M_GRAIN_X) != 0u) {
        // A log along X: its ends (+-X) show the rings, the rest the bark.
        if (face < 2u) { layer = m.x; }
    } else if ((m.w & M_GRAIN_Z) != 0u) {
        if (face >= 4u) { layer = m.x; }
    } else {
        if (face == 2u) { layer = m.x; }
        if (face == 3u) { layer = m.z; }
    }
    // Snap to texel centres so magnified texels stay crisp while the hardware
    // still picks mips and filters anisotropically from the true gradients.
    let ts = 32.0;
    // Calm snaps (its flat tiles keep crisp seams); classic filters smoothly.
    let uvs = select(uv, (floor(uv * ts) + 0.5) / ts, g.style.x > 0.5);
    var albedo = textureSampleGrad(tex, samp, uvs, i32(layer), duv_x, duv_y).rgb;
    // Per-voxel brightness and hue jitter so repeated tiles never read as tiles.
    let vox = vec3<i32>(floor((in.nominal - n0 * 0.5 * in.vscale) / in.vscale));
    let h = hash3u(vox);
    let j = f32(h & 255u) / 255.0 - 0.5;
    let j2 = f32((h >> 8u) & 255u) / 255.0 - 0.5;
    let calm = g.style.x > 0.5;
    let jb = select(0.16 * TEX_JITTER, 0.035, calm);
    let jh = select(0.05 * TEX_JITTER, 0.012, calm);
    albedo = albedo * (1.0 + j * jb) * vec3<f32>(1.0 + j2 * jh, 1.0, 1.0 - j2 * jh);
    if (calm) {
        albedo = calm_tint(albedo, mat, in.world);
    } else if ((m.w & M_CLASSIC_PATCH) != 0u || drift_of(mat, 1u).on) {
        albedo = classic_patch(albedo, mat, in.world);
    }
    if (face == 2u && (m.w & M_UNDERSTORY) != 0u && UNDER_ON) {
        albedo = understory(albedo, in.world, max(length(dw_x), length(dw_y)));
    }
    var glow = f32((m.w >> M_GLOW_SHIFT) & 63u) * 0.1;
    if ((m.w & M_GLOW_SPECKS) != 0u) {
        // Points of light on plain rock: a sparse hashed grid in the world
        // (4 cm cells), so they show in the calm style too, which paints
        // broad colour instead of single texels. Only the points glow.
        let cell = vec3<i32>(floor(in.world * 25.0));
        let h = hash3u(cell);
        let speck = select(0.0, 1.0, (h & 1023u) < 18u);
        let tone = 0.6 + 0.4 * f32((h >> 10u) & 255u) / 255.0;
        albedo = mix(albedo, vec3<f32>(0.10, 0.36, 1.0) * tone, speck);
        glow *= speck * 0.9;
    }
    if ((m.w & M_NIGHT_GLOW) != 0u) {
        // Lamplight behind glass: lit from dusk, dark glass by day.
        let lit = smoothstep(0.0, 0.5, g.air_sun.w);
        glow *= lit;
        albedo *= mix(0.3, 1.0, lit);
    }
    leafy = (m.w & M_FOLIAGE_FW) != 0u;
    // A sloped quad leans off its voxel face, by up to about a voxel on a
    // curved shell, and is lit by its own normal: where the sun grazes it
    // the tight bias striped a dome along its shadow line (2026-11-21,
    // Saltmirror). It takes the tight bias only where the sun meets it
    // squarely.
    let squarely = in.sloped == 0u || dot(n_light, g.sun_dir.xyz) > 0.35;
    // Leaves keep the legacy bias: they cast from 0.5 m leaf cells, not the
    // voxels drawn, and a crown's underside streaked even at a voxel's.
    shadow_kind = select(SHADOW_VOXEL, SHADOW_TIGHT, dot(n0, g.sun_dir.xyz) > 0.0 && in.moved == 0u && squarely);
    if (leafy) {
        shadow_kind = SHADOW_LEGACY;
    }
    if (leafy) {
        albedo = albedo * canopy_dapple(in.world);
    }
    // The season, on anything that grows (world/season.rs). Leaves take its
    // colour; grass goes over to straw with the year. Summer leaves both
    // exactly alone.
    if ((m.w & (M_FOLIAGE_FW | M_GROWS | M_CALM_LEAF)) != 0u) {
        albedo = seasoned(albedo);
    } else if ((m.w & (M_UNDERSTORY | M_CALM_MEADOW)) != 0u) {
        albedo = strawed(albedo);
    }
    // What the weather has left (world/damp.rs): wet ground goes dark, and
    // standing water lies on level bare ground. Plants and leaves darken
    // less and never hold a puddle.
    let plant = leafy || (m.w & (M_GROWS | M_CALM_LEAF | M_UNDERSTORY | M_CALM_MEADOW)) != 0u;
    var dmp = Damp(albedo, 0.0, 0.0, 0.0, 0.0);
    let tidal = g.tide.w > 0.5 && in.world.y < g.tide.z + 0.15 && in.world.y > g.fog.y - 0.3;
    if (glow == 0.0 && (g.damp.x > 0.005 || g.damp.y > 0.003 || g.damp.z > 0.01 || tidal)) {
        dmp = damp_ground(albedo, in.world, n_light, plant, (m.w & (M_UNDERSTORY | M_CALM_MEADOW)) != 0u, (m.w & M_FLOODS) != 0u);
        albedo = dmp.albedo;
    }
    // The swash of the surf running up the beach (world/surf.rs): a sheet of
    // water over the sand, and the sand it leaves dark behind it.
    if (g.surf.w > 0.5 && glow == 0.0 && !plant && in.world.y > g.fog.y - 0.1 && in.world.y < g.fog.y + g.surf.x * 1.2 + 0.1) {
        let sw = swash(in.world, g.cam_pos.w);
        let soak = smoothstep(g.fog.y + g.surf.x * 0.5, g.fog.y, in.world.y);
        albedo = pow(max(albedo, vec3<f32>(0.0)), vec3<f32>(1.0 + 0.5 * soak)) * (1.0 - 0.25 * soak);
        dmp.sheen = max(dmp.sheen, max(soak * 0.6, sw.x));
        dmp.pool = max(dmp.pool, sw.x * 0.6 * step(0.9, n_light.y));
        albedo = mix(albedo, vec3<f32>(0.88, 0.91, 0.92), sw.y * 0.85);
    }
    // Snow lying on whatever faces up: the tops of the leaf blocks in a
    // spruce, ledges, roofs, a log. Patchy, thickest on the flattest faces,
    // with more of it the deeper the snow lies (render FrameParams::snow_cover).
    let cover = g.snow.x;
    if (cover > 0.01 && glow == 0.0) {
        let up = smoothstep(0.3, 0.85, n_light.y);
        let lie = vnoise2(in.world.xz * 1.7 + vec2<f32>(in.world.y * 0.9, 0.0)) * 0.55 + vnoise2(in.world.xz * 6.0) * 0.25;
        var s = up * smoothstep(1.05 - cover, 1.25 - cover, lie + up * 0.45);
        // Off the trees, only where the land itself is near level: every
        // voxel step on a cliff has a little top face, and snow on all of
        // them turns a dark wall white.
        if (!leafy) {
            s *= smoothstep(0.82, 0.95, land_normal(in.world.xz).y);
        }
        let snow = select(vec3<f32>(0.86, 0.89, 0.94), vec3<f32>(0.9, 0.92, 0.96), calm);
        albedo = mix(albedo, snow, s);
    }
    var col = light(albedo, in.world, n_light, in.ao, glow);
    if (dmp.sheen > 0.0 || dmp.pool > 0.0 || dmp.frost > 0.0) {
        col = damp_light(col, in.world, n_light, dmp, max(length(dw_x), length(dw_y)));
    }
    if ((m.w & M_GLINTS) != 0u) {
        let fp = max(length(duv_x), length(duv_y));
        col += glass_glints(in.world, n, uvs, face, fp, (m.w & M_GLINTS_SOFT) != 0u);
    }
    // Fog per pixel: terrain is bound by vertex work, so moving the mist into
    // vs_terrain made it slower, not faster. Alpha over 1 marks standing
    // water for the reflections in post.wgsl (`mirror`): every other opaque
    // surface writes exactly 1.
    return vec4<f32>(apply_fog(col, in.world), 1.0 + dmp.pool);
}

// ---- entities --------------------------------------------------------------

struct EntityOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) mat: u32,
    @location(3) ao: f32,
    @location(4) local: vec3<f32>,
    @location(5) @interpolate(flat) inst: u32,
};

fn entity_transform(qv: QuadVert, inst: EntityInst) -> mat2x3<f32> {
    let s = inst.info.x;
    let r = qrot(inst.rot, (qv.local - inst.info.yzw) * s);
    let l = r + flesh_offset(r, inst.flesh);
    let c = cos(inst.pos_yaw.w);
    let sn = sin(inst.pos_yaw.w);
    let world = inst.pos_yaw.xyz + vec3<f32>(l.x * c + l.z * sn, l.y, -l.x * sn + l.z * c);
    let n = qrot(inst.rot, qv.normal);
    let nr = vec3<f32>(n.x * c + n.z * sn, n.y, -n.x * sn + n.z * c);
    return mat2x3<f32>(world, nr);
}

@vertex
fn vs_entity(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> EntityOut {
    let q = equads[vi / 6u];
    let qv = decode_quad(q, vi % 6u);
    let inst = einst[ii];
    let t = entity_transform(qv, inst);
    var out: EntityOut;
    out.clip = g.view_proj * vec4<f32>(t[0], 1.0);
    // shift.w 1: a shadow caster only (ride.rs `stand_in`), never seen.
    if (inst.shift.w > 0.5 && inst.shift.w < 1.5) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    out.world = t[0];
    out.normal = t[1];
    // The palette row: the voxel's slot within its model's block, plus
    // where that block starts (render/mod.rs `set_models` puts it in the
    // top half of the quad's second word).
    out.mat = max(qv.mat, PALETTE_BASE) - PALETTE_BASE + (q.y >> 16u);
    out.ao = qv.ao;
    out.local = qv.local - qv.normal * 0.5;
    out.inst = ii;
    return out;
}

@fragment
fn fs_entity(in: EntityOut) -> @location(0) vec4<f32> {
    return vec4<f32>(entity_colour(in), 1.0);
}

// shift.w 3 and up: drawn see-through (models.rs `GLASS`), the rest its
// opacity. The edges are held more opaque than the middle, so a glass thing
// keeps its outline and does not fade to a smear.
@fragment
fn fs_entity_glass(in: EntityOut) -> @location(0) vec4<f32> {
    let inst = einst[in.inst];
    let a = clamp(inst.shift.w - 3.0, 0.0, 1.0);
    let v = normalize(g.cam_pos.xyz - in.world);
    let rim = 1.0 - abs(dot(normalize(in.normal), v));
    return vec4<f32>(entity_colour(in), clamp(a + 0.4 * rim * rim, 0.0, 1.0));
}

fn entity_colour(in: EntityOut) -> vec3<f32> {
    let p = palette[in.mat];
    let inst = einst[in.inst];
    let h = hash3u(vec3<i32>(floor(in.local)));
    let j = f32(h & 255u) / 255.0 - 0.5;
    var albedo = max(p.rgb * (1.0 + j * select(0.12, 0.03, g.style.x > 0.5)) * inst.tint.rgb + inst.shift.rgb, vec3<f32>(0.0));
    let n = normalize(in.normal);
    // tint.w above 0 flashes white; below 0 dims the glow (blinking fireflies).
    // glow.x: a coat's gain on the glow, less one (models.rs `Instance::glow`).
    let glow = max(p.w, 0.0) * (1.0 + min(inst.tint.w, 0.0)) * max(1.0 + inst.glow.x, 0.0);
    // Below 0 the palette's fourth value is iridescence (a hummingbird's
    // gorget): dusky until the sun's reflection lines up with the eye, then
    // a flash of its own colour, brighter than lit paint can be.
    let irid = max(-p.w, 0.0);
    var flash = 0.0;
    shadow_kind = SHADOW_TIGHT;
    if (irid > 0.0) {
        let sun = g.sun_dir.xyz;
        let ndl = dot(n, sun);
        if (ndl > 0.0) {
            let v = normalize(g.cam_pos.xyz - in.world);
            let hv = normalize(sun + v);
            flash = pow(max(dot(n, hv), 0.0), 10.0) * shadow_factor(in.world, n) * g.sun_color.w;
        }
        albedo = albedo * mix(0.12, 1.0, min(flash, 1.0));
    }
    // Out in the wet (world/damp.rs), fur, feathers and anything else left
    // outside go darker like the ground, less so underneath. Not under the
    // sea, and not anything under the land.
    if (glow == 0.0 && g.damp.x > 0.005 && in.world.y > g.fog.y) {
        let wet = g.damp.x * under_land(in.world) * mix(0.5, 1.0, smoothstep(-0.2, 0.9, n.y));
        albedo = pow(albedo, vec3<f32>(1.0 + 0.4 * wet)) * (1.0 - 0.2 * wet);
    }
    shadow_kind = SHADOW_TIGHT;
    var col = light(albedo, in.world, n, in.ao, glow);
    col += albedo * g.sun_color.rgb * flash * irid;
    col = mix(col, vec3<f32>(1.0, 1.0, 1.0), max(inst.tint.w, 0.0));
    return apply_fog(col, in.world);
}

// ---- sky -------------------------------------------------------------------

struct FsTri {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> FsTri {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: FsTri;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_sky(in: FsTri) -> @location(0) vec4<f32> {
    let w = g.inv_view_proj * vec4<f32>(in.ndc, 0.5, 1.0);
    let dir = normalize(w.xyz / w.w - g.cam_pos.xyz);
    if (g.fog.z > 0.5) {
        // Exactly what the haze turns everything into far off, so the land
        // culled past the underwater fog (render/mod.rs `near_enough`) is
        // not cut out against a darker background.
        return vec4<f32>(haze(g.cam_pos.xyz + dir * 1000.0).rgb, 1.0);
    }
    if (g.snow.y > 0.5) {
        return vec4<f32>(space_sky(dir), 1.0);
    }
    var c = sky_color(dir);
    let night = g.air_sun.w;
    c += stars(dir) * night;
    c += aurora(dir);
    let s = dot(dir, g.sun_dir.xyz);
    // The key light's disk: the sun, or after sunset a smaller, paler moon.
    let disk = mix(smoothstep(0.99955, 0.9998, s), smoothstep(0.99975, 0.99988, s) * 0.5, night);
    c += g.sun_color.rgb * disk * 40.0;
    let cl = clouds(dir);
    var lit = mix(vec3<f32>(0.78, 0.8, 0.86), vec3<f32>(1.0, 0.98, 0.95), max(dot(g.sun_dir.xyz, vec3<f32>(0.0, 1.0, 0.0)), 0.0));
    if (g.style.x > 0.5) {
        lit = lit * vec3<f32>(1.02, 0.97, 0.9);
    }
    // Once the sun is low, clouds take the sky's colours instead of white:
    // lit from below in gold and rose near the sun, lavender and slate away from it.
    let day = smoothstep(0.0, 0.45, g.air_sun.y);
    let st = max(dot(dir, g.air_sun.xyz), 0.0);
    let dusk = g.sky_horizon.rgb * 0.55 + g.sky_ambient.rgb * 0.12 + g.glow.rgb * (pow(st, 3.0) * 1.2 + 0.12);
    lit = mix(dusk, lit * 1.25, day);
    // Night clouds thin to a veil the stars show through.
    c = mix(c, lit, cl * 0.85 * (1.0 - night * 0.45));
    // Standing in a mist bank, the sky fades into it too. How far the ray
    // runs through the bank depends on how steeply it climbs: straight up it
    // is out in a few tens of metres, along the horizon it stays in the mist
    // for kilometres. Measured over a fixed 300 m, the sky just above a far
    // ridge carried far less mist than the ridge itself did, and the two met
    // in a hard pale band lying along the skyline on any misty night.
    let md = clamp(g.mist.z * 5.0 / max(dir.y, 0.015), 300.0, 5000.0);
    c = mix(c, mist_color(dir), mist_amount(g.cam_pos.xyz + dir * md, md));
    return vec4<f32>(c, 1.0);
}

// A sparse field of small stars, each a soft dot about a pixel and a half
// across so it doesn't flicker as the view turns. Thinner near the horizon.
fn stars(dir: vec3<f32>) -> vec3<f32> {
    if (g.air_sun.w < 0.01 || dir.y < 0.0) {
        return vec3<f32>(0.0);
    }
    return star_field(dir, 260.0, 20u) * smoothstep(0.03, 0.3, dir.y);
}

// One layer of stars: cells `scale` to the radian, `keep` of every 256 lit.
fn star_field(dir: vec3<f32>, scale: f32, keep: u32) -> vec3<f32> {
    let q = dir * scale;
    let cell = floor(q);
    let h = hash3u(vec3<i32>(cell));
    if ((h & 255u) > keep) {
        return vec3<f32>(0.0);
    }
    let off = vec3<f32>(f32((h >> 8u) & 255u), f32((h >> 16u) & 255u), f32(h >> 24u)) / 255.0 * 0.5 + 0.25;
    let d = length(q - cell - off);
    let b = exp(-d * d / 0.03) * (0.4 + f32((h >> 4u) & 15u) / 15.0 * 1.6);
    let tint = mix(vec3<f32>(0.8, 0.85, 1.0), vec3<f32>(1.0, 0.9, 0.78), f32((h >> 12u) & 1u));
    return tint * b;
}

// The sky of a map in space (world.json `space`): near-black all the way
// round, below the horizon as well, with two layers of stars, a faint band
// of the galaxy across it, and the moon. No air, so no clouds and no mist.
fn space_sky(dir: vec3<f32>) -> vec3<f32> {
    // The band: a great circle tilted across the sky, brightest along it,
    // mottled so it reads as dust and not a stripe.
    let pole = normalize(vec3<f32>(0.35, 0.55, -0.76));
    let across = dot(dir, pole);
    let mottle = 0.6 + 0.4 * sin(dir.x * 23.0 + sin(dir.z * 17.0) * 2.0) * sin(dir.y * 19.0 + dir.x * 7.0);
    let band = exp(-across * across / 0.02) * mottle;
    var c = vec3<f32>(0.004, 0.005, 0.011) + vec3<f32>(0.035, 0.032, 0.045) * band;
    c += star_field(dir, 260.0, 24u);
    // Fainter, finer stars, many more of them along the band.
    c += star_field(dir, 640.0, u32(10.0 + 40.0 * band)) * 0.35;
    let s = dot(dir, g.sun_dir.xyz);
    c += g.sun_color.rgb * smoothstep(0.99975, 0.99988, s) * 20.0;
    return c;
}

// ---- northern lights (render/aurora.rs) -------------------------------------
//
// Curtains are vertical sheets hanging from a base altitude of 1 (in units of
// that altitude) up to about 3.4, each running roughly east-west some way to
// the north and folded by slow noise. A ray meets a sheet at one altitude,
// found exactly (two passes, since the fold depends on where along the sheet
// the ray lands), so there is no marching and no banding. What the eye sees
// there is green low down with a sharp lower edge and a violet hem, going
// over to red and magenta at the top, broken into fine vertical rays that
// shimmer along the curtain.

fn au_noise(x: f32, row: i32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let a = f32(hash3u(vec3<i32>(i32(i), row, 977)) & 65535u) / 65535.0;
    let b = f32(hash3u(vec3<i32>(i32(i) + 1, row, 977)) & 65535u) / 65535.0;
    return mix(a, b, f * f * (3.0 - 2.0 * f));
}

// How far north (in base altitudes) curtain k hangs where it passes x: the
// broad sweep alone (`au_sweep`), which the ray's crossing is solved
// against, and the finer folds (`au_ripple`), added once at the end. Solving
// against the fine folds as well diverges wherever the sheet slants across
// the view, and the crossing then jumps from pixel to pixel in streaks.
fn au_sweep(x: f32, k: i32, base: f32, t: f32) -> f32 {
    let r = k * 7 + i32(g.aurora.z);
    let big = (au_noise(x * 0.22 + t * 0.004 + f32(k) * 3.7, r) - 0.5) * 2.6;
    return base + f32(k) * 0.85 + big;
}

fn au_ripple(x: f32, k: i32, t: f32) -> f32 {
    let r = k * 7 + i32(g.aurora.z);
    let small = (au_noise(x * 0.9 - t * 0.012, r + 1) - 0.5) * 0.7;
    let kink = (au_noise(x * 3.1 + t * 0.03, r + 2) - 0.5) * 0.12;
    return small + kink;
}

fn aurora(dir: vec3<f32>) -> vec3<f32> {
    let strength = g.aurora.x;
    if (strength < 0.003 || dir.y < 0.0) {
        return vec3<f32>(0.0);
    }
    let t = g.cam_pos.w;
    let y = max(dir.y, 0.02);
    // Low in the north on a quiet night; on a strong one the first arc
    // comes south past the zenith.
    let base = mix(3.2, -0.6, g.aurora.y);
    var col = vec3<f32>(0.0);
    for (var k = 0; k < 3; k++) {
        // The sheet lies along z = -north(x). Where the ray reaches it, at
        // altitude a, the ray has gone (dir.xz / y) * a across the ground.
        if (abs(dir.z) < 1e-4) {
            continue;
        }
        var north = au_sweep(0.0, k, base, t);
        var x = 0.0;
        var step = 1.0;
        for (var it = 0; it < 4; it++) {
            x = -north * dir.x / dir.z;
            let next = au_sweep(x, k, base, t);
            step = abs(next - north);
            north = next;
        }
        x = -north * dir.x / dir.z;
        north += au_ripple(x, k, t);
        let a = -north * y / dir.z;
        if (a < 0.96 || a > 3.6) {
            continue;
        }
        // Not settled, or the sheet turned to run along the line of sight:
        // there it would draw as a hairline streak. Let it go.
        let slope = abs(au_sweep(x + 0.05, k, base, t) - au_sweep(x - 0.05, k, base, t)) * 10.0 * abs(dir.x / dir.z);
        let settled = (1.0 - smoothstep(0.02, 0.15, step)) * (1.0 - smoothstep(0.55, 0.95, slope));
        let h = (a - 1.0) / 2.4;
        // Brightest along the bottom edge, fading upward; the edge itself
        // soft, or it reads as a skyline.
        let edge = smoothstep(0.96, 1.12, a);
        let fall = exp(-h * 2.6) * 0.85 + 0.15 * (1.0 - h);
        // Fine rays, and stretches of the curtain stronger than others.
        let rk = k * 7 + i32(g.aurora.z) + 3;
        // How far along the curtain one pixel reaches here (x = -north *
        // dir.x / dir.z, so it runs away where the sheet is seen edge-on):
        // fine rays finer than a pixel fade to their mean instead of
        // aliasing into streaks.
        let px = abs(north) / max(dir.z * dir.z, 1e-4) * 0.0015;
        let ray = mix(0.55, 0.35 + 0.65 * pow(au_noise(x * 7.0 + t * 0.09, rk), 2.0), clamp(1.0 - px * 14.0, 0.0, 1.0));
        let ray2 = mix(0.8, 0.6 + 0.4 * au_noise(x * 23.0 - t * 0.21, rk + 1), clamp(1.0 - px * 46.0, 0.0, 1.0));
        let along = mix(0.5, smoothstep(0.18, 0.75, au_noise(x * 0.35 - t * 0.008, rk + 2)), clamp(1.0 - px * 1.5, 0.0, 1.0));
        // Where even the folds are finer than a pixel the sheet is all but
        // edge-on to the eye; let it go rather than draw noise.
        let seen = clamp(1.2 - px * 0.6, 0.0, 1.0);
        // Seen edge-on the sheet is thicker along the line of sight.
        let flat = length(dir.xz);
        let edge_on = clamp(flat / max(abs(dir.z), 0.12), 1.0, 3.0);
        // The further curtains are fainter.
        let far = 1.0 / (1.0 + 0.35 * f32(k));
        let e = edge * fall * ray * ray2 * along * edge_on * far * seen * settled;
        // Strongly saturated: the tone curve and the night grade both pale it.
        let green = vec3<f32>(0.02, 1.0, 0.22);
        let red = vec3<f32>(0.9, 0.06, 0.35);
        let hem = vec3<f32>(0.55, 0.3, 1.0);
        var c = mix(green, red, smoothstep(0.32, 0.8, h));
        c = mix(c, hem, (1.0 - smoothstep(0.0, 0.035, h)) * 0.6);
        col += c * e;
    }
    // A broad faint glow low in the north under the curtains.
    let glow = vec3<f32>(0.05, 0.32, 0.16) * exp(-dir.y * 5.0) * smoothstep(0.1, -0.6, dir.z);
    return (col * 1.4 + glow * 0.5) * strength * smoothstep(0.0, 0.06, dir.y);
}

// ---- water -----------------------------------------------------------------

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // River surface: bed height, current (x, z), 1 (2: foam only, over the
    // sea round a waterfall's foot). The sea: 0, 0, 0, 0.
    @location(1) river: vec4<f32>,
    // The spray record (index + 1) of a waterfall landing near; 0 for none.
    @location(2) @interpolate(flat) fall: u32,
    // The sea's second plane, at high water over the hollows the open sea
    // cannot reach (with a tide only); 0 for the open sea and rivers.
    @location(3) @interpolate(flat) basin: u32,
};

// River surface cells (world/rivers.rs, render::RiverQuad).
struct RiverQuad {
    at: vec4<f32>,
    level: vec4<f32>,
    bed: vec4<f32>,
    flow_x: vec4<f32>,
    flow_z: vec4<f32>,
    ebb: vec4<f32>,
};
@group(0) @binding(12) var<storage, read> rivers: array<RiverQuad>;

// Where each waterfall lands (render::Spray): the splash point and the
// fall's width; the way the water leaves the lip (x, z), the drop, and how
// hard it lands.
struct Spray {
    at: vec4<f32>,
    dir: vec4<f32>,
};
@group(0) @binding(20) var<storage, read> sprays: array<Spray>;

@vertex
fn vs_river(@builtin(vertex_index) vi: u32) -> WaterOut {
    let q = rivers[vi / 6u];
    var order = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    var dx = array<f32, 4>(0.0, 1.0, 1.0, 0.0);
    var dz = array<f32, 4>(0.0, 0.0, 1.0, 1.0);
    let c = order[vi % 6u];
    // Near the sea a river falls with the tide (world/tide.rs).
    let level = q.level[c] - river_fall(q.level[c], q.ebb[c]);
    let world = vec3<f32>(q.at.x + dx[c], level - 0.03, q.at.y + dz[c]);
    var out: WaterOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    // A cell at the sea's level draws only foam over the sea's own surface,
    // unless the tide has gone out below it and left a stream running there
    // (a channel on past the river's mouth).
    let over_sea = q.at.z > 0.5 && level < g.fog.y + 0.03;
    out.river = vec4<f32>(q.bed[c], q.flow_x[c], q.flow_z[c], select(1.0, 2.0, over_sea));
    out.fall = u32(q.at.w + 0.5);
    out.basin = 0u;
    return out;
}

@vertex
fn vs_water(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> WaterOut {
    let c = quad_corner(vi);
    let r = 4000.0;
    // Sea level falls on a voxel boundary, so shoreline top faces would share the
    // water's plane and z-fight (blinking at the water's edge). The smoothed
    // ground moves its corners in quarter-voxel steps (6.25 cm), so a flat beach
    // at sea level has corners at 0 and -6.25 cm: at -5 cm the water lay a
    // centimetre from those and flickered through the sand at a distance
    // (Ribshore's beach). Midway between the first and second step keeps every
    // corner at least 3 cm clear of it. A tide moves the sea between steps,
    // so the plane is kept on that midway lattice and goes down a step at a
    // time. Instance 1 is the plane at high water over the hollows the open
    // sea cannot reach (drawn with a tide only).
    let top = select(g.fog.y, g.tide.z, ii == 1u);
    var y = (round((top - 0.09375) / 0.0625 - 0.5) + 0.5) * 0.0625;
    var xz = g.cam_pos.xz + c * r;
    if (ii == 0u && g.swell.z > 0.0) {
        // With a swell the open sea is a mesh of SEA_GRID x SEA_GRID round
        // the camera, fine near it and coarse toward the horizon (a cubic in
        // the grid coordinate), lifted by the swell. Its middle sits on whole
        // metres so it does not crawl over the waves as the camera moves.
        let cells = SEA_GRID - 1u;
        let cell = vi / 6u;
        let gp = vec2<f32>(f32(cell % cells), f32(cell / cells)) + c * 0.5 + 0.5;
        let u = gp / f32(cells) * 2.0 - 1.0;
        let a = 0.025;
        xz = floor(g.cam_pos.xz) + r * (a * u + (1.0 - a) * u * u * u);
        let step = r * (a + 3.0 * (1.0 - a) * u * u) * (2.0 / f32(cells));
        let spacing = max(step.x, step.y);
        if (spacing < g.swell.w * 0.25) {
            y += swell(xz, g.cam_pos.w, top - ground_height(xz), spacing, shore_at(xz).y).x;
        }
    }
    let world = vec3<f32>(xz.x, y, xz.y);
    var out: WaterOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    // A map in space has no sea, however far down: draw nothing.
    if (g.snow.y > 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    out.world = world;
    out.river = vec4<f32>(0.0);
    out.fall = 0u;
    out.basin = ii;
    return out;
}

fn ground_height(xz: vec2<f32>) -> f32 {
    let size = g.fog.w;
    let dims = vec2<f32>(textureDimensions(heightmap));
    let p = xz / size * dims - 0.5;
    if (any(p < vec2<f32>(0.0)) || any(p > dims - 1.0)) {
        return g.fog.y - 10.0;
    }
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    let a = textureLoad(heightmap, i, 0).r;
    let b = textureLoad(heightmap, i + vec2<i32>(1, 0), 0).r;
    let c = textureLoad(heightmap, i + vec2<i32>(0, 1), 0).r;
    let d = textureLoad(heightmap, i + vec2<i32>(1, 1), 0).r;
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// A hollow below sea level the open sea cannot reach (heightmap g, from
// `Fields::basin_grid`): with a tide its water stays at high water.
fn basin_at(xz: vec2<f32>) -> bool {
    let dims = vec2<i32>(textureDimensions(heightmap));
    let p = vec2<i32>(floor(xz / g.fog.w * vec2<f32>(dims)));
    if (any(p < vec2<i32>(0)) || any(p >= dims)) {
        return false;
    }
    return textureLoad(heightmap, p, 0).g > 0.5;
}

// ---- surf (world/surf.rs) --------------------------------------------------

// Metres seaward of the high-water coast (x) and how open the sea is there
// (y, 0 in a lagoon to 1 off an open coast): heightmap b and a, bilinear.
fn shore_at(xz: vec2<f32>) -> vec2<f32> {
    let dims = vec2<f32>(textureDimensions(heightmap));
    let p = clamp(xz / g.fog.w * dims - 0.5, vec2<f32>(0.0), dims - 1.001);
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    let a = textureLoad(heightmap, i, 0).ba;
    let b = textureLoad(heightmap, i + vec2<i32>(1, 0), 0).ba;
    let c = textureLoad(heightmap, i + vec2<i32>(0, 1), 0).ba;
    let d = textureLoad(heightmap, i + vec2<i32>(1, 1), 0).ba;
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// The waves at a point whose `shore_at` is `sh`: how high they are here (x,
// metres) and where in its cycle the wave is (y: 0 as a crest passes, rising
// to 1 as the next one comes). Crests follow the coast and travel in toward
// it. Some waves come bigger than others, in sets, and the size changes
// along the shore too, so the lines break in sections rather than all at
// once.
fn surf_at(xz: vec2<f32>, t: f32, sh: vec2<f32>) -> vec2<f32> {
    let len = g.surf.y;
    let period = g.surf.z;
    // The crests' wobble is baked into the grid (surf::shore_grid).
    let s = sh.x;
    let phase = fract(s / len + t / period);
    // Which wave this is: sets of bigger ones every seven or so.
    let wave = floor(s / len + t / period);
    let big = 0.6 + 0.4 * sin(wave * 0.9);
    let section = 0.55 + 0.75 * vnoise2(xz * 0.018 + vec2<f32>(wave * 1.7, wave * 0.6));
    let wind = clamp(g.weather.w, 0.5, 2.0);
    let h = g.surf.x * wind * big * section * sh.y;
    return vec2<f32>(h, phase);
}

// Vertices along a side of the open sea's mesh with a swell (render
// SEA_GRID, the same).
const SEA_GRID: u32 = 192u;

// The swell at `xz` (world/surf.rs `swell`, the same): x = how far it lifts
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

// Lacy foam: the noise's ridges, broken up, drifting in with the swell
// (which runs toward the land).
fn surf_lace(xz: vec2<f32>, t: f32) -> f32 {
    let p = xz - g.swell.xy * t * 0.6;
    let a = vnoise2(p * 0.55) * 0.55 + vnoise2(p * 1.7 + vec2<f32>(3.1, 7.7)) * 0.3 + vnoise2(p * 4.3) * 0.15;
    return a;
}

// The swash on the beach after a wave breaks: the water runs up the sand
// fast, then slides back slowly. x = how much a thin sheet of water covers
// ground at `world`, y = the foam along its leading edge.
fn swash(world: vec3<f32>, t: f32) -> vec2<f32> {
    let sf = surf_at(world.xz, t, shore_at(world.xz));
    if (sf.x < 0.02) {
        return vec2<f32>(0.0);
    }
    let f = sf.y;
    let up = smoothstep(0.0, 0.16, f) * (1.0 - smoothstep(0.22, 0.8, f));
    let reach = sf.x * 0.4 * up * (0.75 + 0.5 * vnoise2(world.xz * 0.12));
    let above = world.y - g.fog.y;
    let cover = smoothstep(reach + 0.015, reach - 0.025, above) * step(0.005, reach);
    // Froth rides the front while it climbs, then thins as it drains.
    let lace = surf_lace(world.xz, t);
    let edge = cover * smoothstep(reach - 0.09, reach - 0.01, above) * smoothstep(0.35, 0.6, lace) * (1.0 - 0.6 * smoothstep(0.2, 0.5, f));
    return vec2<f32>(cover, edge);
}

// Ripples from scrolling value noise at three scales; each layer fades out once
// its features shrink below a few pixels, so the far sea doesn't alias.
fn wave_normal(p: vec2<f32>, t: f32, footprint: f32) -> vec3<f32> {
    var g = vec2<f32>(0.0);
    var scales = array<f32, 4>(0.045, 0.18, 0.55, 1.6);
    var amps = array<f32, 4>(0.75, 0.9, 0.45, 0.22);
    var drift = array<vec2<f32>, 4>(vec2<f32>(0.12, 0.07), vec2<f32>(0.35, 0.2), vec2<f32>(-0.3, 0.45), vec2<f32>(0.5, -0.4));
    for (var i = 0; i < 4; i++) {
        let s = scales[i];
        let size = 1.0 / s;
        let keep = 1.0 - smoothstep(size * 0.08, size * 0.25, footprint);
        if (keep <= 0.0) {
            continue;
        }
        let q = p * s + drift[i] * t * s * 2.0 + vec2<f32>(f32(i) * 17.0);
        let e = 0.15;
        let c = vnoise2(q);
        let gx = (vnoise2(q + vec2<f32>(e, 0.0)) - c) / e;
        let gz = (vnoise2(q + vec2<f32>(0.0, e)) - c) / e;
        g += vec2<f32>(gx, gz) * amps[i] * keep * 0.12;
    }
    return normalize(vec3<f32>(-g.x, 1.0, -g.y));
}

@fragment
fn fs_water(in: WaterOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let river = in.river.w > 0.5;
    // With a tide the open sea's plane leaves out the hollows the sea cannot
    // reach (heightmap g), and the high-water plane draws only those.
    if (!river && g.tide.w > 0.5 && (in.basin == 1u) != basin_at(in.world.xz)) {
        discard;
    }
    let t = g.cam_pos.w;
    let view = normalize(in.world - g.cam_pos.xyz);
    let dist = length(in.world - g.cam_pos.xyz);
    // World size of one pixel here, from the screen-space rate of change.
    let footprint = max(length(dpdx(in.world)), length(dpdy(in.world)));
    // The shore grid (surf, swell), read once, and the way out to sea from
    // its rate of change across the screen (derivatives want this outside
    // any branch).
    let sh = shore_at(in.world.xz);
    let dxz = dpdx(in.world.xz);
    let dyz = dpdy(in.world.xz);
    let ds = vec2<f32>(dpdx(sh.x), dpdy(sh.x));
    let det = dxz.x * dyz.y - dxz.y * dyz.x;
    let sgrad = select(vec2<f32>(0.0), vec2<f32>(dyz.y * ds.x - dxz.y * ds.y, dxz.x * ds.y - dyz.x * ds.x) / det, abs(det) > 1e-10);
    let seaward = select(vec2<f32>(0.0), normalize(sgrad), dot(sgrad, sgrad) > 1e-8);
    let calm = g.style.x > 0.5;
    var n = wave_normal(in.world.xz, t, footprint);
    if (river) {
        // Ripples carried by the current: two copies of the pattern slide
        // downstream half a cycle apart, each faded out as it resets.
        let flow = in.river.yz;
        let ph = fract(t * 0.4);
        let ph2 = fract(t * 0.4 + 0.5);
        let n0 = wave_normal(in.world.xz - flow * ph * 2.5, t, footprint);
        let n1 = wave_normal(in.world.xz - flow * ph2 * 2.5 + vec2<f32>(3.7, 1.9), t, footprint);
        let w0 = 1.0 - abs(2.0 * ph - 1.0);
        n = normalize(n0 * w0 + n1 * (1.0 - w0) + vec3<f32>(0.0, 0.35, 0.0));
    }
    if (calm) {
        // Calmer water: less ripple slope, but not glass.
        n = normalize(vec3<f32>(n.x * 0.7, n.y, n.z * 0.7));
    }
    if (g.fog.z > 0.5 && g.cam_pos.y < in.world.y + 0.1) {
        // Seen from below. Inside Snell's window (48.6 degrees either side of
        // straight up) the sky shows through, squeezed and rippled, with a
        // bright rim; past it the surface is a mirror of the dark water.
        let up = view.y;
        let window = smoothstep(0.62, 0.70, up);
        let bent = normalize(vec3<f32>(view.x * 0.75 + n.x * 2.5, max(view.y, 0.05), view.z * 0.75 + n.z * 2.5));
        let sky = sky_color(bent) * (1.0 - 0.85 * g.air_sun.w) + g.sun_color.rgb * g.sun_color.w * pow(max(dot(bent, g.sun_dir.xyz), 0.0), 60.0) * 2.0;
        let rim = exp(-pow((up - 0.66) * 30.0, 2.0)) * 0.35;
        // The waves focus and spread the light coming through.
        let rip = 1.0 - abs(uw_noise(in.world.xz * 0.8 + vec2<f32>(t * 0.25, t * 0.17), 41) * 2.0 - 1.0);
        // Past the window the surface mirrors the water below it, broken
        // into bright and dark bands by the waves, so it reads as a surface
        // and not as more fog.
        let tilt = clamp((n.x * 0.8 + n.z * 0.6) * 5.0, -1.0, 1.0);
        let mirror = underwater_color() * (0.8 + 0.35 * tilt) + uw_shallow() * 0.25 * pow(rip, 4.0);
        var c = mix(mirror, sky * mix(uw_shallow(), vec3<f32>(1.0), 0.6) * (0.78 + 0.6 * pow(rip, 5.0)), window) + uw_shallow() * rim;
        // Froth floating on top: lit from above, so pale from below too.
        let froth = select(surface_froth(in.world.xz, t), 0.0, river);
        let lit = g.sun_color.rgb * g.sun_color.w * 0.35 + g.sky_ambient.rgb * 0.5;
        c = mix(c, vec3<f32>(0.86, 0.93, 0.95) * lit, froth * 0.85);
        return vec4<f32>(apply_fog(c, in.world), max(mix(0.95, 0.85, window), froth));
    }
    let top = select(g.fog.y, g.tide.z, in.basin == 1u);
    let depth = select(max(top - ground_height(in.world.xz), 0.0), max(in.world.y - in.river.x, 0.0), river);
    // Surf (world/surf.rs): the swell shows as moving bands of slope that
    // steepen over the shallows; where the water is about as shallow as the
    // wave is high it breaks, white on its face and crest, with foam left
    // behind it that fades before the next one comes.
    // The swell's own slope, smooth at every distance whatever the mesh holds.
    if (!river && in.basin == 0u && g.swell.z > 0.0) {
        let sw = swell(in.world.xz, t, depth, footprint * 2.0, sh.y);
        n = normalize(n + vec3<f32>(-sw.y, 0.0, -sw.z));
    }
    var surf_foam = 0.0;
    if (!river && in.basin == 0u && g.surf.w > 0.5 && g.water[1].w < 1.5) {
        let sf = surf_at(in.world.xz, t, sh);
        let h = sf.x;
        if (h > 0.01) {
            let ang = sf.y * 6.2831853;
            let shoal = smoothstep(h * 6.0, h * 1.2, depth);
            let slope = h * 3.14159 / g.surf.y * sin(ang) * (0.12 + 1.6 * shoal) * (1.0 - smoothstep(0.6, 3.0, footprint));
            n = normalize(n + vec3<f32>(seaward.x, 0.0, seaward.y) * slope);
            let breaking = smoothstep(h * 1.5, h * 0.8, depth) * smoothstep(0.0, 0.12, depth);
            let bore = max(smoothstep(0.78, 0.985, sf.y), exp(-sf.y * 6.0));
            let lace = surf_lace(in.world.xz, t);
            // Solid white on the breaking face, opening into lace behind it.
            let white = smoothstep(0.62 - 0.35 * bore, 0.72 - 0.3 * bore, lace);
            // Inside the break the whole inner zone stays churned a little.
            let churn = smoothstep(h * 0.9, h * 0.3, depth) * smoothstep(0.5, 0.7, lace) * 0.4;
            surf_foam = breaking * max(white * (0.35 + 0.65 * bore), churn);
        }
    }
    let cosv = max(dot(-view, n), 0.0);
    // Still, murky water (world water `still`): less sky, no shore foam.
    let still = g.water[1].w > 1.5;
    // In still water the trees and banks would be what it mirrors, darker
    // than the sky: a quarter of the sky, and that dimmed.
    let fres = (0.02 + 0.98 * pow(1.0 - cosv, 5.0)) * select(1.0, 0.28, still);
    var refl = sky_color(reflect(view, n));
    if (g.aurora.x > 0.003) {
        refl += aurora(reflect(view, n)) * 1.4;
    }
    if (g.water[1].w > 1.5) {
        refl *= 0.6;
    }
    var shallow = select(vec3<f32>(0.10, 0.38, 0.36), vec3<f32>(0.10, 0.37, 0.38), calm);
    var deep = select(vec3<f32>(0.015, 0.075, 0.13), vec3<f32>(0.02, 0.09, 0.15), calm);
    // The world's own water colour, and biomes that colour theirs (mist map b, a).
    if (g.water[0].w > 0.5) {
        shallow = g.water[0].rgb;
        deep = g.water[1].rgb;
    }
    let wm = textureSampleLevel(mistmap, samp, clamp(in.world.xz / g.fog.w, vec2<f32>(0.002), vec2<f32>(0.998)), 0.0);
    if (g.water[2].w > 0.5) {
        shallow = mix(shallow, g.water[2].rgb, wm.b);
        deep = mix(deep, g.water[3].rgb, wm.b);
    }
    if (g.water[4].w > 0.5) {
        shallow = mix(shallow, g.water[4].rgb, wm.a);
        deep = mix(deep, g.water[5].rgb, wm.a);
    }
    let sun = g.sun_dir.xyz;
    // Lit by what is actually in the sky. The fill follows the sky's own
    // ambient instead of a fixed floor, or a shallow flood glows white
    // under a night sky darker than the water.
    let amb = max(max(g.sky_ambient.r, g.sky_ambient.g), g.sky_ambient.b);
    let lightk = max(dot(sun, vec3<f32>(0.0, 1.0, 0.0)), 0.0) * g.sun_color.w * 0.45 + clamp(amb * 0.9, 0.05, 0.35);
    if (river) {
        // River water: clear and a little green over its stones, never the
        // sea's turquoise shallows.
        shallow = mix(shallow, vec3<f32>(0.05, 0.13, 0.10), 0.6);
        deep = mix(deep, vec3<f32>(0.02, 0.06, 0.06), 0.5);
    }
    var body = mix(shallow, deep, 1.0 - exp(-depth * select(0.22, 0.6, river))) * lightk;
    let spec = pow(max(dot(reflect(view, n), sun), 0.0), 260.0) * g.sun_color.rgb * 6.0;
    var col = mix(body, refl, fres) + spec;
    // No glints on the thin film where the plane runs just over low shore
    // land: only where there is real water under them.
    let glint = water_glints(in.world.xz, t, view, n, max(0.18, footprint * 2.0)) * smoothstep(0.08, 0.35, depth);
    col += g.sun_color.rgb * g.sun_color.w * glint * 14.0;
    // Clear enough to see fish a couple of metres down; rivers clearer still.
    var alpha = clamp(1.0 - exp(-depth * select(select(0.75, 0.5, river), 2.6, still)), 0.0, 1.0);
    alpha = max(alpha, min(glint * 4.0, 1.0));
    alpha = max(alpha, fres);
    // Foam along the shore.
    var fn_ = f32(hash3u(vec3<i32>(vec3<f32>(floor(in.world.x * 4.0), 0.0, floor(in.world.z * 4.0)))) & 255u) / 255.0;
    if (calm) {
        // A smooth foam line instead of a speckled one.
        fn_ = vnoise2(in.world.xz * 0.6);
    }
    let foam = select(1.0, 0.45, river) * smoothstep(select(0.45, 0.18, river), 0.0, depth + (fn_ - 0.5) * 0.25 + sin(t * 1.3 + in.world.x * 0.7) * 0.06);
    var foam_all = select(foam, 0.0, still);
    if (!river && !still) {
        // Froth drifting on open water, fading where the plane runs over
        // land and with distance as it shrinks under a pixel.
        let froth = surface_froth(in.world.xz, t) * smoothstep(0.1, 0.6, depth) * (1.0 - smoothstep(0.3, 1.2, footprint));
        foam_all = max(foam_all, froth * 0.42);
    }
    if (river) {
        // White water over the rapids: patches of foam carried down by the
        // current, two copies half a cycle apart like the ripples.
        let v = in.river.yz;
        let sp = length(v);
        let ph = fract(t * 0.4);
        let w0 = 1.0 - abs(2.0 * ph - 1.0);
        let a = vnoise2((in.world.xz - v * ph * 2.5) * 1.6) * 0.7 + vnoise2((in.world.xz - v * ph * 2.5) * 4.0) * 0.35;
        let b = vnoise2((in.world.xz - v * fract(t * 0.4 + 0.5) * 2.5 + vec2<f32>(5.3, 2.1)) * 1.6) * 0.7
            + vnoise2((in.world.xz - v * fract(t * 0.4 + 0.5) * 2.5 + vec2<f32>(5.3, 2.1)) * 4.0) * 0.35;
        let froth = smoothstep(0.6, 0.85, a * w0 + b * (1.0 - w0));
        foam_all = max(foam_all, smoothstep(1.0, 1.5, sp) * froth * 0.28);
        // Churned white round the foot of a waterfall.
        if (in.fall > 0u) {
            let spot = sprays[in.fall - 1u].at;
            let churn = clamp(1.0 - distance(in.world.xz, spot.xz) / (spot.w * 0.6 + 2.5), 0.0, 1.0);
            let boil = smoothstep(0.3, 0.75, a * w0 + b * (1.0 - w0));
            foam_all = max(foam_all, churn * (boil * 0.75 + churn * 0.45));
        }
    }
    foam_all = max(foam_all, surf_foam);
    col = mix(col, vec3<f32>(0.92, 0.95, 0.96) * lightk * 1.6, foam_all * 0.8);
    alpha = max(alpha, foam_all * 0.85);
    if (in.river.w > 1.5) {
        // Over the sea's own surface: only a fall's churn.
        col = vec3<f32>(0.92, 0.95, 0.96) * lightk * 1.6;
        alpha = select(0.0, foam_all * 0.85, in.fall > 0u);
    }
    return vec4<f32>(apply_fog(col, in.world), alpha);
}

// ---- waterfalls (world/falls.rs, world/rivers.rs) -----------------------------
// Both read the river buffer after its surface cells: first each sheet's
// quads (corners as x, y, z, seconds since the lip in at, level, bed, flow_x;
// offsets across the sheet, -1 to 1, in flow_z), then one record per fall
// for its spray (at = where it lands and its width; level = direction it
// leaves the lip, drop, how hard it lands).

struct FallOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // Seconds since the lip, offset across (-1 to 1).
    @location(1) info: vec2<f32>,
};

@vertex
fn vs_fall(@builtin(vertex_index) vi: u32) -> FallOut {
    let q = rivers[vi / 6u];
    var order = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let c = order[vi % 6u];
    var p = q.at;
    if (c == 1u) {
        p = q.level;
    } else if (c == 2u) {
        p = q.bed;
    } else if (c == 3u) {
        p = q.flow_x;
    }
    // A fall into the sea reaches down to it as the tide goes out: its foot
    // is the only point at high water.
    if (g.tide.w > 0.5 && p.y < g.tide.z + 0.05) {
        p.y -= g.tide.x;
    }
    var out: FallOut;
    out.clip = g.view_proj * vec4<f32>(p.xyz, 1.0);
    out.world = p.xyz;
    out.info = vec2<f32>(p.w, q.flow_z[c]);
    return out;
}

@fragment
fn fs_fall(in: FallOut) -> @location(0) vec4<f32> {
    let t = g.cam_pos.w;
    let secs = in.info.x;
    let u = in.info.y;
    // Streaks carried down at the water's own speed: the pattern is laid out
    // by the time the water has been falling, so it stretches as it speeds up.
    let s = secs - t;
    let a = vnoise2(vec2<f32>(u * 7.0, s * 5.0));
    let b = vnoise2(vec2<f32>(u * 19.0 + 3.1, s * 12.0));
    let streak = a * 0.65 + b * 0.35;
    // Glassy as it leaves the lip, broken white further down.
    let white = clamp(smoothstep(0.42, 0.72, streak) * smoothstep(0.0, 0.25, secs) + smoothstep(0.1, 1.2, secs) * 0.55, 0.0, 1.0);
    // Ragged, thinning edges.
    let rag = vnoise2(vec2<f32>(u * 3.0 + 7.0, s * 4.0));
    let edge = smoothstep(0.0, 0.15 + 0.3 * rag, 1.0 - abs(u));
    let alpha = mix(0.4, 0.92, white) * edge;
    if (alpha < 0.01) {
        discard;
    }
    let view = normalize(in.world - g.cam_pos.xyz);
    var n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    if (dot(n, view) > 0.0) {
        n = -n;
    }
    let sun = g.sun_dir.xyz;
    // Foam scatters light every way, so the sun reaches it on either side.
    let sh = shadow_factor(in.world, n);
    let direct = g.sun_color.rgb * g.sun_color.w * (0.35 + 0.65 * abs(dot(n, sun))) * sh * 0.75;
    let ambient = g.sky_ambient.rgb * 0.8 + g.ground_color.rgb * 0.2;
    let albedo = mix(vec3<f32>(0.10, 0.25, 0.23), vec3<f32>(0.92, 0.95, 0.96), white);
    var col = albedo * (direct + ambient);
    let fres = 0.02 + 0.98 * pow(1.0 - max(dot(-view, n), 0.0), 5.0);
    col = mix(col, sky_color(reflect(view, n)), fres * (1.0 - white) * 0.6);
    return vec4<f32>(apply_fog(col, in.world), alpha);
}

const SPRAY: u32 = 96u;
// The first PUFFS pieces of each fall's spray are mist puffs, the rest droplets.
const PUFFS: u32 = 30u;

struct SprayOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) world: vec3<f32>,
    // Opacity, 1 for a droplet (0 for a puff).
    @location(2) look: vec2<f32>,
};

@vertex
fn vs_spray(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> SprayOut {
    let q = sprays[ii / SPRAY];
    let k = ii % SPRAY;
    let corner = quad_corner(vi);
    var foot = q.at.xyz;
    // A fall into the sea lands on it wherever the tide has it.
    if (g.tide.w > 0.5 && foot.y < g.tide.z + 0.05) {
        foot.y -= g.tide.x;
    }
    let width = q.at.w;
    let fwd = vec3<f32>(q.dir.x, 0.0, q.dir.y);
    let across = vec3<f32>(-q.dir.y, 0.0, q.dir.x);
    let power = q.dir.w;
    let h = hash3u(vec3<i32>(i32(ii), 41, 7));
    let h2 = hash3u(vec3<i32>(i32(ii), 13, 29));
    let r0 = f32(h & 1023u) / 1023.0;
    let r1 = f32((h >> 10u) & 1023u) / 1023.0;
    let r2 = f32((h >> 20u) & 1023u) / 1023.0;
    let r3 = f32(h2 & 1023u) / 1023.0;
    let t = g.cam_pos.w;
    let puff = k < PUFFS;
    var pos: vec3<f32>;
    var size: f32;
    var a: f32;
    if (puff) {
        // Mist rolling out from where the water lands, rising and spreading.
        let life = 5.0 + r3 * 4.0;
        let age = fract(t / life + r0);
        let secs = age * life;
        let start = foot + across * (r1 - 0.5) * width + fwd * (r2 - 0.2) * width * 0.4;
        let out = normalize(fwd + across * (r1 - 0.5) * 1.6);
        pos = start + out * secs * (0.35 + 0.45 * power) + vec3<f32>(0.0, 0.5 + secs * (0.25 + 0.2 * power), 0.0);
        size = (0.8 + 3.2 * age) * (0.6 + 0.4 * power) * clamp(width / 5.0, 0.6, 1.6);
        a = 0.2 * sin(3.14159 * age) * clamp(power, 0.2, 1.0);
    } else {
        // Droplets thrown up and out, falling back.
        let life = 0.6 + r3 * 0.7;
        let age = fract(t / life + r0);
        let secs = age * life;
        let ang = r1 * 6.2831853;
        let radial = vec3<f32>(cos(ang), 0.0, sin(ang));
        let v = radial * (0.6 + 1.8 * r2) * power + fwd * 0.6 + vec3<f32>(0.0, (1.8 + 2.6 * r2) * (0.6 + 0.4 * min(power, 1.0)), 0.0);
        let start = foot + across * (fract(r0 * 7.31) - 0.5) * width * 0.9;
        pos = start + v * secs + vec3<f32>(0.0, -4.9 * secs * secs, 0.0);
        size = 0.035 + 0.04 * r2;
        a = 0.8 * (1.0 - age * age) * step(foot.y, pos.y);
    }
    let to_cam = g.cam_pos.xyz - pos;
    let dist = length(to_cam);
    a *= 1.0 - smoothstep(select(45.0, 160.0, puff), select(70.0, 260.0, puff), dist);
    var out: SprayOut;
    out.corner = corner;
    out.world = pos;
    out.look = vec2<f32>(a, select(1.0, 0.0, puff));
    if (a < 0.003) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    let f = to_cam / max(dist, 1e-3);
    let fb = billboard(f);
    let right = fb.right;
    let up = fb.up;
    // A droplet never shrinks below about a pixel; a puff is drawn forward of
    // its middle so it cuts less into the rock behind.
    if (!puff) {
        size = max(size, dist * 0.0012);
    }
    let centre = pos + f * select(0.0, min(size * 0.8, dist * 0.5), puff);
    let world = centre + (right * corner.x + up * corner.y) * size;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    return out;
}

@fragment
fn fs_spray(in: SprayOut) -> @location(0) vec4<f32> {
    let d = dot(in.corner, in.corner);
    if (d > 1.0) {
        discard;
    }
    let view = normalize(in.world - g.cam_pos.xyz);
    if (in.look.y < 0.5) {
        // A puff: mist's own colour, soft all the way out.
        let a = in.look.x * (exp(-d * 3.0) - 0.05) / 0.95;
        return vec4<f32>(apply_fog(mist_color(view) * 1.1, in.world), max(a, 0.0));
    }
    let sh = shadow_factor(in.world, -view);
    let light = g.sun_color.rgb * g.sun_color.w * (0.3 + 0.7 * sh) * 0.8 + g.sky_ambient.rgb;
    return vec4<f32>(apply_fog(vec3<f32>(0.9, 0.94, 0.96) * light, in.world), in.look.x * (1.0 - d));
}

// ---- overlay lines (render/overlay.rs) --------------------------------------

struct OverlayLine {
    a: vec4<f32>,      // xyz, width in metres
    b: vec4<f32>,
    color: vec4<f32>,  // linear RGBA
};
@group(0) @binding(11) var<storage, read> overlay: array<OverlayLine>;

struct OverlayOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// A ribbon between the line's ends, turned to face the camera: six vertices
// per instance.
@vertex
fn vs_overlay(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> OverlayOut {
    let l = overlay[ii];
    var t = array<f32, 6>(0.0, 1.0, 1.0, 0.0, 1.0, 0.0);
    var s = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let p = mix(l.a.xyz, l.b.xyz, t[vi]);
    let along = l.b.xyz - l.a.xyz;
    let side_dir = cross(along, g.cam_pos.xyz - p);
    let side = side_dir / max(length(side_dir), 1e-6) * l.a.w * 0.5 * s[vi];
    var out: OverlayOut;
    out.clip = g.view_proj * vec4<f32>(p + side, 1.0);
    out.color = l.color;
    return out;
}

@fragment
fn fs_overlay(in: OverlayOut) -> @location(0) vec4<f32> {
    return in.color;
}

// ---- highlight box ---------------------------------------------------------

@vertex
fn vs_box(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // 12 edges, 2 vertices each.
    var e = array<vec3<f32>, 24>(
        vec3<f32>(0.,0.,0.), vec3<f32>(1.,0.,0.), vec3<f32>(1.,0.,0.), vec3<f32>(1.,0.,1.),
        vec3<f32>(1.,0.,1.), vec3<f32>(0.,0.,1.), vec3<f32>(0.,0.,1.), vec3<f32>(0.,0.,0.),
        vec3<f32>(0.,1.,0.), vec3<f32>(1.,1.,0.), vec3<f32>(1.,1.,0.), vec3<f32>(1.,1.,1.),
        vec3<f32>(1.,1.,1.), vec3<f32>(0.,1.,1.), vec3<f32>(0.,1.,1.), vec3<f32>(0.,1.,0.),
        vec3<f32>(0.,0.,0.), vec3<f32>(0.,1.,0.), vec3<f32>(1.,0.,0.), vec3<f32>(1.,1.,0.),
        vec3<f32>(1.,0.,1.), vec3<f32>(1.,1.,1.), vec3<f32>(0.,0.,1.), vec3<f32>(0.,1.,1.));
    let pad = 0.004;
    let lo = g.box_min.xyz - pad;
    let hi = g.box_max.xyz + pad;
    let p = lo + (hi - lo) * e[vi];
    return g.view_proj * vec4<f32>(p, 1.0);
}

@fragment
fn fs_box() -> @location(0) vec4<f32> {
    return vec4<f32>(0.02, 0.02, 0.02, 0.75);
}

// ---- plates: sign boards and their lettering (render/plates.rs) ------------

struct Plate {
    pos: vec4<f32>,    // centre xyz, w: 0 flat colour, 1 font glyph
    right: vec4<f32>,  // half the width, as a vector
    up: vec4<f32>,     // half the height, as a vector
    color: vec4<f32>,  // linear RGBA
    cell: vec4<u32>,   // atlas cell x, y, width, height
};
@group(0) @binding(15) var<storage, read> plates: array<Plate>;
@group(0) @binding(16) var font_atlas: texture_2d<f32>;
@group(0) @binding(17) var font_samp: sampler;

struct PlateOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) texel: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) glyph: u32,
    @location(4) @interpolate(flat) normal: vec3<f32>,
};

@vertex
fn vs_plate(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> PlateOut {
    let pl = plates[ii];
    let c = quad_corner(vi);
    let p = pl.pos.xyz + pl.right.xyz * c.x + pl.up.xyz * c.y;
    var out: PlateOut;
    out.clip = g.view_proj * vec4<f32>(p, 1.0);
    out.world = p;
    out.color = pl.color;
    out.glyph = u32(pl.pos.w + 0.5);
    // A plate is seen from whichever side faces the camera.
    let n = normalize(cross(pl.up.xyz, pl.right.xyz));
    out.normal = select(-n, n, dot(n, g.cam_pos.xyz - pl.pos.xyz) > 0.0);
    // The atlas runs down the page and the plate's v runs up the world.
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    out.texel = (vec2<f32>(pl.cell.xy) + uv * vec2<f32>(pl.cell.zw)) / ATLAS_SIZE;
    return out;
}

@fragment
fn fs_plate(in: PlateOut) -> @location(0) vec4<f32> {
    var a = in.color.a;
    if (in.glyph == 1u) {
        // The atlas holds a distance field: the edge is at 0.5, and one pixel
        // of screen slope across it antialiases the letter at any distance.
        let d = textureSample(font_atlas, font_samp, in.texel).r;
        let w = max(fwidth(d), 0.0015);
        a *= smoothstep(0.5 - w, 0.5 + w, d);
    }
    if (a <= 0.002) {
        discard;
    }
    let col = light(in.color.rgb, in.world, normalize(in.normal), 2.4, 0.0);
    return vec4<f32>(apply_fog(col, in.world), a);
}
