// Keyers. tex0 is the layer.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn ycc(c: vec3<f32>) -> vec3<f32> {
    let y = luma(c);
    return vec3<f32>(y, (c.b - y) / 1.8556, (c.r - y) / 1.5748);
}

// Matte (0 transparent .. 1 opaque) of a straight color against the key.
// prm: 2..4 key color, 5 transparency, 6 highlight, 7 shadow, 8 tolerance, 9 pedestal (percent)
fn ultra_matte(c: vec3<f32>) -> f32 {
    let key = ycc(prm3(2));
    let p = ycc(c);
    let kc = key.yz;
    let klen = max(length(kc), 1e-3);
    // chroma distance from the key relative to the key's own saturation
    let d = length(p.yz - kc) / klen;
    let ped = prm(9) / 100.0 * 0.6;
    let width = mix(0.08, 1.2, prm(8) / 100.0);
    var a = clamp((d - ped) / width, 0.0, 1.0);
    a = pow(a, mix(0.35, 1.6, 1.0 - prm(5) / 100.0));
    // luma protection: bright and dark areas far in luma from the key stay more opaque
    let dy = p.x - key.x;
    a = max(a, clamp(dy * 2.0, 0.0, 1.0) * prm(6) / 100.0);
    a = max(a, clamp(-dy * 2.0, 0.0, 1.0) * prm(7) / 100.0);
    return a;
}

// prm: 0 output (0 composite, 1 alpha channel, 2 color channel), 2..4 key color,
// 5 transparency, 6 highlight, 7 shadow, 8 tolerance, 9 pedestal, 10 choke, 11 soften,
// 12 contrast, 13 desaturate, 14 range, 15 spill, 16 luma (percent)
@fragment
fn fs_ultra_key(in: VOut) -> @location(0) vec4<f32> {
    let src = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    var a = ultra_matte(src.rgb);
    // soften: blur the matte over a small neighborhood
    let soft = prm(11) / 100.0 * 4.0;
    if (soft > 0.05) {
        var acc = 0.0;
        for (var j = -1.0; j <= 1.0; j = j + 1.0) {
            for (var i = -1.0; i <= 1.0; i = i + 1.0) {
                let q = in.uv + vec2<f32>(i, j) * soft / u.out_size;
                acc = acc + ultra_matte(unpremul(textureSampleLevel(tex0, samp, q, 0.0)).rgb);
            }
        }
        a = acc / 9.0;
    }
    let choke = prm(10) / 100.0 * 0.5;
    a = clamp((a - choke) / max(1.0 - choke, 1e-3), 0.0, 1.0);
    let contrast = 1.0 + prm(12) / 100.0 * 4.0;
    a = clamp((a - 0.5) * contrast + 0.5, 0.0, 1.0);
    a = a * src.a;

    // spill suppression: remove key-colored chroma from the foreground
    let key = ycc(prm3(2));
    let kdir = normalize(key.yz + vec2<f32>(1e-5));
    let p = ycc(src.rgb);
    let along = dot(p.yz, kdir);
    let range = mix(0.2, 1.0, prm(14) / 100.0);
    let spill = clamp(along / (length(key.yz) * range + 1e-4), 0.0, 1.0) * prm(15) / 100.0;
    var cbcr = p.yz;
    if (along > 0.0) {
        cbcr = cbcr - kdir * along * spill;
    }
    // desaturate the affected area; luma compensation keeps edges from darkening
    cbcr = cbcr * (1.0 - spill * prm(13) / 100.0);
    let yy = p.x - spill * (1.0 - prm(16) / 100.0) * 0.1;
    let r = yy + 1.5748 * cbcr.y;
    let b = yy + 1.8556 * cbcr.x;
    let g = (yy - 0.2126 * r - 0.0722 * b) / 0.7152;
    let rgb = vec3<f32>(r, g, b);
    let mode = i32(prm(0) + 0.5);
    if (mode == 1) {
        return vec4<f32>(vec3<f32>(a), 1.0);
    }
    if (mode == 2) {
        return vec4<f32>(rgb, 1.0);
    }
    return vec4<f32>(rgb * a, a);
}

// prm(0..2) key color, prm(3) tolerance 0..255, prm(4) edge thin -5..5, prm(5) edge feather 0..10
@fragment
fn fs_color_key(in: VOut) -> @location(0) vec4<f32> {
    let src = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    let d = length(src.rgb - prm3(0)) * 255.0 / sqrt(3.0);
    let t = prm(3) + prm(4) * 4.0;
    let f = prm(5) * 6.0 + 0.5;
    let a = clamp((d - t) / f + 0.5, 0.0, 1.0) * src.a;
    return vec4<f32>(src.rgb * a, a);
}

// prm(0) threshold %, prm(1) cutoff %: pixels darker than the threshold become transparent;
// cutoff widens the transition.
@fragment
fn fs_luma_key(in: VOut) -> @location(0) vec4<f32> {
    let src = unpremul(textureSampleLevel(tex0, samp, in.uv, 0.0));
    let y = luma(src.rgb);
    let t = prm(0) / 100.0;
    let w = prm(1) / 100.0 * 0.5 + 0.002;
    let a = smoothstep(t - w, t + w, y) * src.a;
    return vec4<f32>(src.rgb * a, a);
}
