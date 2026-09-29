// Per-pixel color effects. tex0 is the layer (premultiplied); the output has the same size.
// Each function documents its prm() layout. Formulas are this project's own and provisional
// until measured against reference output (GATE-FX-002/003).

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn src(uv: vec2<f32>) -> vec4<f32> {
    return unpremul(textureSampleLevel(tex0, samp, uv, 0.0));
}

fn out(c: vec4<f32>) -> vec4<f32> {
    return premul(c);
}

// prm(0) brightness -100..100, prm(1) contrast -100..100
@fragment
fn fs_brightness_contrast(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let b = prm(0) / 100.0 * 0.3;
    var k = 1.0 + prm(1) / 100.0;
    if (prm(1) > 0.0) {
        k = 1.0 / max(1.0 - prm(1) / 101.0, 0.01);
    }
    let rgb = (c.rgb - 0.5) * k + 0.5 + b;
    return out(vec4<f32>(rgb, c.a));
}

// prm(0) brightness, prm(1) contrast %, prm(2) hue degrees, prm(3) saturation %,
// prm(4) split screen, prm(5) split percent
@fragment
fn fs_procamp(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    if (prm(4) > 0.5 && in.uv.x > prm(5) / 100.0) {
        return out(c);
    }
    var rgb = (c.rgb - 0.5) * (prm(1) / 100.0) + 0.5 + prm(0) / 100.0 * 0.5;
    // hue and saturation in a YUV-like opponent space
    let y = luma(rgb);
    let cb = rgb.b - y;
    let cr = rgb.r - y;
    let a = prm(2) * PI / 180.0;
    let s = prm(3) / 100.0;
    let cb2 = (cb * cos(a) - cr * sin(a)) * s;
    let cr2 = (cb * sin(a) + cr * cos(a)) * s;
    let r = y + cr2;
    let b = y + cb2;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    return out(vec4<f32>(r, g, b, c.a));
}

// prm(0) in black, prm(1) in white, prm(2) out black, prm(3) out white (0..255), prm(4) gamma
@fragment
fn fs_levels(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let ib = prm(0) / 255.0;
    let iw = max(prm(1) / 255.0, ib + 1e-4);
    var x = clamp((c.rgb - ib) / (iw - ib), vec3<f32>(0.0), vec3<f32>(1.0));
    x = pow(x, vec3<f32>(1.0 / max(prm(4), 0.01)));
    let rgb = prm(2) / 255.0 + x * (prm(3) - prm(2)) / 255.0;
    return out(vec4<f32>(rgb, c.a));
}

// prm(0..11) rows R, G, B as (r, g, b, const) in percent; prm(12) monochrome
@fragment
fn fs_channel_mixer(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let rr = dot(c.rgb, prm3(0)) / 100.0 + prm(3) / 100.0;
    var gg = dot(c.rgb, prm3(4)) / 100.0 + prm(7) / 100.0;
    var bb = dot(c.rgb, prm3(8)) / 100.0 + prm(11) / 100.0;
    if (prm(12) > 0.5) {
        gg = rr;
        bb = rr;
    }
    return out(vec4<f32>(rr, gg, bb, c.a));
}

// prm(0) black input, prm(1) white input (0..255), prm(2) softness %, prm(3) invert
@fragment
fn fs_extract(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let l = luma(c.rgb) * 255.0;
    let soft = prm(2) / 100.0 * 64.0 + 1e-3;
    let lo = smoothstep(prm(0) - soft, prm(0), l);
    let hi = 1.0 - smoothstep(prm(1), prm(1) + soft, l);
    var v = lo * hi;
    if (prm(3) > 0.5) {
        v = 1.0 - v;
    }
    return out(vec4<f32>(vec3<f32>(v), c.a));
}

// prm(0) channel: 0 RGB, 1 R, 2 G, 3 B, 4 luminance, 5 alpha; prm(1) blend with original %
@fragment
fn fs_invert(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    var o = c;
    let ch = i32(prm(0) + 0.5);
    switch ch {
        case 0: { o = vec4<f32>(1.0 - c.rgb, c.a); }
        case 1: { o.r = 1.0 - c.r; }
        case 2: { o.g = 1.0 - c.g; }
        case 3: { o.b = 1.0 - c.b; }
        case 4: {
            let y = luma(c.rgb);
            o = vec4<f32>(c.rgb + (1.0 - 2.0 * y), c.a);
        }
        default: { o.a = 1.0 - c.a; }
    }
    return out(mix(o, c, prm(1) / 100.0));
}

// prm(0..2) map black to, prm(3..5) map white to, prm(6) amount %
@fragment
fn fs_tint(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let t = mix(prm3(0), prm3(3), clamp(luma(c.rgb), 0.0, 1.0));
    return out(vec4<f32>(mix(c.rgb, t, prm(6) / 100.0), c.a));
}

@fragment
fn fs_black_white(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    return out(vec4<f32>(vec3<f32>(luma(c.rgb)), c.a));
}

// prm(0..2) shadow RGB, prm(3..5) midtone RGB, prm(6..8) highlight RGB (-100..100),
// prm(9) preserve luminosity
@fragment
fn fs_color_balance(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let l = clamp(luma(c.rgb), 0.0, 1.0);
    let ws = 1.0 - smoothstep(0.0, 0.5, l);
    let wh = smoothstep(0.5, 1.0, l);
    let wm = 1.0 - ws - wh;
    var rgb = c.rgb + (prm3(0) * ws + prm3(3) * wm + prm3(6) * wh) / 100.0 * 0.25;
    if (prm(9) > 0.5) {
        rgb = rgb + (luma(c.rgb) - luma(rgb));
    }
    return out(vec4<f32>(rgb, c.a));
}

// prm(0) amount to decolor %, prm(1..3) color to leave, prm(4) tolerance %, prm(5) softness %,
// prm(6) match by hue
@fragment
fn fs_leave_color(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let key = prm3(1);
    var d: f32;
    if (prm(6) > 0.5) {
        let a = rgb2hsv(c.rgb).x;
        let b = rgb2hsv(key).x;
        let dh = abs(a - b);
        d = min(dh, 1.0 - dh) * 2.0;
    } else {
        d = length(c.rgb - key) / sqrt(3.0);
    }
    let tol = prm(4) / 100.0;
    let soft = prm(5) / 100.0 + 1e-4;
    let keep = 1.0 - smoothstep(tol, tol + soft, d);
    let grey = vec3<f32>(luma(c.rgb));
    let amount = prm(0) / 100.0 * (1.0 - keep);
    return out(vec4<f32>(mix(c.rgb, grey, amount), c.a));
}

// prm(0) gamma (10 = unchanged)
@fragment
fn fs_gamma(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let g = max(prm(0), 0.1) / 10.0;
    return out(vec4<f32>(pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(g)), c.a));
}

// prm(0..2) target, prm(3..5) replacement, prm(6) similarity %, prm(7) solid colors
@fragment
fn fs_color_replace(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let d = length(c.rgb - prm3(0)) / sqrt(3.0);
    let sim = prm(6) / 100.0;
    let k = 1.0 - smoothstep(sim * 0.8, sim + 1e-4, d);
    var rep = prm3(3);
    if (prm(7) < 0.5) {
        // keep the pixel's lightness, take the replacement's hue and saturation
        let hs = rgb2hsv(rep);
        let v = rgb2hsv(c.rgb).z;
        rep = hsv2rgb(vec3<f32>(hs.x, hs.y, v));
    }
    return out(vec4<f32>(mix(c.rgb, rep, k), c.a));
}

// prm(0) levels
@fragment
fn fs_posterize(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let n = max(prm(0), 2.0);
    let rgb = floor(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * n - 1e-4) / (n - 1.0);
    return out(vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a));
}

// prm(0) threshold 0..254
@fragment
fn fs_solarize(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let t = prm(0) / 255.0;
    let rgb = select(c.rgb, 1.0 - c.rgb, c.rgb > vec3<f32>(t));
    return out(vec4<f32>(rgb, c.a));
}

// prm(0) level 0..255
@fragment
fn fs_threshold(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let v = step(prm(0) / 255.0, luma(c.rgb));
    return out(vec4<f32>(vec3<f32>(v), c.a));
}

// prm(0) amount %, prm(1) color noise, prm(2) clipping, prm(3) frame seed
@fragment
fn fs_noise(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let p = pixel(in.uv);
    let seed = prm(3);
    let amt = prm(0) / 100.0;
    var n: vec3<f32>;
    if (prm(1) > 0.5) {
        n = vec3<f32>(hash21(p + seed), hash21(p + seed + 17.3), hash21(p + seed + 41.7)) - 0.5;
    } else {
        n = vec3<f32>(hash21(p + seed) - 0.5);
    }
    var rgb = c.rgb + n * amt;
    if (prm(2) > 0.5) {
        rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return out(vec4<f32>(rgb, c.a));
}

// prm(0,1) start (layer uv), prm(2..4) start color, prm(5,6) end, prm(7..9) end color,
// prm(10) radial, prm(11) blend with original %
@fragment
fn fs_ramp(in: VOut) -> @location(0) vec4<f32> {
    let c = src(in.uv);
    let aspect = vec2<f32>(u.out_size.x / max(u.out_size.y, 1.0), 1.0);
    let a = prm2(0) * aspect;
    let b = prm2(5) * aspect;
    let p = in.uv * aspect;
    var t: f32;
    if (prm(10) > 0.5) {
        t = length(p - a) / max(length(b - a), 1e-5);
    } else {
        let ab = b - a;
        t = dot(p - a, ab) / max(dot(ab, ab), 1e-8);
    }
    let g = mix(prm3(2), prm3(7), clamp(t, 0.0, 1.0));
    let o = vec4<f32>(g, 1.0);
    return premul(mix(o, c, prm(11) / 100.0));
}
