// Blur-based passes. tex0 is the input; tex1 a second input where noted.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn tap(uv: vec2<f32>, repeat_edge: bool) -> vec4<f32> {
    if (!repeat_edge && (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(tex0, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

// Separable Gaussian. prm(0,1) direction (1,0) or (0,1); prm(2) sigma in pixels;
// prm(3) repeat edge pixels. Wide kernels take at most 64 taps per side with linear filtering.
@fragment
fn fs_blur(in: VOut) -> @location(0) vec4<f32> {
    let sigma = prm(2);
    let repeat_edge = prm(3) > 0.5;
    if (sigma < 0.05) {
        return tap(in.uv, true);
    }
    let texel = prm2(0) / u.out_size;
    let radius = ceil(sigma * 3.0);
    let steps = min(radius, 64.0);
    let stride = radius / steps;
    var acc = tap(in.uv, repeat_edge);
    var wsum = 1.0;
    for (var i = 1.0; i <= steps; i = i + 1.0) {
        let x = i * stride;
        let w = exp(-0.5 * x * x / (sigma * sigma));
        acc = acc + (tap(in.uv + texel * x, repeat_edge) + tap(in.uv - texel * x, repeat_edge)) * w;
        wsum = wsum + 2.0 * w;
    }
    return acc / wsum;
}

// prm(0) direction in degrees, prm(1) length in pixels
@fragment
fn fs_directional_blur(in: VOut) -> @location(0) vec4<f32> {
    let len = prm(1);
    if (len < 0.5) {
        return tap(in.uv, true);
    }
    let a = prm(0) * PI / 180.0;
    // 0 degrees blurs vertically, like the layer's "up" direction
    let dir = vec2<f32>(sin(a), -cos(a)) * len / u.out_size;
    var acc = vec4<f32>(0.0);
    let n = 32.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let t = i / (n - 1.0) - 0.5;
        acc = acc + tap(in.uv + dir * t, false);
    }
    return acc / n;
}

// tex0 original, tex1 blurred. prm(0) amount %, prm(1) threshold 0..1
@fragment
fn fs_unsharp(in: VOut) -> @location(0) vec4<f32> {
    let o = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let b = textureSampleLevel(tex1, samp, in.uv, 0.0);
    let d = o.rgb - b.rgb;
    let mask = step(vec3<f32>(prm(1)), abs(d));
    let rgb = o.rgb + d * mask * prm(0) / 100.0;
    return vec4<f32>(rgb, o.a);
}

// Three-by-three sharpen. prm(0) amount (0..100 is the useful range)
@fragment
fn fs_sharpen(in: VOut) -> @location(0) vec4<f32> {
    let t = 1.0 / u.out_size;
    let c = tap(in.uv, true);
    let n = tap(in.uv + vec2<f32>(0.0, -t.y), true) + tap(in.uv + vec2<f32>(0.0, t.y), true)
        + tap(in.uv + vec2<f32>(-t.x, 0.0), true) + tap(in.uv + vec2<f32>(t.x, 0.0), true);
    let k = prm(0) / 100.0;
    let rgb = c.rgb + (c.rgb * 4.0 - n.rgb) * k * 0.5;
    return vec4<f32>(rgb, c.a);
}

// Shadow source: the layer's alpha moved by prm(0,1) pixels, colored prm(2..4), opacity prm(5).
@fragment
fn fs_shadow(in: VOut) -> @location(0) vec4<f32> {
    let uv = in.uv - prm2(0) / u.out_size;
    let a = tap(uv, false).a * prm(5);
    return vec4<f32>(prm3(2) * a, a);
}

// tex0 layer over tex1 (blurred shadow); prm(0) shadow only
@fragment
fn fs_over(in: VOut) -> @location(0) vec4<f32> {
    let top = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let bottom = textureSampleLevel(tex1, samp, in.uv, 0.0);
    if (prm(0) > 0.5) {
        return bottom * (1.0 - top.a);
    }
    return top + bottom * (1.0 - top.a);
}
