//! Motion templates: ready-made animated graphics (pop-up title, name tag, highlight box,
//! subscribe pop-up, flash ...), like the templates of Apple Motion or Premiere's Motion Graphics.
//!
//! A template is a few graphics clips (text and shapes) with keyframes on their Motion and Opacity
//! components. They land together on free video tracks at the playhead as one group, and every
//! value stays editable in Effect Controls afterwards.

use std::sync::Arc;

use op_core::catalog::{self, EffectKind};
use op_core::*;

use crate::editor::Editor;

/// What the user fills in.
#[derive(Clone, Debug)]
pub struct TemplateOptions {
    /// Main text (title, name, label).
    pub text: String,
    /// Second line, for templates that have one.
    pub sub: String,
    /// Accent color (sRGB 0..1).
    pub accent: [f32; 3],
    /// Length in seconds.
    pub seconds: f64,
}

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    /// Text lines it uses: 0 none, 1 title, 2 title and subtitle.
    pub lines: u8,
    pub default_text: &'static str,
    pub default_sub: &'static str,
    pub accent: [f32; 3],
    pub seconds: f64,
}

pub const TEMPLATES: &[Template] = &[
    Template { id: "pop_title", name: "팝업 제목", note: "제목이 통통 튀며 나타났다 사라짐", lines: 1, default_text: "오늘의 주제", default_sub: "", accent: [1.0, 0.85, 0.1], seconds: 3.0 },
    Template { id: "lower_third", name: "하단 이름표", note: "왼쪽 아래에서 미끄러져 들어오는 이름·직함 띠", lines: 2, default_text: "홍길동", default_sub: "We edit 대표", accent: [0.1, 0.45, 0.95], seconds: 5.0 },
    Template { id: "highlight_box", name: "강조 박스", note: "화면 한가운데를 네모로 짚어 주는 박스와 이름표", lines: 1, default_text: "여기!", default_sub: "", accent: [1.0, 0.2, 0.2], seconds: 3.0 },
    Template { id: "subscribe", name: "구독·좋아요 알림", note: "아래에서 톡 올라오는 빨간 알림 버튼", lines: 1, default_text: "구독 · 좋아요", default_sub: "", accent: [1.0, 0.1, 0.1], seconds: 4.0 },
    Template { id: "flash", name: "화면 번쩍", note: "0.4초 흰색 번쩍임 (장면 전환·충격)", lines: 0, default_text: "", default_sub: "", accent: [1.0, 1.0, 1.0], seconds: 0.4 },
    Template { id: "shake_text", name: "충격 흔들림 글자", note: "글자가 쾅 하고 커졌다가 부르르 떨림 (예능)", lines: 1, default_text: "헉!", default_sub: "", accent: [1.0, 0.9, 0.0], seconds: 2.0 },
    Template { id: "slide_title", name: "슬라이드 제목", note: "왼쪽에서 들어와 밑줄이 그어지는 제목", lines: 1, default_text: "챕터 1", default_sub: "", accent: [1.0, 1.0, 1.0], seconds: 4.0 },
    Template { id: "marker_text", name: "형광펜 글자", note: "글자 뒤로 형광펜이 쭉 칠해짐 (강의·설명)", lines: 1, default_text: "중요한 부분", default_sub: "", accent: [1.0, 0.92, 0.2], seconds: 4.0 },
    Template { id: "bubble", name: "말풍선 팝", note: "동그란 말풍선이 흔들리며 튀어나옴 (리액션)", lines: 1, default_text: "정말?!", default_sub: "", accent: [1.0, 1.0, 1.0], seconds: 2.5 },
    Template { id: "title_band", name: "제목 띠", note: "화면을 가로지르는 색 띠 위에 제목 (챕터 구분)", lines: 2, default_text: "제1장", default_sub: "시작하기", accent: [0.9, 0.25, 0.45], seconds: 3.5 },
];

pub fn find(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

// ------------------------------------------------------------------------------ the layers

enum Body {
    Text(Vec<(&'static str, Value)>),
    Shape(Vec<(&'static str, Value)>),
}

/// One animated value of a layer: `keys` are (seconds from the layer's start, value).
struct Anim {
    effect: &'static str,
    key: &'static str,
    keys: Vec<(f64, Value)>,
}

struct Layer {
    name: String,
    body: Body,
    /// Seconds after the template's start.
    delay: f64,
    anims: Vec<Anim>,
}

fn f(v: f64) -> Value {
    Value::Float(v)
}

fn pt(x: f64, y: f64) -> Value {
    Value::Point([x, y])
}

fn col(c: [f32; 3]) -> Value {
    Value::Color(Rgba::new(c[0], c[1], c[2], 1.0))
}

fn col_a(c: [f32; 3], a: f32) -> Value {
    Value::Color(Rgba::new(c[0], c[1], c[2], a))
}

fn anim(effect: &'static str, key: &'static str, keys: Vec<(f64, Value)>) -> Anim {
    Anim { effect, key, keys }
}

fn pos(keys: Vec<(f64, Value)>) -> Anim {
    anim(catalog::MOTION, "position", keys)
}

fn scale(keys: Vec<(f64, Value)>) -> Anim {
    anim(catalog::MOTION, "scale", keys)
}

fn opacity(keys: Vec<(f64, Value)>) -> Anim {
    anim(catalog::OPACITY, "opacity", keys)
}

/// Where the picture is drawn: width, height and the shorter side (the base of sizes).
struct Frame {
    w: f64,
    h: f64,
    base: f64,
}

fn text_body(font: &str, size: f64, fill: [f32; 3], text: &str, extra: Vec<(&'static str, Value)>) -> Body {
    let mut v = vec![
        ("text", Value::Text(text.to_string())),
        ("font", Value::Text(font.to_string())),
        ("font_style", Value::Choice(1)),
        ("font_size", f(size.round())),
        ("fill", col(fill)),
    ];
    v.extend(extra);
    Body::Text(v)
}

fn outline(color: [f32; 3], width: f64) -> Vec<(&'static str, Value)> {
    vec![("stroke", Value::Bool(true)), ("stroke_color", col(color)), ("stroke_width", f(width.round()))]
}

fn rect(w: f64, h: f64, radius: f64, fill: Value, extra: Vec<(&'static str, Value)>) -> Body {
    let mut v = vec![("width", f(w.round())), ("height", f(h.round())), ("corner_radius", f(radius.round())), ("fill", fill)];
    v.extend(extra);
    Body::Shape(v)
}

fn layers(t: &Template, o: &TemplateOptions, fr: &Frame, font: &str) -> Vec<Layer> {
    let (w, h, b) = (fr.w, fr.h, fr.base);
    let s = o.seconds.max(0.3);
    let acc = o.accent;
    let black = [0.0, 0.0, 0.0];
    let white = [1.0, 1.0, 1.0];
    match t.id {
        "pop_title" => vec![Layer {
            name: o.text.clone(),
            body: text_body(font, b * 0.11, acc, &o.text, [outline(black, b * 0.012), vec![("shadow", Value::Bool(true))]].concat()),
            delay: 0.0,
            anims: vec![
                pos(vec![(0.0, pt(0.5, 0.42))]),
                scale(vec![(0.0, f(0.0)), (0.22, f(118.0)), (0.38, f(100.0)), (s - 0.3, f(100.0)), (s, f(70.0))]),
                opacity(vec![(0.0, f(0.0)), (0.12, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
            ],
        }],
        "lower_third" => {
            let (bx, by) = (0.27, 0.84);
            let slide = |delay: f64, x: f64, y: f64| {
                pos(vec![(0.0, pt(x - 0.4, y)), (0.45, pt(x, y)), (s - delay - 0.45, pt(x, y)), (s - delay, pt(x - 0.4, y))])
            };
            vec![
                Layer { name: "띠".into(), body: rect(w * 0.42, b * 0.17, b * 0.012, col(acc), vec![]), delay: 0.0, anims: vec![slide(0.0, bx, by)] },
                Layer {
                    name: o.text.clone(),
                    body: text_body(font, b * 0.065, white, &o.text, vec![]),
                    delay: 0.15,
                    anims: vec![slide(0.15, bx, by - 0.03)],
                },
                Layer {
                    name: o.sub.clone(),
                    body: text_body(font, b * 0.04, [0.92, 0.95, 1.0], &o.sub, vec![]),
                    delay: 0.25,
                    anims: vec![slide(0.25, bx, by + 0.035)],
                },
            ]
        }
        "highlight_box" => vec![
            Layer {
                name: "박스".into(),
                body: rect(w * 0.3, h * 0.22, b * 0.01, col_a(acc, 0.0), [outline(acc, b * 0.012), vec![]].concat()),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 0.5))]),
                    scale(vec![(0.0, f(160.0)), (0.3, f(100.0)), (0.6, f(106.0)), (0.9, f(100.0)), (1.2, f(106.0)), (1.5, f(100.0)), (s, f(100.0))]),
                    opacity(vec![(0.0, f(0.0)), (0.2, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
                ],
            },
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.07, acc, &o.text, outline(black, b * 0.008)),
                delay: 0.2,
                anims: vec![pos(vec![(0.0, pt(0.5, 0.31))]), scale(vec![(0.0, f(0.0)), (0.2, f(115.0)), (0.35, f(100.0))]), opacity(vec![(0.0, f(100.0)), (s - 0.5, f(100.0)), (s - 0.2, f(0.0))])],
            },
        ],
        "subscribe" => vec![
            Layer {
                name: "버튼".into(),
                body: rect(b * 0.62, b * 0.15, b * 0.04, col(acc), vec![("stroke", Value::Bool(true)), ("stroke_color", col(white)), ("stroke_width", f((b * 0.006).round()))]),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 1.15)), (0.35, pt(0.5, 0.86)), (0.5, pt(0.5, 0.88)), (s - 0.4, pt(0.5, 0.88)), (s, pt(0.5, 1.15))]),
                ],
            },
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.065, white, &o.text, vec![]),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 1.15)), (0.35, pt(0.5, 0.86)), (0.5, pt(0.5, 0.88)), (s - 0.4, pt(0.5, 0.88)), (s, pt(0.5, 1.15))]),
                ],
            },
        ],
        "flash" => vec![Layer {
            name: "번쩍".into(),
            body: rect(w * 1.05, h * 1.05, 0.0, col(white), vec![]),
            delay: 0.0,
            anims: vec![pos(vec![(0.0, pt(0.5, 0.5))]), opacity(vec![(0.0, f(0.0)), (0.05, f(100.0)), (s, f(0.0))])],
        }],
        "shake_text" => {
            let mut jitter = vec![(0.0, pt(0.5, 0.45))];
            let mut x = 1.0;
            let mut t = 0.18;
            while t < s - 0.35 && t < 1.0 {
                jitter.push((t, pt(0.5 + 0.012 * x, 0.45 - 0.006 * x)));
                x = -x;
                t += 0.045;
            }
            jitter.push((s - 0.3, pt(0.5, 0.45)));
            vec![Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.15, acc, &o.text, [outline(black, b * 0.016), vec![("shadow", Value::Bool(true))]].concat()),
                delay: 0.0,
                anims: vec![
                    pos(jitter),
                    scale(vec![(0.0, f(320.0)), (0.12, f(95.0)), (0.2, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
                    opacity(vec![(0.0, f(0.0)), (0.05, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
                ],
            }]
        }
        "slide_title" => vec![
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.1, acc, &o.text, [outline(black, b * 0.01), vec![("shadow", Value::Bool(true))]].concat()),
                delay: 0.0,
                anims: vec![pos(vec![(0.0, pt(-0.3, 0.5)), (0.5, pt(0.5, 0.5)), (s - 0.5, pt(0.5, 0.5)), (s, pt(1.3, 0.5))])],
            },
            Layer {
                name: "밑줄".into(),
                body: rect(w * 0.34, b * 0.012, 0.0, col(acc), vec![]),
                delay: 0.45,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 0.58))]),
                    anim(catalog::MOTION, "uniform_scale", vec![(0.0, Value::Bool(false))]),
                    anim(catalog::MOTION, "scale_width", vec![(0.0, f(0.0)), (0.4, f(100.0))]),
                    opacity(vec![(0.0, f(100.0)), (s - 0.45 - 0.45, f(100.0)), (s - 0.45 - 0.2, f(0.0))]),
                ],
            },
        ],
        "marker_text" => vec![
            Layer {
                name: "형광펜".into(),
                // about as wide as the text (Hangul is as wide as it is tall) plus a little room
                body: rect(b * 0.075 * (o.text.chars().count().max(2) as f64) * 0.95 + b * 0.05, b * 0.095, b * 0.01, col(acc), vec![]),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 0.5))]),
                    anim(catalog::MOTION, "uniform_scale", vec![(0.0, Value::Bool(false))]),
                    anim(catalog::MOTION, "scale_width", vec![(0.1, f(0.0)), (0.7, f(100.0))]),
                    opacity(vec![(0.0, f(70.0)), (s - 0.3, f(70.0)), (s, f(0.0))]),
                ],
            },
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.075, black, &o.text, vec![]),
                delay: 0.0,
                anims: vec![pos(vec![(0.0, pt(0.5, 0.5))]), opacity(vec![(0.0, f(0.0)), (0.15, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))])],
            },
        ],
        "bubble" => vec![
            Layer {
                name: "말풍선".into(),
                body: Body::Shape(vec![
                    ("shape", Value::Choice(1)),
                    ("width", f((b * 0.5).round())),
                    ("height", f((b * 0.3).round())),
                    ("fill", col(white)),
                    ("stroke", Value::Bool(true)),
                    ("stroke_color", col(black)),
                    ("stroke_width", f((b * 0.012).round())),
                ]),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 0.45))]),
                    scale(vec![(0.0, f(0.0)), (0.2, f(125.0)), (0.35, f(92.0)), (0.5, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
                    anim(catalog::MOTION, "rotation", vec![(0.0, f(-8.0)), (0.2, f(5.0)), (0.35, f(-3.0)), (0.5, f(0.0))]),
                ],
            },
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.1, black, &o.text, vec![]),
                delay: 0.0,
                anims: vec![
                    pos(vec![(0.0, pt(0.5, 0.45))]),
                    scale(vec![(0.0, f(0.0)), (0.2, f(125.0)), (0.35, f(92.0)), (0.5, f(100.0)), (s - 0.3, f(100.0)), (s, f(0.0))]),
                    anim(catalog::MOTION, "rotation", vec![(0.0, f(-8.0)), (0.2, f(5.0)), (0.35, f(-3.0)), (0.5, f(0.0))]),
                ],
            },
        ],
        "title_band" => vec![
            Layer {
                name: "띠".into(),
                body: rect(w * 1.05, b * 0.24, 0.0, col(acc), vec![]),
                delay: 0.0,
                anims: vec![pos(vec![(0.0, pt(-0.6, 0.5)), (0.4, pt(0.5, 0.5)), (s - 0.4, pt(0.5, 0.5)), (s, pt(1.6, 0.5))])],
            },
            Layer {
                name: o.text.clone(),
                body: text_body(font, b * 0.09, white, &o.text, vec![]),
                delay: 0.25,
                anims: vec![pos(vec![(0.0, pt(0.5, 0.485))]), opacity(vec![(0.0, f(0.0)), (0.2, f(100.0)), (s - 0.25 - 0.45, f(100.0)), (s - 0.25 - 0.2, f(0.0))])],
            },
            Layer {
                name: o.sub.clone(),
                body: text_body(font, b * 0.045, [1.0, 1.0, 1.0], &o.sub, vec![]),
                delay: 0.4,
                anims: vec![pos(vec![(0.0, pt(0.5, 0.545))]), opacity(vec![(0.0, f(0.0)), (0.2, f(100.0)), (s - 0.4 - 0.4, f(100.0)), (s - 0.4 - 0.15, f(0.0))])],
            },
        ],
        _ => Vec::new(),
    }
}

/// Keyframe times in order, none before the layer starts, none twice (short templates squeeze them).
fn tidy(mut list: Vec<Layer>) -> Vec<Layer> {
    for l in &mut list {
        for a in &mut l.anims {
            a.keys.retain(|(t, _)| *t >= 0.0);
            a.keys.sort_by(|x, y| x.0.total_cmp(&y.0));
            a.keys.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-6);
        }
    }
    list
}

/// The font for template text: Malgun Gothic (Korean, on every Windows PC), else the default.
fn template_font() -> String {
    let fonts = op_render::text::Fonts::global();
    ["Malgun Gothic", "Apple SD Gothic Neo", "Noto Sans CJK KR"]
        .into_iter()
        .find(|f| fonts.families().iter().any(|x| x == f))
        .map_or_else(|| op_render::text::Fonts::default_family().to_string(), str::to_string)
}

impl Editor {
    /// Puts template `id` on the timeline at the playhead as a group of graphics clips on free
    /// video tracks above the material there. Returns how many clips were made.
    pub fn add_motion_template(&mut self, id: &str, opts: &TemplateOptions) -> Result<usize, String> {
        let t = find(id).ok_or("There is no such motion template")?;
        let sid = self.active.ok_or("Open a sequence first")?;
        let seq = self.project.sequence(sid).ok_or("Open a sequence first")?.clone();
        let at = self.playhead();
        let rate = seq.rate();
        let frame = Frame {
            w: f64::from(seq.settings.width),
            h: f64::from(seq.settings.height),
            base: f64::from(seq.settings.width.min(seq.settings.height)),
        };
        let mut o = opts.clone();
        o.seconds = o.seconds.clamp(t.seconds.min(1.0), 60.0);
        if t.lines >= 1 && o.text.trim().is_empty() {
            return Err("Write the text first".into());
        }
        let font = template_font();
        let list = tidy(layers(t, &o, &frame, &font));
        if list.is_empty() {
            return Err("The template has no layers".into());
        }
        let total = Dur::from_seconds(o.seconds).round_frames(rate);
        let range = SeqRange::with_duration(at, total);
        // layers go on consecutive tracks above everything that plays here
        let occupied_top = seq
            .video
            .iter()
            .enumerate()
            .filter(|(_, tr)| tr.clips_in(range).next().is_some())
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let first = (occupied_top..seq.video.len() + list.len())
            .find(|start| {
                (0..list.len()).all(|k| seq.video.get(start + k).is_none_or(|tr| tr.clips_in(range).next().is_none() && !tr.locked))
            })
            .unwrap_or(seq.video.len());
        let count = list.len();
        let made = self.seq_edit("Motion Template", move |p, sid, _| {
            let mut ids = p.ids.clone();
            let group = ids.group();
            let mut made = Vec::new();
            for (k, layer) in list.into_iter().enumerate() {
                let (effect, values) = match &layer.body {
                    Body::Text(v) => (catalog::TEXT, v),
                    Body::Shape(v) => (catalog::SHAPE, v),
                };
                let def = catalog::find(effect).ok_or(EditError::NotFound("graphic".into()))?;
                let mut graphic = Component::new(def, &mut ids);
                for (key, v) in values {
                    if let Some(prm) = graphic.param_mut(key) {
                        prm.value = v.clone();
                    }
                }
                let mut components = default_components(EffectKind::VideoFixed, &mut ids);
                for a in &layer.anims {
                    let Some(comp) = components.iter_mut().find(|c| c.effect == a.effect) else { continue };
                    let Some(prm) = comp.param_mut(a.key) else { continue };
                    if a.keys.len() == 1 {
                        prm.value = a.keys[0].1.clone();
                        continue;
                    }
                    prm.animated = true;
                    prm.keys.clear();
                    for (sec, v) in &a.keys {
                        prm.set_at(SrcTime::from_seconds(*sec), v.clone(), Interp::AutoBezier);
                    }
                }
                components.push(graphic);
                let start = at + Dur::from_seconds(layer.delay).round_frames(rate);
                let len = (total - Dur::from_seconds(layer.delay).round_frames(rate)).max(rate.frame_duration());
                let clip = Clip {
                    id: ids.clip(),
                    name: layer.name,
                    kind: TrackKind::Video,
                    source: ClipSource::Graphic,
                    start,
                    duration: len,
                    source_in: SrcTime::ZERO,
                    speed: Speed::NORMAL,
                    reverse: false,
                    hold: None,
                    enabled: true,
                    link: None,
                    group: Some(group),
                    label: Label::Rose,
                    components,
                    gain_db: 0.0,
                    scale_to_frame: false,
                    channels: None,
                };
                made.push((first + k, clip));
            }
            p.ids = ids;
            let top = made.iter().map(|(i, _)| *i).max().unwrap_or(0);
            while p.sequence(sid).ok_or(EditError::NotFound("sequence".into()))?.video.len() <= top {
                let tid = p.ids.track();
                p.sequence_mut(sid)
                    .ok_or(EditError::NotFound("sequence".into()))?
                    .video
                    .push(Arc::new(Track::new(tid, TrackKind::Video)));
            }
            let s = p.sequence_mut(sid).ok_or(EditError::NotFound("sequence".into()))?;
            let mut clip_ids = Vec::new();
            for (index, clip) in made {
                let tr = s.track_mut(TrackRef::video(index)).ok_or(EditError::NotFound("track".into()))?;
                let pos = tr.clips.partition_point(|c| c.start <= clip.start);
                clip_ids.push(clip.id);
                tr.clips.insert(pos, clip);
            }
            Ok(clip_ids)
        });
        let ids = made.ok_or("The template could not be placed")?;
        debug_assert_eq!(ids.len(), count);
        let n = ids.len();
        self.selection = crate::session::Selection::only(ids);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame { w: 1920.0, h: 1080.0, base: 1080.0 }
    }

    #[test]
    fn every_template_builds_animated_layers_with_real_parameters() {
        for t in TEMPLATES {
            let o = TemplateOptions { text: t.default_text.into(), sub: t.default_sub.into(), accent: t.accent, seconds: t.seconds };
            let list = layers(t, &o, &frame(), "Arial");
            assert!(!list.is_empty(), "{}", t.id);
            let mut any_keys = false;
            for l in &list {
                let (effect, values) = match &l.body {
                    Body::Text(v) => (catalog::TEXT, v),
                    Body::Shape(v) => (catalog::SHAPE, v),
                };
                let def = catalog::find(effect).unwrap();
                for (k, _) in values {
                    assert!(def.params.iter().any(|p| p.key == *k), "{}: {effect} has no {k}", t.id);
                }
                for a in &l.anims {
                    let def = catalog::find(a.effect).unwrap();
                    assert!(def.params.iter().any(|p| p.key == a.key), "{}: {} has no {}", t.id, a.effect, a.key);
                    assert!(a.keys.windows(2).all(|w| w[0].0 < w[1].0), "{}: keyframe times must grow ({})", t.id, a.key);
                    assert!(a.keys.iter().all(|(s, _)| *s >= 0.0 && *s <= t.seconds + 1e-9), "{}: {} outside the clip", t.id, a.key);
                    any_keys |= a.keys.len() > 1;
                }
            }
            assert!(any_keys, "{} does not move", t.id);
        }
    }
}
