// Sprites: grass tufts, flowers and leaf clusters (records: see mesh/sprites.rs).
// Built after common.wgsl and scene.wgsl, whose lighting, fog and tints it uses.

@group(2) @binding(0) var sprite_tex: texture_2d_array<f32>;
@group(2) @binding(1) var sprite_samp: sampler;
// Grass pressed flat round the camera (render/foliage.rs trample_texels): r
// how flat, g and b the way it was laid.
@group(2) @binding(2) var trample_tex: texture_2d<f32>;

// How trampled the ground at `p` is: x how flat (0 to 1), yz the way it was
// laid. Nothing outside the map round the camera.
fn trampled(p: vec3<f32>) -> vec3<f32> {
    let span = TRAMPLE_N * TRAMPLE_CELL;
    let origin = floor(g.cam_pos.xz / TRAMPLE_STEP) * TRAMPLE_STEP - vec2<f32>(span * 0.5);
    let uv = (p.xz - origin) / span;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec3<f32>(0.0);
    }
    let t = textureSampleLevel(trample_tex, sprite_samp, uv, 0.0);
    return vec3<f32>(t.r, t.g * 2.0 - 1.0, t.b * 2.0 - 1.0);
}

// Sprites are lit and fogged per vertex: a card is small, and the fragment
// shader only reads the texture. Lit per pixel they cost 1.4 to 2.3 ms at
// 1440p in a forest, from many overlapping cards each paying for shadows,
// mist and fog.
struct SpriteOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    // The albedo multiplier: the ground block's colour and patch tint, or 1.
    @location(2) @interpolate(flat) tint: vec3<f32>,
    // Light (sun, shadow, sky, light through the leaves) times the share of
    // the colour the fog leaves; then what the fog adds.
    @location(3) light: vec3<f32>,
    @location(4) fog_add: vec3<f32>,
    @location(5) sheen: f32,
    // 1 for grass cards, whose bases shade toward the understory colour.
    @location(6) @interpolate(flat) grass: f32,
    // What the weather has left on it (world/damp.rs): x = frost here, held
    // on in the shade; y = how wet it is.
    @location(7) @interpolate(flat) damp: vec2<f32>,
};

// Thinning with distance: past about 15 m (25 m for leaves) half the grass
// (40% of leaves) shrinks away, each sprite at its own distance so none pop,
// and the rest grow a little wider to keep the cover. x = size, y = width.
fn thinning(root: vec3<f32>, dist: f32, leafy: bool) -> vec2<f32> {
    let r = f32(hash3u(vec3<i32>(floor(root * 16.0))) & 1023u) / 1023.0;
    let near = select(15.0, 25.0, leafy);
    let far = select(38.0, 60.0, leafy);
    let share = select(0.5, 0.4, leafy);
    var size = 1.0;
    if (r < share) {
        let d0 = mix(near, far, r / share);
        size = 1.0 - smoothstep(d0, d0 + 6.0, dist);
    }
    let wide = 1.0 + select(0.3, 0.25, leafy) * smoothstep(near, far, dist);
    return vec2<f32>(size, select(1.0, wide, r >= share));
}

// A card grows out of the ground as you walk up to it, its height following
// how far in it has faded. Straight, that spends most of the growth in the
// last few metres, where the card is big on screen and it reads as grass
// pushing up out of the dirt in front of you. This curve spends it the other
// way round: nearly all of the height arrives while the card is still far
// off and small, and the last of it comes in slowly. GROW_CURVE=1 is the old
// straight ramp.
fn grow(f: f32) -> f32 {
    return pow(clamp(f, 0.0, 1.0), GROW_CURVE);
}

// One shadow tap at a vertex, the light falling on it, and the light through
// it when the camera looks toward the sun.
fn sprite_light(world: vec3<f32>, n: vec3<f32>, leaf: bool) -> vec3<f32> {
    let sun = g.sun_dir.xyz;
    let ndl = max(dot(n, sun), 0.0);
    var sh = 1.0;
    if (ndl > 0.0) {
        sh = shadow_tap(world + n * 0.05);
        if (leaf) {
            sh = leaf_shadow(sh);
        }
    }
    let direct = g.sun_color.rgb * g.sun_color.w * ndl * sh;
    let view = normalize(world - g.cam_pos.xyz);
    let back = pow(max(dot(view, sun), 0.0), 4.0);
    return direct + hemi_ambient(n) + g.sun_color.rgb * g.sun_color.w * back * 0.35;
}

// The wind blows one way everywhere; gusts are a slow noise field carried
// downwind, so patches of meadow bow together as a gust passes. Gusts come
// and go: a slow swell over tens of seconds decides whether any are blowing,
// so there are calm spells between them.
// WIND comes from the prelude (render/shaders.rs).

fn gust(p: vec2<f32>, t: f32) -> f32 {
    let q = (p - WIND * t * 3.5) * 0.06;
    let n = vnoise2(q) * 0.7 + vnoise2(q * 2.7 + vec2<f32>(5.2, 1.3)) * 0.3;
    var swell = vnoise2(vec2<f32>(t * 0.045, 3.7)) * 0.7 + vnoise2(vec2<f32>(t * 0.13, 9.1)) * 0.3;
    return smoothstep(0.42, 0.8, n) * smoothstep(0.4, 0.7, swell) * wind_scale();
}

// How hard the wind blows, from the weather (world/weather.rs): 1 on a fair
// day, 0 in the dead still of the heat, about 1.5 in a squall. It scales the
// gusts and the steady lean alike, so a still day is still and a gusty one
// bends everything further rather than faster.
fn wind_scale() -> f32 {
    return max(g.weather.w, 0.0);
}

// Grass and leaf colour patches, as fs_terrain draws them, so tufts match the
// ground they stand on.
fn veg_tint(albedo: vec3<f32>, mat: u32, world: vec3<f32>) -> vec3<f32> {
    if (g.style.x > 0.5) {
        return calm_tint(albedo, mat, world);
    }
    if ((mats[mat].w & M_CLASSIC_PATCH) != 0u || drift_of(mat, 1u).on) {
        return classic_patch(albedo, mat, world);
    }
    return albedo;
}

// GRASS_FADE, LEAF_FADE and FILL_FADE (the distances over which each kind
// shrinks away) come from the prelude, set in render/foliage.rs.

// Kelp (mesh/sprites.rs STRAND): where a strand `len` metres long from a
// holdfast at `root`, heading `angle`, is a share `tt` of the way along, in
// the sea as it stands now (`fog.y`, which the tide moves). Where the water
// is deeper than the strand it stands up through it, bowing a little at the
// top. Where it is shallower the stipe leans out and curves over into the
// surface, and the rest of its length lies along the surface, drifting: so
// a falling tide lays more of it on the water instead of lifting it into
// the air. The surge rocks the whole of it slowly.
fn kelp_at(root: vec3<f32>, len: f32, angle: f32, tt: f32, time: f32) -> vec3<f32> {
    let head = vec3<f32>(cos(angle), 0.0, sin(angle));
    let d = max(g.fog.y - 0.05 - root.y, 0.1);
    let dv = min(d, len * 0.97);
    // 0 standing free under the surface, 1 reaching it with a metre to spare.
    let ks = smoothstep(0.0, 1.0, len - d);
    let slack = max(len - dv, 0.0);
    let xr = min(0.12 * dv + 0.4 * slack, 0.75 * dv);
    let a = min(length(vec2<f32>(dv, xr)) * (1.0 + 0.08 * ks), len);
    let s = tt * len;
    let ph = dot(root.xz, vec2<f32>(0.11, 0.07)) + angle;
    let surge = vec3<f32>(0.8, 0.0, 0.6) * (sin(time * 0.55 + ph) * 0.5 + sin(time * 1.3 + angle * 3.0) * 0.08) * min(dv, 8.0) * 0.12;
    let uu = min(s / max(a, 1e-3), 1.0);
    let y = dv * mix(uu, uu * (2.0 - uu), ks);
    let x = xr * mix(uu * uu, uu, ks);
    var p = root + vec3<f32>(0.0, y, 0.0) + head * x + surge * uu * uu;
    if (s > a) {
        // Past the bend: on along the surface, wandering, or (standing free)
        // straight on up.
        let r = s - a;
        let turn = angle + 0.35 * sin(time * 0.07 + ph) + 0.3 * sin(r * 0.45 + angle * 5.0);
        p += mix(vec3<f32>(0.0, r, 0.0), vec3<f32>(cos(turn), 0.0, sin(turn)) * r, ks);
    }
    return p;
}

// Stipe segments in a strand (sprites.rs `kelp_segs`, the same).
fn kelp_segs(len: f32) -> f32 {
    return clamp(ceil(len / 0.8), 2.0, 40.0);
}

@vertex
fn vs_sprite(@builtin(vertex_index) vi: u32) -> SpriteOut {
    // Drawn indexed, four vertices a sprite (see render QUAD_INDEX).
    let q = quads[vi / 4u];
    var cu = array<f32, 4>(0.0, 1.0, 1.0, 0.0);
    var cv = array<f32, 4>(0.0, 0.0, 1.0, 1.0);
    let u = cu[vi % 4u];
    let v = cv[vi % 4u];
    let ch = chunks[q.y >> 16u];
    let s = ch.w;
    let local = vec3<f32>(f32(q.x & 1023u), f32((q.x >> 20u) & 2047u), f32((q.x >> 10u) & 1023u)) / 16.0 - vec3<f32>(0.0, 2.0, 0.0);
    let root = ch.xyz + local * s;
    let kind = (q.y >> 8u) & 255u;
    let tint = q.y & 255u;
    let angle = f32(q.z & 255u) / 256.0 * 6.2831853;
    let w0 = f32((q.z >> 8u) & 255u) / 8.0 * s;
    let h = f32((q.z >> 16u) & 255u) / 8.0 * s;
    let seed = f32((q.z >> 24u) & 255u);
    let layer = q.w & 255u;
    let dir = (q.w >> 8u) & 7u;
    let t = g.cam_pos.w;
    let dist = length(root - g.cam_pos.xyz);
    // Cards with the leaf fade class (big plants) thin like leaves; long grass
    // (a big texture layer too) thins like grass.
    // Far grass patches (fade class 3) never thin: each stands for a whole cell.
    var thin = thinning(root, dist, kind != 0u || ((q.w >> 18u) & 3u) == 2u);
    if (kind == 0u && ((q.w >> 18u) & 3u) == 3u) {
        thin = vec2<f32>(1.0);
    }
    let w = w0 * thin.y;
    let gs = gust(root.xz, t);
    let wind3 = vec3<f32>(WIND.x, 0.0, WIND.y);
    var world: vec3<f32>;
    var normal = vec3<f32>(0.0, 1.0, 0.0);
    if (kind == 0u) {
        // Card: stands on the root, turned by `angle`; the top bends downwind.
        // Word3 bits 18-19 say how far it shows (sprites.rs FADE_*): grass,
        // near-only fill grass, or as far as leaves (big plants).
        let fade_class = (q.w >> 18u) & 3u;
        // Word3 bit 20 (sprites.rs STILL): stiff stalks that don't bend in the wind.
        let still = ((q.w >> 20u) & 1u) == 1u;
        var fade = select(select(GRASS_FADE, FILL_FADE, fade_class == 1u), LEAF_FADE, fade_class == 2u);
        if (still) {
            fade = STALK_FADE;
        }
        var fd = 1.0 - smoothstep(fade.x, fade.y, dist);
        if (fade_class == 3u) {
            // The far cover grows in as the tufts shrink away, and then
            // stays: it is the ground's cover from here to the world's edge.
            fd = smoothstep(PATCH_FADE.x, PATCH_FADE.y, dist);
        }
        // Stalks skip the thinning, which shrinks some plants away from 25 m
        // out: a 3 m stalk shrinking reads as one growing out of the ground
        // as you walk up. They shrink only at the far leaf fade.
        // Under the sea only plants standing in it (sawgrass, cattails: big
        // plants and stalks) grow; grass doesn't, unless it is a drowned
        // meadow that says so (sprites.rs WET: eelgrass and its like).
        // With a tide that is high water: nothing that needs dry feet grows
        // on what the sea covers twice a day (world/tide.rs).
        let wet = ((q.w >> 21u) & 1u) == 1u;
        let high = select(g.fog.y, max(g.fog.y, g.tide.z), g.tide.w > 0.5);
        let drowned = root.y < high && fade_class != 2u && !still && !wet;
        let f = grow(fd) * select(1.0, 0.0, drowned) * select(thin.x, 1.0, still);
        var across = vec3<f32>(cos(angle), 0.0, sin(angle));
        var upright = vec3<f32>(0.0, 1.0, 0.0);
        if (fade_class == 3u) {
            // A far patch is one wide card standing in for a whole cell of
            // blades pointing every way, so it has no heading of its own to
            // keep. On a fixed heading, and being much wider than it is
            // tall, it flattens into a dark smear lying on the ground as
            // soon as you look down a slope at it rather than along it.
            // Face it at the camera and let it lean back as well, so it
            // keeps its whole face from any angle; it still stands on its
            // own root, so it never floats.
            let d = normalize(root - g.cam_pos.xyz);
            var r = cross(d, vec3<f32>(0.0, 1.0, 0.0));
            r = select(vec3<f32>(1.0, 0.0, 0.0), normalize(r), dot(r, r) > 1e-6);
            across = r;
            upright = normalize(cross(r, d));
        }
        let hh = h * f;
        let flutter = sin(t * 2.6 + seed * 0.37 + dot(root.xz, vec2<f32>(0.9, 0.4))) * 0.035
            + sin(t * 4.1 + seed) * 0.015;
        let side = vec3<f32>(-WIND.y, 0.0, WIND.x) * sin(t * 1.7 + seed * 0.11) * 0.025;
        var bend = select((wind3 * (0.05 * wind_scale() + 0.28 * gs + flutter) + side) * hh, vec3<f32>(0.0), still);
        if (wet && root.y < g.fog.y - 0.3) {
            // Under the sea the wind does nothing; the swell's surge rocks
            // the blades slowly back and forth instead, in long waves.
            let ph = dot(root.xz, vec2<f32>(0.11, 0.07));
            bend = vec3<f32>(0.8, 0.0, 0.6) * (sin(t * 0.55 + ph) * 0.32 + sin(t * 1.4 + seed) * 0.04) * hh;
        }
        let k = v * v;
        world = root + across * (u - 0.5) * w * f + upright * (v * hh) + bend * k;
        // Keep the blade's length roughly as it bends.
        world.y -= dot(bend, bend) / max(hh, 1e-3) * 0.5 * k;
        if (wet && root.y < g.fog.y - 0.05 && world.y > g.fog.y - 0.05) {
            // Water shallower than the blade is long (a tide part way out):
            // the rest of it floats along under the surface, not through it.
            let over = world.y - (g.fog.y - 0.05);
            let lie = normalize(vec3<f32>(cos(seed * 1.3 + angle) + 0.8, 0.0, sin(seed * 1.3 + angle) + 0.6));
            world = vec3<f32>(world.x, g.fog.y - 0.05, world.z) + lie * over;
        }
        if (wet && g.tide.w > 0.5 && root.y > g.fog.y - 0.05 && root.y < g.tide.z + 0.05) {
            // Left out of the water by the tide (world/tide.rs): the blades
            // lie down on the wet sand, each its own way.
            let lie = vec3<f32>(cos(seed * 1.3 + angle), 0.0, sin(seed * 1.3 + angle));
            world = root + across * (u - 0.5) * w * f + lie * (v * hh * 0.9) + vec3<f32>(0.0, v * hh * 0.12 + 0.02, 0.0);
        }
    } else if (kind == 5u) {
        // Kelp: a stipe segment, a blade or a float on a strand shaped by
        // `kelp_at`. Sizes are in metres here, not voxels: `h` is the
        // strand's length.
        let f = 1.0 - smoothstep(LEAF_FADE.x, LEAF_FADE.y, dist);
        let len = f32((q.z >> 16u) & 255u) / 8.0;
        let part = (q.w >> 8u) & 3u;
        let index = f32((q.w >> 10u) & 255u);
        let sea = g.fog.y;
        let eye = normalize(root - g.cam_pos.xyz);
        if (part == 0u) {
            // A length of stipe from one joint to the next, a ribbon turned
            // to the camera, overlapping its neighbours a little.
            let n = kelp_segs(len);
            let p0 = kelp_at(root, len, angle, index / n, t);
            let p1 = kelp_at(root, len, angle, (index + 1.0) / n, t);
            let dv = normalize(p1 - p0 + vec3<f32>(0.0, 1e-4, 0.0));
            var across = cross(dv, eye);
            across = select(vec3<f32>(1.0, 0.0, 0.0), normalize(across), dot(across, across) > 1e-6);
            let ext = w * 0.3;
            world = mix(p0 - dv * ext, p1 + dv * ext, v) + across * (u - 0.5) * w * f;
            normal = normalize(vec3<f32>(0.0, 1.0, 0.0) - eye * 0.6);
        } else if (part == 1u) {
            // A blade at `index`/255 along the strand, turned by its seed.
            // Under water it rises along the stipe and out to its side,
            // fluttering; on the surface it lies flat, trailing with the
            // canopy. Never above the water.
            let tb = index / 255.0;
            let bl = f32((q.w >> 18u) & 63u) * 0.25;
            let pa = kelp_at(root, len, angle, tb, t);
            let pb = kelp_at(root, len, angle, max(tb - 0.03, 0.0), t);
            let tan = normalize(pa - pb + vec3<f32>(0.0, 1e-4, 0.0));
            let az = seed / 256.0 * 6.2831853;
            let out_dir = vec3<f32>(cos(az), 0.0, sin(az));
            let head = vec3<f32>(cos(angle), 0.0, sin(angle));
            let side = vec3<f32>(-head.z, 0.0, head.x);
            let flutter = sin(t * 0.9 + seed * 0.37 + tb * 6.0) * 0.3;
            let flat = 1.0 - smoothstep(0.15, 0.7, sea - pa.y);
            let under = normalize(tan * 0.8 + out_dir * 0.4 + side * flutter);
            let lying = normalize(vec3<f32>(out_dir.x * 0.3 + head.x + side.x * flutter, 0.0, out_dir.z * 0.3 + head.z + side.z * flutter));
            let dir = normalize(mix(under, lying, flat));
            var across = mix(cross(dir, eye), cross(dir, vec3<f32>(0.0, 1.0, 0.0)), flat);
            across = select(vec3<f32>(1.0, 0.0, 0.0), normalize(across), dot(across, across) > 1e-6);
            let wave = across * sin(v * 5.0 + t * 1.4 + seed) * 0.12 * v * bl;
            world = pa + (dir * v * bl + across * (u - 0.5) * w + wave) * f;
            world.y = min(world.y, sea - 0.03);
            normal = normalize(mix(normalize(vec3<f32>(0.0, 1.0, 0.0) - eye * 0.6), vec3<f32>(0.0, 1.0, 0.0), flat));
        } else {
            // The float on the strand's end, a bulb facing the camera, its
            // top just breaking the surface when it reaches it.
            var pe = kelp_at(root, len, angle, 1.0, t);
            pe.y = min(pe.y, sea - w * 0.15);
            var right = cross(eye, vec3<f32>(0.0, 1.0, 0.0));
            right = select(vec3<f32>(1.0, 0.0, 0.0), normalize(right), dot(right, right) > 1e-6);
            let up = cross(right, eye);
            world = pe + (right * (u - 0.5) + up * (v - 0.5)) * w * f;
            normal = normalize(vec3<f32>(0.0, 1.0, 0.0) - eye * 0.5);
        }
    } else if (kind == 4u) {
        // Pad: a floating leaf lying flat on the water over the bed it
        // roots in, turned by `angle`, lifting and tilting a little on the
        // swell. Its root is already at the water's own surface, sea or
        // river or lake (sprites.rs FLOAT), so there is no level to look up
        // here.
        let f = 1.0 - smoothstep(LEAF_FADE.x, LEAF_FADE.y, dist);
        let ph = seed * 0.37 + dot(root.xz, vec2<f32>(0.21, 0.33));
        let swell = 0.035 * sin(t * 0.9 + ph) + 0.02 * sin(t * 1.7 + ph * 2.3);
        let across = vec3<f32>(cos(angle), 0.0, sin(angle));
        let along = vec3<f32>(-across.z, 0.0, across.x);
        let cu = (u - 0.5) * w * f;
        let cv = (v - 0.5) * h * f;
        world = vec3<f32>(root.x, root.y + 0.04 + swell, root.z) + across * cu + along * cv;
        // The rim rides the swell a touch later than the middle, so a patch
        // of pads ripples instead of sliding as one sheet.
        world.y += 0.02 * sin(t * 1.3 + ph + (cu + cv) * 0.8);
        normal = vec3<f32>(0.0, 1.0, 0.0);
    } else if (kind == 3u) {
        // Arch: a fern frond lying flat, rising from the root at `droop` and
        // curving over so its tip ends near the ground. The tip nods in gusts.
        let f = grow(1.0 - smoothstep(GRASS_FADE.x + 10.0, GRASS_FADE.y + 14.0, dist)) * thin.x;
        let rise = f32((q.w >> 12u) & 63u) * 1.5 * 0.0174533;
        let head = vec3<f32>(cos(angle), 0.0, sin(angle));
        let side = vec3<f32>(-head.z, 0.0, head.x);
        let bob = sin(t * 1.6 + seed * 0.41 + dot(root.xz, vec2<f32>(0.6, 0.8))) * 0.5 + 0.5 + gs;
        // The frond leaves the root at `rise` above level, stays stiff near the
        // root and curls over toward the tip (angle falls with v squared), so it
        // stands up, then arches out to a tip that hangs below level.
        let curl = 2.3 + 0.3 * sin(seed);
        var along_out = 0.0;
        var along_up = 0.0;
        let steps = 8;
        for (var k = 0; k < steps; k++) {
            let vm = (f32(k) + 0.5) / f32(steps) * v;
            let ak = rise - curl * vm * vm;
            along_out += cos(ak) * v / f32(steps) * h;
            along_up += sin(ak) * v / f32(steps) * h;
        }
        along_up -= 0.03 * bob * v * v * h;
        let a_tip = rise - curl * v * v;
        let tip = wind3 * (0.02 * wind_scale() + 0.07 * gs) * h * v * v;
        // Each half of the blade tilts up a little from the rachis, a shallow V.
        let across = (u - 0.5) * w;
        world = root + (head * along_out + side * across + vec3<f32>(0.0, along_up + abs(across) * 0.2, 0.0) + tip) * f;
        // Mostly up, leaning only partly with the frond: a thin frond is lit
        // through from both sides, and the full turn made upright fronds black
        // beside neon tips.
        let frond_n = vec3<f32>(0.0, 1.0, 0.0) * cos(a_tip) - head * sin(a_tip);
        normal = normalize(vec3<f32>(0.0, 1.0, 0.0) + frond_n * 0.45);
    } else if (kind == 2u) {
        // Spray: a conifer branch from the root out along heading `angle`,
        // drooping; plane 0 lies flat, plane 1 stands upright. The tip bobs.
        let f = grow(1.0 - smoothstep(LEAF_FADE.x, LEAF_FADE.y, dist)) * thin.x;
        let droop = f32((q.w >> 12u) & 63u) * 1.5 * 0.0174533;
        let head = vec3<f32>(cos(angle), 0.0, sin(angle));
        let along = head * cos(droop) - vec3<f32>(0.0, sin(droop), 0.0);
        let flat_side = vec3<f32>(-head.z, 0.0, head.x);
        var across = flat_side;
        if (((q.w >> 11u) & 1u) == 1u) {
            across = normalize(cross(along, flat_side));
        }
        let bob = sin(t * 1.9 + seed * 0.3 + dot(root.xz, vec2<f32>(0.7, 0.3))) * 0.5 + gs;
        let tip = (wind3 * (0.015 * wind_scale() + 0.05 * gs) - vec3<f32>(0.0, 0.025 * bob, 0.0)) * h;
        world = root + (across * (u - 0.5) * w + along * v * h + tip * v * v) * f;
        normal = normalize(vec3<f32>(0.0, 0.8, 0.0) + head * 0.5);
    } else {
        // Billboard: centred on the root, facing the camera, fluttering a little.
        let f = grow(1.0 - smoothstep(LEAF_FADE.x, LEAF_FADE.y, dist)) * thin.x;
        let d = normalize(root - g.cam_pos.xyz);
        var right = cross(d, vec3<f32>(0.0, 1.0, 0.0));
        if (dot(right, right) < 1e-6) {
            right = vec3<f32>(1.0, 0.0, 0.0);
        }
        right = normalize(right);
        let up = cross(right, d);
        let roll = angle + sin(t * 2.3 + seed) * 0.06 * (0.4 + gs);
        let r2 = right * cos(roll) + up * sin(roll);
        let u2 = -right * sin(roll) + up * cos(roll);
        let sway = (wind3 * (0.01 * wind_scale() + 0.03 * gs) + vec3<f32>(0.0, 0.01, 0.0) * sin(t * 3.3 + seed * 0.7)) * (1.0 + sin(t * 1.3 + seed));
        world = root + (r2 * (u - 0.5) + u2 * (v - 0.5)) * w * f + sway * f;
        if (dir < 6u) {
            normal = normalize(dir_normal(dir) + vec3<f32>(0.0, 0.6, 0.0));
        }
    }
    // Trampled (creatures/sign.rs): grass and low plants pressed down and
    // laid over the way whatever went through was going. A leaf cluster up
    // in a tree is out of reach of any foot.
    let tr = trampled(root);
    if (kind != 5u && tr.x > 0.004 && root.y - ground_height(root.xz) < 1.2) {
        let up = max(world.y - root.y, 0.0);
        world = vec3<f32>(world.x, root.y + up * (1.0 - 0.85 * tr.x), world.z) + vec3<f32>(tr.y, 0.0, tr.z) * up * 0.45 * tr.x;
    }
    var out: SpriteOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(u, 1.0 - v);
    out.layer = layer;
    var tc = vec3<f32>(1.0);
    if (tint != 0u) {
        // The block's own average colour (its smallest mip).
        let base = textureSampleLevel(tex, samp, vec2<f32>(0.5), i32(mats[tint].x), 16.0).rgb;
        tc = veg_tint(base * 1.3, tint, root);
    }
    out.tint = tc;
    let fog = fog_terms(world);
    // Leaf billboards and big plants' cards count as leaves; grass does not.
    let leaf_card = kind != 0u || ((q.w >> 18u) & 3u) == 2u;
    out.light = sprite_light(world, normal, leaf_card) * fog.a;
    if (leaf_card) {
        // The canopy's broad clumps of sun and shade (scene.wgsl), the same
        // field the voxels behind the cards take.
        out.light *= canopy_dapple(root);
    }
    if (layer == L_GLOW_THREADS) {
        // Glow-worm threads give their own light.
        out.light = max(out.light, vec3<f32>(0.9) * fog.a);
    }
    out.fog_add = fog.rgb;
    // Bent over, a blade shows its paler side.
    out.sheen = gs * v * 0.6 + tr.x * 0.9;
    out.grass = select(0.0, 1.0, UNDER_ON && kind == 0u && tint != 0u && (mats[tint].w & M_UNDERSTORY) != 0u && ((q.w >> 18u) & 3u) != 2u);
    // Frost lasts where the sun has not reached (scene.wgsl damp_ground):
    // one shadow tap at the root picks the shade's amount or the sun's.
    var rime = 0.0;
    if (g.damp.z > 0.01 && !leaf_card) {
        var lit = 0.0;
        if (g.air_sun.w < 0.5) {
            lit = shadow_tap(root + vec3<f32>(0.0, 0.1, 0.0));
        }
        rime = mix(g.damp.z, g.damp.w, lit);
    }
    out.damp = vec2<f32>(rime, g.damp.x);
    return out;
}

fn sprite_shade(in: SpriteOut, tx: vec4<f32>) -> vec3<f32> {
    // Bent blades show their paler sides as a gust passes.
    // Grass darkens toward its base into the understory painted on the ground.
    let base = mix(vec3<f32>(1.0), UNDERSTORY * 0.82, in.grass * (1.0 - smoothstep(0.0, 0.4, 1.0 - in.uv.y)));
    // The season, on everything growing (world/season.rs): grass goes over
    // to straw, a leaf only takes the year's colour. A glow-worm's thread is
    // not a plant and keeps its own.
    var c = tx.rgb * in.tint;
    if (in.layer != L_GLOW_THREADS) {
        c = select(seasoned(c), strawed(c), in.grass > 0.5);
        // Wet blades and leaves go a little darker and deeper; frost greys
        // a blade from its tip down (uv.y is 0 at the tip).
        let w = in.damp.y;
        c = pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 + 0.3 * w)) * (1.0 - 0.12 * w);
        let tip = 1.0 - in.uv.y;
        c = mix(c, vec3<f32>(0.84, 0.88, 0.95), smoothstep(0.0, 1.0, in.damp.x) * mix(0.35, 0.95, tip) * mix(0.5, 1.0, in.grass));
    }
    return c * base * (1.0 + in.sheen * 0.28) * in.light + in.fog_add;
}

// With MSAA: alpha to coverage, alpha sharpened to about a pixel of soft edge.
@fragment
fn fs_sprite(in: SpriteOut) -> @location(0) vec4<f32> {
    let tx = textureSample(sprite_tex, sprite_samp, in.uv, i32(in.layer));
    let a = clamp((tx.a - 0.5) / max(fwidth(tx.a), 1e-4) + 0.5, 0.0, 1.0);
    if (a <= 0.0) {
        discard;
    }
    return vec4<f32>(sprite_shade(in, tx), a);
}

// Without MSAA: a plain cut-out.
@fragment
fn fs_sprite_cut(in: SpriteOut) -> @location(0) vec4<f32> {
    let tx = textureSample(sprite_tex, sprite_samp, in.uv, i32(in.layer));
    if (tx.a < 0.5) {
        discard;
    }
    return vec4<f32>(sprite_shade(in, tx), 1.0);
}

// ---- motes -----------------------------------------------------------------
// Dust and pollen drifting in a box around the camera. Each is fixed in the
// world (the box wraps, it doesn't follow), and shows only where it is in sun
// while the air a couple of metres around it is in shade: inside the shafts
// of light under a canopy, not across an open meadow.
//
// A lamp does the same job indoors. A roofed room -- a cave, or an imported arena
// sunk under the plain -- is never in sun, so the shaft test is zero
// everywhere in it and nothing would ever show; there the lamps are the only
// light there is, so they carry the motes and lend them their colour.

const MOTE_BOX: vec3<f32> = vec3<f32>(18.0, 9.0, 18.0);

struct MoteOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) @interpolate(flat) light: vec3<f32>,
};

// One tap of the shadow map, times the cloud and horizon shade: 1 in sun,
// 0 in shade.
fn shadow_tap(world: vec3<f32>) -> f32 {
    let far = far_shadow(world);
    if (g.sun_dir.w < 0.5) {
        return far;
    }
    let depth = length(world - g.cam_pos.xyz);
    var ci = 0u;
    if (depth > g.splits.x) { ci = 1u; }
    if (depth > g.splits.y) { ci = 2u; }
    // The newer map of cascades 1 and 2 is close enough for a mote.
    let cm = cascade_maps(ci);
    let mi = select(u32(cm.w) + 1u, 0u, ci == 0u);
    let sp = shadow_pos(g.shadow_mats[mi], world);
    if (!sp.inside) {
        return far;
    }
    return far * textureSampleCompareLevel(shadow_map, shadow_samp, sp.uv, i32(select(cm.y, 0.0, ci == 0u)), sp.z - 0.0009);
}

@vertex
fn vs_mote(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> MoteOut {
    let corner = quad_corner(vi);
    let h = hash3u(vec3<i32>(i32(ii), 17, 5));
    let h2 = hash3u(vec3<i32>(i32(ii), 23, 9));
    let r = vec3<f32>(f32(h & 1023u), f32((h >> 10u) & 1023u), f32((h >> 20u) & 1023u)) / 1023.0;
    let ph = f32(h2 & 1023u) / 1023.0 * 6.2831853;
    let t = g.cam_pos.w;
    // A slow drift downwind, a lazy wander, and a little rise.
    let drift = vec3<f32>(WIND.x, 0.0, WIND.y) * t * 0.08
        + vec3<f32>(sin(t * 0.31 + ph) * 0.5, sin(t * 0.23 + ph * 1.7) * 0.35 + t * 0.015, cos(t * 0.27 + ph * 0.6) * 0.5);
    let rel = r * MOTE_BOX + drift;
    let centre = g.cam_pos.xyz + vec3<f32>(0.0, 1.5, 0.0);
    let f = fract((rel - centre) / MOTE_BOX);
    let pos = centre + (f - 0.5) * MOTE_BOX;
    // Fade at the box's walls so wrapping never pops.
    let edge = max(max(abs(f.x - 0.5), abs(f.y - 0.5)), abs(f.z - 0.5));
    var vis = 1.0 - smoothstep(0.3, 0.5, edge);
    let lit = shadow_tap(pos);
    var shade = 0.0;
    shade += 1.0 - shadow_tap(pos + vec3<f32>(2.0, 0.0, 0.0));
    shade += 1.0 - shadow_tap(pos + vec3<f32>(-2.0, 0.0, 0.0));
    shade += 1.0 - shadow_tap(pos + vec3<f32>(0.0, 0.0, 2.0));
    shade += 1.0 - shadow_tap(pos + vec3<f32>(0.0, 0.0, -2.0));
    // Shade on at least three sides: inside a shaft under a canopy, not at the
    // edge of one tree's shadow out in the open.
    let shaft = lit * smoothstep(2.2, 3.6, shade);
    // Lamplight, where there is no sun on it to begin with. A mote has no
    // face, so the normal handed to the lamps points up.
    let lamp = lamp_light(pos, vec3<f32>(0.0, 1.0, 0.0));
    // Thinner than a shaft of sun: a lamp lights the whole room it is in,
    // and every mote in it at once is snow indoors.
    let glow = smoothstep(0.22, 0.9, max(lamp.r, max(lamp.g, lamp.b))) * (1.0 - lit) * 0.45;
    vis *= max(shaft, glow);
    if (g.fog.z > 0.5) {
        // Marine snow: under water every mote shows, a pale fleck drifting
        // slowly down through the light, not only the ones in a sunbeam.
        vis = (1.0 - smoothstep(0.3, 0.5, edge)) * select(0.0, 0.1, (h2 & 3u) == 0u);
    }
    // Twinkle as the mote turns.
    vis *= 0.55 + 0.45 * sin(t * (1.5 + f32((h2 >> 10u) & 255u) / 128.0) + ph);
    let to_cam = g.cam_pos.xyz - pos;
    let dist = length(to_cam);
    let size = max(0.014, dist * 0.0028);
    let fb = billboard(to_cam / max(dist, 1e-3));
    let right = fb.right;
    let up = fb.up;
    var out: MoteOut;
    if (vis < 0.01 || (g.sun_color.w < 0.05 && glow < 0.01)) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        out.corner = corner;
        out.light = vec3<f32>(0.0);
        return out;
    }
    let world = pos + (right * corner.x + up * corner.y) * size;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.corner = corner;
    // Lit by whichever of the two found it.
    let key = select(g.sun_color.rgb * g.sun_color.w, lamp * 2.2, glow > shaft);
    out.light = key * vis * 1.2 * (1.0 - smoothstep(6.0, 9.0, dist));
    return out;
}

@fragment
fn fs_mote(in: MoteOut) -> @location(0) vec4<f32> {
    let d = dot(in.corner, in.corner);
    let a = max(1.0 - d, 0.0);
    return vec4<f32>(in.light * a * a, 0.0);
}
