// Color node passes that need curves, a LUT or a matte (layout Grade):
// tex0 image, tex1 curves (256x2: row 0 master/R/G/B, row 1 hue-hue/hue-sat/lum-sat/sat-sat) or,
// for fs_cg_limit, the picture before the node, lut 3D LUT, matte (red channel).
//
// fs_cg_grade prm: 0-3 lift, 4-7 gamma, 8-11 gain, 12-15 offset (r g b master each),
// 16 contrast, 17 pivot, 18 saturation, 19 hue degrees, 20 temperature, 21 tint, 22 shadows,
// 23 highlights, 24 color boost, 25 rgb curves on, 26 hsl curves on, 27 lut on, 28 lut amount,
// 29 amount, 30 matte on. The formulas are this project's own.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var curves: texture_2d<f32>;
@group(0) @binding(4) var lut: texture_3d<f32>;
@group(0) @binding(5) var matte: texture_2d<f32>;

fn cg_lut(c: vec3<f32>) -> vec3<f32> {
    let n = f32(textureDimensions(lut).x);
    let p = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * (n - 1.0) / n + 0.5 / n;
    return textureSampleLevel(lut, samp, p, 0.0).rgb;
}

fn cg_curve(v: f32, ch: i32, row: i32) -> f32 {
    let x = clamp(v, 0.0, 1.0) * 255.0 / 256.0 + 0.5 / 256.0;
    let c = textureSampleLevel(curves, samp, vec2<f32>(x, (f32(row) + 0.5) / 2.0), 0.0);
    var y = c.r;
    if (ch == 1) { y = c.g; }
    if (ch == 2) { y = c.b; }
    if (ch == 3) { y = c.a; }
    return y;
}

// A curve on the RGB values keeps going with its slope past the displayable range.
fn cg_tone(v: f32, ch: i32) -> f32 {
    return cg_curve(v, ch, 0) + max(v - 1.0, 0.0) + min(v, 0.0);
}

@fragment
fn fs_cg_grade(in: VOut) -> @location(0) vec4<f32> {
    let src = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    var rgb = src.rgb;

    // offset moves the whole range
    let off = prm4(12);
    rgb = rgb + (off.rgb + off.a) * 0.25;

    // white balance in linear light
    let t = prm(20) / 100.0;
    let tn = prm(21) / 100.0;
    if (abs(t) > 1e-4 || abs(tn) > 1e-4) {
        var gains = vec3<f32>(1.0 + t * 0.3, 1.0 - tn * 0.25, 1.0 - t * 0.3);
        gains = gains / max(luma(gains), 1e-4);
        rgb = from_linear(to_linear(max(rgb, vec3<f32>(0.0))) * gains);
    }

    // contrast around the pivot
    rgb = (rgb - prm(17)) * prm(16) + prm(17);

    // lift (shadows), gamma (midtones), gain (highlights)
    let lift = prm4(0);
    let gamma = prm4(4);
    let gain = prm4(8);
    let lf = (lift.rgb + lift.a) * 0.3;
    let gf = (gain.rgb + gain.a) * 0.5 + 1.0;
    rgb = rgb * gf + lf * (1.0 - clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    rgb = sign(rgb) * pow(abs(rgb), exp2(-(gamma.rgb + gamma.a) * 0.6));

    // shadows and highlights
    let y = clamp(luma(rgb), 0.0, 1.0);
    rgb = rgb + prm(23) / 100.0 * 0.3 * smoothstep(0.45, 1.0, y);
    rgb = rgb + prm(22) / 100.0 * 0.3 * (1.0 - smoothstep(0.0, 0.55, y));

    // custom curves
    if (prm(25) > 0.5) {
        rgb = vec3<f32>(cg_tone(rgb.r, 0), cg_tone(rgb.g, 0), cg_tone(rgb.b, 0));
        rgb = vec3<f32>(cg_tone(rgb.r, 1), cg_tone(rgb.g, 2), cg_tone(rgb.b, 3));
    }

    // saturation, boost, hue
    let l = luma(rgb);
    rgb = mix(vec3<f32>(l), rgb, prm(18));
    let chroma = clamp(max(rgb.r, max(rgb.g, rgb.b)) - min(rgb.r, min(rgb.g, rgb.b)), 0.0, 1.0);
    rgb = mix(vec3<f32>(luma(rgb)), rgb, 1.0 + prm(24) / 100.0 * (1.0 - chroma));
    if (abs(prm(19)) > 1e-3 || prm(26) > 0.5) {
        var hsv = rgb2hsv(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
        let over = rgb - clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        hsv.x = hsv.x + prm(19) / 360.0;
        if (prm(26) > 0.5) {
            // 0.5 on a curve leaves the value alone
            hsv.x = hsv.x + (cg_curve(hsv.x, 0, 1) - 0.5);
            var k = 2.0 * cg_curve(hsv.x, 1, 1);
            k = k * 2.0 * cg_curve(clamp(luma(rgb), 0.0, 1.0), 2, 1);
            k = k * 2.0 * cg_curve(hsv.y, 3, 1);
            hsv.y = clamp(hsv.y * k, 0.0, 1.0);
        }
        hsv.x = fract(hsv.x);
        rgb = hsv2rgb(hsv) + over;
    }

    if (prm(27) > 0.5) {
        rgb = mix(rgb, cg_lut(rgb), prm(28));
    }

    var k = prm(29);
    if (prm(30) > 0.5) {
        k = k * textureSampleLevel(matte, samp, in.uv, 0.0).r;
    }
    return premul(vec4<f32>(mix(src.rgb, rgb, k), src.a));
}

// The node's result inside its matte: picture in tex0 (graded), tex1 the picture before.
@fragment
fn fs_cg_limit(in: VOut) -> @location(0) vec4<f32> {
    let g = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let o = textureSampleLevel(curves, samp, in.uv, 0.0);
    let m = textureSampleLevel(matte, samp, in.uv, 0.0).r;
    return mix(o, g, m);
}
