//! Animation presets: ready-made combinations of effects and keyframes (entrances, exits, camera
//! moves and looks) applied to a clip in one step, like dropping an effect on it. Keyframes are
//! placed relative to the clip's start or end, so a preset fits clips of any length.

use crate::catalog::{self, MOTION, OPACITY};
use crate::ids::IdGen;
use crate::model::{Clip, Component};
use crate::params::{InterpChoice, Keyframe, Value};
use crate::time::{Dur, SeqTime, SrcTime};

pub const CAT_PRESET_ZOOM: &str = "Zoom";
pub const CAT_PRESET_FADE: &str = "Fades";
pub const CAT_PRESET_SLIDE: &str = "Slides";
pub const CAT_PRESET_SPIN: &str = "Spins";
pub const CAT_PRESET_BLUR: &str = "Blurs";
pub const CAT_PRESET_SHAKE: &str = "Shakes";
pub const CAT_PRESET_LOOK: &str = "Looks";

#[derive(Clone, Copy, Debug)]
pub struct PresetDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    apply: fn(&mut Clip, &mut IdGen),
}

impl PresetDef {
    /// Adds the preset's effects and keyframes to a video clip.
    pub fn apply(&self, clip: &mut Clip, ids: &mut IdGen) {
        (self.apply)(clip, ids)
    }
}

/// Where a keyframe goes: seconds after the clip start, or (negative) before its end.
type At = f64;

fn time_at(clip: &Clip, at: At) -> SeqTime {
    let len = clip.duration.seconds();
    let last = clip.end() - Dur(1);
    let t = if at >= 0.0 {
        clip.start + Dur::from_seconds(at.min(len))
    } else {
        clip.end() - Dur::from_seconds((-at).min(len))
    };
    t.min(last).max(clip.start)
}

/// Length of an entrance or exit: at most half of the clip.
fn span(clip: &Clip, wanted: f64) -> f64 {
    wanted.min(clip.duration.seconds() / 2.0)
}

/// The component for `effect`: the clip's fixed one, or a new one appended to the clip.
fn component<'a>(clip: &'a mut Clip, ids: &mut IdGen, effect: &str) -> Option<&'a mut Component> {
    let fixed = effect == MOTION || effect == OPACITY;
    if !fixed {
        let def = catalog::find(effect)?;
        clip.components.push(Component::new(def, ids));
        return clip.components.last_mut();
    }
    clip.component_mut(effect)
}

fn set(clip: &mut Clip, ids: &mut IdGen, effect: &str, values: &[(&str, Value)]) {
    if let Some(c) = component(clip, ids, effect) {
        for (k, v) in values {
            if let Some(p) = c.param_mut(k) {
                p.value = v.clone();
            }
        }
    }
}

/// Keyframe times of a preset on this clip (source time).
fn times(clip: &Clip, keys: &[(At, Value)]) -> Vec<(SrcTime, Value)> {
    keys.iter()
        .map(|(at, v)| (clip.to_source(time_at(clip, *at)), v.clone()))
        .collect()
}

/// Animates one parameter; every keyframe eases in and out.
fn animate(c: &mut Component, key: &str, keys: &[(SrcTime, Value)], start: SrcTime) {
    let Some(p) = c.param_mut(key) else { return };
    // keyframes replace the ones this parameter had in the same stretch
    if let (Some(first), Some(last)) = (keys.first(), keys.last()) {
        let (lo, hi) = (first.0.min(last.0), first.0.max(last.0));
        p.keys.retain(|k| k.time < lo || k.time > hi);
    }
    for (t, v) in keys {
        let mut k = Keyframe::new(*t, v.clone(), crate::params::Interp::Linear);
        k.set_interp(InterpChoice::EaseIn);
        k.set_interp(InterpChoice::EaseOut);
        p.keys.push(k);
    }
    p.animated = true;
    p.sort_keys();
    p.value = p.value_at(start);
}

fn anim(clip: &mut Clip, ids: &mut IdGen, effect: &str, key: &str, keys: &[(At, Value)]) {
    let keys = times(clip, keys);
    let start = clip.to_source(clip.start);
    if let Some(c) = component(clip, ids, effect) {
        animate(c, key, &keys, start);
    }
}

fn fl(v: f64) -> Value {
    Value::Float(v)
}

fn pt(x: f64, y: f64) -> Value {
    Value::Point([x, y])
}

// ---------------------------------------------------------------------------- the presets

fn zoom_in(c: &mut Clip, ids: &mut IdGen) {
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(0.0, fl(100.0)), (-0.0001, fl(120.0))],
    );
}

fn zoom_out(c: &mut Clip, ids: &mut IdGen) {
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(0.0, fl(120.0)), (-0.0001, fl(100.0))],
    );
}

fn punch_in(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.4);
    anim(c, ids, MOTION, "scale", &[(0.0, fl(130.0)), (d, fl(100.0))]);
}

fn punch_out(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.4);
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(-d, fl(100.0)), (-0.0001, fl(130.0))],
    );
}

fn pop_in(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.45);
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(0.0, fl(0.0)), (d * 0.6, fl(112.0)), (d, fl(100.0))],
    );
}

fn pop_out(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.45);
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(-d, fl(100.0)), (-d * 0.4, fl(112.0)), (-0.0001, fl(0.0))],
    );
}

fn fade_in(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.5);
    anim(
        c,
        ids,
        OPACITY,
        "opacity",
        &[(0.0, fl(0.0)), (d, fl(100.0))],
    );
}

fn fade_out(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.5);
    anim(
        c,
        ids,
        OPACITY,
        "opacity",
        &[(-d, fl(100.0)), (-0.0001, fl(0.0))],
    );
}

fn fade_in_out(c: &mut Clip, ids: &mut IdGen) {
    fade_in(c, ids);
    fade_out(c, ids);
}

fn slide_in(c: &mut Clip, ids: &mut IdGen, from: (f64, f64)) {
    let d = span(c, 0.6);
    anim(
        c,
        ids,
        MOTION,
        "position",
        &[(0.0, pt(from.0, from.1)), (d, pt(0.5, 0.5))],
    );
}

fn slide_out(c: &mut Clip, ids: &mut IdGen, to: (f64, f64)) {
    let d = span(c, 0.6);
    anim(
        c,
        ids,
        MOTION,
        "position",
        &[(-d, pt(0.5, 0.5)), (-0.0001, pt(to.0, to.1))],
    );
}

fn slide_in_left(c: &mut Clip, ids: &mut IdGen) {
    slide_in(c, ids, (-0.5, 0.5))
}
fn slide_in_right(c: &mut Clip, ids: &mut IdGen) {
    slide_in(c, ids, (1.5, 0.5))
}
fn slide_in_top(c: &mut Clip, ids: &mut IdGen) {
    slide_in(c, ids, (0.5, -0.5))
}
fn slide_in_bottom(c: &mut Clip, ids: &mut IdGen) {
    slide_in(c, ids, (0.5, 1.5))
}
fn slide_out_left(c: &mut Clip, ids: &mut IdGen) {
    slide_out(c, ids, (-0.5, 0.5))
}
fn slide_out_right(c: &mut Clip, ids: &mut IdGen) {
    slide_out(c, ids, (1.5, 0.5))
}

fn spin_in(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.6);
    anim(
        c,
        ids,
        MOTION,
        "rotation",
        &[(0.0, fl(-180.0)), (d, fl(0.0))],
    );
    anim(c, ids, MOTION, "scale", &[(0.0, fl(0.0)), (d, fl(100.0))]);
}

fn spin_out(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.6);
    anim(
        c,
        ids,
        MOTION,
        "rotation",
        &[(-d, fl(0.0)), (-0.0001, fl(180.0))],
    );
    anim(
        c,
        ids,
        MOTION,
        "scale",
        &[(-d, fl(100.0)), (-0.0001, fl(0.0))],
    );
}

fn blur_in(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.5);
    anim(
        c,
        ids,
        "op.video.gaussian_blur",
        "blurriness",
        &[(0.0, fl(80.0)), (d, fl(0.0))],
    );
}

fn blur_out(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.5);
    anim(
        c,
        ids,
        "op.video.gaussian_blur",
        "blurriness",
        &[(-d, fl(0.0)), (-0.0001, fl(80.0))],
    );
}

fn impact_shake(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.5);
    let keys = times(c, &[(0.0, fl(60.0)), (d, fl(0.0))]);
    let start = c.to_source(c.start);
    if let Some(comp) = component(c, ids, "op.video.camera_shake") {
        for (k, v) in [("frequency", 14.0), ("rotation", 3.0), ("zoom", 110.0)] {
            if let Some(p) = comp.param_mut(k) {
                p.value = fl(v);
            }
        }
        animate(comp, "amplitude", &keys, start);
    }
}

fn handheld(c: &mut Clip, ids: &mut IdGen) {
    set(
        c,
        ids,
        "op.video.camera_shake",
        &[
            ("amplitude", fl(6.0)),
            ("frequency", fl(1.2)),
            ("rotation", fl(0.6)),
            ("zoom", fl(104.0)),
        ],
    );
}

fn glitch_burst(c: &mut Clip, ids: &mut IdGen) {
    let d = span(c, 0.4);
    anim(
        c,
        ids,
        "op.video.glitch",
        "intensity",
        &[(0.0, fl(100.0)), (d, fl(0.0))],
    );
}

fn dreamy_glow(c: &mut Clip, ids: &mut IdGen) {
    set(
        c,
        ids,
        "op.video.radiant_glow",
        &[
            ("radius", fl(220.0)),
            ("exposure", fl(-0.8)),
            ("threshold", fl(45.0)),
            ("falloff", fl(30.0)),
            ("tint", Value::Color(crate::Rgba::new(1.0, 0.85, 0.7, 1.0))),
            ("tint_amount", fl(35.0)),
        ],
    );
}

fn neon_glow(c: &mut Clip, ids: &mut IdGen) {
    set(
        c,
        ids,
        "op.video.radiant_glow",
        &[
            ("radius", fl(90.0)),
            ("exposure", fl(1.0)),
            ("threshold", fl(0.0)),
            ("falloff", fl(60.0)),
            ("aberration", fl(30.0)),
        ],
    );
}

fn cinematic(c: &mut Clip, ids: &mut IdGen) {
    set(
        c,
        ids,
        "op.video.vignette",
        &[("amount", fl(30.0)), ("feather", fl(70.0))],
    );
    set(c, ids, "op.video.letterbox", &[]);
}

fn retro_vhs(c: &mut Clip, ids: &mut IdGen) {
    set(c, ids, "op.video.vhs", &[]);
    set(c, ids, "op.video.film_grain", &[("amount", fl(15.0))]);
}

fn vintage_film(c: &mut Clip, ids: &mut IdGen) {
    set(c, ids, "op.video.old_film", &[]);
}

macro_rules! preset {
    ($id:expr, $name:expr, $cat:expr, $f:expr) => {
        PresetDef {
            id: $id,
            name: $name,
            category: $cat,
            apply: $f,
        }
    };
}

pub static PRESETS: &[PresetDef] = &[
    preset!(
        "op.preset.zoom_in",
        "Slow Zoom In",
        CAT_PRESET_ZOOM,
        zoom_in
    ),
    preset!(
        "op.preset.zoom_out",
        "Slow Zoom Out",
        CAT_PRESET_ZOOM,
        zoom_out
    ),
    preset!("op.preset.punch_in", "Punch In", CAT_PRESET_ZOOM, punch_in),
    preset!(
        "op.preset.punch_out",
        "Punch Out",
        CAT_PRESET_ZOOM,
        punch_out
    ),
    preset!("op.preset.pop_in", "Pop In", CAT_PRESET_ZOOM, pop_in),
    preset!("op.preset.pop_out", "Pop Out", CAT_PRESET_ZOOM, pop_out),
    preset!("op.preset.fade_in", "Fade In", CAT_PRESET_FADE, fade_in),
    preset!("op.preset.fade_out", "Fade Out", CAT_PRESET_FADE, fade_out),
    preset!(
        "op.preset.fade_in_out",
        "Fade In and Out",
        CAT_PRESET_FADE,
        fade_in_out
    ),
    preset!(
        "op.preset.slide_in_left",
        "Slide In from Left",
        CAT_PRESET_SLIDE,
        slide_in_left
    ),
    preset!(
        "op.preset.slide_in_right",
        "Slide In from Right",
        CAT_PRESET_SLIDE,
        slide_in_right
    ),
    preset!(
        "op.preset.slide_in_top",
        "Slide In from Top",
        CAT_PRESET_SLIDE,
        slide_in_top
    ),
    preset!(
        "op.preset.slide_in_bottom",
        "Slide In from Bottom",
        CAT_PRESET_SLIDE,
        slide_in_bottom
    ),
    preset!(
        "op.preset.slide_out_left",
        "Slide Out to Left",
        CAT_PRESET_SLIDE,
        slide_out_left
    ),
    preset!(
        "op.preset.slide_out_right",
        "Slide Out to Right",
        CAT_PRESET_SLIDE,
        slide_out_right
    ),
    preset!("op.preset.spin_in", "Spin In", CAT_PRESET_SPIN, spin_in),
    preset!("op.preset.spin_out", "Spin Out", CAT_PRESET_SPIN, spin_out),
    preset!("op.preset.blur_in", "Blur In", CAT_PRESET_BLUR, blur_in),
    preset!("op.preset.blur_out", "Blur Out", CAT_PRESET_BLUR, blur_out),
    preset!(
        "op.preset.impact_shake",
        "Impact Shake",
        CAT_PRESET_SHAKE,
        impact_shake
    ),
    preset!(
        "op.preset.handheld",
        "Handheld Camera",
        CAT_PRESET_SHAKE,
        handheld
    ),
    preset!(
        "op.preset.glitch_burst",
        "Glitch Burst",
        CAT_PRESET_SHAKE,
        glitch_burst
    ),
    preset!(
        "op.preset.dreamy_glow",
        "Dreamy Glow",
        CAT_PRESET_LOOK,
        dreamy_glow
    ),
    preset!(
        "op.preset.neon_glow",
        "Neon Glow",
        CAT_PRESET_LOOK,
        neon_glow
    ),
    preset!(
        "op.preset.cinematic",
        "Cinematic Look",
        CAT_PRESET_LOOK,
        cinematic
    ),
    preset!(
        "op.preset.retro_vhs",
        "Retro VHS",
        CAT_PRESET_LOOK,
        retro_vhs
    ),
    preset!(
        "op.preset.vintage_film",
        "Vintage Film",
        CAT_PRESET_LOOK,
        vintage_film
    ),
];

pub fn find(id: &str) -> Option<&'static PresetDef> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Label;
    use crate::catalog::EffectKind;
    use crate::model::{ClipSource, TrackKind};
    use crate::time::Speed;

    fn clip(seconds: f64) -> Clip {
        let mut ids = IdGen::default();
        Clip {
            id: ids.clip(),
            name: "c".into(),
            kind: TrackKind::Video,
            source: ClipSource::Graphic,
            start: SeqTime::from_seconds(10.0),
            duration: Dur::from_seconds(seconds),
            source_in: SrcTime::ZERO,
            speed: Speed::NORMAL,
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: crate::model::default_components(EffectKind::VideoFixed, &mut ids),
            gain_db: 0.0,
            scale_to_frame: false,
            channels: None,
        }
    }

    #[test]
    fn every_preset_uses_known_effects() {
        let mut seen = std::collections::HashSet::new();
        for p in PRESETS {
            assert!(seen.insert(p.id), "duplicate {}", p.id);
            let mut c = clip(4.0);
            let mut ids = IdGen::default();
            ids.observe(1000);
            p.apply(&mut c, &mut ids);
            assert!(c.components.iter().all(|x| x.def().is_some()), "{}", p.id);
            let changed = c.components.len() > 2
                || c.components
                    .iter()
                    .any(|x| x.params.iter().any(|q| q.animated));
            assert!(changed, "{} changes nothing", p.id);
        }
    }

    #[test]
    fn fades_follow_the_clip() {
        let mut c = clip(4.0);
        let mut ids = IdGen::default();
        ids.observe(1000);
        find("op.preset.fade_in_out")
            .unwrap()
            .apply(&mut c, &mut ids);
        let op = c.component(OPACITY).unwrap();
        let at = |s: f64| op.f64_at("opacity", c.to_source(SeqTime::from_seconds(s)));
        assert!(at(10.0) < 1.0);
        assert!((at(12.0) - 100.0).abs() < 1e-6);
        assert!(at(13.999) < 5.0);
        // a short clip gets shorter fades
        let mut short = clip(0.4);
        find("op.preset.fade_in")
            .unwrap()
            .apply(&mut short, &mut ids);
        let p = short.component(OPACITY).unwrap().param("opacity").unwrap();
        assert_eq!(p.keys.len(), 2);
        assert!(p.keys[1].time <= short.to_source(short.end()));
    }
}
