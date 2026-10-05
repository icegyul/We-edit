// Noise, grain and film looks. tex0 is the layer image (premultiplied, display encoding);
// u.time is the clip time in seconds for animated looks. The formulas are this project's own.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn s0(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex0, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

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

// Roughly bell-shaped noise in -0.5..0.5 for one cell.
fn gnoise(cell: vec2<f32>, seed: f32) -> f32 {
    let o = vec2<f32>(seed * 17.13, seed * 7.71);
    return (lhash(cell + o) + lhash(cell + o + vec2<f32>(31.7, 11.3))
        + lhash(cell + o + vec2<f32>(71.3, 53.9))) / 3.0 - 0.5;
}

fn aspect() -> f32 {
    return u.out_size.x / max(u.out_size.y, 1.0);
}

fn px_of(uv: vec2<f32>) -> vec2<f32> {
    return uv * u.out_size;
}

fn frame_of(fps: f32) -> f32 {
    return floor(u.time * fps);
}

fn hue_tint(h: f32) -> vec3<f32> {
    let c = hsv2rgb(vec3<f32>(fract(h / 360.0), 1.0, 1.0));
    return c - vec3<f32>(luma(c));
}

// ------------------------------------------------------------------------------ stock grain

// prm(0) amount, prm(1) grain size px, prm(2) roughness 0..1 (fine to clumped), prm(3) color
// amount 0..1, prm(4) shadows weight, prm(5) highlights weight, prm(6) frame
@fragment
fn fs_stock_grain(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let size = max(prm(1), 0.5);
    let p = px_of(in.uv);
    let fine = vec3<f32>(
        gnoise(floor(p / size), prm(6)),
        gnoise(floor(p / size) + vec2<f32>(5.0, 3.0), prm(6)),
        gnoise(floor(p / size) + vec2<f32>(9.0, 13.0), prm(6)),
    );
    let big = size * 2.6;
    let clump = vec3<f32>(
        vnoise(p / big + vec2<f32>(prm(6) * 13.7, 0.0)) - 0.5,
        vnoise(p / big + vec2<f32>(prm(6) * 13.7, 41.0)) - 0.5,
        vnoise(p / big + vec2<f32>(prm(6) * 13.7, 83.0)) - 0.5,
    ) * 1.4;
    let n3 = mix(fine, clump, prm(2));
    let n = mix(vec3<f32>(n3.g), n3, prm(3));
    let l = clamp(luma(s.rgb), 0.0, 1.0);
    // grain shows most in the midtones; the two weights lift or flatten the ends
    let w = 4.0 * l * (1.0 - l) + prm(4) * (1.0 - l) * (1.0 - l) + prm(5) * l * l;
    return premul(vec4<f32>(s.rgb + n * prm(0) * 1.1 * w, s.a));
}

// ---------------------------------------------------------------------------- sensor noise

// prm(0) luma noise, prm(1) color noise, prm(2) color blotch size px, prm(3) shadows bias 0..1,
// prm(4) frame
@fragment
fn fs_sensor_noise(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let p = px_of(in.uv);
    let l = clamp(luma(s.rgb), 0.0, 1.0);
    let w = mix(1.0, (1.0 - l) * (1.0 - l) * 1.6 + 0.15, prm(3));
    let ln = gnoise(floor(p), prm(4)) * 2.0;
    let b = max(prm(2), 1.0);
    let cn = vec3<f32>(
        vnoise(p / b + vec2<f32>(prm(4) * 5.3, 0.0)) - 0.5,
        vnoise(p / b + vec2<f32>(prm(4) * 5.3, 37.0)) - 0.5,
        vnoise(p / b + vec2<f32>(prm(4) * 5.3, 71.0)) - 0.5,
    ) * 1.6;
    let rgb = s.rgb + vec3<f32>(ln * prm(0)) * w * 0.5 + (cn - vec3<f32>(luma(cn))) * prm(1) * w * 0.5;
    return premul(vec4<f32>(rgb, s.a));
}

// ---------------------------------------------------------------------------- salt & pepper

// prm(0) amount 0..1, prm(1) speck size px, prm(2) colored, prm(3) frame
@fragment
fn fs_salt_pepper(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let cell = floor(px_of(in.uv) / max(prm(1), 1.0));
    let h = lhash(cell + vec2<f32>(prm(3) * 19.1, prm(3) * 3.7));
    let a = prm(0) * 0.25;
    var rgb = s.rgb;
    if (h < a) {
        rgb = vec3<f32>(0.0);
    } else if (h > 1.0 - a) {
        rgb = vec3<f32>(1.0);
    }
    if (prm(2) > 0.5 && (h < a || h > 1.0 - a)) {
        rgb = vec3<f32>(lhash(cell + vec2<f32>(5.0, prm(3))), lhash(cell + vec2<f32>(9.0, prm(3))),
            lhash(cell + vec2<f32>(13.0, prm(3))));
    }
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------------ TV static

// prm(0) amount, prm(1) size px, prm(2) scan lines 0..1, prm(3) bands 0..1, prm(4) frame
@fragment
fn fs_tv_static(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let p = px_of(in.uv);
    let size = max(prm(1), 1.0);
    let row = floor(p.y / size);
    // some rows get stronger noise, like a failing signal
    let rowgain = 1.0 + prm(3) * 1.5 * step(0.85, lhash(vec2<f32>(row, prm(4))));
    let n = lhash(floor(p / size) + vec2<f32>(prm(4) * 23.7, prm(4) * 5.9));
    var rgb = mix(s.rgb, vec3<f32>(n), clamp(prm(0) * rowgain, 0.0, 1.0));
    let line = 0.5 + 0.5 * sin(p.y * 3.14159 / max(size * 0.5, 1.0));
    rgb = rgb * (1.0 - prm(2) * 0.45 * (1.0 - line));
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------- fractal noise

fn blend_mode(b: vec3<f32>, t: vec3<f32>, mode: i32) -> vec3<f32> {
    if (mode == 1) {
        return select(1.0 - 2.0 * (1.0 - b) * (1.0 - t), 2.0 * b * t, b < vec3<f32>(0.5));
    }
    if (mode == 2) { return b + t; }
    if (mode == 3) { return b * t; }
    if (mode == 4) { return 1.0 - (1.0 - b) * (1.0 - t); }
    if (mode == 5) {
        return (1.0 - 2.0 * t) * b * b + 2.0 * t * b;
    }
    return t;
}

// prm(0) feature size px, prm(1) octaves, prm(2) evolution, prm(3) opacity, prm(4) brightness,
// prm(5) contrast, prm(6) blend mode, prm(7) colored, prm(8..9) offset px, prm(10) seed
@fragment
fn fs_fractal_noise(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let size = max(prm(0), 2.0);
    let ev = prm(2);
    var q = (px_of(in.uv) + prm2(8)) / size + vec2<f32>(prm(10) * 37.1, prm(10) * 11.3);
    var sum = vec3<f32>(0.0);
    var amp = 0.5;
    var norm = 0.0;
    let octaves = i32(clamp(prm(1), 1.0, 8.0));
    for (var i = 0; i < 8; i = i + 1) {
        if (i >= octaves) { break; }
        let o = vec2<f32>(ev * (0.7 + f32(i) * 0.31), -ev * (0.4 + f32(i) * 0.23));
        let r = vnoise(q + o);
        let g = vnoise(q + o + vec2<f32>(51.0, 17.0));
        let b = vnoise(q + o + vec2<f32>(23.0, 91.0));
        sum = sum + amp * vec3<f32>(r, mix(r, g, prm(7)), mix(r, b, prm(7)));
        norm = norm + amp;
        q = q * 2.03 + vec2<f32>(17.1, 9.2);
        amp = amp * 0.5;
    }
    var n = sum / max(norm, 1e-4);
    n = clamp((n - 0.5) * prm(5) + 0.5 + prm(4), vec3<f32>(0.0), vec3<f32>(1.0));
    let mixed = blend_mode(s.rgb, n, i32(prm(6) + 0.5));
    return premul(vec4<f32>(mix(s.rgb, mixed, prm(3)), s.a));
}

// -------------------------------------------------------------------------------- film dust

fn seg_dist(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - (a + ab * t));
}

// prm(0) dust, prm(1) hairs, prm(2) scratches (all 0..1), prm(3) size scale, prm(4) seed,
// prm(5) frames per second the dirt changes at, prm(6) 0 dark, 1 light, 2 both
@fragment
fn fs_film_dust(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let p = px_of(in.uv);
    let k = max(prm(3), 0.1) * u.out_size.y / 1080.0;
    let frame = frame_of(max(prm(5), 1.0));
    let sd = prm(4);
    var rgb = s.rgb;
    var ink = 0.0;
    var light = 0.0;

    // dust: specks of uneven shape
    let cs = 48.0 * k;
    let cell = floor(p / cs);
    let hit = lhash(cell + vec2<f32>(frame * 0.71, sd)) ;
    if (hit > 1.0 - prm(0) * 0.12) {
        let ctr = (cell + vec2<f32>(lhash(cell + frame), lhash(cell - frame))) * cs;
        let d = length(p - ctr);
        let r = (1.0 + 3.0 * lhash(cell + vec2<f32>(7.0, frame))) * k;
        let wob = 0.6 + 0.8 * vnoise((p - ctr) / max(k, 0.3) * 0.9 + vec2<f32>(frame, sd));
        let m = 1.0 - smoothstep(r * wob * 0.6, r * wob + 0.8, d);
        if (lhash(cell + vec2<f32>(3.0, sd)) > 0.5) { ink = max(ink, m); } else { light = max(light, m); }
    }

    // hairs: a short bent line in a cell
    let hs = 220.0 * k;
    let hc = floor(p / hs);
    if (lhash(hc + vec2<f32>(frame * 0.37, sd + 9.0)) > 1.0 - prm(1) * 0.18) {
        let base = (hc + vec2<f32>(0.2 + 0.6 * lhash(hc + frame), 0.2 + 0.6 * lhash(hc - frame))) * hs;
        var a = base;
        var ang = lhash(hc + vec2<f32>(sd, 1.0)) * 6.28;
        var dmin = 1e9;
        for (var i = 0; i < 6; i = i + 1) {
            ang = ang + (lhash(hc + vec2<f32>(f32(i), frame)) - 0.5) * 1.2;
            let b = a + vec2<f32>(cos(ang), sin(ang)) * 16.0 * k;
            dmin = min(dmin, seg_dist(p, a, b));
            a = b;
        }
        ink = max(ink, 1.0 - smoothstep(0.4 * k, 1.4 * k, dmin));
    }

    // scratches: long thin vertical lines that stay a few frames
    for (var j = 0.0; j < 3.0; j = j + 1.0) {
        let slot = floor(frame / 4.0);
        if (lhash(vec2<f32>(j * 13.0 + sd, slot)) < prm(2) * 0.8) {
            let x = lhash(vec2<f32>(j + sd * 3.0, slot * 1.3)) * u.out_size.x;
            let wob = (vnoise(vec2<f32>(p.y / (140.0 * k), j + slot)) - 0.5) * 6.0 * k;
            let d = abs(p.x - x - wob);
            let vis = smoothstep(0.0, 0.2, vnoise(vec2<f32>(p.y / (260.0 * k), j * 7.0 + slot)));
            light = max(light, (1.0 - smoothstep(0.0, 1.3 * k, d)) * vis * 0.7);
        }
    }
    let mode = i32(prm(6) + 0.5);
    if (mode == 0) { light = 0.0; }
    if (mode == 1) { light = max(light, ink); ink = 0.0; }
    rgb = mix(rgb, vec3<f32>(0.04), ink * 0.9);
    rgb = mix(rgb, vec3<f32>(0.95, 0.93, 0.85), light);
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------- film emulation

fn tone(rgb_in: vec3<f32>, con: f32, sat: f32, lift: f32, sh: vec3<f32>, hi: vec3<f32>) -> vec3<f32> {
    var rgb = (rgb_in - 0.5) * con + 0.5;
    let l = clamp(luma(rgb), 0.0, 1.0);
    rgb = mix(vec3<f32>(l), rgb, sat);
    rgb = rgb + sh * (1.0 - l) * (1.0 - l) + hi * l * l;
    return rgb * (1.0 - lift) + lift;
}

// prm(0) look, prm(1) amount
@fragment
fn fs_film_emulation(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let c = s.rgb;
    let look = i32(prm(0) + 0.5);
    var g = c;
    if (look == 0) {
        // warm negative: golden highlights, slightly lifted shadows
        g = tone(c, 1.08, 1.12, 0.02, vec3<f32>(0.01, 0.0, -0.01), vec3<f32>(0.06, 0.025, -0.05));
    } else if (look == 1) {
        // cool stock: green-cyan shadows, crisp
        g = tone(c, 1.1, 1.15, 0.01, vec3<f32>(-0.025, 0.02, 0.035), vec3<f32>(-0.01, 0.03, 0.01));
    } else if (look == 2) {
        // bleach bypass: silver retention
        let l = luma(c);
        let ov = select(1.0 - 2.0 * (1.0 - l) * (1.0 - c), 2.0 * l * c, vec3<f32>(l) < vec3<f32>(0.5));
        g = mix(vec3<f32>(l), ov, 0.9);
        g = tone(g, 1.12, 0.62, 0.0, vec3<f32>(0.0), vec3<f32>(0.0));
    } else if (look == 3) {
        // cross process: steep channel curves
        let r = smoothstep(0.0, 1.0, clamp((c.r - 0.5) * 1.35 + 0.5, 0.0, 1.0));
        let gg = pow(clamp(c.g, 0.0, 1.0), 0.88);
        let b = 0.14 + 0.68 * c.b;
        g = vec3<f32>(r, gg, b);
        g = tone(g, 1.05, 1.25, 0.0, vec3<f32>(0.0, 0.01, 0.03), vec3<f32>(0.05, 0.04, -0.06));
    } else if (look == 4) {
        // two-strip: orange and cyan only
        g = vec3<f32>(c.r * 0.92 + c.g * 0.08, c.g * 0.55 + c.b * 0.45, c.g * 0.42 + c.b * 0.58);
        g = tone(g, 1.08, 1.2, 0.0, vec3<f32>(-0.02, 0.01, 0.03), vec3<f32>(0.04, 0.0, -0.04));
    } else if (look == 5) {
        // silver noir
        let l = luma(c);
        g = tone(vec3<f32>(l), 1.35, 0.0, 0.015, vec3<f32>(-0.01, 0.0, 0.015), vec3<f32>(0.01, 0.005, -0.005));
        g = g * vec3<f32>(0.99, 1.0, 1.02);
    } else if (look == 6) {
        // day for night: dark, blue, desaturated
        g = tone(c * 0.55, 1.15, 0.65, 0.0, vec3<f32>(-0.03, 0.0, 0.07), vec3<f32>(-0.02, 0.02, 0.1));
        g = g * vec3<f32>(0.7, 0.85, 1.2);
    } else if (look == 7) {
        // faded matte
        g = tone(c, 0.9, 0.85, 0.09, vec3<f32>(0.01, 0.01, 0.02), vec3<f32>(0.02, 0.01, -0.01));
        g = g * 0.94 + 0.0;
    } else if (look == 8) {
        // teal and orange
        g = tone(c, 1.1, 1.1, 0.01, vec3<f32>(-0.045, 0.02, 0.055), vec3<f32>(0.065, 0.02, -0.065));
    } else {
        // seventies: warm yellow fade with magenta shadows
        g = tone(c, 0.95, 0.9, 0.06, vec3<f32>(0.03, -0.01, 0.02), vec3<f32>(0.05, 0.03, -0.05));
    }
    return premul(vec4<f32>(mix(c, g, prm(1)), s.a));
}

// prm(0) amount, prm(1) contrast, prm(2) color kept 0..1
@fragment
fn fs_bleach_bypass(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let c = s.rgb;
    let l = luma(c);
    let ov = select(1.0 - 2.0 * (1.0 - l) * (1.0 - c), 2.0 * l * c, vec3<f32>(l) < vec3<f32>(0.5));
    var g = mix(c, ov, 0.9);
    let gl = luma(g);
    g = mix(vec3<f32>(gl), g, prm(2));
    g = (g - 0.5) * prm(1) + 0.5;
    return premul(vec4<f32>(mix(c, g, prm(0)), s.a));
}

// prm(0) amount, prm(1) shadow hue, prm(2) highlight hue, prm(3) shadow strength,
// prm(4) highlight strength, prm(5) midtone balance -1..1, prm(6) saturation
@fragment
fn fs_split_tone(in: VOut) -> @location(0) vec4<f32>  {
    let s = unpremul(s0(in.uv));
    var rgb = s.rgb;
    let l = clamp(luma(rgb), 0.0, 1.0);
    let bal = clamp(0.5 + prm(5) * 0.35, 0.05, 0.95);
    let ws = 1.0 - smoothstep(0.0, bal, l);
    let wh = smoothstep(bal, 1.0, l);
    var g = rgb + hue_tint(prm(1)) * prm(3) * 0.35 * ws + hue_tint(prm(2)) * prm(4) * 0.35 * wh;
    g = mix(vec3<f32>(luma(g)), g, prm(6));
    return premul(vec4<f32>(mix(rgb, g, prm(0)), s.a));
}

// ------------------------------------------------------------------------------- gate weave

// prm(0) weave px, prm(1) rotation degrees, prm(2) frames per second, prm(3) flicker 0..1,
// prm(4) seed
@fragment
fn fs_gate_weave(in: VOut) -> @location(0) vec4<f32> {
    let fr = frame_of(max(prm(2), 1.0));
    let sd = prm(4) * 3.7;
    // smooth path through random points, so the picture drifts instead of jumping
    let t = fr * 0.35;
    let ix = floor(t);
    let fx = fract(t);
    let w = fx * fx * (3.0 - 2.0 * fx);
    let ox = mix(lhash(vec2<f32>(ix, sd)), lhash(vec2<f32>(ix + 1.0, sd)), w) - 0.5;
    let oy = mix(lhash(vec2<f32>(ix, sd + 9.0)), lhash(vec2<f32>(ix + 1.0, sd + 9.0)), w) - 0.5;
    let rot = (mix(lhash(vec2<f32>(ix, sd + 17.0)), lhash(vec2<f32>(ix + 1.0, sd + 17.0)), w) - 0.5)
        * prm(1) * 0.0174533 * 2.0;
    var d = (in.uv - vec2<f32>(0.5)) * u.out_size;
    let cs = cos(rot);
    let sn = sin(rot);
    d = vec2<f32>(d.x * cs - d.y * sn, d.x * sn + d.y * cs);
    let uv = (d + vec2<f32>(ox, oy) * 2.0 * prm(0)) / u.out_size + vec2<f32>(0.5);
    let c = s0(uv);
    let fl = 1.0 + (lhash(vec2<f32>(fr, sd + 31.0)) - 0.5) * prm(3) * 0.4;
    return vec4<f32>(c.rgb * fl, c.a);
}

// ----------------------------------------------------------------------------- pro-mist

// prm(0) amount, prm(1) radius px, prm(2) threshold 0..1, prm(3) warmth -1..1
@fragment
fn fs_promist(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    var glow = vec3<f32>(0.0);
    let r = max(prm(1), 1.0);
    // a different rotation at every pixel turns the banding of few taps into fine noise
    let spin = fract(52.9829189 * fract(dot(px_of(in.uv), vec2<f32>(0.06711056, 0.00583715)))) * 6.2832;
    for (var i = 0; i < 28; i = i + 1) {
        let fi = f32(i) + 0.5;
        let rad = sqrt(fi / 28.0);
        let a = fi * 2.39996 + spin;
        let o = vec2<f32>(cos(a), sin(a)) * rad * r / u.out_size;
        let t = unpremul(s0(in.uv + o)).rgb;
        let w = 1.0 - rad * 0.5;
        acc = acc + t * w;
        glow = glow + t * smoothstep(prm(2), 1.0, luma(t)) * w;
        wsum = wsum + w;
    }
    acc = acc / wsum;
    glow = glow / wsum;
    let warm = vec3<f32>(1.0 + prm(3) * 0.12, 1.0, 1.0 - prm(3) * 0.12);
    let rgb = s.rgb * (1.0 - prm(0) * 0.35) + acc * prm(0) * 0.35 + glow * warm * prm(0) * 0.9;
    return premul(vec4<f32>(rgb, s.a));
}

// ------------------------------------------------------------------------------- streaks

// prm(0) threshold, prm(1) length px, prm(2) intensity, prm(3..5) tint
@fragment
fn fs_anamorphic(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let s = unpremul(c);
    var streak = vec3<f32>(0.0);
    var wsum = 0.0;
    let len = max(prm(1), 2.0);
    let jitter = fract(52.9829189 * fract(dot(px_of(in.uv), vec2<f32>(0.06711056, 0.00583715)))) - 0.5;
    for (var i = -16; i <= 16; i = i + 1) {
        let x = (f32(i) + jitter) / 16.0;
        let w = exp(-abs(x) * 3.2);
        let t = unpremul(s0(in.uv + vec2<f32>(x * len / u.out_size.x, 0.0))).rgb;
        let b = pow(max(luma(t) - prm(0), 0.0) / max(1.0 - prm(0), 0.05), 2.5);
        streak = streak + t * b * w;
        wsum = wsum + w;
    }
    streak = streak / wsum * 1.1;
    let tint = prm3(3);
    return premul(vec4<f32>(s.rgb + streak * tint * prm(2), s.a));
}

// ------------------------------------------------------------------ chromatic aberration

// prm(0) amount px at the edge (radial) or total shift (linear), prm(1) mode 0 radial 1 linear,
// prm(2) angle degrees (linear)
@fragment
fn fs_chromatic(in: VOut) -> @location(0) vec4<f32> {
    let g = s0(in.uv);
    let ctr = vec2<f32>(0.5);
    var dir = (in.uv - ctr) * 2.0;
    if (prm(1) > 0.5) {
        let a = radians(prm(2));
        dir = vec2<f32>(cos(a), sin(a)) * 0.7;
    }
    let stp = dir * prm(0) / u.out_size * 0.5;
    var r = 0.0;
    var b = 0.0;
    for (var i = 0; i < 7; i = i + 1) {
        let f = (f32(i) / 6.0) * 2.0 - 1.0;
        r = r + s0(in.uv + stp * (1.0 + f * 0.5)).r;
        b = b + s0(in.uv - stp * (1.0 + f * 0.5)).b;
    }
    let rgb = vec3<f32>(r / 7.0, g.g, b / 7.0);
    return vec4<f32>(rgb, g.a);
}

// ------------------------------------------------------------------------------- super 8

// prm(0) grain, prm(1) weave px, prm(2) flicker, prm(3) warmth, prm(4) fade, prm(5) light leak,
// prm(6) vignette, prm(7) saturation, prm(8) seed
@fragment
fn fs_super8(in: VOut) -> @location(0) vec4<f32> {
    let fr = frame_of(18.0);
    let sd = prm(8);
    var uv = in.uv;
    let jx = (lhash(vec2<f32>(fr, sd)) - 0.5) * 2.0;
    let jy = (lhash(vec2<f32>(fr, sd + 5.0)) - 0.5) * 2.0;
    uv = uv + vec2<f32>(jx, jy) * prm(1) / u.out_size;
    let c = s0(uv);
    var rgb = unpremul(c).rgb;
    let p = px_of(in.uv);

    // warm, faded, slightly soft color
    let l = luma(rgb);
    rgb = mix(vec3<f32>(l), rgb, prm(7));
    rgb = rgb * vec3<f32>(1.0 + prm(3) * 0.14, 1.0 + prm(3) * 0.03, 1.0 - prm(3) * 0.16);
    rgb = rgb * (1.0 - prm(4) * 0.45) + prm(4) * vec3<f32>(0.09, 0.07, 0.05);

    // grain
    let n = gnoise(floor(p / max(1.8 * u.out_size.y / 1080.0, 0.8)), fr + sd);
    rgb = rgb + vec3<f32>(n) * prm(0) * 0.9;

    // flicker
    rgb = rgb * (1.0 + (lhash(vec2<f32>(fr, sd + 21.0)) - 0.5) * prm(2) * 0.5);

    // light leak: a warm glow that drifts in from the left or right edge
    let phase = vnoise(vec2<f32>(u.time * 0.35, sd));
    let side = select(1.0 - in.uv.x, in.uv.x, lhash(vec2<f32>(floor(u.time * 0.2), sd)) > 0.5);
    let reach = 0.18 + 0.5 * phase;
    let leak = smoothstep(reach, 0.0, side) * smoothstep(0.3, 0.8, phase) * prm(5);
    let leak_col = mix(vec3<f32>(1.0, 0.35, 0.08), vec3<f32>(1.0, 0.75, 0.25), vnoise(vec2<f32>(in.uv.y * 3.0, u.time * 0.5)));
    rgb = 1.0 - (1.0 - rgb) * (1.0 - leak_col * leak);

    // vignette
    let v = smoothstep(0.35, 1.0, length((in.uv - vec2<f32>(0.5)) * vec2<f32>(aspect() * 0.8, 1.0) * 1.5));
    rgb = rgb * (1.0 - prm(6) * v * 0.8);
    return premul(vec4<f32>(rgb, unpremul(c).a));
}
