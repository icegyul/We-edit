// Video scopes on the GPU: compute passes accumulate counts, a fragment pass draws them.
// The analyzed image is the sequence working encoding (Rec.709 display-referred).

struct ScopeU {
    // image size, grid size
    size: vec2<u32>,
    grid: vec2<u32>,
    // 0 luma waveform, 1 RGB parade, 2 vectorscope, 3 histogram
    kind: u32,
    gain: f32,
    pad: vec2<u32>,
};

@group(0) @binding(0) var<uniform> su: ScopeU;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;

fn col709(c: vec3<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec3<f32>(y, (c.b - y) / 1.8556, (c.r - y) / 1.5748);
}

fn bump(x: u32, y: u32) {
    if (x < su.grid.x && y < su.grid.y) {
        atomicAdd(&counts[y * su.grid.x + x], 1u);
    }
}

fn row_of(v: f32) -> u32 {
    // -0.1 .. 1.1 mapped onto the grid height (superwhite and superblack stay visible)
    let t = clamp((1.1 - v) / 1.2, 0.0, 1.0);
    return u32(t * f32(su.grid.y - 1u));
}

@compute @workgroup_size(16, 16)
fn cs_accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= su.size.x || id.y >= su.size.y) {
        return;
    }
    // premultiplied color is the frame as seen over black
    let c = textureLoad(image, vec2<i32>(id.xy), 0).rgb;
    switch su.kind {
        case 0u: {
            let x = id.x * su.grid.x / su.size.x;
            bump(x, row_of(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722))));
        }
        case 1u: {
            let third = su.grid.x / 3u;
            let x = id.x * third / su.size.x;
            bump(x, row_of(c.r));
            bump(third + x, row_of(c.g));
            bump(2u * third + x, row_of(c.b));
        }
        case 2u: {
            let v = col709(c);
            let half = f32(su.grid.x) * 0.5;
            let px = u32(clamp(half + v.y * half * 1.8, 0.0, f32(su.grid.x - 1u)));
            let py = u32(clamp(half - v.z * half * 1.8, 0.0, f32(su.grid.y - 1u)));
            bump(px, py);
        }
        default: {
            // histogram: 256 columns per channel, rows 0..3 = R, G, B, luma
            let q = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
            let y = dot(q, vec3<f32>(0.2126, 0.7152, 0.0722));
            bump(u32(q.r * 255.0), 0u);
            bump(u32(q.g * 255.0), 1u);
            bump(u32(q.b * 255.0), 2u);
            bump(u32(y * 255.0), 3u);
        }
    }
}

struct DrawOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_scope(@builtin(vertex_index) i: u32) -> DrawOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: DrawOut;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@group(0) @binding(3) var<storage, read> counts_r: array<u32>;

fn count_at(x: u32, y: u32) -> f32 {
    return f32(counts_r[min(y, su.grid.y - 1u) * su.grid.x + min(x, su.grid.x - 1u)]);
}

@fragment
fn fs_scope(in: DrawOut) -> @location(0) vec4<f32> {
    let gx = u32(in.uv.x * f32(su.grid.x));
    let background = vec4<f32>(0.06, 0.06, 0.07, 1.0);
    if (su.kind == 3u) {
        // histogram: four stacked bands
        let band = u32(in.uv.y * 4.0);
        let local = 1.0 - fract(in.uv.y * 4.0);
        var peak = 1.0;
        for (var i = 0u; i < 256u; i = i + 1u) {
            peak = max(peak, count_at(i, band));
        }
        let h = count_at(u32(in.uv.x * 255.0), band) / peak;
        var colors = array<vec3<f32>, 4>(vec3<f32>(0.9, 0.2, 0.2), vec3<f32>(0.2, 0.85, 0.3), vec3<f32>(0.25, 0.45, 1.0), vec3<f32>(0.85, 0.85, 0.85));
        if (local < h) {
            return vec4<f32>(colors[band] * 0.85, 1.0);
        }
        return background;
    }
    let gy = u32(in.uv.y * f32(su.grid.y));
    let n = count_at(gx, gy);
    let k = clamp(log(1.0 + n * su.gain) / log(1.0 + 64.0), 0.0, 1.0);
    var tint = vec3<f32>(0.35, 1.0, 0.45);
    if (su.kind == 1u) {
        let third = f32(su.grid.x) / 3.0;
        let part = u32(f32(gx) / third);
        var colors = array<vec3<f32>, 3>(vec3<f32>(1.0, 0.3, 0.3), vec3<f32>(0.3, 1.0, 0.4), vec3<f32>(0.35, 0.55, 1.0));
        tint = colors[min(part, 2u)];
    }
    if (su.kind == 2u) {
        tint = vec3<f32>(0.85, 0.95, 0.85);
    }
    var o = background.rgb + tint * k;
    // graticule
    if (su.kind != 2u) {
        let v = 1.1 - in.uv.y * 1.2;
        let lines = abs(fract(v * 10.0 + 0.5) - 0.5) / 10.0;
        if (lines < 0.0025) {
            o = o + vec3<f32>(0.12);
        }
    } else {
        let d = length(in.uv - 0.5) * 2.0;
        if (abs(d - 0.9) < 0.004 || abs(in.uv.x - 0.5) < 0.0015 || abs(in.uv.y - 0.5) < 0.0015) {
            o = o + vec3<f32>(0.12);
        }
    }
    return vec4<f32>(o, 1.0);
}
