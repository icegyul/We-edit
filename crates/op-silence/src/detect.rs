//! Finding the silences: which moments are sound, which stretches are long enough to cut, and
//! how much air to leave around the words.

use crate::analysis::{Analysis, HOP};

/// A stretch of time, in seconds from the start of the analysed audio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub start: f64,
    pub end: f64,
}

impl Range {
    pub fn len(&self) -> f64 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// How a moment is told apart from silence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Method {
    /// Quieter than a fixed level, in dBFS (the classic threshold: auto-editor, unsilence...).
    Loudness { threshold_db: f32 },
    /// Quieter than the recording's loud end by this many dB; follows the recording's level.
    Relative { below_peak_db: f32 },
    /// Within this many dB of the recording's own noise floor (at most 55 dB under its loud
    /// parts): no level to guess.
    Adaptive { margin_db: f32 },
    /// Voice-band energy that is not steady noise, 0..1 (higher cuts less speech).
    Speech { sensitivity: f32 },
    /// A small neural voice detector, 0..1: ignores music, hum and room noise.
    Neural { sensitivity: f32 },
}

impl Method {
    pub const NAMES: [&'static str; 5] = [
        "Loudness",
        "Relative to Loudest",
        "Adaptive Noise Floor",
        "Speech Band",
        "Voice Detector (AI)",
    ];

    pub fn index(&self) -> usize {
        match self {
            Method::Loudness { .. } => 0,
            Method::Relative { .. } => 1,
            Method::Adaptive { .. } => 2,
            Method::Speech { .. } => 3,
            Method::Neural { .. } => 4,
        }
    }

    pub fn default_for(index: usize) -> Method {
        match index {
            0 => Method::Loudness {
                threshold_db: -40.0,
            },
            1 => Method::Relative {
                below_peak_db: 30.0,
            },
            2 => Method::Adaptive { margin_db: 9.0 },
            3 => Method::Speech { sensitivity: 0.5 },
            _ => Method::Neural { sensitivity: 0.5 },
        }
    }
}

/// What to look for and how much to leave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub method: Method,
    /// Shorter silences stay (seconds).
    pub min_silence: f64,
    /// Shorter sounds count as silence: clicks and breaths (seconds).
    pub min_sound: f64,
    /// Silence kept after the sound before it, and before the sound after it (seconds).
    pub pad_before: f64,
    pub pad_after: f64,
    /// Count only what the neural detector hears as a voice (music and noise are silence).
    pub voice_only: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            method: Method::Adaptive { margin_db: 9.0 },
            min_silence: 0.4,
            min_sound: 0.08,
            pad_before: 0.08,
            pad_after: 0.12,
            voice_only: false,
        }
    }
}

/// Smallest cut worth making, in seconds.
const MIN_CUT: f64 = 0.04;

/// Which moments are sound under `p`.
pub fn activity(a: &Analysis, p: &Params) -> Vec<bool> {
    let n = a.db.len();
    let mut active: Vec<bool> = match p.method {
        Method::Loudness { threshold_db } => a.db.iter().map(|d| *d >= threshold_db).collect(),
        Method::Relative { below_peak_db } => {
            let t = a.peak_db() - below_peak_db;
            a.db.iter().map(|d| *d >= t).collect()
        }
        Method::Adaptive { margin_db } => {
            // noise gates and noise suppression leave stretches of digital silence far below any
            // real room tone: the floor never counts as lower than 55 dB under the loud parts
            let floor = a.noise_floor_db().max(a.peak_db() - 55.0);
            let t = floor + margin_db;
            a.db.iter().map(|d| *d >= t).collect()
        }
        Method::Speech { sensitivity } => {
            let t = 0.75 - 0.6 * sensitivity.clamp(0.0, 1.0);
            a.speech.iter().map(|s| *s >= t).collect()
        }
        Method::Neural { sensitivity } => {
            let t = 0.8 - 0.6 * sensitivity.clamp(0.0, 1.0);
            if a.has_neural() {
                a.neural.iter().map(|s| *s >= t).collect()
            } else {
                // without the detector's curve, the speech band stands in
                a.speech
                    .iter()
                    .map(|s| *s >= 0.75 - 0.6 * sensitivity.clamp(0.0, 1.0))
                    .collect()
            }
        }
    };
    if p.voice_only && a.has_neural() {
        for (x, v) in active.iter_mut().zip(&a.neural) {
            *x = *x && *v >= 0.5;
        }
    }
    active.resize(n, false);
    active
}

/// Runs of equal values as (value, first, one past last).
fn runs(v: &[bool]) -> Vec<(bool, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j < v.len() && v[j] == v[i] {
            j += 1;
        }
        out.push((v[i], i, j));
        i = j;
    }
    out
}

/// The silences to cut, in order, without overlaps.
pub fn detect(a: &Analysis, p: &Params) -> Vec<Range> {
    let mut active = activity(a, p);
    let frames = |s: f64| (s / HOP).round() as usize;
    let n = active.len();
    // short pauses are not silence: speech is full of them between syllables
    let min_silence = frames(p.min_silence).max(1);
    for (v, s, e) in runs(&active) {
        if !v && e - s < min_silence {
            active[s..e].fill(true);
        }
    }
    // what is left of sound shorter than `min_sound` is a click or a breath, not sound; the
    // silences around it join into one longer silence
    let min_sound = frames(p.min_sound);
    if min_sound > 1 {
        for (v, s, e) in runs(&active) {
            if v && e - s < min_sound {
                active[s..e].fill(false);
            }
        }
    }
    let mut out = Vec::new();
    for (v, s, e) in runs(&active) {
        if v {
            continue;
        }
        let start = if s == 0 {
            0.0
        } else {
            s as f64 * HOP + p.pad_before
        };
        let end = if e == n {
            e as f64 * HOP
        } else {
            e as f64 * HOP - p.pad_after
        };
        if end - start >= MIN_CUT {
            out.push(Range { start, end });
        }
    }
    out
}

/// Moves each edge of `r` to the quietest moment within `window` seconds of it (never past the
/// other edge), so a cut placed from approximate word times lands between sounds.
pub fn snap_to_quiet(r: Range, a: &Analysis, window: f64) -> Range {
    let quietest = |t: f64, lo: f64, hi: f64| -> f64 {
        let (f0, f1) = (
            ((t - window).max(lo) / HOP) as usize,
            (((t + window).min(hi)) / HOP) as usize,
        );
        let mut best = (f64::MAX, t);
        for f in f0..=f1.min(a.db.len().saturating_sub(1)) {
            let d = a.db[f] as f64 + 0.0001 * (f as f64 * HOP - t).abs();
            if d < best.0 {
                best = (d, f as f64 * HOP + HOP / 2.0);
            }
        }
        best.1
    };
    if a.db.is_empty() || r.len() <= 2.0 * HOP {
        return r;
    }
    let mid = (r.start + r.end) / 2.0;
    let start = quietest(r.start, 0.0, mid);
    let end = quietest(r.end, mid, a.duration());
    if end > start { Range { start, end } } else { r }
}

/// Total length of `ranges`.
pub fn total(ranges: &[Range]) -> f64 {
    ranges.iter().map(Range::len).sum()
}
