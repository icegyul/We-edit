//! Typed effect parameters and keyframe animation.
//!
//! Keyframe times are in the clip's source time domain (`SrcTime`), so trimming a clip does not
//! move its animation relative to the picture. Interpolation follows the temporal model users know
//! from professional editors: each keyframe has an incoming and an outgoing interpolation, and
//! Bezier segments are shaped by a speed (value units per second) and an influence (fraction of
//! the segment) on each side. Point values additionally follow a spatial path.
//!
//! The Bezier and Auto Bezier shapes are provisional until measured against the reference
//! application (docs/timeline-behavior.md, GATE-FMT-004).

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::time::SrcTime;

/// A parameter value (DM-FX-001: typed, ordered, never silently coerced).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A 2D point. Coordinates are normalized to the frame the parameter refers to.
    Point([f64; 2]),
    Color(Rgba),
    Choice(u32),
    Text(String),
    /// A tone curve: control points in 0..1, sorted by x.
    Curve(Vec<[f32; 2]>),
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Bool(b) => *b as i32 as f64,
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            Value::Choice(c) => *c as f64,
            _ => 0.0,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            v => v.as_f64() != 0.0,
        }
    }

    pub fn as_point(&self) -> [f64; 2] {
        match self {
            Value::Point(p) => *p,
            v => [v.as_f64(), v.as_f64()],
        }
    }

    pub fn as_color(&self) -> Rgba {
        match self {
            Value::Color(c) => *c,
            _ => Rgba::WHITE,
        }
    }

    pub fn as_choice(&self) -> u32 {
        match self {
            Value::Choice(c) => *c,
            v => v.as_f64().max(0.0) as u32,
        }
    }

    pub fn as_text(&self) -> &str {
        match self {
            Value::Text(t) => t,
            _ => "",
        }
    }

    pub fn as_curve(&self) -> &[[f32; 2]] {
        match self {
            Value::Curve(c) => c,
            _ => &[],
        }
    }

    /// Whether values of this type change smoothly between keyframes.
    pub fn interpolates(&self) -> bool {
        matches!(
            self,
            Value::Int(_) | Value::Float(_) | Value::Point(_) | Value::Color(_)
        )
    }

    fn components(&self) -> Vec<f64> {
        match self {
            Value::Int(i) => vec![*i as f64],
            Value::Float(f) => vec![*f],
            Value::Point(p) => p.to_vec(),
            Value::Color(c) => vec![c.r as f64, c.g as f64, c.b as f64, c.a as f64],
            _ => vec![],
        }
    }

    fn with_components(&self, v: &[f64]) -> Value {
        match self {
            Value::Int(_) => Value::Int(v[0].round() as i64),
            Value::Float(_) => Value::Float(v[0]),
            Value::Point(_) => Value::Point([v[0], v[1]]),
            Value::Color(_) => Value::Color(Rgba::new(
                v[0] as f32,
                v[1] as f32,
                v[2] as f32,
                v[3] as f32,
            )),
            other => other.clone(),
        }
    }
}

/// Temporal interpolation on one side of a keyframe.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Interp {
    #[default]
    Linear,
    Bezier,
    AutoBezier,
    ContinuousBezier,
    Hold,
}

/// Spatial interpolation for point parameters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum SpatialInterp {
    Linear,
    #[default]
    AutoBezier,
    Bezier,
}

/// Speed (value units per second) and influence (0..1 of the segment) on one side.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Ease {
    pub speed: f64,
    pub influence: f64,
}

impl Default for Ease {
    fn default() -> Self {
        Ease {
            speed: 0.0,
            influence: 1.0 / 3.0,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: SrcTime,
    pub value: Value,
    #[serde(default)]
    pub interp_in: Interp,
    #[serde(default)]
    pub interp_out: Interp,
    #[serde(default)]
    pub ease_in: Ease,
    #[serde(default)]
    pub ease_out: Ease,
    #[serde(default)]
    pub spatial: SpatialInterp,
    /// Incoming/outgoing spatial tangents (offsets from the point) for `SpatialInterp::Bezier`.
    #[serde(default)]
    pub tangent_in: [f64; 2],
    #[serde(default)]
    pub tangent_out: [f64; 2],
}

impl Keyframe {
    pub fn new(time: SrcTime, value: Value, interp: Interp) -> Keyframe {
        Keyframe {
            time,
            value,
            interp_in: interp,
            interp_out: interp,
            ease_in: Ease::default(),
            ease_out: Ease::default(),
            spatial: SpatialInterp::AutoBezier,
            tangent_in: [0.0; 2],
            tangent_out: [0.0; 2],
        }
    }

    /// Applies one of the temporal interpolation menu choices.
    pub fn set_interp(&mut self, choice: InterpChoice) {
        match choice {
            InterpChoice::Linear => {
                self.interp_in = Interp::Linear;
                self.interp_out = Interp::Linear;
            }
            InterpChoice::Bezier => {
                self.interp_in = Interp::Bezier;
                self.interp_out = Interp::Bezier;
            }
            InterpChoice::AutoBezier => {
                self.interp_in = Interp::AutoBezier;
                self.interp_out = Interp::AutoBezier;
            }
            InterpChoice::ContinuousBezier => {
                self.interp_in = Interp::ContinuousBezier;
                self.interp_out = Interp::ContinuousBezier;
            }
            InterpChoice::Hold => {
                self.interp_out = Interp::Hold;
            }
            InterpChoice::EaseIn => {
                self.interp_in = Interp::Bezier;
                self.ease_in = Ease {
                    speed: 0.0,
                    influence: 1.0 / 3.0,
                };
            }
            InterpChoice::EaseOut => {
                self.interp_out = Interp::Bezier;
                self.ease_out = Ease {
                    speed: 0.0,
                    influence: 1.0 / 3.0,
                };
            }
        }
    }
}

/// Entries of the keyframe interpolation menu.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InterpChoice {
    Linear,
    Bezier,
    AutoBezier,
    ContinuousBezier,
    Hold,
    EaseIn,
    EaseOut,
}

impl InterpChoice {
    pub const ALL: [InterpChoice; 7] = [
        InterpChoice::Linear,
        InterpChoice::Bezier,
        InterpChoice::AutoBezier,
        InterpChoice::ContinuousBezier,
        InterpChoice::Hold,
        InterpChoice::EaseIn,
        InterpChoice::EaseOut,
    ];

    pub fn key(self) -> &'static str {
        match self {
            InterpChoice::Linear => "interp.linear",
            InterpChoice::Bezier => "interp.bezier",
            InterpChoice::AutoBezier => "interp.auto_bezier",
            InterpChoice::ContinuousBezier => "interp.continuous_bezier",
            InterpChoice::Hold => "interp.hold",
            InterpChoice::EaseIn => "interp.ease_in",
            InterpChoice::EaseOut => "interp.ease_out",
        }
    }
}

/// One parameter of a component instance. When `animated` is false the keyframes are ignored and
/// `value` is used; both are kept (DM-FX-004).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Param {
    pub key: String,
    pub value: Value,
    #[serde(default)]
    pub animated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<Keyframe>,
}

impl Param {
    pub fn new(key: impl Into<String>, value: Value) -> Param {
        Param {
            key: key.into(),
            value,
            animated: false,
            keys: Vec::new(),
        }
    }

    pub fn is_animated(&self) -> bool {
        self.animated && !self.keys.is_empty()
    }

    /// Value at a source time.
    pub fn value_at(&self, t: SrcTime) -> Value {
        if !self.is_animated() {
            return self.value.clone();
        }
        evaluate(&self.keys, t)
    }

    /// Index of a keyframe exactly at `t`.
    pub fn key_at(&self, t: SrcTime) -> Option<usize> {
        self.keys.iter().position(|k| k.time == t)
    }

    /// Sets the value at `t`: adds or updates a keyframe when animated, otherwise sets `value`.
    pub fn set_at(&mut self, t: SrcTime, v: Value, default_interp: Interp) {
        if self.animated {
            match self.keys.iter().position(|k| k.time == t) {
                Some(i) => self.keys[i].value = v,
                None => {
                    let pos = self.keys.partition_point(|k| k.time < t);
                    self.keys.insert(pos, Keyframe::new(t, v, default_interp));
                }
            }
        } else {
            self.value = v;
        }
    }

    /// Turns animation on (adding a keyframe with the current value at `t`) or off (keeping the
    /// value at `t` and removing every keyframe).
    pub fn toggle_animation(&mut self, t: SrcTime, default_interp: Interp) {
        if self.animated {
            self.value = self.value_at(t);
            self.animated = false;
            self.keys.clear();
        } else {
            self.animated = true;
            self.keys = vec![Keyframe::new(t, self.value.clone(), default_interp)];
        }
    }

    pub fn sort_keys(&mut self) {
        self.keys.sort_by_key(|k| k.time);
        self.keys.dedup_by_key(|k| k.time);
    }
}

/// Evaluates a sorted keyframe list.
pub fn evaluate(keys: &[Keyframe], t: SrcTime) -> Value {
    let Some(first) = keys.first() else {
        return Value::Float(0.0);
    };
    if t <= first.time || keys.len() == 1 {
        return first.value.clone();
    }
    let last = keys.last().unwrap();
    if t >= last.time {
        return last.value.clone();
    }
    let i = keys.partition_point(|k| k.time <= t) - 1;
    let (a, b) = (&keys[i], &keys[i + 1]);
    if a.interp_out == Interp::Hold || !a.value.interpolates() {
        return a.value.clone();
    }
    let span = (b.time - a.time).seconds();
    if span <= 0.0 {
        return b.value.clone();
    }
    let x = (t - a.time).seconds() / span;
    let prev = i.checked_sub(1).map(|j| &keys[j]);
    let next = keys.get(i + 2);
    match (&a.value, &b.value) {
        (Value::Float(_) | Value::Int(_), Value::Float(_) | Value::Int(_)) => {
            let va = a.value.as_f64();
            let vb = b.value.as_f64();
            let (so, io) = out_tangent(prev, a, b, span);
            let (si, ii) = in_tangent(a, b, next, span);
            let v = bezier_value(x, va, vb, so * span, io, si * span, ii);
            a.value.with_components(&[v])
        }
        (Value::Point(pa), Value::Point(pb)) => {
            let u = eased_fraction(prev, a, b, next, span, x);
            let p = spatial_point(prev, a, b, next, *pa, *pb, u);
            Value::Point(p)
        }
        _ => {
            let u = eased_fraction(prev, a, b, next, span, x);
            let ca = a.value.components();
            let cb = b.value.components();
            if ca.len() != cb.len() {
                return a.value.clone();
            }
            let v: Vec<f64> = ca.iter().zip(&cb).map(|(p, q)| p + (q - p) * u).collect();
            a.value.with_components(&v)
        }
    }
}

fn linear_slope(a: &Keyframe, b: &Keyframe) -> f64 {
    let span = (b.time - a.time).seconds();
    if span <= 0.0 {
        0.0
    } else {
        (b.value.as_f64() - a.value.as_f64()) / span
    }
}

/// Automatic slope at `k` from its neighbors: the average slope, flattened at local extremes so
/// the curve never overshoots the keyed values.
fn auto_slope(prev: Option<&Keyframe>, k: &Keyframe, next: Option<&Keyframe>) -> f64 {
    match (prev, next) {
        (Some(p), Some(n)) => {
            let v = k.value.as_f64();
            let (vp, vn) = (p.value.as_f64(), n.value.as_f64());
            if (v - vp) * (vn - v) <= 0.0 {
                return 0.0;
            }
            let span = (n.time - p.time).seconds();
            if span <= 0.0 { 0.0 } else { (vn - vp) / span }
        }
        (Some(p), None) => linear_slope(p, k),
        (None, Some(n)) => linear_slope(k, n),
        (None, None) => 0.0,
    }
}

/// Outgoing slope (units/s) and influence of `a` towards `b`.
fn out_tangent(prev: Option<&Keyframe>, a: &Keyframe, b: &Keyframe, _span: f64) -> (f64, f64) {
    match a.interp_out {
        Interp::Linear | Interp::Hold => (linear_slope(a, b), 1.0 / 3.0),
        Interp::AutoBezier => (auto_slope(prev, a, Some(b)), 1.0 / 3.0),
        Interp::Bezier | Interp::ContinuousBezier => (a.ease_out.speed, a.ease_out.influence),
    }
}

/// Incoming slope and influence of `b` coming from `a`.
fn in_tangent(a: &Keyframe, b: &Keyframe, next: Option<&Keyframe>, _span: f64) -> (f64, f64) {
    match b.interp_in {
        Interp::Linear | Interp::Hold => (linear_slope(a, b), 1.0 / 3.0),
        Interp::AutoBezier => (auto_slope(Some(a), b, next), 1.0 / 3.0),
        Interp::Bezier | Interp::ContinuousBezier => (b.ease_in.speed, b.ease_in.influence),
    }
}

/// Value of a temporal Bezier segment at normalized time `x` (0..1).
/// `do_` and `di` are value changes per whole segment implied by the slopes; `io`/`ii` influences.
fn bezier_value(x: f64, va: f64, vb: f64, do_: f64, io: f64, di: f64, ii: f64) -> f64 {
    let (io, ii) = clamp_influences(io, ii);
    // control points in (time, value), time normalized to 0..1
    let (x1, y1) = (io, va + do_ * io);
    let (x2, y2) = (1.0 - ii, vb - di * ii);
    let s = solve_bezier_x(x, x1, x2);
    cubic(s, va, y1, y2, vb)
}

fn clamp_influences(io: f64, ii: f64) -> (f64, f64) {
    let io = io.clamp(0.01, 1.0);
    let ii = ii.clamp(0.01, 1.0);
    let sum = io + ii;
    if sum > 1.0 {
        (io / sum, ii / sum)
    } else {
        (io, ii)
    }
}

fn cubic(s: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let m = 1.0 - s;
    m * m * m * p0 + 3.0 * m * m * s * p1 + 3.0 * m * s * s * p2 + s * s * s * p3
}

fn cubic_deriv(s: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let m = 1.0 - s;
    3.0 * m * m * (p1 - p0) + 6.0 * m * s * (p2 - p1) + 3.0 * s * s * (p3 - p2)
}

/// Finds the curve parameter whose time coordinate is `x`. The time polynomial is monotonic
/// because both inner control points stay inside 0..1 in order.
fn solve_bezier_x(x: f64, x1: f64, x2: f64) -> f64 {
    let mut s = x;
    for _ in 0..8 {
        let f = cubic(s, 0.0, x1, x2, 1.0) - x;
        let d = cubic_deriv(s, 0.0, x1, x2, 1.0);
        if f.abs() < 1e-10 {
            return s;
        }
        if d.abs() < 1e-9 {
            break;
        }
        s = (s - f / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        s = 0.5 * (lo + hi);
        if cubic(s, 0.0, x1, x2, 1.0) < x {
            lo = s;
        } else {
            hi = s;
        }
    }
    s
}

/// Fraction of the way from `a` to `b` (0..1) at normalized time `x`, for multi-dimensional values:
/// the temporal curve works on distance traveled.
fn eased_fraction(
    prev: Option<&Keyframe>,
    a: &Keyframe,
    b: &Keyframe,
    next: Option<&Keyframe>,
    span: f64,
    x: f64,
) -> f64 {
    let len = distance(&a.value, &b.value).max(1e-12);
    let lin = 1.0; // fraction per segment for a linear side
    let side = |interp: Interp, ease: &Ease, auto: f64| -> (f64, f64) {
        match interp {
            Interp::Linear | Interp::Hold => (lin, 1.0 / 3.0),
            Interp::AutoBezier => (auto, 1.0 / 3.0),
            Interp::Bezier | Interp::ContinuousBezier => (ease.speed * span / len, ease.influence),
        }
    };
    // auto: keep constant speed through a keyframe using neighbor distances
    let auto_out = match prev {
        Some(p) => {
            let total = distance(&p.value, &a.value) + len;
            let t = (b.time - p.time).seconds();
            if t > 0.0 && total > 0.0 {
                (total / t) * span / len
            } else {
                lin
            }
        }
        None => lin,
    };
    let auto_in = match next {
        Some(n) => {
            let total = len + distance(&b.value, &n.value);
            let t = (n.time - a.time).seconds();
            if t > 0.0 && total > 0.0 {
                (total / t) * span / len
            } else {
                lin
            }
        }
        None => lin,
    };
    let (so, io) = side(a.interp_out, &a.ease_out, auto_out);
    let (si, ii) = side(b.interp_in, &b.ease_in, auto_in);
    bezier_value(x, 0.0, 1.0, so, io, si, ii).clamp(-0.5, 1.5)
}

fn distance(a: &Value, b: &Value) -> f64 {
    let ca = a.components();
    let cb = b.components();
    ca.iter()
        .zip(&cb)
        .map(|(p, q)| (q - p) * (q - p))
        .sum::<f64>()
        .sqrt()
}

/// Position along the spatial path from `pa` to `pb` at arc-length fraction `u`.
fn spatial_point(
    prev: Option<&Keyframe>,
    a: &Keyframe,
    b: &Keyframe,
    next: Option<&Keyframe>,
    pa: [f64; 2],
    pb: [f64; 2],
    u: f64,
) -> [f64; 2] {
    let lerp = |t: f64| [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
    let (c1, c2) = match (a.spatial, b.spatial) {
        (SpatialInterp::Linear, SpatialInterp::Linear) => return lerp(u),
        _ => {
            let out = match a.spatial {
                SpatialInterp::Linear => [0.0, 0.0],
                SpatialInterp::Bezier => a.tangent_out,
                SpatialInterp::AutoBezier => {
                    auto_tangent(prev.map(|k| k.value.as_point()), pa, Some(pb))
                }
            };
            let inn = match b.spatial {
                SpatialInterp::Linear => [0.0, 0.0],
                SpatialInterp::Bezier => b.tangent_in,
                SpatialInterp::AutoBezier => {
                    let t = auto_tangent(Some(pa), pb, next.map(|k| k.value.as_point()));
                    [-t[0], -t[1]]
                }
            };
            (
                [pa[0] + out[0], pa[1] + out[1]],
                [pb[0] + inn[0], pb[1] + inn[1]],
            )
        }
    };
    if (c1 == pa && c2 == pb) || u <= 0.0 || u >= 1.0 {
        return if u <= 0.0 {
            lerp(u.min(0.0))
        } else if u >= 1.0 {
            lerp(u.max(1.0))
        } else {
            lerp(u)
        };
    }
    // arc-length parameterization of the cubic path
    const N: usize = 48;
    let point = |s: f64| {
        [
            cubic(s, pa[0], c1[0], c2[0], pb[0]),
            cubic(s, pa[1], c1[1], c2[1], pb[1]),
        ]
    };
    let mut lengths = [0.0f64; N + 1];
    let mut last = pa;
    for (i, l) in lengths.iter_mut().enumerate().skip(1) {
        let p = point(i as f64 / N as f64);
        *l = ((p[0] - last[0]).powi(2) + (p[1] - last[1]).powi(2)).sqrt();
        last = p;
    }
    for i in 1..=N {
        lengths[i] += lengths[i - 1];
    }
    let total = lengths[N];
    if total <= 1e-12 {
        return pa;
    }
    let target = u * total;
    let i = lengths.partition_point(|l| *l < target).clamp(1, N);
    let seg = lengths[i] - lengths[i - 1];
    let f = if seg > 0.0 {
        (target - lengths[i - 1]) / seg
    } else {
        0.0
    };
    point((i as f64 - 1.0 + f) / N as f64)
}

/// Outgoing spatial handle at `p` (Catmull-Rom style, one third of the neighbor chord).
fn auto_tangent(prev: Option<[f64; 2]>, p: [f64; 2], next: Option<[f64; 2]>) -> [f64; 2] {
    match (prev, next) {
        (Some(a), Some(b)) => [(b[0] - a[0]) / 6.0, (b[1] - a[1]) / 6.0],
        (None, Some(b)) => [(b[0] - p[0]) / 3.0, (b[1] - p[1]) / 3.0],
        (Some(a), None) => [(p[0] - a[0]) / 3.0, (p[1] - a[1]) / 3.0],
        (None, None) => [0.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::SrcTime;

    fn t(s: f64) -> SrcTime {
        SrcTime::from_seconds(s)
    }

    fn param(keys: Vec<Keyframe>) -> Param {
        Param {
            key: "x".into(),
            value: Value::Float(0.0),
            animated: true,
            keys,
        }
    }

    #[test]
    fn linear_and_hold() {
        let p = param(vec![
            Keyframe::new(t(0.0), Value::Float(0.0), Interp::Linear),
            Keyframe::new(t(1.0), Value::Float(10.0), Interp::Linear),
        ]);
        assert!((p.value_at(t(0.25)).as_f64() - 2.5).abs() < 1e-9);
        assert_eq!(p.value_at(t(-1.0)).as_f64(), 0.0);
        assert_eq!(p.value_at(t(5.0)).as_f64(), 10.0);
        let mut h = p.clone();
        h.keys[0].interp_out = Interp::Hold;
        assert_eq!(h.value_at(t(0.99)).as_f64(), 0.0);
    }

    #[test]
    fn ease_in_out_is_symmetric_and_slow_at_ends() {
        let mut a = Keyframe::new(t(0.0), Value::Float(0.0), Interp::Linear);
        let mut b = Keyframe::new(t(1.0), Value::Float(100.0), Interp::Linear);
        a.set_interp(InterpChoice::EaseOut);
        b.set_interp(InterpChoice::EaseIn);
        let p = param(vec![a, b]);
        let mid = p.value_at(t(0.5)).as_f64();
        assert!((mid - 50.0).abs() < 1e-6, "{mid}");
        let early = p.value_at(t(0.05)).as_f64();
        assert!(
            early < 5.0 * 0.5,
            "ease must start slower than linear: {early}"
        );
        let late = p.value_at(t(0.95)).as_f64();
        assert!((100.0 - late - early).abs() < 1e-6);
    }

    #[test]
    fn auto_bezier_does_not_overshoot_extremes() {
        let p = param(vec![
            Keyframe::new(t(0.0), Value::Float(0.0), Interp::AutoBezier),
            Keyframe::new(t(1.0), Value::Float(10.0), Interp::AutoBezier),
            Keyframe::new(t(2.0), Value::Float(0.0), Interp::AutoBezier),
        ]);
        for i in 0..=200 {
            let v = p.value_at(t(i as f64 / 100.0)).as_f64();
            assert!((-1e-9..=10.0 + 1e-9).contains(&v), "{v}");
        }
    }

    #[test]
    fn points_follow_a_path_and_hit_keys() {
        let k = |s: f64, x: f64, y: f64| Keyframe::new(t(s), Value::Point([x, y]), Interp::Linear);
        let p = Param {
            key: "pos".into(),
            value: Value::Point([0.5, 0.5]),
            animated: true,
            keys: vec![k(0.0, 0.0, 0.0), k(1.0, 1.0, 0.0), k(2.0, 1.0, 1.0)],
        };
        assert_eq!(p.value_at(t(1.0)).as_point(), [1.0, 0.0]);
        // with auto-bezier spatial handles, the path bulges past the straight chord near the corner
        let mid = p.value_at(t(0.5)).as_point();
        assert!(mid[0] > 0.3 && mid[0] < 0.7, "{mid:?}");
        let mut lin = p.clone();
        for key in &mut lin.keys {
            key.spatial = SpatialInterp::Linear;
        }
        let m = lin.value_at(t(0.5)).as_point();
        assert!((m[0] - 0.5).abs() < 1e-9 && m[1].abs() < 1e-9);
    }

    #[test]
    fn toggling_animation_keeps_the_current_value() {
        let mut p = Param::new("x", Value::Float(3.0));
        p.toggle_animation(t(0.0), Interp::Linear);
        p.set_at(t(1.0), Value::Float(5.0), Interp::Linear);
        assert_eq!(p.keys.len(), 2);
        p.toggle_animation(t(0.5), Interp::Linear);
        assert!(!p.animated && p.keys.is_empty());
        assert!((p.value.as_f64() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn non_interpolating_values_hold() {
        let p = Param {
            key: "c".into(),
            value: Value::Choice(0),
            animated: true,
            keys: vec![
                Keyframe::new(t(0.0), Value::Choice(1), Interp::Linear),
                Keyframe::new(t(1.0), Value::Choice(2), Interp::Linear),
            ],
        };
        assert_eq!(p.value_at(t(0.9)), Value::Choice(1));
    }
}
