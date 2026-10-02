//! Render plans: what the renderer draws for one sequence frame (architecture 4, stage 1).
//!
//! A plan is an immutable description built from a project snapshot. It resolves track order,
//! transitions, source times and animated parameter values, so the renderer never interprets
//! editing rules and the same plan renders identically for preview and export.

use crate::catalog::{self, EffectKind};
use crate::color::Rgba;
use crate::ids::*;
use crate::model::*;
use crate::params::Value;
use crate::time::*;

/// Maximum nesting depth rendered; deeper references draw nothing.
pub const MAX_NESTING: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct EvalComponent {
    pub id: ComponentId,
    pub effect: String,
    pub values: Vec<(String, Value)>,
}

impl EvalComponent {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn f64(&self, key: &str) -> f64 {
        self.get(key)
            .map(|v| v.as_f64())
            .unwrap_or_else(|| self.default(key).as_f64())
    }

    pub fn f32(&self, key: &str) -> f32 {
        self.f64(key) as f32
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key)
            .map(|v| v.as_bool())
            .unwrap_or_else(|| self.default(key).as_bool())
    }

    pub fn choice(&self, key: &str) -> u32 {
        self.get(key)
            .map(|v| v.as_choice())
            .unwrap_or_else(|| self.default(key).as_choice())
    }

    pub fn point(&self, key: &str) -> [f64; 2] {
        self.get(key)
            .map(|v| v.as_point())
            .unwrap_or_else(|| self.default(key).as_point())
    }

    pub fn color(&self, key: &str) -> Rgba {
        self.get(key)
            .map(|v| v.as_color())
            .unwrap_or_else(|| self.default(key).as_color())
    }

    pub fn text(&self, key: &str) -> String {
        self.get(key)
            .map(|v| v.as_text().to_string())
            .unwrap_or_default()
    }

    pub fn curve(&self, key: &str) -> Vec<[f32; 2]> {
        self.get(key)
            .map(|v| v.as_curve().to_vec())
            .unwrap_or_default()
    }

    fn default(&self, key: &str) -> Value {
        catalog::find(&self.effect)
            .and_then(|d| d.param(key))
            .map(|p| p.default_value())
            .unwrap_or(Value::Float(0.0))
    }
}

/// Evaluates a component at a source time.
pub fn eval_component(c: &Component, t: SrcTime) -> EvalComponent {
    EvalComponent {
        id: c.id,
        effect: c.effect.clone(),
        values: c
            .params
            .iter()
            .map(|p| (p.key.clone(), p.value_at(t)))
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerSource {
    Media {
        asset: AssetId,
        time: SrcTime,
    },
    Sequence {
        sequence: SequenceId,
        time: SeqTime,
    },
    Color(Rgba),
    Transparent,
    Bars,
    /// Applies its effects to everything below.
    Adjustment,
    /// Pixels come from Text/Shape components.
    Graphic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipLayer {
    pub clip: ClipId,
    pub source: LayerSource,
    /// Size of the source picture before Motion.
    pub source_size: (u32, u32),
    pub scale_to_frame: bool,
    /// Enabled components in stack order, evaluated at `src_time`.
    pub components: Vec<EvalComponent>,
    pub src_time: SrcTime,
    /// The clip's length on the timeline, in seconds.
    pub duration: f64,
}

impl ClipLayer {
    pub fn fixed(&self, effect: &str) -> Option<&EvalComponent> {
        self.components.iter().find(|c| c.effect == effect)
    }

    /// Standard and graphic components (not Motion/Opacity) in order.
    pub fn effects(&self) -> impl Iterator<Item = &EvalComponent> {
        self.components
            .iter()
            .filter(|c| c.effect != catalog::MOTION && c.effect != catalog::OPACITY)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackContent {
    Clip(ClipLayer),
    Transition {
        effect: String,
        progress: f64,
        params: EvalComponent,
        from: Option<ClipLayer>,
        to: Option<ClipLayer>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackPlan {
    /// Video track index (V1 = 0).
    pub track: usize,
    pub content: TrackContent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FramePlan {
    pub sequence: SequenceId,
    pub width: u32,
    pub height: u32,
    pub time: SeqTime,
    pub linear: bool,
    /// Bottom to top.
    pub tracks: Vec<TrackPlan>,
}

/// Builds the plan for sequence time `t`. Returns an empty plan for a missing sequence.
pub fn frame_plan(project: &Project, sequence: SequenceId, t: SeqTime) -> FramePlan {
    let Some(seq) = project.sequence(sequence) else {
        return FramePlan {
            sequence,
            width: 16,
            height: 16,
            time: t,
            linear: false,
            tracks: vec![],
        };
    };
    let mut tracks = Vec::new();
    for (i, track) in seq.video.iter().enumerate() {
        if !track.enabled {
            continue;
        }
        if let Some(tr) = track.transition_at(t) {
            let from = tr
                .from
                .and_then(|id| track.clip(id))
                .filter(|c| c.enabled)
                .map(|c| layer(project, seq, c, t));
            let to = tr
                .to
                .and_then(|id| track.clip(id))
                .filter(|c| c.enabled)
                .map(|c| layer(project, seq, c, t));
            let params = EvalComponent {
                id: ComponentId(tr.id.0),
                effect: tr.effect.clone(),
                values: tr
                    .params
                    .iter()
                    .map(|p| (p.key.clone(), p.value.clone()))
                    .collect(),
            };
            tracks.push(TrackPlan {
                track: i,
                content: TrackContent::Transition {
                    effect: tr.effect.clone(),
                    progress: tr.progress(t),
                    params,
                    from,
                    to,
                },
            });
            continue;
        }
        if let Some(c) = track.clip_at(t).filter(|c| c.enabled) {
            tracks.push(TrackPlan {
                track: i,
                content: TrackContent::Clip(layer(project, seq, c, t)),
            });
        }
    }
    FramePlan {
        sequence,
        width: seq.settings.width,
        height: seq.settings.height,
        time: t,
        linear: seq.settings.linear_compositing,
        tracks,
    }
}

/// Time at which a clip samples its source, honoring Posterize Time.
fn sample_time(c: &Clip, t: SeqTime) -> SeqTime {
    let Some(pt) = c
        .components
        .iter()
        .find(|x| x.enabled && x.effect == "op.video.posterize_time")
    else {
        return t;
    };
    let rate = pt.f64_at("rate", c.to_source(t)).max(0.01);
    let offset = (t - c.start).seconds();
    let q = (offset * rate).floor() / rate;
    c.start + Dur::from_seconds(q)
}

fn layer(project: &Project, seq: &Sequence, c: &Clip, t: SeqTime) -> ClipLayer {
    let t = sample_time(c, t);
    let mut src = c.to_source(t);
    if let Some(avail) = project.available(&c.source) {
        // transition handles beyond the media repeat the edge frame
        let last = avail.end - Dur(1);
        src = src.clamp(avail.start, last.max(avail.start));
    }
    let source = match &c.source {
        ClipSource::Asset { asset, .. } => LayerSource::Media {
            asset: *asset,
            time: src,
        },
        ClipSource::Sequence { sequence, .. } => LayerSource::Sequence {
            sequence: *sequence,
            time: src.cast(),
        },
        ClipSource::Graphic => LayerSource::Graphic,
        ClipSource::Generator { item } => match project.item(*item).map(|i| &i.kind) {
            Some(ItemKind::Synthetic { generator, .. }) => match generator {
                Generator::ColorMatte { color } => LayerSource::Color(*color),
                Generator::BlackVideo => LayerSource::Color(Rgba::BLACK),
                Generator::TransparentVideo => LayerSource::Transparent,
                Generator::BarsAndTone => LayerSource::Bars,
                Generator::AdjustmentLayer => LayerSource::Adjustment,
            },
            _ => LayerSource::Transparent,
        },
    };
    let components = c
        .components
        .iter()
        .filter(|x| x.enabled && x.def().is_some_and(|d| d.kind.is_video()))
        .map(|x| eval_component(x, src))
        .collect();
    ClipLayer {
        clip: c.id,
        source,
        source_size: project.source_size(&c.source, &seq.settings),
        scale_to_frame: c.scale_to_frame,
        components,
        src_time: src,
        duration: c.duration.seconds(),
    }
}

/// Whether a component definition affects pixels.
pub fn is_video_component(c: &Component) -> bool {
    c.def()
        .is_some_and(|d| d.kind.is_video() && d.kind != EffectKind::VideoTransition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Label;
    use crate::params::{Interp, Keyframe};

    fn graphic_clip(p: &mut Project, start: f64, dur: f64) -> Clip {
        let mut ids = p.ids.clone();
        let c = Clip {
            id: ids.clip(),
            name: "g".into(),
            kind: TrackKind::Video,
            source: ClipSource::Graphic,
            start: SeqTime::from_seconds(start),
            duration: Dur::from_seconds(dur),
            source_in: SrcTime::ZERO,
            speed: Speed::NORMAL,
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: default_components(EffectKind::VideoFixed, &mut ids),
            gain_db: 0.0,
            scale_to_frame: false,
            channels: None,
        };
        p.ids = ids;
        c
    }

    #[test]
    fn plan_orders_tracks_and_evaluates_keyframes() {
        let mut p = Project::new("t");
        let (sid, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        let mut a = graphic_clip(&mut p, 0.0, 10.0);
        let op = a
            .component_mut(catalog::OPACITY)
            .unwrap()
            .param_mut("opacity")
            .unwrap();
        op.animated = true;
        op.keys = vec![
            Keyframe::new(SrcTime::ZERO, Value::Float(0.0), Interp::Linear),
            Keyframe::new(
                SrcTime::from_seconds(2.0),
                Value::Float(100.0),
                Interp::Linear,
            ),
        ];
        let b = graphic_clip(&mut p, 2.0, 3.0);
        let seq = p.sequence_mut(sid).unwrap();
        seq.track_mut(TrackRef::video(0)).unwrap().clips.push(a);
        seq.track_mut(TrackRef::video(2)).unwrap().clips.push(b);

        let plan = frame_plan(&p, sid, SeqTime::from_seconds(1.0));
        assert_eq!(plan.tracks.len(), 1);
        let TrackContent::Clip(l) = &plan.tracks[0].content else {
            panic!()
        };
        assert!((l.fixed(catalog::OPACITY).unwrap().f64("opacity") - 50.0).abs() < 1e-9);

        let plan = frame_plan(&p, sid, SeqTime::from_seconds(3.0));
        assert_eq!(
            plan.tracks.iter().map(|t| t.track).collect::<Vec<_>>(),
            vec![0, 2]
        );

        // hidden tracks are skipped
        p.sequence_mut(sid)
            .unwrap()
            .track_mut(TrackRef::video(2))
            .unwrap()
            .enabled = false;
        assert_eq!(
            frame_plan(&p, sid, SeqTime::from_seconds(3.0)).tracks.len(),
            1
        );
    }

    #[test]
    fn transitions_show_both_sides() {
        let mut p = Project::new("t");
        let (sid, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        let a = graphic_clip(&mut p, 0.0, 5.0);
        let b = graphic_clip(&mut p, 5.0, 5.0);
        let tr = Transition {
            id: p.ids.transition(),
            effect: catalog::CROSS_DISSOLVE.into(),
            cut: SeqTime::from_seconds(5.0),
            duration: Dur::from_seconds(1.0),
            alignment: Alignment::CenterAtCut,
            from: Some(a.id),
            to: Some(b.id),
            params: vec![],
        };
        let t = p
            .sequence_mut(sid)
            .unwrap()
            .track_mut(TrackRef::video(0))
            .unwrap();
        t.clips = vec![a, b];
        t.transitions.push(tr);
        let plan = frame_plan(&p, sid, SeqTime::from_seconds(4.75));
        let TrackContent::Transition {
            progress, from, to, ..
        } = &plan.tracks[0].content
        else {
            panic!()
        };
        assert!((progress - 0.25).abs() < 1e-9);
        assert!(from.is_some() && to.is_some());
    }
}
