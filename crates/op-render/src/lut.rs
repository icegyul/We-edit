//! Color lookup tables: `.cube` 3D LUT files and tone curves.

use std::path::Path;

/// A parsed 3D LUT: `size`^3 RGB entries, red varying fastest.
#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
}

/// Parses the Adobe/Resolve `.cube` text format (3D tables only).
pub fn parse_cube(text: &str) -> Result<Cube, String> {
    let mut size = 0usize;
    let mut data = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut parts = l.split_whitespace();
        let first = parts.next().unwrap_or("");
        match first {
            "LUT_3D_SIZE" => {
                size = parts
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("bad LUT_3D_SIZE")?;
                if !(2..=256).contains(&size) {
                    return Err(format!("unsupported LUT size {size}"));
                }
            }
            "LUT_1D_SIZE" => return Err("1D LUTs are not supported".into()),
            // lookups are done over 0..1; a custom input domain is not remapped
            "TITLE" | "DOMAIN_MIN" | "DOMAIN_MAX" | "LUT_3D_INPUT_RANGE" => {}
            _ => {
                let r: f32 = first.parse().map_err(|_| format!("unexpected line: {l}"))?;
                let g: f32 = parts
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("short data line")?;
                let b: f32 = parts
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("short data line")?;
                data.push([r, g, b]);
            }
        }
    }
    if size == 0 {
        return Err("missing LUT_3D_SIZE".into());
    }
    if data.len() != size * size * size {
        return Err(format!(
            "expected {} entries, found {}",
            size * size * size,
            data.len()
        ));
    }
    Ok(Cube { size, data })
}

pub fn load_cube(path: &Path) -> Result<Cube, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_cube(&text)
}

/// Uploads a cube as an Rgba16Float 3D texture.
pub fn upload_cube(device: &wgpu::Device, queue: &wgpu::Queue, cube: &Cube) -> wgpu::TextureView {
    let n = cube.size as u32;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lut"),
        size: wgpu::Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: n,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = Vec::with_capacity(cube.data.len() * 8);
    for c in &cube.data {
        for v in [c[0], c[1], c[2], 1.0] {
            bytes.extend_from_slice(&f16_bits(v).to_le_bytes());
        }
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(n * 8),
            rows_per_image: Some(n),
        },
        wgpu::Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: n,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// IEEE half-precision bits of an f32 (round to nearest even).
pub fn f16_bits(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        let round = (m >> 12) & 1;
        return sign | ((m >> 13) + round) as u16;
    }
    let m = mant >> 13;
    let round = (mant >> 12) & 1;
    (sign | ((e as u16) << 10) | m as u16).wrapping_add(round as u16)
}

/// Samples a curve given by control points (x sorted, 0..1) at 256 positions with a monotone
/// cubic (Fritsch-Carlson) so the curve never overshoots between points.
pub fn curve_table(points: &[[f32; 2]]) -> [f32; 256] {
    let mut pts: Vec<[f32; 2]> = points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6);
    let mut out = [0f32; 256];
    if pts.len() < 2 {
        for (i, o) in out.iter_mut().enumerate() {
            *o = i as f32 / 255.0;
        }
        return out;
    }
    let n = pts.len();
    let mut delta = vec![0f32; n - 1];
    for i in 0..n - 1 {
        delta[i] = (pts[i + 1][1] - pts[i][1]) / (pts[i + 1][0] - pts[i][0]).max(1e-6);
    }
    let mut m = vec![0f32; n];
    m[0] = delta[0];
    m[n - 1] = delta[n - 2];
    for i in 1..n - 1 {
        m[i] = if delta[i - 1] * delta[i] <= 0.0 {
            0.0
        } else {
            (delta[i - 1] + delta[i]) / 2.0
        };
    }
    for i in 0..n - 1 {
        if delta[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let a = m[i] / delta[i];
        let b = m[i + 1] / delta[i];
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[i] = t * a * delta[i];
            m[i + 1] = t * b * delta[i];
        }
    }
    for (k, o) in out.iter_mut().enumerate() {
        let x = k as f32 / 255.0;
        let y = if x <= pts[0][0] {
            pts[0][1]
        } else if x >= pts[n - 1][0] {
            pts[n - 1][1]
        } else {
            let i = pts.partition_point(|p| p[0] <= x) - 1;
            let h = pts[i + 1][0] - pts[i][0];
            let t = (x - pts[i][0]) / h;
            let (t2, t3) = (t * t, t * t * t);
            (2.0 * t3 - 3.0 * t2 + 1.0) * pts[i][1]
                + (t3 - 2.0 * t2 + t) * h * m[i]
                + (-2.0 * t3 + 3.0 * t2) * pts[i + 1][1]
                + (t3 - t2) * h * m[i + 1]
        };
        *o = y;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_parses_identity() {
        let mut text = String::from("TITLE \"x\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text.push_str(&format!("{r} {g} {b}\n"));
                }
            }
        }
        let c = parse_cube(&text).unwrap();
        assert_eq!(c.size, 2);
        assert_eq!(c.data[1], [1.0, 0.0, 0.0]);
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    }

    #[test]
    fn curves_are_monotone_and_hit_points() {
        let t = curve_table(&[[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]);
        assert!((t[0] - 0.0).abs() < 1e-6 && (t[255] - 1.0).abs() < 1e-6);
        assert!((t[128] - 0.7).abs() < 0.01);
        assert!(t.windows(2).all(|w| w[1] >= w[0] - 1e-6));
        let id = curve_table(&[[0.0, 0.0], [1.0, 1.0]]);
        assert!((id[100] - 100.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(0.0), 0);
    }
}
