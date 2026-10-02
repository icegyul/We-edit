//! Animated captions: words laid out in lines and drawn one by one, so each word can have its
//! own color, size, position, visibility, highlight box or underline as it is spoken. Drawn on
//! the CPU like Text (glyph outlines, distance-transform stroke and glow) and placed by the GPU.

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};
use op_core::Rgba;
use op_core::captions::TimedWord;

use crate::text::{Fonts, TextImage, distance};

pub const CLASSIC: u32 = 0;
pub const BOXED: u32 = 1;
pub const KARAOKE: u32 = 2;
pub const HIGHLIGHT: u32 = 3;
pub const POP: u32 = 4;
pub const ONE_WORD: u32 = 5;
pub const TYPEWRITER: u32 = 6;
pub const BOUNCE: u32 = 7;
pub const NEON: u32 = 8;
pub const CREATOR: u32 = 9;
pub const FADE: u32 = 10;
pub const UNDERLINE: u32 = 11;

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionStyle {
    pub style: u32,
    /// Animation strength, 1.0 = normal.
    pub strength: f32,
    pub family: String,
    pub font_style: u32,
    /// Font size in output pixels.
    pub size: f32,
    pub uppercase: bool,
    /// Line width limit in output pixels.
    pub max_width: f32,
    pub fill: Rgba,
    pub highlight: Rgba,
    pub stroke: Option<(Rgba, f32)>,
    /// Box color for the boxed style, alpha included.
    pub background: Rgba,
}

impl CaptionStyle {
    /// Whether the picture changes while a caption is on screen.
    pub fn animated(&self) -> bool {
        !matches!(self.style, CLASSIC | BOXED)
    }
}

/// How one word looks at the current moment.
#[derive(Clone, Debug)]
struct Look {
    alpha: f32,
    scale: f32,
    /// Vertical offset in pixels (negative is up).
    lift: f32,
    color: Rgba,
    /// Karaoke: the highlight color covers this fraction of the word from the left.
    sweep: Option<f32>,
    pill: bool,
    underline: Option<f32>,
    glow: f32,
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn ease_out_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// The word being spoken: the last one that has started (it stays lit until the next one).
pub fn current(words: &[TimedWord], t: f64) -> Option<usize> {
    words.iter().rposition(|w| w.start <= t)
}

fn looks(style: &CaptionStyle, words: &[TimedWord], t: f64) -> Vec<Look> {
    let cur = current(words, t);
    let k = style.strength.max(0.0);
    let plain = Look {
        alpha: 1.0,
        scale: 1.0,
        lift: 0.0,
        color: style.fill,
        sweep: None,
        pill: false,
        underline: None,
        glow: 0.0,
    };
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let mut l = plain.clone();
            let age = (t - w.start) as f32;
            let len = ((w.end - w.start) as f32).max(0.05);
            let progress = (age / len).clamp(0.0, 1.0);
            let is_cur = cur == Some(i);
            let spoken = cur.is_some_and(|c| i < c);
            let future = cur.is_none_or(|c| i > c);
            match style.style {
                KARAOKE => {
                    if spoken {
                        l.color = style.highlight;
                    } else if is_cur {
                        l.sweep = Some(progress);
                    }
                }
                HIGHLIGHT => {
                    if is_cur {
                        l.pill = true;
                        // dark text on a light box, light text on a dark one
                        let lum = 0.2126 * style.highlight.r
                            + 0.7152 * style.highlight.g
                            + 0.0722 * style.highlight.b;
                        l.color = if lum > 0.6 {
                            Rgba::new(0.05, 0.05, 0.05, 1.0)
                        } else {
                            style.fill
                        };
                        l.scale = 1.0 + 0.04 * k;
                    }
                }
                POP | CREATOR => {
                    if is_cur {
                        let x = (age / 0.15).clamp(0.0, 1.0);
                        let settle = if style.style == CREATOR { 0.12 } else { 0.1 };
                        l.scale = 1.0 + k * (settle + 0.16 * (1.0 - x));
                        l.color = style.highlight;
                    }
                }
                ONE_WORD => {
                    if is_cur {
                        let x = (age / 0.22).clamp(0.0, 1.0);
                        l.scale = 1.0 + k * 0.4 * (ease_out_back(x) - 1.0);
                        l.alpha = (age / 0.06).clamp(0.0, 1.0);
                    } else {
                        l.alpha = 0.0;
                    }
                }
                TYPEWRITER => {
                    if future {
                        l.alpha = 0.0;
                    }
                }
                BOUNCE => {
                    if future {
                        l.alpha = 0.0;
                    } else if is_cur {
                        let x = (age / 0.28).clamp(0.0, 1.0);
                        l.alpha = (age / 0.1).clamp(0.0, 1.0);
                        l.lift = style.size * 0.45 * k * (1.0 - ease_out_back(x));
                        l.scale = 0.85 + 0.15 * ease_out_back(x);
                    }
                }
                NEON => {
                    // tubes in the highlight color; the spoken word burns white hot
                    l.color = mix(style.highlight, Rgba::WHITE, 0.25);
                    l.glow = 0.55;
                    if is_cur {
                        l.color = mix(style.highlight, Rgba::WHITE, 0.8);
                        l.glow = 1.0;
                        l.scale = 1.0 + 0.05 * k;
                    }
                }
                FADE => {
                    if future {
                        l.alpha = 0.3;
                    } else if is_cur {
                        l.alpha = 0.3 + 0.7 * (age / 0.25).clamp(0.0, 1.0);
                    }
                }
                UNDERLINE if is_cur => l.underline = Some(progress.max(0.05)),
                _ => {}
            }
            l.alpha = l.alpha.clamp(0.0, 1.0);
            l.scale = l.scale.max(0.05);
            l
        })
        .collect()
}

/// Glyphs of one word: (glyph, from the fallback font, x) and the advance width.
fn shape(
    font: &FontArc,
    fallback: &FontArc,
    text: &str,
    size: f32,
) -> (Vec<(GlyphId, bool, f32)>, f32) {
    let sc = font.as_scaled(PxScale::from(size));
    let fsc = fallback.as_scaled(PxScale::from(size));
    let mut x = 0.0;
    let mut prev: Option<(GlyphId, bool)> = None;
    let mut out = Vec::new();
    for ch in text.chars().filter(|c| !c.is_control()) {
        let mut id = font.glyph_id(ch);
        let mut fb = false;
        if id.0 == 0 {
            let alt = fallback.glyph_id(ch);
            if alt.0 != 0 {
                id = alt;
                fb = true;
            }
        }
        if let Some((p, pfb)) = prev
            && pfb == fb
        {
            x += if fb { fsc.kern(p, id) } else { sc.kern(p, id) };
        }
        out.push((id, fb, x));
        x += if fb {
            fsc.h_advance(id)
        } else {
            sc.h_advance(id)
        };
        prev = Some((id, fb));
    }
    (out, x)
}

/// Premultiplied float canvas.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn over(&mut self, x: usize, y: usize, c: Rgba, a: f32) {
        let a = (a * c.a).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[y * self.w + x];
        p[0] = c.r * a + p[0] * (1.0 - a);
        p[1] = c.g * a + p[1] * (1.0 - a);
        p[2] = c.b * a + p[2] * (1.0 - a);
        p[3] = a + p[3] * (1.0 - a);
    }

    /// A filled rounded rectangle with an anti-aliased edge.
    fn rounded(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: Rgba, alpha: f32) {
        let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (hx, hy) = ((x1 - x0) / 2.0 - r, (y1 - y0) / 2.0 - r);
        let ys = (y0.floor().max(0.0) as usize)..((y1.ceil() as usize).min(self.h));
        let xs = (x0.floor().max(0.0) as usize)..((x1.ceil() as usize).min(self.w));
        for y in ys {
            for x in xs.clone() {
                let dx = ((x as f32 + 0.5 - cx).abs() - hx).max(0.0);
                let dy = ((y as f32 + 0.5 - cy).abs() - hy).max(0.0);
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.over(x, y, c, cov * alpha);
            }
        }
    }
}

struct Placed {
    /// Left edge, center y of the word's text box at scale 1.
    x: f32,
    cy: f32,
    width: f32,
}

pub fn rasterize(style: &CaptionStyle, words: &[TimedWord], t: f64, fonts: &Fonts) -> TextImage {
    let font = fonts.font(&style.family, style.font_style);
    let fallback = fonts.fallback().clone();
    // a single word on screen is shown big
    let big = if style.style == ONE_WORD { 1.7 } else { 1.0 };
    let size = (style.size * big).clamp(4.0, 1000.0);
    let sc = font.as_scaled(PxScale::from(size));
    let (ascent, descent) = (sc.ascent(), sc.descent());
    let text_h = ascent - descent;
    let line_h = text_h * 1.18;
    let space = sc.h_advance(font.glyph_id(' ')).max(size * 0.25);
    let upper = style.uppercase || style.style == CREATOR;
    let texts: Vec<String> = words
        .iter()
        .map(|w| {
            if upper {
                w.text.to_uppercase()
            } else {
                w.text.clone()
            }
        })
        .collect();
    let looks = looks(style, words, t);
    // one word at a time shows only the current word, alone on its line
    let shown: Vec<usize> = if style.style == ONE_WORD {
        current(words, t).into_iter().collect()
    } else {
        (0..words.len()).collect()
    };
    let widths: Vec<f32> = texts
        .iter()
        .map(|s| shape(&font, &fallback, s, size).1)
        .collect();
    // greedy line breaking within the width limit
    let max_w = style.max_width.max(size * 2.0);
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut cur_line: Vec<usize> = Vec::new();
    let mut cur_w = 0.0;
    for &i in &shown {
        let add = if cur_line.is_empty() {
            widths[i]
        } else {
            space + widths[i]
        };
        if !cur_line.is_empty() && cur_w + add > max_w {
            lines.push(std::mem::take(&mut cur_line));
            cur_w = 0.0;
        }
        cur_w += if cur_line.is_empty() {
            widths[i]
        } else {
            space + widths[i]
        };
        cur_line.push(i);
    }
    if !cur_line.is_empty() {
        lines.push(cur_line);
    }
    let line_widths: Vec<f32> = lines
        .iter()
        .map(|l| l.iter().map(|&i| widths[i]).sum::<f32>() + space * (l.len().max(1) - 1) as f32)
        .collect();
    let block_w = line_widths.iter().copied().fold(1.0f32, f32::max);
    let block_h = (line_h * lines.len().max(1) as f32).max(text_h);
    // neon light has no outline: the glow is its edge
    let stroke_w = match style.stroke {
        Some(_) if style.style == NEON => 0.0,
        Some((_, w)) => w.max(0.0) * if style.style == CREATOR { 1.6 } else { 1.0 },
        None => 0.0,
    };
    let glow_r = if style.style == NEON {
        size * 0.35
    } else {
        0.0
    };
    // room for strokes, glows, boxes and words that grow while they pop
    let pad = (stroke_w + glow_r + size * 0.6 + 4.0).ceil();
    let w = ((block_w + pad * 2.0).ceil() as usize).clamp(1, 16384);
    let h = ((block_h + pad * 2.0).ceil() as usize).clamp(1, 16384);
    let mut canvas = Canvas {
        w,
        h,
        px: vec![[0.0; 4]; w * h],
    };
    let mut placed: Vec<Option<Placed>> = (0..words.len()).map(|_| None).collect();
    for (li, line) in lines.iter().enumerate() {
        let mut x = pad + (block_w - line_widths[li]) / 2.0;
        let cy = pad + line_h * li as f32 + line_h / 2.0;
        for &i in line {
            placed[i] = Some(Placed {
                x,
                cy,
                width: widths[i],
            });
            x += widths[i] + space;
        }
    }
    // boxes behind the lines
    if style.style == BOXED {
        let bp = size * 0.28;
        for (li, line) in lines.iter().enumerate() {
            let Some(first) = line.first().and_then(|i| placed[*i].as_ref()) else {
                continue;
            };
            let x0 = first.x;
            let x1 = x0 + line_widths[li];
            let cy = first.cy;
            canvas.rounded(
                x0 - bp,
                cy - text_h / 2.0 - bp * 0.6,
                x1 + bp,
                cy + text_h / 2.0 + bp * 0.6,
                size * 0.2,
                style.background,
                1.0,
            );
        }
    }
    for &i in &shown {
        let (Some(p), l) = (placed[i].as_ref(), &looks[i]) else {
            continue;
        };
        if l.alpha <= 0.0 {
            continue;
        }
        let s = l.scale;
        let wsize = size * s;
        let cx = p.x + p.width / 2.0;
        let cy = p.cy + l.lift;
        let ww = p.width * s;
        if l.pill {
            let bp = size * 0.2;
            canvas.rounded(
                cx - ww / 2.0 - bp,
                cy - text_h * s / 2.0 - bp * 0.35,
                cx + ww / 2.0 + bp,
                cy + text_h * s / 2.0 + bp * 0.35,
                size * 0.22,
                style.highlight,
                l.alpha,
            );
        }
        draw_word(
            &mut canvas,
            &font,
            &fallback,
            &texts[i],
            wsize,
            cx - ww / 2.0,
            cy,
            style,
            l,
            stroke_w * s,
            glow_r * s,
        );
        if let Some(f) = l.underline {
            let y = cy + text_h * s / 2.0 + size * 0.04;
            let th = (size * 0.09).max(2.0);
            canvas.rounded(
                cx - ww / 2.0,
                y,
                cx - ww / 2.0 + ww * f,
                y + th,
                th / 2.0,
                style.highlight,
                l.alpha,
            );
        }
    }
    let mut rgba = vec![0u8; w * h * 4];
    for (i, p) in canvas.px.iter().enumerate() {
        for k in 0..4 {
            rgba[i * 4 + k] = (p[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    TextImage {
        width: w as u32,
        height: h as u32,
        rgba,
        anchor: [w as f32 / 2.0, h as f32 / 2.0],
    }
}

/// One word at `size`, its text box starting at `x0` and centered on `cy`: glow, stroke, then
/// the fill (split for the karaoke sweep).
#[allow(clippy::too_many_arguments)]
fn draw_word(
    canvas: &mut Canvas,
    font: &FontArc,
    fallback: &FontArc,
    text: &str,
    size: f32,
    x0: f32,
    cy: f32,
    style: &CaptionStyle,
    look: &Look,
    stroke_w: f32,
    glow_r: f32,
) {
    let sc = font.as_scaled(PxScale::from(size));
    let baseline = cy + (sc.ascent() + sc.descent()) / 2.0;
    let (glyphs, width) = shape(font, fallback, text, size);
    let margin = (stroke_w + glow_r + 2.0).ceil();
    let lx = (x0 - margin).floor();
    let ly = (baseline - sc.ascent() - margin).floor();
    let lw = (width + margin * 2.0 + 2.0).ceil().max(1.0) as usize;
    let lh = (sc.ascent() - sc.descent() + margin * 2.0 + 2.0)
        .ceil()
        .max(1.0) as usize;
    let mut cov = vec![0f32; lw * lh];
    for (id, fb, gx) in &glyphs {
        let g = id.with_scale_and_position(PxScale::from(size), point(x0 + gx, baseline));
        let outline = if *fb {
            fallback.outline_glyph(g)
        } else {
            font.outline_glyph(g)
        };
        if let Some(o) = outline {
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let px = (b.min.x + gx as f32 - lx) as i32;
                let py = (b.min.y + gy as f32 - ly) as i32;
                if px >= 0 && py >= 0 && (px as usize) < lw && (py as usize) < lh {
                    let i = py as usize * lw + px as usize;
                    cov[i] = (cov[i] + c).min(1.0);
                }
            });
        }
    }
    let dist = (stroke_w > 0.0 || glow_r > 0.0).then(|| {
        let mask: Vec<bool> = cov.iter().map(|c| *c >= 0.5).collect();
        distance(&mask, lw, lh)
    });
    let split = look.sweep.map(|f| x0 + width * f);
    for y in 0..lh {
        let cyp = ly as i64 + y as i64;
        if cyp < 0 || cyp as usize >= canvas.h {
            continue;
        }
        for x in 0..lw {
            let cxp = lx as i64 + x as i64;
            if cxp < 0 || cxp as usize >= canvas.w {
                continue;
            }
            let i = y * lw + x;
            let (px, py) = (cxp as usize, cyp as usize);
            if let Some(d) = &dist {
                if glow_r > 0.0 && look.glow > 0.0 {
                    let g = (-d[i] / (glow_r * 0.45)).exp() * look.glow;
                    canvas.over(px, py, style.highlight, g * look.alpha);
                }
                if let Some((c, _)) = style.stroke
                    && stroke_w > 0.0
                {
                    let a = (stroke_w + 0.5 - d[i]).clamp(0.0, 1.0).max(cov[i]);
                    canvas.over(px, py, c, a * look.alpha);
                }
            }
            if cov[i] > 0.0 {
                let color = match split {
                    Some(sx) if (cxp as f32 + 0.5) < sx => style.highlight,
                    _ => look.color,
                };
                canvas.over(px, py, color, cov[i] * look.alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<TimedWord> {
        ["Hola", "a", "todos", "hoy"]
            .iter()
            .enumerate()
            .map(|(i, w)| TimedWord {
                text: w.to_string(),
                start: i as f64 * 0.5,
                end: i as f64 * 0.5 + 0.45,
            })
            .collect()
    }

    fn style(style: u32) -> CaptionStyle {
        CaptionStyle {
            style,
            strength: 1.0,
            family: String::new(),
            font_style: 1,
            size: 48.0,
            uppercase: false,
            max_width: 800.0,
            fill: Rgba::WHITE,
            highlight: Rgba::new(1.0, 0.84, 0.04, 1.0),
            stroke: Some((Rgba::BLACK, 4.0)),
            background: Rgba::new(0.0, 0.0, 0.0, 0.75),
        }
    }

    /// Pixels close to the highlight color (yellow).
    fn yellow(img: &TextImage) -> usize {
        img.rgba
            .chunks(4)
            .filter(|p| p[3] > 200 && p[0] > 200 && p[1] > 160 && p[2] < 80)
            .count()
    }

    fn ink(img: &TextImage) -> usize {
        img.rgba.chunks(4).filter(|p| p[3] > 128).count()
    }

    #[test]
    fn current_word_follows_time() {
        let w = words();
        assert_eq!(current(&w, -0.1), None);
        assert_eq!(current(&w, 0.0), Some(0));
        assert_eq!(current(&w, 0.7), Some(1));
        assert_eq!(current(&w, 9.0), Some(3));
    }

    #[test]
    fn every_style_draws_something() {
        let fonts = Fonts::global();
        for s in 0..12 {
            let img = rasterize(&style(s), &words(), 0.6, &fonts);
            assert!(ink(&img) > 200, "style {s}");
        }
    }

    #[test]
    fn karaoke_colors_spoken_words_progressively() {
        let fonts = Fonts::global();
        let early = yellow(&rasterize(&style(KARAOKE), &words(), 0.05, &fonts));
        let late = yellow(&rasterize(&style(KARAOKE), &words(), 1.9, &fonts));
        assert!(late > early * 3, "{early} -> {late}");
        // classic never highlights
        assert_eq!(
            yellow(&rasterize(&style(CLASSIC), &words(), 1.9, &fonts)),
            0
        );
    }

    #[test]
    fn reveal_styles_grow_with_time() {
        let fonts = Fonts::global();
        for s in [TYPEWRITER, BOUNCE] {
            let a = ink(&rasterize(&style(s), &words(), 0.3, &fonts));
            let b = ink(&rasterize(&style(s), &words(), 1.9, &fonts));
            assert!(b > a, "style {s}: {a} -> {b}");
        }
        // one word at a time: the image holds about one word
        let one = rasterize(&style(ONE_WORD), &words(), 1.2, &fonts);
        let all = rasterize(&style(CLASSIC), &words(), 1.2, &fonts);
        assert!(one.width < all.width);
    }

    #[test]
    fn highlight_box_and_boxed_draw_boxes() {
        let fonts = Fonts::global();
        let hb = rasterize(&style(HIGHLIGHT), &words(), 0.6, &fonts);
        assert!(
            yellow(&hb) > 500,
            "the current word sits on a highlight box"
        );
        let boxed = rasterize(&style(BOXED), &words(), 0.6, &fonts);
        let dark = boxed
            .rgba
            .chunks(4)
            .filter(|p| p[3] > 150 && p[0] < 40)
            .count();
        assert!(dark > ink(&rasterize(&style(CLASSIC), &words(), 0.6, &fonts)) / 2);
    }
}
