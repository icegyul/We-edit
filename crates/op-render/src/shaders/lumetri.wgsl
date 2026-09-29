// Lumetri Color: one grading pass (LUTs, white balance, tone, creative, curves, wheels,
// vignette). Sharpen runs as a separate pass. The formulas are this project's own; they aim for
// the documented behavior of each control, not numerical parity (docs/effects-catalog.md 7).
//
// prm: 0 temperature, 1 tint, 2 exposure, 3 contrast, 4 highlights, 5 shadows, 6 whites,
// 7 blacks, 8 saturation, 9 look intensity, 10 faded film, 11 vibrance, 12 creative saturation,
// 13-14 shadow tint wheel, 15-16 highlight tint wheel, 17 tint balance, 18-19 shadows wheel,
// 20 shadows luma, 21-22 midtones wheel, 23 midtones luma, 24-25 highlights wheel,
// 26 highlights luma, 27 vignette amount, 28 midpoint, 29 roundness, 30 feather,
// 31 input LUT on, 32 look LUT on, 33 curves on

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var curves: texture_2d<f32>;
@group(0) @binding(4) var lut_in: texture_3d<f32>;
@group(0) @binding(5) var lut_look: texture_3d<f32>;

fn apply_lut(t: texture_3d<f32>, c: vec3<f32>) -> vec3<f32> {
    let n = f32(textureDimensions(t).x);
    let p = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * (n - 1.0) / n + 0.5 / n;
    return textureSampleLevel(t, samp, p, 0.0).rgb;
}

fn curve(v: f32, ch: i32) -> f32 {
    let x = clamp(v, 0.0, 1.0) * 255.0 / 256.0 + 0.5 / 256.0;
    let c = textureSampleLevel(curves, samp, vec2<f32>(x, 0.5), 0.0);
    var y = c.r;
    if (ch == 1) { y = c.g; }
    if (ch == 2) { y = c.b; }
    if (ch == 3) { y = c.a; }
    // keep values beyond the displayable range moving with the curve's end slope
    return y + max(v - 1.0, 0.0) + min(v, 0.0);
}

// Color of a wheel offset (x, y in the unit disk): hue by angle, strength by radius.
fn wheel(w: vec2<f32>) -> vec3<f32> {
    let r = length(w);
    if (r < 1e-5) {
        return vec3<f32>(0.0);
    }
    let hue = fract(atan2(w.y, w.x) / (2.0 * PI));
    let c = hsv2rgb(vec3<f32>(hue, 1.0, 1.0));
    return (c - luma(c)) * min(r, 1.0);
}

@fragment
fn fs_lumetri(in: VOut) -> @location(0) vec4<f32> {
    let src = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    var rgb = src.rgb;
    if (prm(31) > 0.5) {
        rgb = apply_lut(lut_in, rgb);
    }

    // white balance and exposure in linear light
    var lin = to_linear(max(rgb, vec3<f32>(0.0)));
    let t = prm(0) / 100.0;
    let tn = prm(1) / 100.0;
    var gains = vec3<f32>(1.0 + t * 0.3, 1.0 - tn * 0.25, 1.0 - t * 0.3);
    gains = gains / max(luma(gains), 1e-4);
    lin = lin * gains * exp2(prm(2));
    rgb = from_linear(lin);

    // tone
    let k = 1.0 + prm(3) / 100.0 * 0.6;
    let y0 = luma(rgb);
    rgb = rgb + ((y0 - 0.5) * k + 0.5 - y0);
    let y = clamp(luma(rgb), 0.0, 1.0);
    let wh = smoothstep(0.45, 1.0, y);
    let ws = 1.0 - smoothstep(0.0, 0.55, y);
    rgb = rgb + prm(4) / 100.0 * 0.3 * wh;
    rgb = rgb + prm(5) / 100.0 * 0.3 * ws;
    rgb = rgb * (1.0 + prm(6) / 100.0 * 0.25 * y * y);
    rgb = rgb + prm(7) / 100.0 * 0.12 * (1.0 - y) * (1.0 - y);
    rgb = mix(vec3<f32>(luma(rgb)), rgb, prm(8) / 100.0);

    // creative
    if (prm(32) > 0.5) {
        rgb = mix(rgb, apply_lut(lut_look, rgb), prm(9) / 100.0);
    }
    let fade = prm(10) / 100.0;
    rgb = mix(rgb, vec3<f32>(0.1) + rgb * 0.85, fade);
    let s = max(rgb.r, max(rgb.g, rgb.b)) - min(rgb.r, min(rgb.g, rgb.b));
    let l1 = luma(rgb);
    rgb = mix(vec3<f32>(l1), rgb, 1.0 + prm(11) / 100.0 * (1.0 - clamp(s, 0.0, 1.0)));
    rgb = mix(vec3<f32>(luma(rgb)), rgb, prm(12) / 100.0);
    let pivot = clamp(0.5 + prm(17) / 100.0 * 0.35, 0.05, 0.95);
    let yl = clamp(luma(rgb), 0.0, 1.0);
    rgb = rgb + wheel(prm2(13)) * 0.25 * (1.0 - smoothstep(0.0, pivot, yl));
    rgb = rgb + wheel(prm2(15)) * 0.25 * smoothstep(pivot, 1.0, yl);

    // curves
    if (prm(33) > 0.5) {
        rgb = vec3<f32>(curve(rgb.r, 0), curve(rgb.g, 0), curve(rgb.b, 0));
        rgb = vec3<f32>(curve(rgb.r, 1), curve(rgb.g, 2), curve(rgb.b, 3));
    }

    // color wheels: lift, gamma, gain
    let lift = wheel(prm2(18)) * 0.3 + prm(20) / 100.0 * 0.3;
    let gamma = wheel(prm2(21)) * 0.5 + prm(23) / 100.0 * 0.5;
    let gain = wheel(prm2(24)) * 0.4 + prm(26) / 100.0 * 0.4;
    rgb = rgb * (1.0 + gain) + lift * (1.0 - clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    rgb = sign(rgb) * pow(abs(rgb), exp2(-gamma));

    // vignette
    let amount = prm(27);
    if (abs(amount) > 1e-4) {
        let aspect = u.out_size.x / max(u.out_size.y, 1.0);
        let rnd = prm(29) / 100.0;
        var d = in.uv - 0.5;
        // roundness 0 follows the frame shape, +100 is a circle, -100 a stretched ellipse
        let sx = mix(1.0, aspect, clamp(rnd, 0.0, 1.0)) * mix(1.0, 0.6, clamp(-rnd, 0.0, 1.0));
        d.x = d.x * sx;
        let r = length(d) * 2.0 / max(sx, 1.0) * 0.8;
        let mid = prm(28) / 100.0;
        let feather = max(prm(30) / 100.0, 0.01);
        let v = smoothstep(mid - feather * 0.5, mid + feather * 0.5, r);
        let a = amount / 5.0;
        if (a < 0.0) {
            rgb = rgb * (1.0 + a * v);
        } else {
            rgb = rgb + (1.0 - rgb) * a * v;
        }
    }
    return premul(vec4<f32>(rgb, src.a));
}
