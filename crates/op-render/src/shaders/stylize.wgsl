// Stylize effects and vector graphics. tex0 is the layer.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn at(uv: vec2<f32>, dx: f32, dy: f32) -> vec4<f32> {
    let q = uv + vec2<f32>(dx, dy) / u.out_size;
    return unpremul(textureSampleLevel(tex0, samp, clamp(q, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0));
}

// prm(0) invert, prm(1) blend with original %
@fragment
fn fs_find_edges(in: VOut) -> @location(0) vec4<f32> {
    let c = at(in.uv, 0.0, 0.0);
    let gx = -at(in.uv, -1.0, -1.0).rgb - 2.0 * at(in.uv, -1.0, 0.0).rgb - at(in.uv, -1.0, 1.0).rgb
        + at(in.uv, 1.0, -1.0).rgb + 2.0 * at(in.uv, 1.0, 0.0).rgb + at(in.uv, 1.0, 1.0).rgb;
    let gy = -at(in.uv, -1.0, -1.0).rgb - 2.0 * at(in.uv, 0.0, -1.0).rgb - at(in.uv, 1.0, -1.0).rgb
        + at(in.uv, -1.0, 1.0).rgb + 2.0 * at(in.uv, 0.0, 1.0).rgb + at(in.uv, 1.0, 1.0).rgb;
    let mag = clamp(sqrt(gx * gx + gy * gy) * 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
    // edges are dark lines on white unless inverted
    var e = 1.0 - mag;
    if (prm(0) > 0.5) {
        e = mag;
    }
    return premul(mix(vec4<f32>(e, c.a), c, prm(1) / 100.0));
}

// prm(0) direction deg, prm(1) relief, prm(2) contrast, prm(3) blend with original %
@fragment
fn fs_emboss(in: VOut) -> @location(0) vec4<f32> {
    let c = at(in.uv, 0.0, 0.0);
    let a = prm(0) * PI / 180.0;
    let d = vec2<f32>(cos(a), -sin(a)) * max(prm(1), 0.1);
    let hi = luma(at(in.uv, d.x, d.y).rgb);
    let lo = luma(at(in.uv, -d.x, -d.y).rgb);
    let v = 0.5 + (hi - lo) * prm(2) / 100.0;
    return premul(mix(vec4<f32>(vec3<f32>(v), c.a), c, prm(3) / 100.0));
}

// Rectangle or ellipse drawn onto the canvas in tex0 (pixel space of the output).
// prm(0) shape (0 rectangle, 1 ellipse), prm(1,2) center px, prm(3,4) half size px,
// prm(5) rotation deg, prm(6) corner radius px, prm(7..10) fill RGBA (straight),
// prm(11) stroke on, prm(12..15) stroke RGBA, prm(16) stroke width px, prm(17) feather px,
// prm(18) opacity
fn shape_distance(p: vec2<f32>) -> f32 {
    let hs = max(prm2(3), vec2<f32>(0.0));
    if (prm(0) > 0.5) {
        // approximate signed distance to an ellipse
        let k0 = length(p / max(hs, vec2<f32>(1e-3)));
        let k1 = length(p / max(hs * hs, vec2<f32>(1e-3)));
        return k0 * (k0 - 1.0) / max(k1, 1e-5);
    }
    let r = min(prm(6), min(hs.x, hs.y));
    let q = abs(p) - hs + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_shape(in: VOut) -> @location(0) vec4<f32> {
    let dst = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let a = -prm(5) * PI / 180.0;
    let d0 = pixel(in.uv) - prm2(1);
    let p = vec2<f32>(d0.x * cos(a) - d0.y * sin(a), d0.x * sin(a) + d0.y * cos(a));
    let dist = shape_distance(p);
    let feather = max(prm(17), 0.75);
    let fill_cov = clamp(0.5 - dist / feather, 0.0, 1.0);
    let fill = prm4(7);
    var col = vec4<f32>(fill.rgb * fill.a, fill.a) * fill_cov;
    if (prm(11) > 0.5 && prm(16) > 0.0) {
        let w = prm(16);
        let sc = clamp(0.5 - (abs(dist) - w * 0.5) / 0.75, 0.0, 1.0);
        let st = prm4(12);
        let s = vec4<f32>(st.rgb * st.a, st.a) * sc;
        col = s + col * (1.0 - s.a);
    }
    col = col * prm(18);
    return col + dst * (1.0 - col.a);
}
