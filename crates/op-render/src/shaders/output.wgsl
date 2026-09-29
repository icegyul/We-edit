// Delivery conversion for export: BT.709 limited-range Y'CbCr planes (8 or 10 bit) and alpha.
// tex0 is the finished sequence frame (premultiplied). prm(0) straight color (for formats that
// store alpha separately), prm(1) bit depth.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn color(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSampleLevel(tex0, samp, uv, 0.0);
    if (prm(0) > 0.5) {
        return vec4<f32>(clamp(unpremul(c).rgb, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(c.a, 0.0, 1.0));
    }
    // over black
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn ycbcr(c: vec3<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec3<f32>(y, (c.b - y) / 1.8556, (c.r - y) / 1.5748);
}

fn codes(uv: vec2<f32>) -> vec3<f32> {
    let v = ycbcr(color(uv).rgb);
    return vec3<f32>(16.0 + 219.0 * v.x, 128.0 + 224.0 * v.y, 128.0 + 224.0 * v.z);
}

@fragment
fn fs_luma8(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(codes(in.uv).x / 255.0, 0.0, 0.0, 1.0);
}

// Rendered at chroma plane size: bilinear sampling at the block center averages the covered
// pixels for 2x subsampling.
@fragment
fn fs_chroma8(in: VOut) -> @location(0) vec4<f32> {
    let c = codes(in.uv);
    return vec4<f32>(c.y / 255.0, c.z / 255.0, 0.0, 1.0);
}

@fragment
fn fs_luma16(in: VOut) -> @location(0) vec4<u32> {
    let scale = exp2(prm(1) - 8.0);
    return vec4<u32>(u32(round(codes(in.uv).x * scale)), 0u, 0u, 1u);
}

@fragment
fn fs_chroma16(in: VOut) -> @location(0) vec4<u32> {
    let scale = exp2(prm(1) - 8.0);
    let c = codes(in.uv);
    return vec4<u32>(u32(round(c.y * scale)), u32(round(c.z * scale)), 0u, 1u);
}

@fragment
fn fs_alpha16(in: VOut) -> @location(0) vec4<u32> {
    let maxv = exp2(prm(1)) - 1.0;
    return vec4<u32>(u32(round(color(in.uv).a * maxv)), 0u, 0u, 1u);
}

@fragment
fn fs_rgba8(in: VOut) -> @location(0) vec4<f32> {
    return color(in.uv);
}
