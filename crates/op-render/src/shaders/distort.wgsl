// Geometry effects in layer space. tex0 is the layer; outside it is transparent.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn sample_clear(uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex0));
    let c = textureSampleLevel(tex0, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let fx = clamp(min(uv.x, 1.0 - uv.x) * size.x + 0.5, 0.0, 1.0);
    let fy = clamp(min(uv.y, 1.0 - uv.y) * size.y + 0.5, 0.0, 1.0);
    return c * fx * fy;
}

// Inverse affine map prm(0..5) from output pixels to input pixels, opacity prm(6).
@fragment
fn fs_affine(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let q = vec2<f32>(prm(0) * p.x + prm(1) * p.y + prm(2), prm(3) * p.x + prm(4) * p.y + prm(5));
    return sample_clear(q / u.in_size) * prm(6);
}

// Inverse homography prm(0..8) (row-major) from output pixels to input pixels.
// prm(9) specular highlight strength, prm(10..11) highlight center in uv.
@fragment
fn fs_homography(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let x = prm(0) * p.x + prm(1) * p.y + prm(2);
    let y = prm(3) * p.x + prm(4) * p.y + prm(5);
    let w = prm(6) * p.x + prm(7) * p.y + prm(8);
    if (w <= 1e-6) {
        return vec4<f32>(0.0);
    }
    let uv = vec2<f32>(x, y) / w / u.in_size;
    var c = sample_clear(uv);
    if (prm(9) > 0.0) {
        let d = length(uv - prm2(10));
        let h = exp(-d * d * 20.0) * prm(9);
        c = vec4<f32>(c.rgb + h * c.a, c.a);
    }
    return c;
}

// prm(0,1) reflection center (uv), prm(2) angle in degrees
@fragment
fn fs_mirror(in: VOut) -> @location(0) vec4<f32> {
    let aspect = vec2<f32>(u.out_size.x / max(u.out_size.y, 1.0), 1.0);
    let c = prm2(0) * aspect;
    let a = prm(2) * PI / 180.0;
    let n = vec2<f32>(cos(a), sin(a));
    let p = in.uv * aspect;
    let d = dot(p - c, n);
    var q = p;
    if (d > 0.0) {
        q = p - 2.0 * d * n;
    }
    return sample_clear(q / aspect);
}

// prm(0,1) shift center to (uv), prm(2) blend with original %
@fragment
fn fs_offset(in: VOut) -> @location(0) vec4<f32> {
    let shift = prm2(0) - 0.5;
    let o = textureSampleLevel(tex0, samp, fract(in.uv - shift), 0.0);
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    return mix(o, c, prm(2) / 100.0);
}

// prm(0) radius in pixels, prm(1,2) center (uv)
@fragment
fn fs_spherize(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let c = prm2(1) * u.out_size;
    let r = prm(0);
    let d = p - c;
    let len = length(d);
    if (r <= 0.5 || len >= r) {
        return sample_clear(in.uv);
    }
    let nd = len / r;
    // map the disk onto a sphere cap: points near the center magnify
    let k = asin(clamp(nd, 0.0, 1.0)) / (PI * 0.5) / max(nd, 1e-5);
    let q = c + d * k;
    return sample_clear(q / u.out_size);
}

// prm(0) angle in degrees, prm(1) radius % of the half-diagonal, prm(2,3) center (uv)
@fragment
fn fs_twirl(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let c = prm2(2) * u.out_size;
    let r = prm(1) / 100.0 * length(u.out_size) * 0.5;
    let d = p - c;
    let len = length(d);
    if (r <= 0.5 || len >= r) {
        return sample_clear(in.uv);
    }
    let f = 1.0 - len / r;
    let a = prm(0) * PI / 180.0 * f * f;
    let q = c + vec2<f32>(d.x * cos(a) - d.y * sin(a), d.x * sin(a) + d.y * cos(a));
    return sample_clear(q / u.out_size);
}

fn wave(kind: i32, x: f32) -> f32 {
    let t = fract(x);
    switch kind {
        case 1: { return select(-1.0, 1.0, t < 0.5); }
        case 2: { return 1.0 - 4.0 * abs(t - 0.5); }
        case 3: { return 2.0 * t - 1.0; }
        default: { return sin(x * 2.0 * PI); }
    }
}

// prm(0) wave type, prm(1) height px, prm(2) width px, prm(3) direction deg, prm(4) speed
// (cycles per second), prm(5) phase deg. u.time is the clip time in seconds.
@fragment
fn fs_wave_warp(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let a = prm(3) * PI / 180.0;
    // direction 90 degrees moves pixels up and down along a horizontal wave
    let along = vec2<f32>(sin(a), -cos(a));
    let across = vec2<f32>(along.y, -along.x);
    let x = dot(p, across) / max(prm(2), 1.0) + prm(5) / 360.0 - u.time * prm(4);
    let q = p + along * wave(i32(prm(0) + 0.5), x) * prm(1);
    return sample_clear(q / u.out_size);
}

// prm(0) count
@fragment
fn fs_replicate(in: VOut) -> @location(0) vec4<f32> {
    let n = max(prm(0), 1.0);
    return textureSampleLevel(tex0, samp, fract(in.uv * n), 0.0);
}

// prm(0..3) left, top, right, bottom (0..1), prm(4) zoom, prm(5) feather in pixels
@fragment
fn fs_crop(in: VOut) -> @location(0) vec4<f32> {
    let l = prm(0);
    let t = prm(1);
    let r = 1.0 - prm(2);
    let b = 1.0 - prm(3);
    var uv = in.uv;
    if (prm(4) > 0.5) {
        uv = vec2<f32>(mix(l, r, in.uv.x), mix(t, b, in.uv.y));
    }
    let px = uv * u.out_size;
    let f = max(prm(5), 0.5);
    let dx = min(px.x - l * u.out_size.x, r * u.out_size.x - px.x);
    let dy = min(px.y - t * u.out_size.y, b * u.out_size.y - px.y);
    let k = clamp(min(dx, dy) / f + select(0.5, 0.0, prm(5) > 0.5), 0.0, 1.0);
    return textureSampleLevel(tex0, samp, uv, 0.0) * k;
}

// prm(0) amount 0..100 (fraction of the shorter side)
@fragment
fn fs_edge_feather(in: VOut) -> @location(0) vec4<f32> {
    let px = pixel(in.uv);
    let d = min(min(px.x, u.out_size.x - px.x), min(px.y, u.out_size.y - px.y));
    let f = max(prm(0) / 100.0 * min(u.out_size.x, u.out_size.y) * 0.5, 0.5);
    let k = clamp(d / f, 0.0, 1.0);
    return textureSampleLevel(tex0, samp, in.uv, 0.0) * k;
}

// prm(0) horizontal blocks, prm(1) vertical blocks, prm(2) sharp colors
@fragment
fn fs_mosaic(in: VOut) -> @location(0) vec4<f32> {
    let n = max(vec2<f32>(prm(0), prm(1)), vec2<f32>(1.0));
    let cell = floor(in.uv * n);
    if (prm(2) > 0.5) {
        return textureSampleLevel(tex0, samp, (cell + 0.5) / n, 0.0);
    }
    // average of a 3x3 grid inside the cell
    var acc = vec4<f32>(0.0);
    for (var j = 0.0; j < 3.0; j = j + 1.0) {
        for (var i = 0.0; i < 3.0; i = i + 1.0) {
            acc = acc + textureSampleLevel(tex0, samp, (cell + (vec2<f32>(i, j) + 0.5) / 3.0) / n, 0.0);
        }
    }
    return acc / 9.0;
}

// prm(0) completion 0..1, prm(1) wipe angle deg, prm(2) feather px
@fragment
fn fs_linear_wipe(in: VOut) -> @location(0) vec4<f32> {
    let a = prm(1) * PI / 180.0;
    // 90 degrees wipes from left to right
    let dir = vec2<f32>(sin(a), -cos(a));
    let p = pixel(in.uv) - u.out_size * 0.5;
    let half = 0.5 * (abs(dir.x) * u.out_size.x + abs(dir.y) * u.out_size.y);
    let f = max(prm(2), 0.5);
    let edge = mix(-half - f, half + f, prm(0));
    let k = clamp((dot(p, dir) - edge) / f + 0.5, 0.0, 1.0);
    return textureSampleLevel(tex0, samp, in.uv, 0.0) * k;
}

// prm(0) completion, prm(1) start angle deg, prm(2,3) center uv, prm(4) 0 clockwise,
// 1 counterclockwise, 2 both; prm(5) feather in degrees
@fragment
fn fs_radial_wipe(in: VOut) -> @location(0) vec4<f32> {
    let d = (in.uv - prm2(2)) * u.out_size;
    var ang = atan2(d.x, -d.y) * 180.0 / PI - prm(1);
    ang = ang - floor(ang / 360.0) * 360.0;
    let mode = i32(prm(4) + 0.5);
    var a = ang;
    if (mode == 1) {
        a = 360.0 - ang;
    }
    let sweep = prm(0) * 360.0;
    if (mode == 2) {
        a = min(ang, 360.0 - ang) * 2.0;
    }
    let f = max(prm(5), 0.5);
    let k = clamp((a - sweep) / f + 0.5, 0.0, 1.0);
    return textureSampleLevel(tex0, samp, in.uv, 0.0) * k;
}
