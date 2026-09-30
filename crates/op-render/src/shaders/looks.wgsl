// Light, motion and stylized looks. tex0 is the layer image (premultiplied, display encoding);
// tex1 a second input where noted. u.time is the clip time in seconds for animated looks.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn s0(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex0, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

fn s0_clear(uv: vec2<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(tex0, samp, uv, 0.0);
}

// Exact integer hash of a lattice point: float hashes can differ between neighbouring cells on
// some GPUs and break the continuity of value noise.
fn lhash(i: vec2<f32>) -> f32 {
    let x = bitcast<u32>(i32(i.x));
    let y = bitcast<u32>(i32(i.y));
    var h = x * 1597334677u ^ y * 3812015801u;
    h = h * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) * (1.0 / 4294967295.0);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = lhash(i);
    let b = lhash(i + vec2<f32>(1.0, 0.0));
    let c = lhash(i + vec2<f32>(0.0, 1.0));
    let d = lhash(i + vec2<f32>(1.0, 1.0));
    let w = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + amp * vnoise(q);
        q = q * 2.03 + vec2<f32>(17.1, 9.2);
        amp = amp * 0.5;
    }
    return sum;
}

fn aspect() -> f32 {
    return u.out_size.x / max(u.out_size.y, 1.0);
}

// ------------------------------------------------------------------------------------ glow

// Bright part of the image in linear light. prm(0) threshold 0..1, prm(1) knee.
@fragment
fn fs_bright_pass(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    let lin = max(to_linear(s.rgb), vec3<f32>(0.0));
    let l = luma(lin);
    let k = smoothstep(prm(0) - prm(1), prm(0) + prm(1), l);
    return vec4<f32>(lin * k * c.a, c.a * k);
}

// 4-tap box downsample (the output is half the input size).
@fragment
fn fs_downsample(in: VOut) -> @location(0) vec4<f32> {
    let o = 0.5 / u.in_size;
    return (s0(in.uv + vec2<f32>(-o.x, -o.y)) + s0(in.uv + vec2<f32>(o.x, -o.y))
        + s0(in.uv + vec2<f32>(-o.x, o.y)) + s0(in.uv + vec2<f32>(o.x, o.y))) * 0.25;
}

// tex0 * prm(0) + tex1 * prm(1); tex0 may be smaller (bilinear upsampling).
@fragment
fn fs_add_up(in: VOut) -> @location(0) vec4<f32> {
    let a = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let b = textureSampleLevel(tex1, samp, in.uv, 0.0);
    return a * prm(0) + b * prm(1);
}

// tex0 original, tex1 glow (linear, premultiplied). prm(0) gain, prm(1..3) tint color,
// prm(4) tint amount, prm(5) chromatic aberration, prm(6) glow only.
@fragment
fn fs_glow_composite(in: VOut) -> @location(0) vec4<f32> {
    let o = s0(in.uv);
    let ab = prm(5);
    var g: vec3<f32>;
    if (ab > 0.0001) {
        let d = in.uv - vec2<f32>(0.5);
        let r = textureSampleLevel(tex1, samp, vec2<f32>(0.5) + d * (1.0 - ab), 0.0).r;
        let gg = textureSampleLevel(tex1, samp, in.uv, 0.0).g;
        let b = textureSampleLevel(tex1, samp, vec2<f32>(0.5) + d * (1.0 + ab), 0.0).b;
        g = vec3<f32>(r, gg, b);
    } else {
        g = textureSampleLevel(tex1, samp, in.uv, 0.0).rgb;
    }
    g = g * prm(0);
    let tint = prm3(1);
    g = mix(g, vec3<f32>(luma(g)) * tint, clamp(prm(4), 0.0, 1.0));
    let so = unpremul(o);
    var sum = to_linear(so.rgb) * o.a + g;
    var a = clamp(o.a + luma(g) * (1.0 - o.a), 0.0, 1.0);
    if (prm(6) > 0.5) {
        sum = g;
        a = clamp(luma(g), 0.0, 1.0);
    }
    if (a <= 1e-5) {
        return vec4<f32>(0.0);
    }
    let straight = from_linear(sum / a);
    return vec4<f32>(straight * a, a);
}

// ------------------------------------------------------------------------------ RGB split

// prm(0) separation px, prm(1) angle degrees, prm(2) mode (0 linear, 1 radial)
@fragment
fn fs_rgb_split(in: VOut) -> @location(0) vec4<f32> {
    var d: vec2<f32>;
    if (prm(2) > 0.5) {
        d = (in.uv - vec2<f32>(0.5)) * prm(0) / max(u.out_size.x, 1.0) * 2.0;
    } else {
        let a = prm(1) * PI / 180.0;
        d = vec2<f32>(cos(a), sin(a)) * prm(0) / u.out_size;
    }
    let r = s0(in.uv + d);
    let g = s0(in.uv);
    let b = s0(in.uv - d);
    return vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
}

// ---------------------------------------------------------------------------------- glitch

// prm(0) intensity 0..1, prm(1) block size px, prm(2) color shift px, prm(3) scan lines 0..1,
// prm(4) speed Hz, prm(5) seed
@fragment
fn fs_glitch(in: VOut) -> @location(0) vec4<f32> {
    let inten = prm(0);
    let tick = floor(u.time * max(prm(4), 0.01));
    let seed = prm(5);
    let px = pixel(in.uv);
    let bs = max(prm(1), 2.0);
    let row = floor(px.y / (bs * 0.5));
    var uv = in.uv;
    let r1 = hash21(vec2<f32>(row + seed * 3.1, tick));
    if (r1 < inten * 0.5) {
        let shift = (hash21(vec2<f32>(row * 1.7 + seed, tick * 3.1)) - 0.5) * 2.0 * inten * 0.12;
        uv.x = uv.x + shift;
    }
    let blk = floor(px / bs);
    let r2 = hash21(blk + vec2<f32>(tick * 0.37 + seed, seed * 0.11));
    if (r2 > 1.0 - inten * 0.06) {
        uv = (floor(uv * u.out_size / bs) + vec2<f32>(hash21(blk + tick), hash21(blk - tick))) * bs
            / u.out_size;
    }
    let cs = vec2<f32>(prm(2) * inten * (0.5 + hash21(vec2<f32>(tick, seed))), 0.0) / u.out_size;
    let r = s0(uv + cs);
    let g = s0(uv);
    let b = s0(uv - cs);
    var c = vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
    let lines = 1.0 - prm(3) * 0.35 * (0.5 + 0.5 * sin(px.y * PI));
    return vec4<f32>(c.rgb * lines, c.a);
}

// ------------------------------------------------------------------------------ film grain

// prm(0) amount 0..1, prm(1) grain size px, prm(2) color grain, prm(3) frame seed
@fragment
fn fs_film_grain(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    let p = pixel(in.uv) / max(prm(1), 0.5);
    let off = vec2<f32>(prm(3) * 17.13, prm(3) * 7.71);
    var n = vec3<f32>(vnoise(p + off) - 0.5);
    if (prm(2) > 0.5) {
        n = vec3<f32>(vnoise(p + off), vnoise(p + off + 31.7), vnoise(p + off + 71.3)) - 0.5;
    }
    let l = luma(s.rgb);
    let w = 4.0 * l * (1.0 - l) + 0.25;
    return premul(vec4<f32>(s.rgb + n * prm(0) * 0.5 * w, s.a));
}

// -------------------------------------------------------------------------------- vignette

// prm(0) amount -1..1, prm(1) midpoint 0..1, prm(2) roundness -1..1, prm(3) feather 0..1,
// prm(4..6) color
@fragment
fn fs_vignette(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    var d = (in.uv - vec2<f32>(0.5)) * 2.0;
    d.x = d.x * mix(1.0, aspect(), clamp(prm(2), 0.0, 1.0));
    let n = mix(2.0, 8.0, clamp(-prm(2), 0.0, 1.0));
    let dist = pow(pow(abs(d.x), n) + pow(abs(d.y), n), 1.0 / n) / 1.414;
    let mid = mix(0.3, 1.1, prm(1));
    let fe = max(prm(3), 0.02);
    let v = smoothstep(mid - fe * 0.5, mid + fe * 0.5, dist);
    var rgb = s.rgb;
    if (prm(0) >= 0.0) {
        rgb = mix(rgb, prm3(4), v * prm(0));
    } else {
        rgb = mix(rgb, vec3<f32>(1.0), v * -prm(0));
    }
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------- cinematic bars

// prm(0) target aspect ratio, prm(1..3) bar color, prm(4) opacity
@fragment
fn fs_letterbox(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let target_ratio = max(prm(0), 0.05);
    let a = aspect();
    var bar = false;
    if (target_ratio > a) {
        let h = a / target_ratio;
        bar = abs(in.uv.y - 0.5) > h * 0.5;
    } else {
        let w = target_ratio / a;
        bar = abs(in.uv.x - 0.5) > w * 0.5;
    }
    if (!bar) {
        return c;
    }
    let k = clamp(prm(4), 0.0, 1.0);
    return mix(c, vec4<f32>(prm3(1), 1.0), k);
}

// ------------------------------------------------------------------------ lens distortion

// prm(0) curvature -1..1 (positive bulges), prm(1..2) center, prm(3) zoom
@fragment
fn fs_lens(in: VOut) -> @location(0) vec4<f32> {
    let a = aspect();
    let c = prm2(1);
    var p = (in.uv - c) * vec2<f32>(a, 1.0);
    let r2 = dot(p, p);
    p = p * (1.0 - prm(0) * 0.6 * r2) / max(prm(3), 0.05);
    return s0_clear(c + p / vec2<f32>(a, 1.0));
}

// ----------------------------------------------------------------------------- radial blur

// prm(0) amount 0..100, prm(1..2) center, prm(3) type (0 spin, 1 zoom)
@fragment
fn fs_radial_blur(in: VOut) -> @location(0) vec4<f32> {
    let amt = prm(0);
    if (amt < 0.01) {
        return s0(in.uv);
    }
    let a = aspect();
    let c = prm2(1);
    let p = (in.uv - c) * vec2<f32>(a, 1.0);
    var acc = vec4<f32>(0.0);
    let n = 32.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let f = i / (n - 1.0) - 0.5;
        var q: vec2<f32>;
        if (prm(3) > 0.5) {
            q = p * (1.0 + f * amt * 0.01);
        } else {
            let ang = f * amt * PI / 180.0;
            let cs = cos(ang);
            let sn = sin(ang);
            q = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
        }
        acc = acc + s0(c + q / vec2<f32>(a, 1.0));
    }
    return acc / n;
}

// ----------------------------------------------------------------------------- motion tile

fn mirror1(x: f32) -> f32 {
    let m = abs(x) % 2.0;
    return select(m, 2.0 - m, m > 1.0);
}

// prm(0..1) tile center, prm(2..3) tile size, prm(4..5) output size (fractions of the layer),
// prm(6) mirror edges, prm(7) phase degrees, prm(8) horizontal phase shift
@fragment
fn fs_motion_tile(in: VOut) -> @location(0) vec4<f32> {
    let q = (in.uv - vec2<f32>(0.5)) * vec2<f32>(prm(4), prm(5)) + vec2<f32>(0.5);
    let ts = max(vec2<f32>(prm(2), prm(3)), vec2<f32>(0.001));
    var t = (q - prm2(0)) / ts + vec2<f32>(0.5);
    let phase = prm(7) / 360.0;
    if (prm(8) > 0.5) {
        t.x = t.x + floor(t.y) * phase;
    } else {
        t.y = t.y + floor(t.x) * phase;
    }
    var f = fract(t);
    if (prm(6) > 0.5) {
        f = vec2<f32>(mirror1(t.x), mirror1(t.y));
    }
    return s0(f);
}

// ----------------------------------------------------------------------------- light leaks

// prm(0) intensity, prm(1..3) color 1, prm(4..6) color 2, prm(7) scale, prm(8) speed,
// prm(9) seed, prm(10) blend (0 screen, 1 add)
@fragment
fn fs_light_leaks(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    let tt = u.time * prm(8);
    let sd = prm(9);
    let a = aspect();
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(a, 1.0);
    var leak = vec3<f32>(0.0);
    // soft glows that drift in from the edges of the frame
    for (var k = 0.0; k < 4.0; k = k + 1.0) {
        let ang = lhash(vec2<f32>(k, sd)) * 6.283 + tt * (0.15 + 0.07 * k);
        let dist = 0.55 + 0.25 * sin(tt * (0.23 + 0.05 * k) + k * 1.7 + sd);
        let ctr = vec2<f32>(cos(ang) * a, sin(ang)) * dist;
        let r = (0.35 + 0.25 * lhash(vec2<f32>(sd, k))) * max(prm(7), 0.05);
        let d = length(p - ctr) / r;
        let col = mix(prm3(1), prm3(4), lhash(vec2<f32>(k * 3.0 + 1.0, sd)));
        leak = leak + col * exp(-d * d * 1.6);
    }
    // gentle texture inside the glow
    leak = leak * (0.75 + 0.5 * fbm(p * 2.0 + vec2<f32>(tt * 0.2, sd)));
    leak = leak * prm(0);
    var rgb: vec3<f32>;
    if (prm(10) > 0.5) {
        rgb = s.rgb + leak;
    } else {
        rgb = vec3<f32>(1.0) - (vec3<f32>(1.0) - clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)))
            * (vec3<f32>(1.0) - clamp(leak, vec3<f32>(0.0), vec3<f32>(1.0)));
    }
    return premul(vec4<f32>(rgb, s.a));
}

// -------------------------------------------------------------------------------- old film

// prm(0) sepia, prm(1) grain, prm(2) scratches, prm(3) dust, prm(4) flicker, prm(5) vignette,
// prm(6) seed (all 0..1 except the seed)
@fragment
fn fs_old_film(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    let frame = floor(u.time * 24.0);
    let sd = prm(6);
    var rgb = s.rgb;
    let sep = vec3<f32>(
        dot(rgb, vec3<f32>(0.393, 0.769, 0.189)),
        dot(rgb, vec3<f32>(0.349, 0.686, 0.168)),
        dot(rgb, vec3<f32>(0.272, 0.534, 0.131)),
    );
    rgb = mix(rgb, sep, prm(0));
    rgb = rgb * (1.0 + (hash21(vec2<f32>(frame, sd)) - 0.5) * prm(4) * 0.35);
    let px = pixel(in.uv);
    rgb = rgb + (vnoise(px * 0.8 + vec2<f32>(frame * 91.7, sd)) - 0.5) * prm(1) * 0.3;
    // scratches: a few thin vertical lines that jump every few frames
    for (var k = 0.0; k < 3.0; k = k + 1.0) {
        let slot = floor(frame / 3.0);
        if (hash21(vec2<f32>(k * 13.0 + sd, slot)) < prm(2)) {
            let x = hash21(vec2<f32>(k + sd * 3.0, slot * 1.3)) * u.out_size.x;
            let d = abs(px.x - x);
            rgb = mix(rgb, vec3<f32>(0.9, 0.85, 0.75), (1.0 - smoothstep(0.0, 1.5, d)) * 0.6);
        }
    }
    // dust: small dark specks that change every frame
    let cell = floor(px / 36.0);
    if (hash21(cell + vec2<f32>(frame * 0.71, sd)) > 1.0 - prm(3) * 0.03) {
        let ctr = (cell + vec2<f32>(hash21(cell + frame), hash21(cell - frame))) * 36.0;
        let d = length(px - ctr);
        rgb = mix(rgb, vec3<f32>(0.08), 1.0 - smoothstep(1.0, 3.0, d));
    }
    let v = smoothstep(0.35, 0.95, length((in.uv - vec2<f32>(0.5)) * 1.4));
    rgb = rgb * (1.0 - prm(5) * v);
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------------------- VHS

// prm(0) color bleed px, prm(1) noise, prm(2) tracking lines, prm(3) jitter px,
// prm(4) saturation, prm(5) seed
@fragment
fn fs_vhs(in: VOut) -> @location(0) vec4<f32> {
    let px = pixel(in.uv);
    let frame = floor(u.time * 30.0);
    let sd = prm(5);
    var uv = in.uv;
    uv.x = uv.x + (hash21(vec2<f32>(floor(px.y / 2.0), frame + sd)) - 0.5) * prm(3) / u.out_size.x;
    let band = fract(u.time * 0.13 + sd * 0.37);
    let bd = abs(in.uv.y - band);
    if (bd < 0.04 * prm(2)) {
        uv.x = uv.x + (hash21(vec2<f32>(floor(px.y), frame)) - 0.5) * 0.02 * prm(2);
    }
    let cb = prm(0) / u.out_size.x;
    var r = 0.0;
    var b = 0.0;
    for (var i = 0.0; i < 5.0; i = i + 1.0) {
        let o = (i - 2.0) * cb * 0.5;
        r = r + s0(uv + vec2<f32>(cb + o, 0.0)).r;
        b = b + s0(uv - vec2<f32>(cb - o, 0.0)).b;
    }
    let g = s0(uv);
    var rgb = unpremul(vec4<f32>(r / 5.0, g.g, b / 5.0, g.a)).rgb;
    rgb = mix(vec3<f32>(luma(rgb)), rgb, prm(4));
    rgb = rgb + (hash21(px + vec2<f32>(frame * 7.3, sd)) - 0.5) * prm(1) * 0.3;
    if (bd < 0.012 * prm(2)) {
        rgb = rgb + vec3<f32>(0.25) * prm(2);
    }
    return premul(vec4<f32>(rgb, g.a));
}

// ---------------------------------------------------------------------------------- strobe

// prm(0..2) color, prm(3) frequency Hz, prm(4) flash duration 0..1, prm(5) blend with original
@fragment
fn fs_strobe(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let ph = fract(u.time * max(prm(3), 0.01));
    if (ph >= prm(4)) {
        return c;
    }
    let s = unpremul(c);
    return premul(vec4<f32>(mix(prm3(0), s.rgb, prm(5)), s.a));
}

// ---------------------------------------------------------------------------- kaleidoscope

// prm(0) segments, prm(1) angle degrees, prm(2..3) center
@fragment
fn fs_kaleidoscope(in: VOut) -> @location(0) vec4<f32> {
    let a = aspect();
    let c = prm2(2);
    let p = (in.uv - c) * vec2<f32>(a, 1.0);
    let r = length(p);
    let rot = prm(1) * PI / 180.0;
    let seg = 2.0 * PI / max(prm(0), 2.0);
    var ang = atan2(p.y, p.x) - rot;
    ang = ang - seg * floor(ang / seg);
    if (ang > seg * 0.5) {
        ang = seg - ang;
    }
    ang = ang + rot;
    let q = vec2<f32>(cos(ang), sin(ang)) * r;
    return s0(c + q / vec2<f32>(a, 1.0));
}

// ------------------------------------------------------------------------------ shake

// Affine transform (output pixels -> input pixels, prm(0..5)) that fills the frame by mirroring
// the image beyond its edges, so a shaking layer never shows gaps.
@fragment
fn fs_affine_mirror(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let q = vec2<f32>(prm(0) * p.x + prm(1) * p.y + prm(2), prm(3) * p.x + prm(4) * p.y + prm(5));
    let uv = q / u.in_size;
    return s0(vec2<f32>(mirror1(uv.x), mirror1(uv.y)));
}
