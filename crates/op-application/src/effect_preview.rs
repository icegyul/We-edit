//! Animated previews of effects and transitions for the Effects panel: a small picture of the
//! program's own making goes through the real renderer and a few frames are kept. Video effects
//! are shown with their first amount setting swept up and down, transitions as a change from one
//! picture to another.

use std::sync::Arc;

use op_core::catalog::{EffectDef, EffectKind, ParamKind};
use op_core::*;
use op_media::{PixelLayout, Plane, VideoFrame};
use op_render::{FrameSource, Renderer, Request};
use op_timeline::{EditOptions, Patch, SourceClip, add_transition, overwrite};

/// Size and length of a preview.
pub const WIDTH: u32 = 192;
pub const HEIGHT: u32 = 108;
pub const FRAMES: usize = 14;

/// The frames of one preview, straight RGBA8.
#[derive(Clone)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A picture made for previews (nothing borrowed from anywhere): a sunset over hills and water
/// with a strip of color patches, or, as `variant` 1, a night scene. It has smooth gradients,
/// hard edges, fine lines and saturated colors so that blurs, distortions, keys and grades all
/// show what they do.
pub fn picture(w: u32, h: u32, variant: u32) -> Vec<u8> {
    let mut out = vec![255u8; (w * h * 4) as usize];
    let (wf, hf) = (w as f32, h as f32);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / wf, y as f32 / hf);
            let horizon = 0.62;
            let mut c: [f32; 3];
            if v < horizon {
                // sky
                let t = v / horizon;
                c = if variant == 0 {
                    let top = [0.18, 0.16, 0.45];
                    let bottom = [1.0, 0.62, 0.22];
                    [
                        top[0] + (bottom[0] - top[0]) * t,
                        top[1] + (bottom[1] - top[1]) * t,
                        top[2] + (bottom[2] - top[2]) * t,
                    ]
                } else {
                    let top = [0.02, 0.05, 0.16];
                    let bottom = [0.10, 0.35, 0.55];
                    [
                        top[0] + (bottom[0] - top[0]) * t,
                        top[1] + (bottom[1] - top[1]) * t,
                        top[2] + (bottom[2] - top[2]) * t,
                    ]
                };
                // the sun (or moon), with a glow
                let (cx, cy) = (if variant == 0 { 0.68 } else { 0.30 }, 0.46);
                let d = (((u - cx) * wf / hf).powi(2) + (v - cy).powi(2)).sqrt();
                let disc = 1.0 - smooth(0.085, 0.092, d);
                let glow = (1.0 - smooth(0.0, 0.35, d)) * 0.45;
                let body = if variant == 0 {
                    [1.0, 0.95, 0.6]
                } else {
                    [0.93, 0.95, 1.0]
                };
                for k in 0..3 {
                    c[k] = c[k] + (body[k] - c[k]) * glow.min(1.0) * 0.6;
                    c[k] = c[k] * (1.0 - disc) + body[k] * disc;
                }
                // hills: two silhouettes
                let h1 = 0.50 + 0.07 * (u * 9.0).sin() + 0.04 * (u * 23.0 + 1.0).sin();
                let h2 = 0.55 + 0.05 * (u * 6.0 + 2.0).sin();
                if v > h2 {
                    c = [0.10, 0.08, 0.14];
                } else if v > h1 {
                    c = [0.28, 0.14, 0.22];
                    if variant == 1 {
                        c = [0.05, 0.12, 0.16];
                    }
                }
            } else {
                // water with horizontal bands and the sun's reflection
                let t = (v - horizon) / (1.0 - horizon);
                let base = if variant == 0 {
                    [0.95 - 0.7 * t, 0.45 - 0.3 * t, 0.30 + 0.15 * t]
                } else {
                    [0.05, 0.25 - 0.1 * t, 0.40 - 0.1 * t]
                };
                let band = 0.5 + 0.5 * (t * 60.0 + (u * 40.0).sin() * 0.6).sin();
                let refl =
                    (1.0 - smooth(
                        0.0,
                        0.07 + 0.1 * t,
                        (u - if variant == 0 { 0.68 } else { 0.30 }).abs(),
                    )) * (1.0 - t);
                c = [
                    base[0] * (0.75 + 0.25 * band) + refl * 0.5,
                    base[1] * (0.75 + 0.25 * band) + refl * 0.4,
                    base[2] * (0.75 + 0.25 * band) + refl * 0.2,
                ];
            }
            // a strip of color patches and a grey ramp along the top: color work is visible
            if v < 0.12 {
                let i = (u * 8.0) as usize;
                let patches: [[f32; 3]; 8] = [
                    [1.0, 0.0, 0.0],
                    [1.0, 0.6, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 0.8, 0.0],
                    [0.0, 0.9, 0.9],
                    [0.0, 0.2, 1.0],
                    [0.8, 0.0, 1.0],
                    [0.9, 0.9, 0.9],
                ];
                if v < 0.07 {
                    c = patches[i.min(7)];
                } else {
                    let g = u;
                    c = [g, g, g];
                }
            }
            // a thin white frame and a diagonal: distortions and keys have an edge to bend
            let border = (x < 2 || y < 2 || x >= w - 2 || y >= h - 2) as u32 as f32;
            let diag = ((x as f32 - y as f32 * wf / hf).abs() < 1.0) as u32 as f32 * 0.6;
            let mark = border.max(diag);
            for v in &mut c {
                *v = *v * (1.0 - mark) + mark;
            }
            let i = ((y * w + x) * 4) as usize;
            out[i] = (c[0].clamp(0.0, 1.0) * 255.0) as u8;
            out[i + 1] = (c[1].clamp(0.0, 1.0) * 255.0) as u8;
            out[i + 2] = (c[2].clamp(0.0, 1.0) * 255.0) as u8;
            out[i + 3] = 255;
        }
    }
    out
}

fn frame_of(w: u32, h: u32, variant: u32) -> Arc<VideoFrame> {
    Arc::new(VideoFrame {
        width: w,
        height: h,
        layout: PixelLayout::Rgba8,
        planes: vec![Plane {
            data: picture(w, h, variant),
            width: w,
            height: h,
            bytes_per_row: w * 4,
        }],
        color: ColorInfo::default(),
        index: 0,
    })
}

/// The two pictures, by asset.
struct Pictures {
    /// The id the project gave the first picture's asset.
    first: AssetId,
    a: Arc<VideoFrame>,
    b: Arc<VideoFrame>,
}

impl FrameSource for Pictures {
    fn video_frame(&self, asset: &MediaAsset, _t: SrcTime) -> Option<Arc<VideoFrame>> {
        Some(if asset.id == self.first {
            self.a.clone()
        } else {
            self.b.clone()
        })
    }
}

fn asset(id: u64, w: u32, h: u32) -> MediaAsset {
    MediaAsset {
        id: AssetId(id),
        path: format!("preview-{id}.png"),
        proxy: None,
        kind: MediaKind::Still,
        video: Some(VideoStream {
            index: 0,
            codec: "png".into(),
            width: w,
            height: h,
            pixel_aspect: (1, 1),
            rate: Rate::FPS_25,
            frames: 1,
            pixel_format: "rgba".into(),
            bit_depth: 8,
            alpha: AlphaMode::None,
            color: ColorInfo::default(),
            field_order: FieldOrder::Progressive,
            start: Dur::ZERO,
            rotation: 0,
            timecode: None,
        }),
        audio: vec![],
        duration: Dur::from_seconds(10.0),
        interpretation: Interpretation::default(),
        file_size: 0,
        modified_unix: 0,
    }
}

/// The setting swept to show an effect at work: the first number that is not a switch, from off
/// to a strong value and back.
fn amount_keys(def: &'static EffectDef, comp: &mut Component, length: f64) -> bool {
    let has_float = def
        .params
        .iter()
        .any(|p| !p.group.starts_with('_') && matches!(p.kind, ParamKind::Float { .. }));
    for spec in def.params.iter().filter(|p| !p.group.starts_with('_')) {
        // an angle sweeps from straight to a clear turn, a whole number across its range
        let swept: Option<(Value, Value)> = if has_float {
            None
        } else {
            match spec.kind {
                ParamKind::Angle { default } => {
                    Some((Value::Float(default), Value::Float(default + 50.0)))
                }
                ParamKind::Int { min, max, default } if max > min => {
                    let hi = if default > min {
                        default
                    } else {
                        min + (max - min).min(40) / 2
                    };
                    Some((Value::Int(min), Value::Int(hi)))
                }
                _ => None,
            }
        };
        if let Some((off, strong)) = swept
            && let Some(p) = comp.param_mut(spec.key)
        {
            p.animated = true;
            p.keys = [
                (0.0, off.clone()),
                (length * 0.35, strong.clone()),
                (length * 0.65, strong),
                (length, off),
            ]
            .into_iter()
            .map(|(t, v)| Keyframe::new(SrcTime::from_seconds(t), v, Interp::Linear))
            .collect();
            return true;
        }
        let ParamKind::Float {
            default,
            min,
            max,
            soft_min,
            soft_max,
            ..
        } = spec.kind
        else {
            continue;
        };
        let strong =
            if default.abs() > 1e-6 && (soft_max - default).abs() < default.abs() * 4.0 + 1e-9 {
                default
            } else if soft_max > 0.0 {
                soft_max * 0.55
            } else {
                max * 0.5
            };
        let off = if min <= 0.0 && soft_min <= 0.0 {
            0.0
        } else {
            soft_min.max(min)
        };
        if (strong - off).abs() < 1e-9 {
            continue;
        }
        if let Some(p) = comp.param_mut(spec.key) {
            p.animated = true;
            p.keys = vec![
                (0.0, off),
                (length * 0.35, strong),
                (length * 0.65, strong),
                (length, off),
            ]
            .into_iter()
            .map(|(t, v)| Keyframe::new(SrcTime::from_seconds(t), Value::Float(v), Interp::Linear))
            .collect();
            return true;
        }
        break;
    }
    false
}

/// Renders the preview of `def`, or None for audio (no picture to show).
pub fn render(r: &mut Renderer, def: &'static EffectDef) -> Option<Preview> {
    let video = def.kind.is_video();
    if !video {
        return None;
    }
    let transition = def.kind.is_transition();
    let length = if transition { 1.0 } else { 1.6 };
    let mut p = Project::new("preview");
    let root = p.root;
    let (first, item_a) = p.add_asset(root, asset(0, WIDTH, HEIGHT));
    let (_, item_b) = p.add_asset(root, asset(1, WIDTH, HEIGHT));
    let (sid, _) = p.add_sequence(
        root,
        "Preview",
        SequenceSettings {
            width: WIDTH,
            height: HEIGHT,
            rate: Rate::FPS_25,
            ..Default::default()
        },
    );
    let opts = EditOptions::default();
    let patch = Patch {
        video: Some(0),
        audio: vec![],
    };
    let mut times: Vec<f64> = Vec::new();
    if transition {
        // A, then B (with media before its start for the overlap), a second long transition
        let a = SourceClip::from_asset(
            &p,
            item_a,
            SrcRange::with_duration(SrcTime::ZERO, Dur::from_seconds(1.5)),
        )
        .ok()?;
        let b = SourceClip::from_asset(
            &p,
            item_b,
            SrcRange::with_duration(SrcTime::from_seconds(1.0), Dur::from_seconds(1.5)),
        )
        .ok()?;
        let (next, _) = p
            .transact(|p| {
                overwrite(p, sid, &a, SeqTime::ZERO, &patch, opts)?;
                overwrite(p, sid, &b, SeqTime::from_seconds(1.5), &patch, opts)
            })
            .ok()?;
        p = next;
        let (next, _) = p
            .transact(|p| {
                add_transition(
                    p,
                    sid,
                    TrackRef::video(0),
                    SeqTime::from_seconds(1.5),
                    def.id,
                    Dur::from_seconds(length),
                    None,
                    opts,
                )
            })
            .ok()?;
        p = next;
        // the picture holds a moment before and after the change
        for i in 0..FRAMES {
            let f = i as f64 / (FRAMES - 1) as f64;
            times.push(1.0 + length * f);
        }
    } else {
        let a = SourceClip::from_item(&p, item_a).ok()?;
        let (next, ids) = p
            .transact(|p| overwrite(p, sid, &a, SeqTime::ZERO, &patch, opts))
            .ok()?;
        p = next;
        let mut ids_gen = p.ids.clone();
        let mut comp = Component::new(def, &mut ids_gen);
        if def.id == "op.video.color_nodes" {
            // a graph with a look in it, so the card shows what the effect does
            let mut graph = op_core::colorgraph::ColorGraph::default();
            if let Some(op_core::colorgraph::Node {
                kind: op_core::colorgraph::NodeKind::Corrector(c),
                ..
            }) = graph.node_mut(2)
            {
                c.grade.lift = [-0.2, 0.0, 0.25, 0.0];
                c.grade.gain = [0.3, 0.05, -0.3, 0.0];
                c.grade.saturation = 1.3;
                c.grade.contrast = 1.15;
            }
            if let Some(p) = comp.param_mut("graph") {
                p.value = Value::Text(graph.to_json());
            }
        }
        let animated = amount_keys(def, &mut comp, length);
        p.ids = ids_gen;
        p.sequence_mut(sid)?.clip_mut(ids[0])?.components.push(comp);
        if !animated {
            // nothing to sweep (a flip, black and white...): the original picture fades away
            // over it and back, so the change is seen
            let over = Patch {
                video: Some(1),
                audio: vec![],
            };
            let (next, over_ids) = p
                .transact(|p| overwrite(p, sid, &a, SeqTime::ZERO, &over, opts))
                .ok()?;
            p = next;
            let clip = p.sequence_mut(sid)?.clip_mut(over_ids[0])?;
            if let Some(prm) = clip
                .component_mut(catalog::OPACITY)
                .and_then(|o| o.param_mut("opacity"))
            {
                prm.animated = true;
                prm.keys = [
                    (0.0, 100.0),
                    (length * 0.15, 100.0),
                    (length * 0.45, 0.0),
                    (length * 0.75, 0.0),
                    (length, 100.0),
                ]
                .into_iter()
                .map(|(t, v)| {
                    Keyframe::new(SrcTime::from_seconds(t), Value::Float(v), Interp::Linear)
                })
                .collect();
            }
        }
        for i in 0..FRAMES {
            times.push(length * i as f64 / FRAMES as f64);
        }
    }
    let source = Pictures {
        first,
        a: frame_of(WIDTH, HEIGHT, 0),
        b: frame_of(WIDTH, HEIGHT, 1),
    };
    r.forget_media();
    let mut frames = Vec::with_capacity(FRAMES);
    for t in times {
        let tex = r.render(&Request {
            project: &p,
            sequence: sid,
            time: SeqTime::from_seconds(t),
            scale: 1.0,
            source: &source,
        });
        frames.push(r.rgba8(&tex));
        r.recycle(tex);
    }
    Some(Preview {
        width: WIDTH,
        height: HEIGHT,
        frames,
    })
}

/// Which kinds have a preview.
pub fn has_preview(kind: EffectKind) -> bool {
    kind.is_video()
}
