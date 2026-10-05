// Color node passes on mattes and pictures (layout Main). A matte is kept in the red channel
// (shown as gray with alpha 1). tex0 and tex1 are the two inputs.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn cg_band(v: f32, low: f32, high: f32, ls: f32, hs: f32) -> f32 {
    let a = clamp((v - (low - ls)) / max(ls, 1e-4), 0.0, 1.0);
    let b = 1.0 - clamp((v - high) / max(hs, 1e-4), 0.0, 1.0);
    var m = a * b;
    if (ls <= 0.0) { m = select(0.0, b, v >= low); }
    if (hs <= 0.0) { m = m * select(0.0, 1.0, v <= high); }
    return m;
}

fn gray(m: f32) -> vec4<f32> {
    return vec4<f32>(m, m, m, 1.0);
}

// Qualifier on tex0. prm: 0 use hue, 1 hue center, 2 hue width, 3 hue softness, 4-7 saturation
// (low, high, low soft, high soft), 8-11 luminance, 12 invert
@fragment
fn fs_cg_qualify(in: VOut) -> @location(0) vec4<f32> {
    let c = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let hsv = rgb2hsv(rgb);
    var m = 1.0;
    if (prm(0) > 0.5) {
        let d = abs(fract(hsv.x - prm(1) + 0.5) - 0.5);
        m = 1.0 - clamp((d - prm(2) * 0.5) / max(prm(3), 1e-4), 0.0, 1.0);
        // a gray has no hue to select
        m = m * clamp(hsv.y * 8.0, 0.0, 1.0);
    }
    m = m * cg_band(hsv.y, prm(4), prm(5), prm(6), prm(7));
    m = m * cg_band(luma(rgb), prm(8), prm(9), prm(10), prm(11));
    if (prm(12) > 0.5) { m = 1.0 - m; }
    return gray(m);
}

// Clean black / white, gain, offset and invert of a matte. prm: 0 gain, 1 offset, 2 invert,
// 3 clean black, 4 clean white
@fragment
fn fs_cg_adjust(in: VOut) -> @location(0) vec4<f32> {
    var m = textureSampleLevel(tex0, samp, in.uv, 0.0).r;
    m = clamp((m - prm(3)) / max(prm(4) - prm(3), 1e-4), 0.0, 1.0);
    m = clamp(m * prm(0) + prm(1), 0.0, 1.0);
    if (prm(2) > 0.5) { m = 1.0 - m; }
    return gray(m);
}

fn cg_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - (a + ab * t));
}

// One window added (largest wins) to the matte in tex0. prm: 0 kind (0 rectangle, 1 ellipse,
// 2 linear, 3 polygon), 1-2 center, 3-4 size, 5 rotation (radians), 6 softness, 7 invert,
// 8 points, 9.. up to 8 points
@fragment
fn fs_cg_window(in: VOut) -> @location(0) vec4<f32> {
    let acc = textureSampleLevel(tex0, samp, in.uv, 0.0).r;
    let size = u.out_size;
    let kind = i32(prm(0) + 0.5);
    let ca = cos(prm(5));
    let sa = sin(prm(5));
    var q = (in.uv - prm2(1)) * size;
    q = vec2<f32>(q.x * ca + q.y * sa, -q.x * sa + q.y * ca);
    let hext = max(prm2(3) * size * 0.5, vec2<f32>(1.0));
    let soft = max(prm(6) * min(hext.x, hext.y), 0.75);
    var sd = 0.0;
    if (kind == 0) {
        let d = abs(q) - hext;
        sd = max(d.x, d.y);
    } else if (kind == 1) {
        sd = (length(q / hext) - 1.0) * min(hext.x, hext.y);
    } else if (kind == 2) {
        // full on the upper side of the center line, fading over the softness
        let len = max(prm(6), 0.01) * size.y;
        sd = (q.y) * 1.0;
        let m = clamp(0.5 - q.y / (2.0 * len), 0.0, 1.0);
        var r = m;
        if (prm(7) > 0.5) { r = 1.0 - r; }
        return gray(max(acc, r));
    } else {
        let n = i32(prm(8) + 0.5);
        let p = in.uv * size;
        var inside = false;
        var dist = 1e9;
        for (var i = 0; i < 8; i = i + 1) {
            if (i >= n) { break; }
            let j = (i + 1) % n;
            let a = vec2<f32>(prm(9u + u32(i) * 2u), prm(10u + u32(i) * 2u)) * size;
            let b = vec2<f32>(prm(9u + u32(j) * 2u), prm(10u + u32(j) * 2u)) * size;
            dist = min(dist, cg_seg(p, a, b));
            if ((a.y > p.y) != (b.y > p.y)
                && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) {
                inside = !inside;
            }
        }
        sd = select(dist, -dist, inside);
    }
    var cov = clamp(0.5 - sd / (2.0 * soft), 0.0, 1.0);
    if (prm(7) > 0.5) { cov = 1.0 - cov; }
    return gray(max(acc, cov));
}

@fragment
fn fs_cg_mul(in: VOut) -> @location(0) vec4<f32> {
    let a = textureSampleLevel(tex0, samp, in.uv, 0.0).r;
    let b = textureSampleLevel(tex1, samp, in.uv, 0.0).r;
    return gray(a * b);
}

@fragment
fn fs_cg_invert_matte(in: VOut) -> @location(0) vec4<f32> {
    return gray(1.0 - textureSampleLevel(tex0, samp, in.uv, 0.0).r);
}

// A picture (tex0) through a matte (tex1): all channels scaled, so the matte becomes alpha.
@fragment
fn fs_cg_cut(in: VOut) -> @location(0) vec4<f32> {
    let p = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let m = textureSampleLevel(tex1, samp, in.uv, 0.0).r;
    return p * m;
}

// The picture with what the matte leaves out dimmed.
@fragment
fn fs_cg_highlight(in: VOut) -> @location(0) vec4<f32> {
    let p = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let m = textureSampleLevel(tex1, samp, in.uv, 0.0).r;
    let dim = vec4<f32>(vec3<f32>(luma(p.rgb)) * 0.25, p.a);
    return mix(dim, p, m);
}

// Weighted mean of what has been mixed so far (tex0, weight prm0) and the next (tex1, prm1).
@fragment
fn fs_cg_wmix(in: VOut) -> @location(0) vec4<f32> {
    let a = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let b = textureSampleLevel(tex1, samp, in.uv, 0.0);
    return (a * prm(0) + b * prm(1)) / max(prm(0) + prm(1), 1e-4);
}

// tex1 (premultiplied) over tex0 with a blend mode. prm: 0 mode (0 normal, 1 add, 2 screen,
// 3 multiply, 4 overlay, 5 difference), 1 opacity
@fragment
fn fs_cg_blend(in: VOut) -> @location(0) vec4<f32> {
    let base = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let top = textureSampleLevel(tex1, samp, in.uv, 0.0) * prm(1);
    let mode = i32(prm(0) + 0.5);
    if (mode == 0) {
        return top + base * (1.0 - top.a);
    }
    let b = unpremul(base).rgb;
    let t = unpremul(top).rgb;
    var r = t;
    if (mode == 1) {
        r = b + t;
    } else if (mode == 2) {
        r = 1.0 - (1.0 - b) * (1.0 - t);
    } else if (mode == 3) {
        r = b * t;
    } else if (mode == 4) {
        r = select(1.0 - 2.0 * (1.0 - b) * (1.0 - t), 2.0 * b * t, b < vec3<f32>(0.5));
    } else {
        r = abs(b - t);
    }
    let rgb = top.a * base.a * r + (1.0 - base.a) * top.rgb + (1.0 - top.a) * base.rgb;
    return vec4<f32>(rgb, top.a + base.a - top.a * base.a);
}
