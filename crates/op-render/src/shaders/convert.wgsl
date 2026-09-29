// Decoded planes -> premultiplied working RGBA, in display orientation.
//
// Planes are unsigned-integer textures read with textureLoad, so 8, 10, 12 and 16-bit sources
// share one path without CPU conversion.
//
// prm(0) kind: 0 planar YUV, 1 packed RGBA
// prm(1) maximum code value (255, 1023, 4095, 65535)
// prm(2), prm(3) chroma subsampling shift (x, y)
// prm(4) Kr, prm(5) Kb
// prm(6) full range (1) or limited (0)
// prm(7) alpha: 0 opaque, 1 straight, 2 premultiplied
// prm(8) has an alpha plane (YUV)
// prm(9) clockwise display rotation in degrees
// prm(10) invert alpha
// prm(11) bit depth

@group(0) @binding(2) var plane0: texture_2d<u32>;
@group(0) @binding(3) var plane1: texture_2d<u32>;
@group(0) @binding(4) var plane2: texture_2d<u32>;
@group(0) @binding(5) var plane3: texture_2d<u32>;

fn load1(t: texture_2d<u32>, p: vec2<i32>) -> f32 {
    let d = vec2<i32>(textureDimensions(t));
    return f32(textureLoad(t, clamp(p, vec2<i32>(0), d - 1), 0).r);
}

// Bilinear read of a single-channel plane at pixel coordinate `p` (pixel centers at .5).
fn bilinear1(t: texture_2d<u32>, p: vec2<f32>) -> f32 {
    let q = p - 0.5;
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    let a = load1(t, i);
    let b = load1(t, i + vec2<i32>(1, 0));
    let c = load1(t, i + vec2<i32>(0, 1));
    let d = load1(t, i + vec2<i32>(1, 1));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

fn load4(t: texture_2d<u32>, p: vec2<i32>) -> vec4<f32> {
    let d = vec2<i32>(textureDimensions(t));
    return vec4<f32>(textureLoad(t, clamp(p, vec2<i32>(0), d - 1), 0));
}

fn bilinear4(t: texture_2d<u32>, p: vec2<f32>) -> vec4<f32> {
    let q = p - 0.5;
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    let a = load4(t, i);
    let b = load4(t, i + vec2<i32>(1, 0));
    let c = load4(t, i + vec2<i32>(0, 1));
    let d = load4(t, i + vec2<i32>(1, 1));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Display uv -> stored uv for a clockwise display rotation.
fn unrotate(uv: vec2<f32>, deg: f32) -> vec2<f32> {
    if (deg > 45.0 && deg < 135.0) {
        return vec2<f32>(uv.y, 1.0 - uv.x);
    }
    if (deg >= 135.0 && deg < 225.0) {
        return vec2<f32>(1.0 - uv.x, 1.0 - uv.y);
    }
    if (deg >= 225.0 && deg < 315.0) {
        return vec2<f32>(1.0 - uv.y, uv.x);
    }
    return uv;
}

@fragment
fn fs_convert(in: VOut) -> @location(0) vec4<f32> {
    let suv = unrotate(in.uv, prm(9));
    let size0 = vec2<f32>(textureDimensions(plane0));
    let p0 = suv * size0;
    let maxv = prm(1);
    var rgba: vec4<f32>;
    if (prm(0) < 0.5) {
        let depth = prm(11);
        let unit = exp2(depth - 8.0);
        var y = bilinear1(plane0, p0);
        let pc = suv * vec2<f32>(textureDimensions(plane1));
        var cb = bilinear1(plane1, pc);
        var cr = bilinear1(plane2, pc);
        if (prm(6) > 0.5) {
            y = y / maxv;
            let mid = exp2(depth - 1.0);
            cb = (cb - mid) / maxv;
            cr = (cr - mid) / maxv;
        } else {
            y = (y - 16.0 * unit) / (219.0 * unit);
            cb = (cb - 128.0 * unit) / (224.0 * unit);
            cr = (cr - 128.0 * unit) / (224.0 * unit);
        }
        let kr = prm(4);
        let kb = prm(5);
        let kg = 1.0 - kr - kb;
        let r = y + 2.0 * (1.0 - kr) * cr;
        let b = y + 2.0 * (1.0 - kb) * cb;
        let g = (y - kr * r - kb * b) / kg;
        var a = 1.0;
        if (prm(8) > 0.5) {
            a = bilinear1(plane3, p0) / maxv;
        }
        rgba = vec4<f32>(r, g, b, a);
    } else {
        rgba = bilinear4(plane0, p0) / maxv;
        if (prm(7) < 0.5) {
            rgba.a = 1.0;
        }
    }
    if (prm(10) > 0.5) {
        rgba.a = 1.0 - rgba.a;
    }
    rgba.a = clamp(rgba.a, 0.0, 1.0);
    // straight sources are premultiplied here; premultiplied ones pass through
    if (prm(7) > 1.5) {
        return rgba;
    }
    return vec4<f32>(rgba.rgb * rgba.a, rgba.a);
}
