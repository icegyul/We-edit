// Shared declarations for every full-screen pass.
//
// Textures in the working pipeline hold premultiplied RGBA in the sequence working encoding
// (display-referred Rec.709 values, unclamped). `u.p` carries per-pass parameters read with
// `prm(i)`; the Rust side packs them in the same order (see params.rs).

struct U {
    out_size: vec2<f32>,
    in_size: vec2<f32>,
    scale: f32,
    time: f32,
    progress: f32,
    flags: u32,
    p: array<vec4<f32>, 14>,
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the target; uv (0,0) is the top-left corner.
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: VOut;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn prm(i: u32) -> f32 {
    return u.p[i / 4u][i % 4u];
}

fn prm2(i: u32) -> vec2<f32> {
    return vec2<f32>(prm(i), prm(i + 1u));
}

fn prm3(i: u32) -> vec3<f32> {
    return vec3<f32>(prm(i), prm(i + 1u), prm(i + 2u));
}

fn prm4(i: u32) -> vec4<f32> {
    return vec4<f32>(prm(i), prm(i + 1u), prm(i + 2u), prm(i + 3u));
}

fn flag(bit: u32) -> bool {
    return (u.flags & (1u << bit)) != 0u;
}

const PI: f32 = 3.14159265358979;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn unpremul(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 1e-6) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

fn premul(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

// Rec.709 encoding <-> linear light (pure power 2.4 display model).
fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return sign(c) * pow(abs(c), vec3<f32>(2.4));
}

fn from_linear(c: vec3<f32>) -> vec3<f32> {
    return sign(c) * pow(abs(c), vec3<f32>(1.0 / 2.4));
}

fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let d = mx - mn;
    var h = 0.0;
    if (d > 1e-6) {
        if (mx == c.r) {
            h = (c.g - c.b) / d;
            if (h < 0.0) { h = h + 6.0; }
        } else if (mx == c.g) {
            h = (c.b - c.r) / d + 2.0;
        } else {
            h = (c.r - c.g) / d + 4.0;
        }
        h = h / 6.0;
    }
    var s = 0.0;
    if (mx > 1e-6) { s = d / mx; }
    return vec3<f32>(h, s, mx);
}

fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + k.xyz) * 6.0 - k.www);
    return c.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q = q + dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn pixel(uv: vec2<f32>) -> vec2<f32> {
    return uv * u.out_size;
}
