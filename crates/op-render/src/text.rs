//! Text layers: system fonts (fontdb), glyph outlines (ab_glyph) and CPU rasterization with
//! stroke and background box. The result is cached and drawn by the GPU like any image;
//! shadows are added on the GPU.

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};
use op_core::Rgba;
use parking_lot::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub text: String,
    pub family: String,
    /// 0 regular, 1 bold, 2 italic, 3 bold italic
    pub style: u32,
    /// Font size in output pixels.
    pub size: f32,
    /// 0 left, 1 center, 2 right
    pub align: u32,
    /// Extra spacing between characters, in thousandths of the font size.
    pub tracking: f32,
    /// Extra spacing between lines, in pixels.
    pub leading: f32,
    pub fill: Rgba,
    pub stroke: Option<(Rgba, f32)>,
    pub background: Option<(Rgba, f32)>,
}

#[derive(Clone, Debug)]
pub struct TextImage {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA8.
    pub rgba: Vec<u8>,
    /// Pixel position of the alignment anchor (left/center/right of the block, vertical center).
    pub anchor: [f32; 2],
}

/// Font lookup with a bundled fallback.
pub struct Fonts {
    db: fontdb::Database,
    loaded: Mutex<HashMap<(String, u32), FontArc>>,
    fallback: FontArc,
}

static FONTS: std::sync::OnceLock<Arc<Fonts>> = std::sync::OnceLock::new();

impl Fonts {
    /// The process-wide font set (system fonts are scanned once, on first use).
    pub fn global() -> Arc<Fonts> {
        FONTS
            .get_or_init(|| {
                let mut db = fontdb::Database::new();
                db.load_system_fonts();
                let fallback = FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT)
                    .expect("bundled font");
                Arc::new(Fonts {
                    db,
                    loaded: Mutex::new(HashMap::new()),
                    fallback,
                })
            })
            .clone()
    }

    /// Family names available for the font menus, sorted.
    pub fn families(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .db
            .faces()
            .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
            .collect();
        v.sort_by_key(|s| s.to_lowercase());
        v.dedup();
        v
    }

    /// The bundled font used for characters the chosen font lacks.
    pub(crate) fn fallback(&self) -> &FontArc {
        &self.fallback
    }

    pub fn default_family() -> &'static str {
        if cfg!(windows) {
            "Arial"
        } else if cfg!(target_os = "macos") {
            "Helvetica"
        } else {
            "DejaVu Sans"
        }
    }

    pub fn font(&self, family: &str, style: u32) -> FontArc {
        let key = (family.to_string(), style);
        if let Some(f) = self.loaded.lock().get(&key) {
            return f.clone();
        }
        let fam = if family.is_empty() {
            Self::default_family()
        } else {
            family
        };
        let families = [fontdb::Family::Name(fam), fontdb::Family::SansSerif];
        let query = fontdb::Query {
            families: &families,
            weight: if style & 1 == 1 {
                fontdb::Weight::BOLD
            } else {
                fontdb::Weight::NORMAL
            },
            stretch: fontdb::Stretch::Normal,
            style: if style & 2 == 2 {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
        };
        let font = self
            .db
            .query(&query)
            .and_then(|id| {
                let mut out = None;
                self.db.with_face_data(id, |data, index| {
                    out = ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index)
                        .ok()
                        .map(FontArc::new);
                });
                out
            })
            .unwrap_or_else(|| self.fallback.clone());
        self.loaded.lock().insert(key, font.clone());
        font
    }

    /// The file contents and face index of an installed family (regular or semibold), only
    /// when that exact family is installed.
    pub fn face_data(&self, family: &str, semibold: bool) -> Option<(Vec<u8>, u32)> {
        let families = [fontdb::Family::Name(family)];
        let id = self.db.query(&fontdb::Query {
            families: &families,
            weight: if semibold {
                fontdb::Weight::SEMIBOLD
            } else {
                fontdb::Weight::NORMAL
            },
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        })?;
        let mut out = None;
        self.db.with_face_data(id, |data, index| {
            out = Some((data.to_vec(), index));
        });
        out
    }
}

/// Exact Euclidean distance transform (Felzenszwalb-Huttenlocher) of a binary mask; returns
/// the distance in pixels from each pixel to the nearest set pixel.
pub(crate) fn distance(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e20;
    fn edt1(f: &[f32], out: &mut [f32]) {
        let n = f.len();
        if n == 0 {
            return;
        }
        let mut v = vec![0usize; n];
        let mut z = vec![0f32; n + 1];
        let mut k = 0usize;
        z[0] = -INF;
        z[1] = INF;
        let parab = |q: usize, p: usize| {
            ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32)
        };
        for q in 1..n {
            let mut s = parab(q, v[k]);
            while s <= z[k] && k > 0 {
                k -= 1;
                s = parab(q, v[k]);
            }
            if s <= z[k] {
                // only possible at k == 0 with equal parabolas: keep the first
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate() {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let d = q as f32 - v[k] as f32;
            *o = d * d + f[v[k]];
        }
    }
    let mut f: Vec<f32> = mask.iter().map(|m| if *m { 0.0 } else { INF }).collect();
    let mut col = vec![0f32; h];
    let mut tmp = vec![0f32; h.max(w)];
    for x in 0..w {
        for y in 0..h {
            col[y] = f[y * w + x];
        }
        edt1(&col, &mut tmp[..h]);
        for y in 0..h {
            f[y * w + x] = tmp[y];
        }
    }
    for y in 0..h {
        let row: Vec<f32> = f[y * w..(y + 1) * w].to_vec();
        edt1(&row, &mut f[y * w..(y + 1) * w]);
    }
    f.iter().map(|v| v.sqrt()).collect()
}

/// Rasterizes a text block.
pub fn rasterize(style: &TextStyle, fonts: &Fonts) -> TextImage {
    let font = fonts.font(&style.family, style.style);
    let size = style.size.clamp(1.0, 2000.0);
    let scaled = font.as_scaled(PxScale::from(size));
    let fallback = fonts.fallback.clone();
    let fb_scaled = fallback.as_scaled(PxScale::from(size));
    let line_h = scaled.ascent() - scaled.descent() + scaled.line_gap() + style.leading;
    let tracking = style.tracking * size / 1000.0;

    // layout: (glyph, use_fallback, x) per line
    let lines: Vec<&str> = style.text.split('\n').collect();
    let mut layout: Vec<Vec<(GlyphId, bool, f32)>> = Vec::new();
    let mut widths = Vec::new();
    for line in &lines {
        let mut x = 0.0;
        let mut prev: Option<(GlyphId, bool)> = None;
        let mut glyphs = Vec::new();
        for ch in line.chars().filter(|c| !c.is_control()) {
            let mut id = font.glyph_id(ch);
            let mut fb = false;
            if id.0 == 0 && ch != ' ' {
                let alt = fallback.glyph_id(ch);
                if alt.0 != 0 {
                    id = alt;
                    fb = true;
                }
            }
            if let Some((p, pfb)) = prev
                && pfb == fb
            {
                x += if fb {
                    fb_scaled.kern(p, id)
                } else {
                    scaled.kern(p, id)
                };
            }
            glyphs.push((id, fb, x));
            x += if fb {
                fb_scaled.h_advance(id)
            } else {
                scaled.h_advance(id)
            } + tracking;
            prev = Some((id, fb));
        }
        widths.push((x - tracking).max(0.0));
        layout.push(glyphs);
    }
    let block_w = widths.iter().copied().fold(0.0f32, f32::max).max(1.0);
    let block_h = (line_h * lines.len() as f32 - style.leading).max(size);
    let stroke_w = style.stroke.map(|s| s.1.max(0.0)).unwrap_or(0.0);
    let bg_pad = style.background.map(|b| b.1.max(0.0)).unwrap_or(0.0);
    let pad = (stroke_w + bg_pad + 2.0).ceil();
    let w = (block_w + pad * 2.0).ceil() as usize;
    let h = (block_h + pad * 2.0).ceil() as usize;
    let w = w.clamp(1, 16384);
    let h = h.clamp(1, 16384);
    let mut cov = vec![0f32; w * h];
    for (li, glyphs) in layout.iter().enumerate() {
        let off = match style.align {
            0 => 0.0,
            2 => block_w - widths[li],
            _ => (block_w - widths[li]) / 2.0,
        };
        let baseline = pad + scaled.ascent() + li as f32 * line_h;
        for (id, fb, x) in glyphs {
            let glyph =
                id.with_scale_and_position(PxScale::from(size), point(pad + off + x, baseline));
            let outline = if *fb {
                fallback.outline_glyph(glyph)
            } else {
                font.outline_glyph(glyph)
            };
            if let Some(g) = outline {
                let b = g.px_bounds();
                g.draw(|gx, gy, c| {
                    let px = b.min.x as i32 + gx as i32;
                    let py = b.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        let i = py as usize * w + px as usize;
                        cov[i] = (cov[i] + c).min(1.0);
                    }
                });
            }
        }
    }
    let stroke_cov: Option<Vec<f32>> = (stroke_w > 0.0).then(|| {
        let mask: Vec<bool> = cov.iter().map(|c| *c >= 0.5).collect();
        let d = distance(&mask, w, h);
        d.iter()
            .zip(&cov)
            .map(|(d, c)| (stroke_w + 0.5 - d).clamp(0.0, 1.0).max(*c))
            .collect()
    });
    let mut rgba = vec![0u8; w * h * 4];
    let over = |dst: &mut [f32; 4], c: Rgba, a: f32| {
        let a = (a * c.a).clamp(0.0, 1.0);
        for (k, v) in [c.r, c.g, c.b].iter().enumerate() {
            dst[k] = v * a + dst[k] * (1.0 - a);
        }
        dst[3] = a + dst[3] * (1.0 - a);
    };
    let (bx0, by0, bx1, by1) = (
        pad - bg_pad,
        pad - bg_pad,
        pad + block_w + bg_pad,
        pad + block_h + bg_pad,
    );
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut px = [0f32; 4];
            if let Some((c, _)) = style.background {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                if fx >= bx0 && fx <= bx1 && fy >= by0 && fy <= by1 {
                    over(&mut px, c, 1.0);
                }
            }
            if let (Some(sc), Some((c, _))) = (&stroke_cov, style.stroke) {
                over(&mut px, c, sc[i]);
            }
            over(&mut px, style.fill, cov[i]);
            for k in 0..4 {
                rgba[i * 4 + k] = (px[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }
    let ax = match style.align {
        0 => pad,
        2 => pad + block_w,
        _ => pad + block_w / 2.0,
    };
    TextImage {
        width: w as u32,
        height: h as u32,
        rgba,
        anchor: [ax, pad + block_h / 2.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(text: &str) -> TextStyle {
        TextStyle {
            text: text.into(),
            family: String::new(),
            style: 0,
            size: 48.0,
            align: 1,
            tracking: 0.0,
            leading: 0.0,
            fill: Rgba::WHITE,
            stroke: None,
            background: None,
        }
    }

    #[test]
    fn text_has_ink_and_grows_with_lines() {
        let fonts = Fonts::global();
        let one = rasterize(&style("Hello"), &fonts);
        let ink: u32 = one.rgba.chunks(4).map(|p| p[3] as u32).sum();
        assert!(ink > 1000, "no ink");
        let two = rasterize(&style("Hello\nWorld"), &fonts);
        assert!(two.height > one.height);
        let mut s = style("Hi");
        s.stroke = Some((Rgba::BLACK, 6.0));
        let stroked = rasterize(&s, &fonts);
        let plain = rasterize(&style("Hi"), &fonts);
        let count = |img: &TextImage| img.rgba.chunks(4).filter(|p| p[3] > 128).count();
        assert!(count(&stroked) > count(&plain));
    }

    #[test]
    fn distance_transform_is_euclidean() {
        let (w, h) = (9, 9);
        let mut mask = vec![false; w * h];
        mask[4 * w + 4] = true;
        let d = distance(&mask, w, h);
        assert_eq!(d[4 * w + 4], 0.0);
        assert!((d[4 * w + 7] - 3.0).abs() < 1e-5);
        assert!((d[7 * w + 7] - (18f32).sqrt()).abs() < 1e-4);
    }
}
