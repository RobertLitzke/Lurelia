// Occlusion culling (render/occlusion.rs). At the end of each frame the depth
// buffer is boiled down into a pyramid where each texel holds the farthest
// depth under it (reversed Z: the smallest value). The next frame tests each
// visible chunk's box against it, as seen from the frame the pyramid came
// from, and turns off the draws of chunks that were wholly hidden there.

// ---- the pyramid -------------------------------------------------------------

@group(0) @binding(0) var src_depth: DEPTH_TEXTURE;
@group(0) @binding(1) var src_mip: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<r32float, write>;

// Level 0 is half the screen in each direction (rounded up): each texel takes
// the farthest of the 2x2 pixels and all their samples.
@compute @workgroup_size(8, 8)
fn cs_first(@builtin(global_invocation_id) id: vec3<u32>) {
    let dn = textureDimensions(dst);
    if (id.x >= dn.x || id.y >= dn.y) {
        return;
    }
    let sn = textureDimensions(src_depth);
    var far = 1.0;
    for (var y = 0u; y < 2u; y++) {
        for (var x = 0u; x < 2u; x++) {
            let p = min(id.xy * 2u + vec2<u32>(x, y), sn - 1u);
            for (var s = 0u; s < DEPTH_SAMPLES; s++) {
                far = min(far, DEPTH_LOAD);
            }
        }
    }
    textureStore(dst, id.xy, vec4<f32>(far, 0.0, 0.0, 0.0));
}

// Each further level takes the farthest of the 2x2 texels below it. Sizes
// halve rounding down, so where the level below is odd the last row and
// column take three.
@compute @workgroup_size(8, 8)
fn cs_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let dn = textureDimensions(dst);
    if (id.x >= dn.x || id.y >= dn.y) {
        return;
    }
    let sn = textureDimensions(src_mip);
    let odd = (sn & vec2<u32>(1u)) == vec2<u32>(1u);
    let nx = select(2u, 3u, id.x == dn.x - 1u && odd.x);
    let ny = select(2u, 3u, id.y == dn.y - 1u && odd.y);
    var far = 1.0;
    for (var y = 0u; y < ny; y++) {
        for (var x = 0u; x < nx; x++) {
            let p = min(id.xy * 2u + vec2<u32>(x, y), sn - 1u);
            far = min(far, textureLoad(src_mip, p, 0).r);
        }
    }
    textureStore(dst, id.xy, vec4<f32>(far, 0.0, 0.0, 0.0));
}

// ---- the test ------------------------------------------------------------------

struct Cull {
    // The view-projection the pyramid was drawn with.
    view_proj: mat4x4<f32>,
    // Screen width and height in pixels, then the pyramid's level count.
    screen: vec4<f32>,
    // x: how many draws to look at.
    count: vec4<u32>,
};

@group(0) @binding(3) var<uniform> c: Cull;
@group(0) @binding(4) var pyramid: texture_2d<f32>;
// Two vec4s per chunk: its box's min and max corners.
@group(0) @binding(5) var<storage, read> boxes: array<vec4<f32>>;
// The chunk each draw belongs to; NO_CHUNK for draws never culled (shadows).
@group(0) @binding(6) var<storage, read> owner: array<u32>;
// The indirect draws, five words each; word 1 is the instance count.
@group(0) @binding(7) var<storage, read_write> draws: array<u32>;

const NO_CHUNK: u32 = 0xffffffffu;

// Whether the box was wholly behind what the pyramid's frame drew. Anything
// that frame couldn't see (off its screen, crossing its near plane) counts as
// visible.
fn hidden(lo3: vec3<f32>, hi3: vec3<f32>) -> bool {
    var lo = vec2<f32>(1e9);
    var hi = vec2<f32>(-1e9);
    var near = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let corner = select(lo3, hi3, vec3<bool>((i & 1u) != 0u, (i & 2u) != 0u, (i & 4u) != 0u));
        let p = c.view_proj * vec4<f32>(corner, 1.0);
        if (p.w <= 0.05) {
            return false;
        }
        let n = p.xyz / p.w;
        lo = min(lo, n.xy);
        hi = max(hi, n.xy);
        near = max(near, n.z);
    }
    if (lo.x < -1.0 || lo.y < -1.0 || hi.x > 1.0 || hi.y > 1.0 || near >= 1.0) {
        return false;
    }
    let size = c.screen.xy;
    // Pixel rectangle, y down.
    let p0 = vec2<f32>(lo.x * 0.5 + 0.5, 0.5 - hi.y * 0.5) * size;
    let p1 = vec2<f32>(hi.x * 0.5 + 0.5, 0.5 - lo.y * 0.5) * size;
    let extent = max(p1.x - p0.x, p1.y - p0.y);
    // A level-L texel covers 2^(L+1) pixels (the last row and column a
    // little more); pick the level where the rectangle spans at most about
    // three texels each way.
    let levels = u32(c.screen.z);
    let level = min(u32(max(ceil(log2(max(extent, 1.0))) - 2.0, 0.0)), levels - 1u);
    let texel = f32(1u << (level + 1u));
    let dims = textureDimensions(pyramid, level);
    let t0 = min(vec2<u32>(max(p0 / texel, vec2<f32>(0.0))), dims - 1u);
    let t1 = min(vec2<u32>(max(p1 / texel, vec2<f32>(0.0))), dims - 1u);
    var far = 1.0;
    for (var y = t0.y; y <= t1.y; y++) {
        for (var x = t0.x; x <= t1.x; x++) {
            far = min(far, textureLoad(pyramid, vec2<u32>(x, y), i32(level)).r);
        }
    }
    // Reversed Z: hidden when even the farthest thing drawn there is nearer
    // than the box's nearest point.
    return far > near;
}

@compute @workgroup_size(64)
fn cs_cull(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= c.count.x) {
        return;
    }
    let k = owner[i];
    if (k == NO_CHUNK) {
        return;
    }
    if (hidden(boxes[2u * k].xyz, boxes[2u * k + 1u].xyz)) {
        draws[i * 5u + 1u] = 0u;
    }
}
