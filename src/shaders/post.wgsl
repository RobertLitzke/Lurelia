// Tone mapping and grade from the resolved HDR image to the output.

@group(0) @binding(0) var hdr: texture_2d<f32>;
// s: x = 1 for the calm style, y = speed, z = camera near (m), w = time of day.
// x: x = speed streaks, y = a clock wrapped to 64 s for them.
// w: the weather (world/weather.rs): x = rain, y = snow, z = the wind's
//    multiplier, w = the camera's bearing in radians. x, y and z are 0, 0
//    and 1 on a fair day.
// v: x = how far up the camera looks, so the falling stuff sweeps past as the
//    view turns; yzw = the sky's horizon colour, which the rain is lit by.
// m: x = the camera's pitch (radians), y = tan of half the vertical field
//    of view, z = the heat mirage (0 to 1), w = 1 when reflections are on.
// c: xyz = the camera, w = sea level.
// vp, ivp: the frame's view-projection and its inverse, for the
//    reflections' march (`mirror`).
struct Post {
    s: vec4<f32>,
    x: vec4<f32>,
    w: vec4<f32>,
    v: vec4<f32>,
    m: vec4<f32>,
    c: vec4<f32>,
    vp: mat4x4<f32>,
    ivp: mat4x4<f32>,
};
@group(0) @binding(1) var<uniform> post: Post;
// Sun rays: xy = the sun's position on screen (uv), z = strength (0 = none),
// w = 1 when the light is the moon.
@group(0) @binding(2) var<uniform> shafts: vec4<f32>;
// Scene depth (reversed, infinite far): DEPTH_TEXTURE is filled in at load
// time with the multisampled or the plain type.
@group(0) @binding(3) var depth_tex: DEPTH_TEXTURE;
// Sun rays at half resolution (fs_rays), and a linear sampler to upscale them.
@group(0) @binding(4) var rays_tex: texture_2d<f32>;
@group(0) @binding(5) var lin: sampler;
// The camera's lens (src/camera/lens.rs). frame: the viewfinder in uv
// (x0, y0, x1, y1). p: x = focus distance (m), y = blur at infinity, z = the
// most blur, w = blur outside the frame (0 = no viewfinder mask); y, z and w
// are in pixels at 1080 tall. All zero when the camera isn't up.
struct Lens {
    frame: vec4<f32>,
    p: vec4<f32>,
};
@group(0) @binding(6) var<uniform> lens: Lens;
// The look's gradient strips (render/grade.rs); the grade reads row 1 (the
// tint) and row 2 (the gradient map), both by luminance. Row 0 is the
// distance ramp the scene shader uses.
@group(0) @binding(7) var gradetex: texture_2d<f32>;
// The grade's knobs, in the order render::grade::Grade::uniform writes them.
struct GradeU {
    a: vec4<f32>, // ramp reach (m), ramp held to the horizon's value, haze from ramp, haze density
    b: vec4<f32>, // sky from ramp, sky reach, shade from ramp, sun-side warmth
    c: vec4<f32>, // sun glow, crown shading, exposure, tint strength
    d: vec4<f32>, // gradient map, saturation, contrast, vignette
    e: vec4<f32>, // time-of-day tint, ramp curve, distance slabs, spare
};
@group(0) @binding(8) var<uniform> gd: GradeU;

// A hash of one cell of a precipitation layer. post.wgsl is the one module
// built without common.wgsl, so it carries its own.
fn hash_cell(cell: vec2<f32>, layer: i32) -> u32 {
    var h = u32(i32(cell.x) * 374761393 + i32(cell.y) * 668265263 + layer * 1274126177);
    h = (h ^ (h >> 13u)) * 1274126177u;
    return h ^ (h >> 16u);
}

// A hash of one number, 0 to 1: each streak lane's phase and speed.
fn hash1(x: f32) -> f32 {
    return fract(sin(x * 127.1 + 0.7) * 43758.5453);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// A sample from gradient strip row `row` at `t` (0..1), with the half-texel
// offset that keeps the ends on the first and last samples.
fn strip(t: f32, row: f32) -> vec3<f32> {
    let n = vec2<f32>(textureDimensions(gradetex));
    let u = (clamp(t, 0.0, 1.0) * (n.x - 1.0) + 0.5) / n.x;
    return textureSampleLevel(gradetex, lin, vec2<f32>(u, (row + 0.5) / n.y), 0.0).rgb;
}

// How much of the distance softening a pixel gets: none within about 90 m,
// all of it by 380 m. The sky counts as far.
fn far_weight(p: vec2<i32>) -> f32 {
    let d = textureLoad(depth_tex, p, 0);
    if (d <= 0.0) {
        return 1.0;
    }
    return smoothstep(90.0, 380.0, post.s.z / d);
}

// A light blur of the distance (a 3x3 box, faded in with distance). Only far
// neighbours are gathered, so a near trunk never bleeds onto the hills behind
// it; the sky itself is left alone, but distant ridges soften against it.
fn soften_distance(p: vec2<i32>, c: vec3<f32>) -> vec3<f32> {
    let d = textureLoad(depth_tex, p, 0);
    if (d <= 0.0) {
        return c;
    }
    let w = smoothstep(90.0, 380.0, post.s.z / d);
    if (w < 0.01) {
        return c;
    }
    let hi = vec2<i32>(textureDimensions(hdr)) - 1;
    var acc = c;
    var sum = 1.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if (x == 0 && y == 0) {
                continue;
            }
            let q = clamp(p + vec2<i32>(x, y), vec2<i32>(0), hi);
            let k = far_weight(q);
            acc += textureLoad(hdr, q, 0).rgb * k;
            sum += k;
        }
    }
    return mix(c, acc / sum, w * 0.75);
}

// Light streaming from the sun past whatever stands in front of it: march from
// each pixel toward the sun, gathering the brightest sky near it, which
// leaves and branches break into rays.
fn sun_rays(px: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(hdr));
    let sun = shafts.xy * dims;
    // Fade toward the screen edges away from the sun; past the reach, no work.
    let reach = 1.0 - smoothstep(0.3, 1.0, length((px - sun) / dims.y));
    if (reach <= 0.0) {
        return vec3<f32>(0.0);
    }
    let n = 40;
    let step = (sun - px) / f32(n) * 0.85;
    var q = px;
    var w = 1.0;
    var acc = 0.0;
    for (var i = 0; i < n; i++) {
        q += step;
        let s = textureLoad(hdr, vec2<i32>(clamp(q, vec2<f32>(0.0), dims - 1.0)), 0).rgb;
        let l = dot(s, vec3<f32>(0.2126, 0.7152, 0.0722));
        acc += min(max(l - 1.05, 0.0), 2.0) * w;
        w *= 0.965;
    }
    let tint = mix(vec3<f32>(1.0, 0.84, 0.58), vec3<f32>(0.7, 0.76, 1.0), shafts.w);
    return tint * acc / f32(n) * shafts.z * reach;
}

// The rays pass: one ray per 2x2 block of pixels, written to a half-size
// target that fs_post upscales. Rays are soft, so nothing is lost, and it is a
// quarter of the 40-tap march (0.85 ms at 1440p at full size).
@fragment
fn fs_rays(@builtin(position) clip: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(sun_rays(clip.xy * 2.0), 1.0);
}

// Blur radius in pixels at 1080 tall for the depth at p: thin-lens circle
// of confusion, blur at infinity times |1 - focus / distance|.
fn coc_at(p: vec2<i32>) -> f32 {
    let d = textureLoad(depth_tex, p, 0);
    // Reversed infinite depth: distance = near / d, so 1 / distance = d / near.
    let inv = max(d, 0.0) / post.s.z;
    return min(lens.p.y * abs(1.0 - lens.p.x * inv), lens.p.z);
}

fn outside_frame(uv: vec2<f32>) -> bool {
    return uv.x < lens.frame.x || uv.y < lens.frame.y || uv.x > lens.frame.z || uv.y > lens.frame.w;
}

// Depth of field by gathering: a golden-angle disc of taps, each kept only
// where its own blur reaches this pixel, so a sharp subject doesn't smear onto
// the soft background behind it. Outside the viewfinder every tap counts.
fn defocus(px: vec2<f32>, uv: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(hdr));
    let k = dims.y / 1080.0;
    let p = vec2<i32>(px);
    let out = lens.p.w > 0.0 && outside_frame(uv);
    var r = coc_at(p);
    if (out) {
        r = max(r, lens.p.w);
    }
    r *= k;
    let c0 = textureLoad(hdr, p, 0).rgb;
    if (r < 0.75) {
        return c0;
    }
    let n = 24;
    var acc = c0;
    var sum = 1.0;
    for (var i = 0; i < n; i++) {
        let f = (f32(i) + 0.5) / f32(n);
        let rad = r * sqrt(f);
        let a = f32(i) * 2.39996;
        let q = vec2<i32>(clamp(px + vec2<f32>(cos(a), sin(a)) * rad, vec2<f32>(0.0), dims - 1.0));
        var w = 1.0;
        if (!out) {
            w = smoothstep(rad - 1.0, rad + 1.0, coc_at(q) * k);
        }
        acc += textureLoad(hdr, q, 0).rgb * w;
        sum += w;
    }
    return acc / sum;
}

struct FsTri {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_post(@builtin(vertex_index) vi: u32) -> FsTri {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: FsTri;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Outside the viewfinder: greyed and darkened (the blur is in defocus).
// Rain and snow, in front of everything: a few layers of falling marks over
// the finished frame, each layer standing for a slab of air at its own
// distance, so the near ones are long, fast and sparse and the far ones a
// fine drift. Nothing about it is in the world -- it is weather you look
// through, not geometry -- so it costs one pass over the pixels and reaches
// as far as the view does.
//
// Two things keep it from reading as a sheet of dirt on the screen. The field
// is offset by where the camera is looking, so turning sweeps the fall past
// rather than carrying it along with you. And it is held off whatever is
// close: at arm's length there is nothing between you and it to rain on, and
// streaks over your own hands give the whole thing away.
fn precipitation(uv: vec2<f32>, clip: vec2<f32>, aspect: f32) -> vec3<f32> {
    let rain = post.w.x;
    let snow = post.w.y;
    if (rain + snow < 0.004) {
        return vec3<f32>(0.0);
    }
    let t = g_time();
    // How far off the nearest thing in this direction is: no fall over
    // anything within a couple of metres.
    let d = textureLoad(depth_tex, vec2<i32>(clip), 0);
    let near = select(smoothstep(0.6, 3.0, post.s.z / d), 1.0, d <= 0.0);
    if (near < 0.01) {
        return vec3<f32>(0.0);
    }
    // The view's own sweep. A bearing in radians over a 1-radian-ish field of
    // view is about one screen width, so the two scale together well enough
    // for parallax; the layers each take their own share of it.
    let sweep = vec2<f32>(post.w.w * 0.55, -post.v.x * 0.5);
    let wind = post.w.z;
    var acc = 0.0;
    for (var i = 0; i < 4; i++) {
        // Near snow is real flakes in the world round the camera (game.rs,
        // fx `snowflake`), which move past as you do; a screen layer that
        // close reads as flakes stuck to the glass. Only the far drift is
        // drawn here.
        if (snow > rain && i < 2) {
            continue;
        }
        let fi = f32(i);
        // Each layer is further off: smaller marks, more of them, slower.
        let depth = 1.0 + fi * 1.35;
        // The grid is far finer than the fall is dense, which is the whole
        // trick: at one mark per cell and half the cells firing, a jitter
        // inside the cell still leaves the grid legible and the rain reads as
        // ruled lines on the glass. A fifteenth of a fine grid does not.
        let cells = vec2<f32>(60.0, 25.0) * depth;
        // Rain leans downwind and falls fast; snow drifts and wanders.
        let lean = select(0.26 * wind, 0.30 * wind, rain > snow);
        let fall = select(0.11, 1.5, rain > snow) / depth;
        // Each layer is shifted off the others, so four grids of the same
        // shape never line up into one.
        var q = vec2<f32>(uv.x * aspect, uv.y) + sweep / depth + vec2<f32>(fi * 0.37, fi * 0.21);
        q.x = q.x - lean * (q.y + t * fall * 0.5);
        if (snow > rain) {
            // Snow is not blown along a line: it wanders as it comes down.
            q.x = q.x + sin(t * 0.5 + fi * 2.1 + q.y * 7.0) * 0.045 / depth;
        }
        var p = q * cells;
        // Screen y runs down, so the pattern is read further up as time
        // goes on and the marks come down. (It was added, and everything
        // rose: snow, slow enough to see it, went up the screen.)
        p.y = p.y - t * fall * cells.y * 0.55;
        let cell = floor(p);
        let f = p - cell;
        let h = hash_cell(cell, i);
        // Only a few cells in each layer carry a drop, so it never tiles.
        let lit = f32(h & 255u) / 255.0;
        let thick = max(rain, snow);
        if (lit > 0.02 + 0.08 * thick) {
            continue;
        }
        let off = vec2<f32>(f32((h >> 8u) & 255u), f32((h >> 16u) & 255u)) / 255.0;
        // No two drops the same: a drop is anywhere from a quarter to full
        // strength, which is most of what keeps a field of them from reading
        // as ruled lines on the glass.
        let weight = 0.25 + 0.75 * f32((h >> 24u) & 15u) / 15.0;
        // Where this pixel sits from the mark, in screen units rather than
        // cell units: a cell is wider than it is tall, so measuring in cells
        // squashes a drop into a lozenge and a flake into a streak.
        let d = (f - vec2<f32>(off.x, 0.5)) / cells;
        var m = 0.0;
        if (rain > snow) {
            // A streak: a hair-thin line, soft at both ends, its length its
            // own, falling from somewhere of its own in the cell.
            let len = (0.25 + 0.55 * f32((h >> 28u) & 7u) / 7.0) / cells.y;
            let dy = abs(d.y + (off.y - 0.5) * 0.4 / cells.y) / len;
            m = (1.0 - smoothstep(0.0, 0.0012, abs(d.x))) * (1.0 - smoothstep(0.3, 1.0, dy)) * weight;
        } else {
            // A flake: a soft round dot, a few pixels across near to and a
            // speck away off.
            let r = length(vec2<f32>(d.x, d.y + (off.y - 0.5) / cells.y));
            m = (1.0 - smoothstep(0.0, 0.0055 / depth, r)) * weight;
        }
        // The far layers are fainter as well as finer: a curtain of rain
        // reads as depth only if the back of it is paler than the front.
        acc = acc + m * (0.9 / (depth * depth));
    }
    acc = acc * near * max(rain, snow);
    // Rain is a pale glass line that takes its colour from the sky it is
    // falling through; snow is brighter and plainly white.
    let tint = mix(post.v.yzw * 1.5 + vec3<f32>(0.1), vec3<f32>(1.0, 1.0, 1.02), snow);
    let gain = mix(0.45, 0.8, snow);
    return tint * acc * gain;
}

// The seconds clock the falling stuff runs on: the streaks' 64-second wrap,
// which is long enough that no fall repeats visibly and short enough that
// the float stays exact.
fn g_time() -> f32 {
    return post.x.y;
}

@fragment
fn fs_post(in: FsTri) -> @location(0) vec4<f32> {
    var c = graded(in);
    let dims = vec2<f32>(textureDimensions(hdr));
    c = c + precipitation(in.uv, in.clip.xy, dims.x / max(dims.y, 1.0));
    if (lens.p.w > 0.0 && outside_frame(in.uv)) {
        let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
        c = mix(c, vec3<f32>(l), 0.75) * 0.5;
    }
    return vec4<f32>(c, 1.0);
}

// Where a world point falls on screen: uv, and its view depth (metres in
// front of the camera; 0 or less behind it).
fn to_screen(q: vec3<f32>) -> vec3<f32> {
    let clip = post.vp * vec4<f32>(q, 1.0);
    if (clip.w <= 0.05) {
        return vec3<f32>(-1.0, -1.0, 0.0);
    }
    let ndc = clip.xy / clip.w;
    return vec3<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, clip.w);
}

// How far off the scene is at uv, metres of view depth; very far for sky.
fn scene_depth(uv: vec2<f32>, dims: vec2<f32>) -> f32 {
    let d = textureLoad(depth_tex, vec2<i32>(clamp(uv * dims, vec2<f32>(0.0), dims - 1.0)), 0);
    return select(post.s.z / max(d, 1e-9), 1e9, d <= 0.0);
}

// Reflections in standing water. The terrain writes alpha over 1 where it
// holds a pool or a flooded pan, and the water pass adds its own coverage on
// top of the bed (render/mod.rs), so alpha - 1 is how much of a mirror a
// pixel is; nothing else ever writes more than 1. From the water's surface
// (the sea's level for open water, whose own depth is not in the buffer, or
// the ground itself for a pool) the view is reflected about straight up and
// marched out across the depth buffer; the first thing it passes behind,
// within a thickness that grows with distance, is what the water shows, over
// the sky the scene shader already put there. Off the screen or into the sky
// it finds nothing and the sky stays.
fn mirror(px: vec2<f32>, uv: vec2<f32>, c: vec3<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(hdr));
    let a = textureLoad(hdr, vec2<i32>(px), 0).a;
    let m = clamp(a - 1.0, 0.0, 1.0);
    if (m < 0.02 || post.m.w < 0.5) {
        return c;
    }
    let d = textureLoad(depth_tex, vec2<i32>(px), 0);
    if (d <= 0.0) {
        return c;
    }
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let h = post.ivp * ndc;
    var p = h.xyz / h.w;
    let cam = post.c.xyz;
    let v = normalize(p - cam);
    if (v.y > -0.002) {
        return c;
    }
    // Open water: the bed is what the buffer holds; the mirror is the surface.
    if (p.y < post.c.w - 0.02 && cam.y > post.c.w) {
        p = cam + v * ((post.c.w - cam.y) / v.y);
    }
    // A slow ripple, so the image shivers rather than standing like glass.
    let t = post.x.y;
    let rip = vec2<f32>(sin(p.x * 0.9 + t * 1.7) + sin(p.z * 1.3 - t * 1.1), sin(p.z * 0.8 + t * 1.3) + sin(p.x * 1.1 + t * 0.9)) * 0.0035;
    let r = normalize(vec3<f32>(v.x + rip.x, -v.y, v.z + rip.y));
    var tt = 0.6;
    var prev = 0.0;
    var hit = -1.0;
    for (var i = 0; i < 44; i++) {
        let q = to_screen(p + r * tt);
        if (q.z <= 0.0 || q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0) {
            break;
        }
        let sd = scene_depth(q.xy, dims);
        if (q.z > sd + 0.05) {
            if (q.z < sd + max(2.0, sd * 0.06) + tt * 0.08) {
                hit = tt;
            }
            break;
        }
        prev = tt;
        tt *= 1.22;
    }
    if (hit < 0.0) {
        return c;
    }
    // Close in on where it crossed.
    var lo = prev;
    var hi = hit;
    for (var i = 0; i < 6; i++) {
        let mid = (lo + hi) * 0.5;
        let q = to_screen(p + r * mid);
        if (q.z > scene_depth(q.xy, dims)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let q = to_screen(p + r * hi);
    let edge = smoothstep(0.0, 0.06, min(min(q.x, 1.0 - q.x), min(q.y, 1.0 - q.y)));
    let img = textureLoad(hdr, vec2<i32>(clamp(q.xy * dims, vec2<f32>(0.0), dims - 1.0)), 0).rgb;
    let cosv = -v.y;
    let fres = 0.02 + 0.98 * pow(1.0 - cosv, 5.0);
    return mix(c, img * 0.92, m * mix(fres, 1.0, 0.15) * edge);
}

// The heat mirage over a flat (world.json `mirage`). Light coming in just
// under the horizon is bent back up off the hot air over the ground, so the
// far flat shows what is just above the horizon, upside down and squeezed:
// sky-coloured water from a hundred and fifty metres out, and anything
// standing out there floating over its own reflection with its foot lost in
// it. The air over the flat shimmers. Nearer ground keeps its own colour, so
// the water is always out ahead and never where you stand.
fn mirage(uv: vec2<f32>, c: vec3<f32>) -> vec3<f32> {
    let k = post.m.z;
    if (k < 0.01) {
        return c;
    }
    let dims = vec2<f32>(textureDimensions(hdr));
    let pitch = post.m.x;
    let th = post.m.y;
    let ny = 1.0 - uv.y * 2.0;
    let e = pitch + atan(ny * th);
    if (e > 0.03 || e < -0.06) {
        return c;
    }
    let dist = scene_depth(uv, dims);
    let far = smoothstep(60.0, 140.0, dist);
    if (far <= 0.0) {
        return c;
    }
    let t = post.x.y;
    // The shimmer: a fine waver, strongest at the horizon.
    let at_line = 1.0 - smoothstep(0.0, 0.03, abs(e));
    let wav = sin(uv.y * dims.y * 0.7 + t * 9.0) * 0.6 + sin(uv.x * dims.x * 0.04 - t * 5.3) * 0.4;
    let src = uv + vec2<f32>(wav * 1.2, wav * 0.6) / dims * k * far * at_line;
    var out = textureLoad(hdr, vec2<i32>(clamp(src * dims, vec2<f32>(0.0), dims - 1.0)), 0).rgb;
    if (e < 0.0) {
        // Under the line: what stands four times the angle above it, so the
        // reflection comes out squeezed, the way a mirage's does.
        let e2 = -e * 4.0 + wav * 0.0008;
        let ny2 = tan(e2 - pitch) / th;
        let v2 = 0.5 - ny2 * 0.5;
        let up = textureLoad(hdr, vec2<i32>(clamp(vec2<f32>(src.x, v2) * dims, vec2<f32>(0.0), dims - 1.0)), 0).rgb;
        out = mix(out, up * vec3<f32>(0.96, 0.98, 1.0), k * smoothstep(0.0, 0.002, -e));
    }
    return mix(c, out, far);
}

fn graded(in: FsTri) -> vec3<f32> {
    let p = vec2<i32>(in.clip.xy);
    var c: vec3<f32>;
    if (lens.p.y > 0.0 || lens.p.w > 0.0) {
        c = defocus(in.clip.xy, in.uv);
    } else {
        c = soften_distance(p, textureLoad(hdr, p, 0).rgb);
    }
    c = mirror(in.clip.xy, in.uv, c);
    c = mirage(in.uv, c);
    // Speed: a radial blur that leaves the middle sharp and streaks the edges,
    // and a darker rim, both growing with post.s.y.
    let fx = post.s.y;
    if (fx > 0.001) {
        let dims = vec2<f32>(textureDimensions(hdr));
        let centre = dims * 0.5;
        let d = in.clip.xy - centre;
        let r = length(d) / length(centre);
        let k = fx * 0.07 * smoothstep(0.2, 0.9, r);
        var acc = c;
        for (var i = 1; i <= 8; i++) {
            let q = in.clip.xy - d * k * f32(i) / 8.0;
            acc += textureLoad(hdr, vec2<i32>(clamp(q, vec2<f32>(0.0), dims - 1.0)), 0).rgb;
        }
        c = acc / 9.0;
        c *= 1.0 - fx * 0.1 * smoothstep(0.6, 1.1, r);
    }
    if (shafts.z > 0.001) {
        c += textureSampleLevel(rays_tex, lin, in.uv, 0.0).rgb;
    }
    // The grade. Every amount comes from the look's knobs (render/grade.rs),
    // so the LOOK page changes it while the game runs; both styles take the
    // same path and differ only in their numbers.
    c = aces(c * max(gd.c.z, 0.0));
    // Time of day (post.s.w): warmer through golden hour, cooler and a little
    // bluer by moonlight.
    if (gd.e.x > 0.0) {
        let warm = max(post.s.w, 0.0) * gd.e.x;
        let cool = max(-post.s.w, 0.0) * gd.e.x;
        c = c * mix(vec3<f32>(1.0), vec3<f32>(1.06, 0.99, 0.84), min(warm, 1.0));
        // Moonlight: colour drains toward a blue-violet grey, as eyes see at
        // night, so the gold grass reads silver rather than yellow. In the
        // night proper (post.s.w below -1) the grey turns a paler blue.
        let lm = lum(c);
        let tint = mix(vec3<f32>(0.86, 0.9, 1.18), vec3<f32>(0.84, 0.97, 1.12), clamp((cool - 1.0) * 2.0, 0.0, 1.0));
        c = mix(c, lm * tint, min(cool, 1.0) * 0.55);
    }
    // The luminance strip, twice over. As a tint it multiplies, so each pixel
    // keeps its own hue (this is the old split tone: teal shadows, gold
    // highlights). As a gradient map it replaces the hue with the strip's at
    // the pixel's own brightness, which is the flat colour of the calm look.
    let l = lum(c);
    if (gd.c.w > 0.0) {
        c = mix(c, c * strip(l, 1.0), gd.c.w);
    }
    if (gd.d.x > 0.0) {
        let m = strip(l, 2.0);
        c = mix(c, l * (m / max(lum(m), 0.001)), min(gd.d.x, 1.0));
    }
    let l2 = lum(c);
    c = mix(vec3<f32>(l2), c, gd.d.y);
    if (gd.d.z > 0.0) {
        c = mix(c, c * c * (3.0 - 2.0 * c), min(gd.d.z, 1.0));
    }
    // Speed streaks: thin dashes running outward along rays at the rim, a
    // few lanes of the circle at a time, each with its own speed and phase.
    // They go light on a dark view and dark on a bright one, so they show
    // against sky as well as ground. After the grade, where the numbers are
    // the ones the screen gets.
    let st = post.x.x;
    if (st > 0.001) {
        let dims = vec2<f32>(textureDimensions(hdr));
        let centre = dims * 0.5;
        let d = in.clip.xy - centre;
        let r = length(d) / length(centre);
        let rim = smoothstep(0.38, 0.95, r);
        if (rim > 0.001) {
            let lanes = 44.0;
            let a = (atan2(d.y, d.x) * 0.159155 + 0.5) * lanes;
            let lane = floor(a);
            // Thin across its lane, and only some lanes carry a streak.
            let across = abs(fract(a) - 0.5) * 2.0;
            let line = 1.0 - smoothstep(0.1, 0.7, across);
            let on = step(0.45, hash1(lane));
            let sp = 1.1 + 1.3 * hash1(lane + 31.0);
            let u = fract(hash1(lane + 7.0) + post.x.y * sp);
            // Squared by hand: pow() of a negative base is not defined.
            let g = (r - mix(0.35, 1.15, u)) * 4.2;
            let dash = exp(-g * g);
            let s = st * rim * line * on * dash;
            // Relative to the pixel, and blended rather than switched: a fixed
            // navy over bright sunset cloud read as a dark smudge at the rim,
            // and the hard switch flickered along every cloud edge.
            let lc = lum(c);
            let lift = c + (vec3<f32>(1.0) - c) * 0.6;
            let sink = c * 0.8;
            let to = mix(lift, sink, smoothstep(0.5, 0.75, lc));
            c = mix(c, to, s * 0.5);
        }
    }
    let v = in.uv - 0.5;
    c *= 1.0 - dot(v, v) * gd.d.w;
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}
