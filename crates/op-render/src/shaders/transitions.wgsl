// Video transitions. tex0 is the outgoing image (A), tex1 the incoming image (B), both already
// composited through Motion onto transparent sequence-sized canvases. u.progress runs 0..1.
// The Rust side swaps A/B and reverses progress for the Reverse option.

@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

fn ta(uv: vec2<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(tex0, samp, uv, 0.0);
}

fn tb(uv: vec2<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(tex1, samp, uv, 0.0);
}

// Directions in catalog order: west, east, north, south, north-west, north-east, south-west,
// south-east. The vector points where the incoming image travels.
fn direction(i: i32) -> vec2<f32> {
    switch i {
        case 1: { return vec2<f32>(-1.0, 0.0); }
        case 2: { return vec2<f32>(0.0, 1.0); }
        case 3: { return vec2<f32>(0.0, -1.0); }
        case 4: { return vec2<f32>(1.0, 1.0); }
        case 5: { return vec2<f32>(-1.0, 1.0); }
        case 6: { return vec2<f32>(1.0, -1.0); }
        case 7: { return vec2<f32>(-1.0, -1.0); }
        default: { return vec2<f32>(1.0, 0.0); }
    }
}

// prm(0) mode: 0 cross, 1 dip to black, 2 dip to white, 3 film, 4 additive, 5 non-additive
@fragment
fn fs_tr_dissolve(in: VOut) -> @location(0) vec4<f32> {
    let a = ta(in.uv);
    let b = tb(in.uv);
    let t = u.progress;
    switch i32(prm(0) + 0.5) {
        case 1, 2: {
            var c = vec4<f32>(0.0, 0.0, 0.0, 1.0);
            if (i32(prm(0) + 0.5) == 2) {
                c = vec4<f32>(1.0);
            }
            if (t < 0.5) {
                return mix(a, c, t * 2.0);
            }
            return mix(c, b, t * 2.0 - 1.0);
        }
        case 3: {
            let la = vec4<f32>(to_linear(a.rgb), a.a);
            let lb = vec4<f32>(to_linear(b.rgb), b.a);
            let m = mix(la, lb, t);
            return vec4<f32>(from_linear(m.rgb), m.a);
        }
        case 4: {
            let wa = clamp(2.0 - 2.0 * t, 0.0, 1.0);
            let wb = clamp(2.0 * t, 0.0, 1.0);
            return a * wa + b * wb;
        }
        case 5: {
            // non-additive mix: the brighter image wins per pixel, so B shows first in its
            // brightest areas
            return max(a * min(1.0, 2.0 * (1.0 - t)), b * min(1.0, 2.0 * t));
        }
        default: {
            return mix(a, b, t);
        }
    }
}

fn edge_mix(a: vec4<f32>, b: vec4<f32>, d: f32, feather: f32, border: f32, border_color: vec3<f32>) -> vec4<f32> {
    // d: signed distance in pixels, negative inside the revealed (B) region
    let f = max(feather, 0.75);
    let k = clamp(0.5 - d / f, 0.0, 1.0);
    var o = mix(a, b, k);
    if (border > 0.0) {
        let bk = clamp(0.5 - (abs(d) - border * 0.5) / 0.75, 0.0, 1.0);
        o = mix(o, vec4<f32>(border_color, 1.0), bk);
    }
    return o;
}

// Iris shapes growing from a center. prm(0) shape (0 round, 1 box, 2 cross, 3 diamond),
// prm(1,2) center uv, prm(3) border px, prm(4..6) border color, prm(7) feather px
@fragment
fn fs_tr_iris(in: VOut) -> @location(0) vec4<f32> {
    let px = pixel(in.uv);
    let c = prm2(1) * u.out_size;
    let d = px - c;
    let far = max(max(length(c), length(u.out_size - c)), max(length(vec2<f32>(c.x, u.out_size.y - c.y)), length(vec2<f32>(u.out_size.x - c.x, c.y))));
    let r = u.progress * far * 1.02;
    var dist: f32;
    switch i32(prm(0) + 0.5) {
        case 1: { dist = max(abs(d.x) / u.out_size.x * u.out_size.y, abs(d.y)) - r * 0.72; }
        case 2: {
            let w = r * (0.3 + 0.7 * u.progress * u.progress);
            dist = min(max(abs(d.x) - w, abs(d.y) - r), max(abs(d.x) - r, abs(d.y) - w));
        }
        case 3: { dist = (abs(d.x) + abs(d.y)) * 0.7071 - r * 1.0; }
        default: { dist = length(d) - r; }
    }
    return edge_mix(ta(in.uv), tb(in.uv), dist, prm(7), prm(3), prm3(4));
}

// prm(0) mode: 0 push, 1 slide, 2 whip, 3 inset; prm(1) direction; prm(2) border px;
// prm(3..5) border color
@fragment
fn fs_tr_slide(in: VOut) -> @location(0) vec4<f32> {
    let dir = direction(i32(prm(1) + 0.5));
    let t = u.progress;
    let mode = i32(prm(0) + 0.5);
    if (mode == 0 || mode == 2) {
        // both images travel together
        let e = select(t, t * t * (3.0 - 2.0 * t), mode == 2);
        let offset = dir * e;
        var a = ta(in.uv - offset);
        var b = tb(in.uv - offset + dir);
        if (mode == 2) {
            // motion blur along the travel direction, strongest mid-way
            let blur = sin(t * PI) * 0.15;
            var acc_a = vec4<f32>(0.0);
            var acc_b = vec4<f32>(0.0);
            for (var i = 0.0; i < 12.0; i = i + 1.0) {
                let s = (i / 11.0 - 0.5) * blur;
                acc_a = acc_a + ta(in.uv - offset + dir * s);
                acc_b = acc_b + tb(in.uv - offset + dir + dir * s);
            }
            a = acc_a / 12.0;
            b = acc_b / 12.0;
        }
        return a + b;
    }
    if (mode == 3) {
        // B grows from the corner/edge it comes from
        let origin = vec2<f32>(0.5) - dir * 0.5;
        let s = max(t, 1e-4);
        let q = (in.uv - origin) / s + origin;
        let inside = all(q >= vec2<f32>(0.0)) && all(q <= vec2<f32>(1.0));
        if (inside) {
            return tb(q);
        }
        return ta(in.uv);
    }
    // slide: B moves in over A
    let q = in.uv - dir * (t - 1.0);
    var o = ta(in.uv);
    if (all(q >= vec2<f32>(0.0)) && all(q <= vec2<f32>(1.0))) {
        let b = tb(q);
        o = b + o * (1.0 - b.a);
        if (prm(2) > 0.0) {
            let px = q * u.out_size;
            let e = min(min(px.x, u.out_size.x - px.x), min(px.y, u.out_size.y - px.y));
            if (e < prm(2)) {
                o = vec4<f32>(prm3(3), 1.0);
            }
        }
    }
    return o;
}

// prm(0) mode: 0 split, 1 barn doors, 2 center split; prm(1) orientation (0 vertical seam,
// 1 horizontal seam)
@fragment
fn fs_tr_split(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let mode = i32(prm(0) + 0.5);
    let horizontal = prm(1) > 0.5;
    if (mode == 2) {
        // four quarters of A move to the corners
        let s = sign(in.uv - 0.5);
        let q = in.uv - s * t * 0.5;
        if (all(sign(q - 0.5) == s)) {
            return ta(q);
        }
        return tb(in.uv);
    }
    var x = in.uv.x;
    if (horizontal) {
        x = in.uv.y;
    }
    let open = t * 0.5;
    if (mode == 1) {
        // barn doors: B appears between two edges moving out from the center
        if (abs(x - 0.5) < open) {
            return tb(in.uv);
        }
        return ta(in.uv);
    }
    // split: the two halves of A slide away
    if (abs(x - 0.5) < open) {
        return tb(in.uv);
    }
    let shift = select(open, -open, x < 0.5);
    var q = in.uv;
    if (horizontal) {
        q.y = q.y - shift;
    } else {
        q.x = q.x - shift;
    }
    return ta(q);
}

// prm(0) mode: 0 wipe, 1 clock, 2 venetian blinds, 3 checker, 4 random blocks;
// prm(1) direction / orientation; prm(2) border px; prm(3..5) border color; prm(6) feather px;
// prm(7,8) center (clock) or columns/rows; prm(9) start angle or seed; prm(10) bands
@fragment
fn fs_tr_wipe(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let a = ta(in.uv);
    let b = tb(in.uv);
    let px = pixel(in.uv);
    switch i32(prm(0) + 0.5) {
        case 1: {
            let d = (in.uv - prm2(7)) * u.out_size;
            var ang = atan2(d.x, -d.y) * 180.0 / PI - prm(9);
            ang = ang - floor(ang / 360.0) * 360.0;
            let sweep = t * 360.0;
            // distance in pixels along the arc at this radius
            let dist = (ang - sweep) * PI / 180.0 * max(length(d), 1.0);
            return edge_mix(a, b, dist, prm(6), prm(2), prm3(3));
        }
        case 2: {
            let n = max(prm(10), 2.0);
            var x = in.uv.y;
            if (prm(1) > 0.5) {
                x = in.uv.x;
            }
            let band = fract(x * n);
            let size = select(u.out_size.y, u.out_size.x, prm(1) > 0.5) / n;
            return edge_mix(a, b, (band - t) * size, prm(6), prm(2), prm3(3));
        }
        case 3: {
            let cells = max(prm2(7), vec2<f32>(1.0));
            let cell = floor(in.uv * cells);
            let odd = (i32(cell.x) + i32(cell.y)) % 2;
            let local = fract(in.uv.x * cells.x);
            var tt = t * 2.0;
            if (odd == 1) {
                tt = t * 2.0 - 1.0;
            }
            let dist = (local - tt) * u.out_size.x / cells.x;
            return edge_mix(a, b, dist, 0.75, prm(2), prm3(3));
        }
        case 4: {
            let cells = max(prm2(7), vec2<f32>(1.0));
            let cell = floor(in.uv * cells);
            let r = hash21(cell + prm(9) * 1.37);
            if (r < t) {
                return b;
            }
            return a;
        }
        default: {
            let dir = direction(i32(prm(1) + 0.5));
            let n = normalize(dir);
            let p = px - u.out_size * 0.5;
            let half = 0.5 * (abs(n.x) * u.out_size.x + abs(n.y) * u.out_size.y);
            let f = max(prm(6), 0.75);
            let edge = mix(-half - f, half + f, t);
            // B is revealed behind the moving edge
            let dist = dot(p, n) - edge;
            return edge_mix(a, b, dist, prm(6), prm(2), prm3(3));
        }
    }
}

// Cross zoom: A zooms in and blurs, then B zooms out. prm(0,1) center uv.
@fragment
fn fs_tr_zoom(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let c = prm2(0);
    let zoom = select(1.0 + t * 4.0, 1.0 + (1.0 - t) * 4.0, t > 0.5);
    let strength = sin(t * PI) * 0.1;
    var acc = vec4<f32>(0.0);
    for (var i = 0.0; i < 16.0; i = i + 1.0) {
        let z = zoom * (1.0 + strength * i / 15.0);
        let q = (in.uv - c) / z + c;
        if (t < 0.5) {
            acc = acc + ta(q);
        } else {
            acc = acc + tb(q);
        }
    }
    return acc / 16.0;
}

// Flip over: A turns around a center axis and B is on its back. prm(0) axis (0 horizontal flip
// around the vertical axis, 1 vertical), prm(1..3) fill color
@fragment
fn fs_tr_flip(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let ang = t * PI;
    let s = abs(cos(ang));
    let vertical = prm(0) > 0.5;
    var x = in.uv.x;
    if (vertical) {
        x = in.uv.y;
    }
    let q = (x - 0.5) / max(s, 1e-4) + 0.5;
    let fill = vec4<f32>(prm3(1), 1.0);
    if (q < 0.0 || q > 1.0) {
        return fill;
    }
    var uv = vec2<f32>(q, in.uv.y);
    if (vertical) {
        uv = vec2<f32>(in.uv.x, q);
    }
    var c: vec4<f32>;
    if (t < 0.5) {
        c = ta(uv);
    } else {
        // the back face's coordinates mirror the front's, so B reads correctly at the end
        c = tb(uv);
    }
    let shade = 0.6 + 0.4 * s;
    let lit = vec4<f32>(c.rgb * shade, c.a);
    return lit + fill * (1.0 - c.a);
}

// ------------------------------------------------------------------ motion and light transitions

fn mirror1(x: f32) -> f32 {
    let m = abs(x) % 2.0;
    return select(m, 2.0 - m, m > 1.0);
}

// Samples with mirrored edges, so zoomed-out or rotated images fill the frame.
fn am(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex0, samp, vec2<f32>(mirror1(uv.x), mirror1(uv.y)), 0.0);
}

fn bm(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(tex1, samp, vec2<f32>(mirror1(uv.x), mirror1(uv.y)), 0.0);
}

fn pick(first: bool, uv: vec2<f32>) -> vec4<f32> {
    if (first) {
        return am(uv);
    }
    return bm(uv);
}

fn ease_in3(t: f32) -> f32 {
    return t * t * t;
}

fn ease_out3(t: f32) -> f32 {
    let r = 1.0 - t;
    return 1.0 - r * r * r;
}

fn peak(t: f32) -> f32 {
    return 1.0 - abs(t - 0.5) * 2.0;
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

fn tnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(lhash(i), lhash(i + vec2<f32>(1.0, 0.0)), w.x),
        mix(lhash(i + vec2<f32>(0.0, 1.0)), lhash(i + vec2<f32>(1.0, 1.0)), w.x),
        w.y,
    );
}

fn tfbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + amp * tnoise(q);
        q = q * 2.03 + vec2<f32>(17.1, 9.2);
        amp = amp * 0.5;
    }
    return sum;
}

// Zoom through the cut. prm(0) mode (0 zoom in, 1 zoom out), prm(1) zoom factor,
// prm(2) motion blur 0..1, prm(3..4) center
@fragment
fn fs_tr_zoom_motion(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let z = max(prm(1), 1.01);
    let c = prm2(3);
    let first = t < 0.5;
    var s: f32;
    if (prm(0) < 0.5) {
        if (first) {
            s = mix(1.0, z, ease_in3(t * 2.0));
        } else {
            s = mix(1.0 / z, 1.0, ease_out3(t * 2.0 - 1.0));
        }
    } else {
        if (first) {
            s = mix(1.0, 1.0 / z, ease_in3(t * 2.0));
        } else {
            s = mix(z, 1.0, ease_out3(t * 2.0 - 1.0));
        }
    }
    let k = peak(t);
    let bs = prm(2) * k * k * 0.35;
    var acc = vec4<f32>(0.0);
    let n = 24.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let f = i / (n - 1.0) - 0.5;
        let si = s * (1.0 + f * bs);
        acc = acc + pick(first, c + (in.uv - c) / si);
    }
    return acc / n;
}

// prm(0) rotations, prm(1) direction (0 clockwise), prm(2) motion blur, prm(3) zoom at the cut
@fragment
fn fs_tr_spin(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let first = t < 0.5;
    let total = prm(0) * PI * select(1.0, -1.0, prm(1) > 0.5);
    var ang: f32;
    if (first) {
        ang = total * ease_in3(t * 2.0);
    } else {
        ang = -total * (1.0 - ease_out3(t * 2.0 - 1.0));
    }
    let k = peak(t);
    let zoom = 1.0 + (max(prm(3), 1.0) - 1.0) * k;
    let bs = prm(2) * k * k * 0.6;
    let a = u.out_size.x / max(u.out_size.y, 1.0);
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(a, 1.0) / zoom;
    var acc = vec4<f32>(0.0);
    let n = 24.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let r = ang + (i / (n - 1.0) - 0.5) * bs;
        let cs = cos(r);
        let sn = sin(r);
        let q = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);
        acc = acc + pick(first, vec2<f32>(0.5) + q / vec2<f32>(a, 1.0));
    }
    return acc / n;
}

// prm(0) direction (0 horizontal, 1 vertical), prm(1) stretch factor, prm(2) motion blur
@fragment
fn fs_tr_stretch(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let first = t < 0.5;
    let z = max(prm(1), 1.01);
    var s: f32;
    if (first) {
        s = mix(1.0, z, ease_in3(t * 2.0));
    } else {
        s = mix(z, 1.0, ease_out3(t * 2.0 - 1.0));
    }
    let k = peak(t);
    let bs = prm(2) * k * 0.5;
    let axis = select(vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), prm(0) > 0.5);
    var acc = vec4<f32>(0.0);
    let n = 16.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let si = s * (1.0 + (i / (n - 1.0) - 0.5) * bs);
        let scale = mix(vec2<f32>(1.0), vec2<f32>(si), axis);
        acc = acc + pick(first, vec2<f32>(0.5) + (in.uv - vec2<f32>(0.5)) / scale);
    }
    return acc / n;
}

// Push with easing and motion blur. prm(0) direction, prm(1) motion blur
@fragment
fn fs_tr_smooth_slide(in: VOut) -> @location(0) vec4<f32> {
    let dir = direction(i32(prm(0) + 0.5));
    let t = u.progress;
    let e = select(4.0 * t * t * t, 1.0 - pow(-2.0 * t + 2.0, 3.0) / 2.0, t >= 0.5);
    let speed = select(12.0 * t * t, 12.0 * (1.0 - t) * (1.0 - t), t >= 0.5);
    let blur = prm(1) * speed * 0.03;
    var acc = vec4<f32>(0.0);
    let n = 16.0;
    for (var i = 0.0; i < n; i = i + 1.0) {
        let off = dir * (e + (i / (n - 1.0) - 0.5) * blur);
        acc = acc + ta(in.uv - off) + tb(in.uv - off + dir);
    }
    return acc / n;
}

// B appears first where A is dark. prm(0) softness 0..1, prm(1) invert
@fragment
fn fs_tr_luma_fade(in: VOut) -> @location(0) vec4<f32> {
    let a = ta(in.uv);
    let b = tb(in.uv);
    var v = luma(unpremul(a).rgb);
    if (prm(1) > 0.5) {
        v = 1.0 - v;
    }
    let s = max(prm(0), 0.01);
    let x = u.progress * (1.0 + 2.0 * s) - s;
    return mix(a, b, smoothstep(v - s, v + s, x));
}

// prm(0..2) flash color, prm(3) intensity
@fragment
fn fs_tr_flash(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let base = select(tb(in.uv), ta(in.uv), t < 0.5);
    let w = pow(peak(t), 3.0) * prm(3);
    let lit = vec4<f32>(base.rgb * (1.0 + w * 2.0), base.a);
    return mix(lit, vec4<f32>(prm3(0), 1.0), clamp(w, 0.0, 1.0));
}

// prm(0) mode (0 light leak, 1 film burn), prm(1..3) color, prm(4) intensity
@fragment
fn fs_tr_light(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let base = mix(ta(in.uv), tb(in.uv), smoothstep(0.35, 0.65, t));
    let k = pow(sin(t * PI), 1.5) * prm(4);
    let asp = u.out_size.x / max(u.out_size.y, 1.0);
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(asp, 1.0);
    var light: vec3<f32>;
    if (prm(0) > 0.5) {
        let n = tfbm(p * 3.0 + vec2<f32>(t * 2.0, -t * 1.3));
        let m = smoothstep(1.0 - k * 0.8, 1.0 - k * 0.8 + 0.3, n + (1.0 - length(p)) * 0.3);
        light = mix(prm3(1), vec3<f32>(1.0, 0.95, 0.8), m * m) * m * 1.5;
    } else {
        let n = tfbm(p * 1.4 + vec2<f32>(t * 1.5, t * 0.7));
        light = prm3(1) * smoothstep(0.35, 0.8, n) * k * 1.6
            + vec3<f32>(1.0, 0.9, 0.7) * smoothstep(0.6, 1.0, n) * k;
    }
    let s = unpremul(base);
    let rgb = vec3<f32>(1.0) - (vec3<f32>(1.0) - clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)))
        * (vec3<f32>(1.0) - clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)));
    let alpha = max(s.a, clamp(luma(light), 0.0, 1.0));
    return vec4<f32>(rgb * alpha, alpha);
}

// prm(0) intensity, prm(1) block size px, prm(2) color shift px
@fragment
fn fs_tr_glitch(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let k = pow(peak(t), 2.0) * prm(0);
    let tick = floor(t * 24.0);
    let px = pixel(in.uv);
    let bs = max(prm(1), 4.0);
    let row = floor(px.y / (bs * 0.5));
    var uv = in.uv;
    var first = t < 0.5;
    let r1 = hash21(vec2<f32>(row, tick));
    if (r1 < k * 0.6) {
        uv.x = uv.x + (hash21(vec2<f32>(row * 1.7, tick * 3.1)) - 0.5) * k * 0.25;
        if (hash21(vec2<f32>(row, tick + 7.0)) < 0.5) {
            first = !first;
        }
    }
    let cs = vec2<f32>(prm(2) * k, 0.0) / u.out_size;
    let r = pick(first, uv + cs);
    let g = pick(first, uv);
    let b = pick(first, uv - cs);
    return vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
}

// prm(0) separation px at the peak
@fragment
fn fs_tr_chroma(in: VOut) -> @location(0) vec4<f32> {
    let t = u.progress;
    let d = vec2<f32>(prm(0) * sin(t * PI), 0.0) / u.out_size;
    let m = smoothstep(0.4, 0.6, t);
    let r = mix(am(in.uv + d), bm(in.uv + d), m);
    let g = mix(am(in.uv), bm(in.uv), m);
    let b = mix(am(in.uv - d), bm(in.uv - d), m);
    return vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a)));
}
