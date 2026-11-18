// Text and shapes in pixel coordinates.

struct Screen {
    size: vec4<f32>,
};
struct Inst {
    rect: vec4<f32>,
    color: vec4<f32>,
    // x: glyph number; yzw: per-kind data (see render/hud.rs).
    glyph: vec4<u32>,
};
@group(0) @binding(0) var<uniform> screen: Screen;
@group(0) @binding(1) var<storage, read> items: array<Inst>;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var notebook: texture_2d<f32>;
@group(0) @binding(4) var backdrop0: texture_2d<f32>;
@group(0) @binding(5) var backdrop1: texture_2d<f32>;
@group(0) @binding(6) var backdrop_samp: sampler;
@group(0) @binding(7) var logo: texture_2d<f32>;
@group(0) @binding(8) var mip_samp: sampler;

// MAP_GLYPH, BACKDROP_GLYPH, LOGO_GLYPH, TEXT_GLYPH, GRAD_H_GLYPH, ATLAS_SIZE,
// ATLAS_SPREAD and LOGO_SIZE come from the prelude.

const K_SOLID: u32 = 0u;
const K_MAP: u32 = 1u;
const K_BACK0: u32 = 2u;
const K_BACK1: u32 = 3u;
const K_TEXT: u32 = 4u;
const K_LOGO: u32 = 5u;
const K_GRAD_H: u32 = 6u;
const K_GRAD_V: u32 = 7u;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) texel: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) kind: u32,
    // TEXT: (screen pixels per unit of distance value, softness in pixels).
    @location(4) @interpolate(flat) params: vec2<f32>,
    @location(5) @interpolate(flat) lod: f32,
    @location(6) @interpolate(flat) color2: vec4<f32>,
};

@vertex
fn vs_hud(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let c = corners[vi];
    let it = items[ii];
    let px = it.rect.xy + it.rect.zw * c;
    var out: Out;
    out.clip = vec4<f32>(px.x / screen.size.x * 2.0 - 1.0, 1.0 - px.y / screen.size.y * 2.0, 0.0, 1.0);
    out.color = it.color;
    out.uv = c;
    out.texel = vec2<f32>(0.0);
    out.params = vec2<f32>(0.0);
    out.lod = 0.0;
    out.color2 = vec4<f32>(0.0);
    let gl = it.glyph.x;
    out.kind = K_SOLID;
    if (gl == MAP_GLYPH) {
        out.kind = K_MAP;
    } else if (gl == BACKDROP_GLYPH) {
        out.kind = K_BACK0;
    } else if (gl == BACKDROP_GLYPH + 1u) {
        out.kind = K_BACK1;
    } else if (gl == TEXT_GLYPH) {
        out.kind = K_TEXT;
        let cell = vec2<f32>(f32(it.glyph.y & 0xffffu), f32(it.glyph.y >> 16u));
        let wh = vec2<f32>(f32(it.glyph.z & 0xffffu), f32(it.glyph.z >> 16u));
        out.texel = (cell + c * wh) / ATLAS_SIZE;
        let ppt = max(it.rect.z / wh.x, 1e-3);
        out.params = vec2<f32>(ppt * 2.0 * ATLAS_SPREAD, bitcast<f32>(it.glyph.w));
        out.lod = max(0.0, log2(1.0 / ppt) - 0.5);
    } else if (gl == LOGO_GLYPH) {
        out.kind = K_LOGO;
        out.lod = max(0.0, log2(LOGO_SIZE.x / max(it.rect.z, 1.0)));
    } else if (gl == GRAD_H_GLYPH || gl == GRAD_H_GLYPH + 1u) {
        out.kind = select(K_GRAD_H, K_GRAD_V, gl == GRAD_H_GLYPH + 1u);
        out.color2 = unpack4x8unorm(it.glyph.y);
    }
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_hud(in: Out) -> @location(0) vec4<f32> {
    if (in.kind == K_MAP) {
        let n = screen.size.z;
        let m = textureLoad(notebook, vec2<i32>(clamp(in.uv * n, vec2<f32>(0.0), vec2<f32>(n - 0.01))), 0);
        return vec4<f32>(m.rgb * in.color.rgb, in.color.a);
    }
    if (in.kind == K_BACK0) {
        let b = textureSampleLevel(backdrop0, backdrop_samp, in.uv, 0.0);
        return vec4<f32>(b.rgb * in.color.rgb, in.color.a);
    }
    if (in.kind == K_BACK1) {
        let b = textureSampleLevel(backdrop1, backdrop_samp, in.uv, 0.0);
        return vec4<f32>(b.rgb * in.color.rgb, in.color.a);
    }
    if (in.kind == K_TEXT) {
        let d = textureSampleLevel(atlas, mip_samp, in.texel, in.lod).r;
        // Distance from the edge in screen pixels, positive inside.
        let dist = (d - 0.5) * in.params.x;
        if (in.params.y > 0.0) {
            // A halo: the whole distance range outside the edge, fading to
            // nothing at the cell's border so no box shows.
            let h = smoothstep(0.04, 0.56, d);
            return vec4<f32>(in.color.rgb, in.color.a * h * h);
        }
        let a = clamp(dist + 0.5, 0.0, 1.0);
        return vec4<f32>(in.color.rgb, in.color.a * a);
    }
    if (in.kind == K_LOGO) {
        // Premultiplied sRGB: divide the alpha back out, then to linear.
        let p = textureSampleLevel(logo, mip_samp, in.uv, in.lod);
        let rgb = srgb_to_linear(p.rgb / max(p.a, 1e-4));
        return vec4<f32>(rgb * in.color.rgb, p.a * in.color.a);
    }
    if (in.kind == K_GRAD_H) {
        return mix(in.color, in.color2, in.uv.x);
    }
    if (in.kind == K_GRAD_V) {
        return mix(in.color, in.color2, in.uv.y);
    }
    return in.color;
}
