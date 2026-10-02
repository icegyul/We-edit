// Artistic and stylized looks: print, paint, drawing, neon, lens and screen effects. tex0 is the
// layer image (premultiplied, display encoding). Pixel sizes arrive already multiplied by the
// preview scale. u.time is the clip time in seconds for animated looks.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn s0(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex0, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

fn px_luma(uv: vec2<f32>) -> f32 {
    return luma(unpremul(s0(uv)).rgb);
}

fn rot2(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

// Sobel gradient magnitude of the luma, `w` pixels between taps.
fn sobel(uv: vec2<f32>, w: f32) -> f32 {
    let o = w / u.out_size;
    let tl = px_luma(uv + vec2<f32>(-o.x, -o.y));
    let t = px_luma(uv + vec2<f32>(0.0, -o.y));
    let tr = px_luma(uv + vec2<f32>(o.x, -o.y));
    let l = px_luma(uv + vec2<f32>(-o.x, 0.0));
    let r = px_luma(uv + vec2<f32>(o.x, 0.0));
    let bl = px_luma(uv + vec2<f32>(-o.x, o.y));
    let b = px_luma(uv + vec2<f32>(0.0, o.y));
    let br = px_luma(uv + vec2<f32>(o.x, o.y));
    let gx = (tr + 2.0 * r + br) - (tl + 2.0 * l + bl);
    let gy = (bl + 2.0 * b + br) - (tl + 2.0 * t + tr);
    return sqrt(gx * gx + gy * gy);
}

// ------------------------------------------------------------------------------- halftone

// Coverage of one rotated dot screen at pixel `p` for a channel read through `pick`.
fn dot_cover(p: vec2<f32>, size: f32, angle: f32, channel: i32, contrast: f32) -> f32 {
    let q = rot2(p, angle);
    let cell = floor(q / size);
    let center = (cell + vec2<f32>(0.5)) * size;
    let uv = rot2(center, -angle) / u.out_size;
    let s = unpremul(s0(uv));
    var v: f32;
    if (channel < 0) {
        v = luma(s.rgb);
    } else {
        v = s.rgb[channel];
    }
    // ink amount, pushed by the contrast setting
    let ink = clamp((1.0 - v - 0.5) * (1.0 + contrast) + 0.5, 0.0, 1.0);
    let radius = size * 0.5 * sqrt(ink) * 1.414;
    let d = length(q - center);
    return 1.0 - smoothstep(radius - 0.75, radius + 0.75, d);
}

// prm(0) dot size px, prm(1) angle degrees, prm(2) mode (0 one ink, 1 CMY), prm(3) contrast,
// prm(4..6) ink color, prm(7..9) paper color, prm(10) mix
@fragment
fn fs_halftone(in: VOut) -> @location(0) vec4<f32> {
    let c = s0(in.uv);
    let src = unpremul(c);
    let p = pixel(in.uv);
    let size = max(prm(0), 2.0);
    let a = prm(1) * PI / 180.0;
    var rgb: vec3<f32>;
    if (prm(2) > 0.5) {
        // cyan, magenta and yellow screens at classic, well separated angles
        let cy = dot_cover(p, size, a + 0.2618, 0, prm(3));
        let mg = dot_cover(p, size, a + 1.309, 1, prm(3));
        let ye = dot_cover(p, size, a, 2, prm(3));
        rgb = prm3(7) * (vec3<f32>(1.0) - vec3<f32>(cy, mg, ye));
    } else {
        let k = dot_cover(p, size, a + 0.7854, -1, prm(3));
        rgb = mix(prm3(7), prm3(4), k);
    }
    rgb = mix(src.rgb, rgb, clamp(prm(10), 0.0, 1.0));
    return premul(vec4<f32>(rgb, src.a));
}

// -------------------------------------------------------------------------------- duotone

// prm(0..2) shadow color, prm(3..5) highlight color, prm(6) contrast, prm(7) mix
@fragment
fn fs_duotone(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    var l = clamp(luma(s.rgb), 0.0, 1.0);
    l = clamp((l - 0.5) * (1.0 + prm(6)) + 0.5, 0.0, 1.0);
    l = l * l * (3.0 - 2.0 * l);
    let tone = mix(prm3(0), prm3(3), l);
    return premul(vec4<f32>(mix(s.rgb, tone, clamp(prm(7), 0.0, 1.0)), s.a));
}

// ------------------------------------------------------------------------------ oil paint

// Kuwahara filter: each pixel takes the mean of the calmest of the four quadrants around it,
// which flattens texture into brush-like patches while keeping edges. prm(0) radius px (1..10),
// prm(1) mix
@fragment
fn fs_oil_paint(in: VOut) -> @location(0) vec4<f32> {
    let r = i32(clamp(round(prm(0)), 1.0, 10.0));
    let o = 1.0 / u.out_size;
    var best = vec3<f32>(0.0);
    var best_var = 1e9;
    for (var q = 0; q < 4; q = q + 1) {
        let sx = select(-1, 1, (q & 1) == 1);
        let sy = select(-1, 1, (q & 2) == 2);
        var sum = vec3<f32>(0.0);
        var sum2 = vec3<f32>(0.0);
        var n = 0.0;
        for (var j = 0; j <= r; j = j + 1) {
            for (var i = 0; i <= r; i = i + 1) {
                let uv = in.uv + vec2<f32>(f32(i * sx), f32(j * sy)) * o;
                let v = unpremul(s0(uv)).rgb;
                sum = sum + v;
                sum2 = sum2 + v * v;
                n = n + 1.0;
            }
        }
        let mean = sum / n;
        let variance = sum2 / n - mean * mean;
        let total = variance.r + variance.g + variance.b;
        if (total < best_var) {
            best_var = total;
            best = mean;
        }
    }
    let s = unpremul(s0(in.uv));
    return premul(vec4<f32>(mix(s.rgb, best, clamp(prm(1), 0.0, 1.0)), s.a));
}

// --------------------------------------------------------------------------- pencil sketch

// prm(0) line strength, prm(1) line width px, prm(2) shading (cross hatching) 0..1,
// prm(3) hatch spacing px, prm(4..6) pencil color, prm(7..9) paper color, prm(10) keep color
@fragment
fn fs_sketch(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let edge = clamp(sobel(in.uv, max(prm(1), 0.5)) * prm(0) * 2.0, 0.0, 1.0);
    let p = pixel(in.uv);
    let dark = 1.0 - clamp(luma(s.rgb), 0.0, 1.0);
    let sp = max(prm(3), 2.0);
    // two hatch directions, the second only in the darkest areas
    let h1 = step(fract((p.x + p.y) / sp), 0.18) * step(0.35, dark);
    let h2 = step(fract((p.x - p.y) / sp), 0.18) * step(0.65, dark);
    let shade = clamp((h1 + h2) * prm(2), 0.0, 1.0);
    let ink = clamp(edge + shade * 0.6, 0.0, 1.0);
    let paper = mix(prm3(7), s.rgb * 0.35 + prm3(7) * 0.65, clamp(prm(10), 0.0, 1.0));
    let rgb = mix(paper, prm3(4), ink);
    return premul(vec4<f32>(rgb, s.a));
}

// ---------------------------------------------------------------------------- neon edges

// prm(0) thickness px, prm(1) glow intensity, prm(2) color mode (0 rainbow, 1 single),
// prm(3..5) color, prm(6) hue speed (turns per second), prm(7) background (0 black .. 1 image)
@fragment
fn fs_neon_edges(in: VOut) -> @location(0) vec4<f32> {
    let s = unpremul(s0(in.uv));
    let w = max(prm(0), 0.5);
    // a wider second ring of taps makes the soft halo around the line
    let e = clamp(sobel(in.uv, w) * 1.6, 0.0, 1.0);
    let halo = clamp(sobel(in.uv, w * 3.0) * 0.6, 0.0, 1.0);
    var col: vec3<f32>;
    if (prm(2) > 0.5) {
        col = prm3(3);
    } else {
        let h = fract(in.uv.x * 0.6 + in.uv.y * 0.3 + u.time * prm(6));
        col = hsv2rgb(vec3<f32>(h, 0.85, 1.0));
    }
    let bg = s.rgb * clamp(prm(7), 0.0, 1.0);
    let lit = col * (e * 1.4 + halo * 0.8) * prm(1);
    return premul(vec4<f32>(bg + lit, max(s.a, clamp(e + halo, 0.0, 1.0))));
}

// ------------------------------------------------------------------------------ lens flare

fn flare_disc(p: vec2<f32>, c: vec2<f32>, r: f32, soft: f32) -> f32 {
    let d = length(p - c);
    return 1.0 - smoothstep(r * (1.0 - soft), r, d);
}

// prm(0..1) light position (0..1), prm(2) brightness, prm(3) size, prm(4..6) tint,
// prm(7) streak amount
@fragment
fn fs_lens_flare(in: VOut) -> @location(0) vec4<f32> {
    let o = s0(in.uv);
    let asp = u.out_size.x / max(u.out_size.y, 1.0);
    let scale = vec2<f32>(asp, 1.0);
    let p = (in.uv - vec2<f32>(0.5)) * scale;
    let l = (prm2(0) - vec2<f32>(0.5)) * scale;
    let size = max(prm(3), 0.05);
    let tint = prm3(4);
    var light = vec3<f32>(0.0);
    // the source: a hot core and a wide glow
    let d = length(p - l);
    light = light + vec3<f32>(1.0) * exp(-d * 40.0 / size) * 1.5;
    light = light + tint * exp(-d * 6.0 / size) * 0.6;
    // anamorphic streak
    let streak = exp(-abs(p.y - l.y) * 220.0 / size) * exp(-abs(p.x - l.x) * 1.8 / size);
    light = light + tint * streak * prm(7);
    // ghosts on the line from the light through the center
    var ghosts = array<vec3<f32>, 5>(
        vec3<f32>(-0.35, 0.06, 0.0),
        vec3<f32>(-0.7, 0.03, 1.0),
        vec3<f32>(0.25, 0.09, 2.0),
        vec3<f32>(-1.15, 0.12, 0.0),
        vec3<f32>(-1.5, 0.05, 1.0),
    );
    for (var i = 0; i < 5; i = i + 1) {
        let g = ghosts[i];
        let c = l * g.x;
        let disc = flare_disc(p, c, g.y * size * 2.0, 0.6);
        var gc = tint;
        if (g.z > 1.5) {
            gc = vec3<f32>(0.4, 0.8, 1.0);
        } else if (g.z > 0.5) {
            gc = vec3<f32>(1.0, 0.55, 0.3);
        }
        light = light + gc * disc * 0.18;
    }
    // a faint halo ring around the center
    let ring = abs(length(p - l * -0.5) - 0.32 * size * 2.0);
    light = light + tint * (1.0 - smoothstep(0.0, 0.012, ring)) * 0.08;
    light = light * prm(2);
    let s = unpremul(o);
    let rgb = s.rgb + light;
    let a = clamp(s.a + luma(light) * (1.0 - s.a), 0.0, 1.0);
    return vec4<f32>(rgb * a, a);
}

// ---------------------------------------------------------------------------------- ripple

// prm(0..1) center, prm(2) amplitude px, prm(3) wavelength px, prm(4) speed (waves per second),
// prm(5) fade with distance 0..1
@fragment
fn fs_ripple(in: VOut) -> @location(0) vec4<f32> {
    let p = pixel(in.uv);
    let c = prm2(0) * u.out_size;
    let v = p - c;
    let d = length(v);
    let wl = max(prm(3), 2.0);
    let fade = exp(-d / max(u.out_size.x, 1.0) * prm(5) * 6.0);
    let wave = sin((d / wl - u.time * prm(4)) * 2.0 * PI) * prm(2) * fade;
    let dir = select(vec2<f32>(0.0), v / d, d > 0.001);
    let q = p + dir * wave;
    let lit = 1.0 + cos((d / wl - u.time * prm(4)) * 2.0 * PI) * 0.06 * fade * sign(prm(2));
    let c2 = s0(q / u.out_size);
    return vec4<f32>(c2.rgb * lit, c2.a);
}

// ------------------------------------------------------------------------------ zoom pulse

// prm(0) pulses per second, prm(1) amount (0..1), prm(2..3) center, prm(4) sharpness,
// prm(5) phase offset in seconds
@fragment
fn fs_zoom_pulse(in: VOut) -> @location(0) vec4<f32> {
    let t = (u.time + prm(5)) * prm(0);
    // a quick punch at the start of each beat that settles back
    let f = fract(t);
    let k = pow(1.0 - f, max(prm(4), 0.5));
    let zoom = 1.0 + prm(1) * k;
    let c = prm2(2);
    let uv = c + (in.uv - c) / zoom;
    // a slight blur along the zoom while it moves
    let step_uv = (in.uv - c) * prm(1) * k * 0.02;
    var col = vec4<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        col = col + s0(uv - step_uv * f32(i));
    }
    return col / 5.0;
}

// --------------------------------------------------------------------------------- CRT TV

// prm(0) curvature, prm(1) scanlines 0..1, prm(2) shadow mask 0..1, prm(3) vignette 0..1,
// prm(4) line count, prm(5) flicker 0..1
@fragment
fn fs_crt(in: VOut) -> @location(0) vec4<f32> {
    var uv = in.uv * 2.0 - vec2<f32>(1.0);
    let k = prm(0) * 0.25;
    uv = uv * (1.0 + k * dot(uv, uv));
    let tex_uv = uv * 0.5 + vec2<f32>(0.5);
    if (any(tex_uv < vec2<f32>(0.0)) || any(tex_uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let c = unpremul(s0(tex_uv));
    var rgb = c.rgb;
    let lines = max(prm(4), 50.0);
    let scan = 0.5 + 0.5 * cos(tex_uv.y * lines * 2.0 * PI);
    rgb = rgb * mix(1.0, 0.55 + 0.45 * scan, clamp(prm(1), 0.0, 1.0)) * (1.0 + prm(1) * 0.25);
    let col = i32(floor(pixel(in.uv).x)) % 3;
    var mask = vec3<f32>(0.75);
    mask[col] = 1.25;
    rgb = rgb * mix(vec3<f32>(1.0), mask, clamp(prm(2), 0.0, 1.0));
    let vig = 1.0 - dot(uv * 0.7, uv * 0.7);
    rgb = rgb * mix(1.0, clamp(vig, 0.0, 1.0), clamp(prm(3), 0.0, 1.0));
    let fl = 1.0 - prm(5) * 0.08 * (0.5 + 0.5 * sin(u.time * 60.0));
    return premul(vec4<f32>(rgb * fl, c.a));
}
