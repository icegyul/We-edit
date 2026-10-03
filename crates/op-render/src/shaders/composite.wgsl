// Compositing: layers through Motion onto the sequence frame with blend modes, plus utility
// passes (copy, mix, fill, bars, matte, present).

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

// Samples with transparent outside the texture.
fn sample_clear(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(t));
    // half a texel of soft edge keeps transformed borders antialiased
    let edge = 0.5 / size;
    let inside = step(-edge, uv) * step(uv, 1.0 + edge);
    let c = textureSampleLevel(t, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let fx = clamp(min(uv.x, 1.0 - uv.x) * size.x + 0.5, 0.0, 1.0);
    let fy = clamp(min(uv.y, 1.0 - uv.y) * size.y + 0.5, 0.0, 1.0);
    return c * inside.x * inside.y * fx * fy;
}

// Layer uv for an output pixel through the inverse Motion matrix in prm(0..5).
fn layer_uv(px: vec2<f32>) -> vec2<f32> {
    let lx = prm(0) * px.x + prm(1) * px.y + prm(2);
    let ly = prm(3) * px.x + prm(4) * px.y + prm(5);
    return vec2<f32>(lx, ly) / u.in_size;
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var o = c;
    if (n < 0.0) {
        o = l + (o - l) * l / max(l - n, 1e-6);
    }
    if (x > 1.0) {
        o = l + (o - l) * (1.0 - l) / max(x - l, 1e-6);
    }
    return o;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + (l - lum(c)));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let d = mx - mn;
    if (d <= 1e-6) {
        return vec3<f32>(0.0);
    }
    return (c - mn) * s / d;
}

fn soft_light(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    var d: f32;
    if (b <= 0.25) {
        d = ((16.0 * b - 12.0) * b + 4.0) * b;
    } else {
        d = sqrt(max(b, 0.0));
    }
    return b + (2.0 * s - 1.0) * (d - b);
}

fn color_burn(b: f32, s: f32) -> f32 {
    if (b >= 1.0) { return 1.0; }
    if (s <= 0.0) { return 0.0; }
    return 1.0 - min(1.0, (1.0 - b) / s);
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if (b <= 0.0) { return 0.0; }
    if (s >= 1.0) { return 1.0; }
    return min(1.0, b / (1.0 - s));
}

fn vivid(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return color_burn(b, 2.0 * s);
    }
    return color_dodge(b, 2.0 * s - 1.0);
}

fn pin(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return min(b, 2.0 * s);
    }
    return max(b, 2.0 * s - 1.0);
}

fn hard(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return b * 2.0 * s;
    }
    let t = 2.0 * s - 1.0;
    return b + t - b * t;
}

// B(cb, cs) of the blend modes in catalog order, on straight colors.
fn blend_fn(mode: i32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 2: { return min(cb, cs); }
        case 3: { return cb * cs; }
        case 4: { return vec3<f32>(color_burn(cb.r, cs.r), color_burn(cb.g, cs.g), color_burn(cb.b, cs.b)); }
        case 5: { return max(cb + cs - 1.0, vec3<f32>(0.0)); }
        case 6: { return select(cb, cs, lum(cs) < lum(cb)); }
        case 7: { return max(cb, cs); }
        case 8: { return cb + cs - cb * cs; }
        case 9: { return vec3<f32>(color_dodge(cb.r, cs.r), color_dodge(cb.g, cs.g), color_dodge(cb.b, cs.b)); }
        case 10: { return cb + cs; }
        case 11: { return select(cb, cs, lum(cs) > lum(cb)); }
        case 12: { return vec3<f32>(hard(cs.r, cb.r), hard(cs.g, cb.g), hard(cs.b, cb.b)); }
        case 13: { return vec3<f32>(soft_light(cb.r, cs.r), soft_light(cb.g, cs.g), soft_light(cb.b, cs.b)); }
        case 14: { return vec3<f32>(hard(cb.r, cs.r), hard(cb.g, cs.g), hard(cb.b, cs.b)); }
        case 15: { return vec3<f32>(vivid(cb.r, cs.r), vivid(cb.g, cs.g), vivid(cb.b, cs.b)); }
        case 16: { return cb + 2.0 * cs - 1.0; }
        case 17: { return vec3<f32>(pin(cb.r, cs.r), pin(cb.g, cs.g), pin(cb.b, cs.b)); }
        case 18: { return step(vec3<f32>(1.0), cb + cs); }
        case 19: { return abs(cb - cs); }
        case 20: { return cb + cs - 2.0 * cb * cs; }
        case 21: { return max(cb - cs, vec3<f32>(0.0)); }
        case 22: { return select(cb / max(cs, vec3<f32>(1e-6)), vec3<f32>(1.0), cs <= vec3<f32>(0.0)); }
        case 23: { return set_lum(set_sat(cs, sat(cb)), lum(cb)); }
        case 24: { return set_lum(set_sat(cb, sat(cs)), lum(cb)); }
        case 25: { return set_lum(cs, lum(cb)); }
        case 26: { return set_lum(cb, lum(cs)); }
        default: { return cs; }
    }
}

// Composites premultiplied `s` over premultiplied `d` with a blend mode.
fn blend(d: vec4<f32>, s: vec4<f32>, mode: i32, px: vec2<f32>) -> vec4<f32> {
    if (mode == 0) {
        return s + d * (1.0 - s.a);
    }
    if (mode == 1) {
        // Dissolve: each pixel shows the layer fully or not at all, with probability = alpha
        let r = hash21(px + vec2<f32>(prm(9) * 17.0, prm(9) * 3.0));
        if (r < s.a) {
            return vec4<f32>(s.rgb / max(s.a, 1e-6), 1.0);
        }
        return d;
    }
    let cb = unpremul(d);
    let cs = unpremul(s);
    let b = blend_fn(mode, cb.rgb, cs.rgb);
    let a = s.a + d.a - s.a * d.a;
    let rgb = (1.0 - d.a) * s.rgb + (1.0 - s.a) * d.rgb + s.a * d.a * b;
    return vec4<f32>(rgb, a);
}

// tex0: destination, tex1: layer. prm(6) opacity, prm(7) blend mode, prm(8) linear light.
@fragment
fn fs_composite(in: VOut) -> @location(0) vec4<f32> {
    let px = pixel(in.uv);
    var d = textureSampleLevel(tex0, samp, in.uv, 0.0);
    var s = sample_clear(tex1, layer_uv(px)) * prm(6);
    let linear = prm(8) > 0.5;
    if (linear) {
        d = vec4<f32>(to_linear(d.rgb), d.a);
        s = vec4<f32>(to_linear(s.rgb), s.a);
    }
    var o = blend(d, s, i32(prm(7) + 0.5), px);
    if (linear) {
        o = vec4<f32>(from_linear(o.rgb), o.a);
    }
    return o;
}

// Layer through a transform onto a transparent canvas. prm(6) opacity.
@fragment
fn fs_place(in: VOut) -> @location(0) vec4<f32> {
    return sample_clear(tex1, layer_uv(pixel(in.uv))) * prm(6);
}

@fragment
fn fs_copy(in: VOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(tex0, samp, in.uv, 0.0);
}

// mix(tex0, tex1, prm(0))
@fragment
fn fs_mix(in: VOut) -> @location(0) vec4<f32> {
    let a = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let b = textureSampleLevel(tex1, samp, in.uv, 0.0);
    return mix(a, b, prm(0));
}

// Solid premultiplied color prm(0..3).
@fragment
fn fs_fill(in: VOut) -> @location(0) vec4<f32> {
    let c = prm4(0);
    return vec4<f32>(c.rgb * c.a, c.a);
}

// SMPTE-style color bars (75% bars, reverse bars strip, PLUGE row) in video levels.
@fragment
fn fs_bars(in: VOut) -> @location(0) vec4<f32> {
    let x = in.uv.x;
    let y = in.uv.y;
    var bars = array<vec3<f32>, 7>(
        vec3<f32>(0.75, 0.75, 0.75),
        vec3<f32>(0.75, 0.75, 0.0),
        vec3<f32>(0.0, 0.75, 0.75),
        vec3<f32>(0.0, 0.75, 0.0),
        vec3<f32>(0.75, 0.0, 0.75),
        vec3<f32>(0.75, 0.0, 0.0),
        vec3<f32>(0.0, 0.0, 0.75),
    );
    let i = min(i32(x * 7.0), 6);
    if (y < 0.67) {
        return vec4<f32>(bars[i], 1.0);
    }
    if (y < 0.75) {
        var c = vec3<f32>(0.075);
        if (i % 2 == 0) {
            c = bars[6 - i];
        }
        return vec4<f32>(c, 1.0);
    }
    // bottom row: -I, white, +Q, black, then PLUGE (below black, black, above black)
    let j = x * 6.0;
    if (j < 1.0) { return vec4<f32>(0.0, 0.13, 0.3, 1.0); }
    if (j < 2.0) { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }
    if (j < 3.0) { return vec4<f32>(0.2, 0.0, 0.42, 1.0); }
    if (j < 4.5) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    let k = (j - 4.5) / 1.5 * 3.0;
    if (k < 1.0) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    if (k < 2.0) { return vec4<f32>(0.04, 0.04, 0.04, 1.0); }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// Track matte: tex0 layer (sequence space), tex1 matte. prm(0) use luma, prm(1) reverse.
@fragment
fn fs_matte(in: VOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let m = textureSampleLevel(tex1, samp, in.uv, 0.0);
    var k = m.a;
    if (prm(0) > 0.5) {
        k = clamp(luma(unpremul(m).rgb) * m.a, 0.0, 1.0);
    }
    if (prm(1) > 0.5) {
        k = 1.0 - k;
    }
    return c * k;
}

// Final frame for the monitor: over black, clamped to display range. prm(0) show alpha as gray.
@fragment
fn fs_present(in: VOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    if (prm(0) > 0.5) {
        return vec4<f32>(vec3<f32>(c.a), 1.0);
    }
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// Unpremultiplied RGBA for export formats with alpha.
@fragment
fn fs_straight(in: VOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let s = unpremul(c);
    return vec4<f32>(clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(c.a, 0.0, 1.0));
}
