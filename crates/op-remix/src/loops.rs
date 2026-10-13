//! Where music repeats: pairs of moments (a "loop start" and a "loop end", both on beats) where
//! the sound is nearly the same note for note, as loud, and keeps matching for several beats
//! before and after. Jumping from the end back to the start (to repeat the stretch) or from the
//! start forward to the end (to skip it) is then hard to hear.
//!
//! The method is the one of PyMusicLooper (arkrow, MIT license): candidates are beat pairs whose
//! chroma vectors are close, they are scored by the weighted cosine similarity of the chroma of
//! the following (or preceding) twelve beats. Added here: the loop end is moved a few
//! milliseconds so that the waveforms join in phase.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::beats;
use crate::dsp::{self, Cancelled, Features, HOP, RATE};

/// Everything measured about a piece of music.
pub struct Analysis {
    pub features: Features,
    /// Beats per minute.
    pub bpm: f32,
    /// Beat period in frames.
    pub period: f32,
    /// Beat frames.
    pub beats: Vec<usize>,
    /// The sound, mono at `RATE` Hz.
    pub pcm: Arc<Vec<f32>>,
    pub seconds: f64,
}

/// A place where the music can jump: from `end` back to `start` (repeat) or from `start`
/// forward to `end` (skip). Seconds from the start of the analysed sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loop {
    pub start: f64,
    pub end: f64,
    /// 0..1, higher joins better.
    pub score: f32,
}

impl Loop {
    pub fn length(&self) -> f64 {
        self.end - self.start
    }
}

#[derive(Debug, PartialEq)]
pub enum AnalyzeError {
    Cancelled,
    /// Fewer than ten seconds: too short to find repeats in.
    TooShort,
}

impl From<Cancelled> for AnalyzeError {
    fn from(_: Cancelled) -> Self {
        AnalyzeError::Cancelled
    }
}

/// Measures `pcm` (mono, `RATE` Hz): notes, loudness, tempo and beats.
pub fn analyze(pcm: Vec<f32>, progress: &mut dyn FnMut(f32), cancel: &AtomicBool) -> Result<Analysis, AnalyzeError> {
    let seconds = pcm.len() as f64 / RATE as f64;
    if seconds < 10.0 {
        return Err(AnalyzeError::TooShort);
    }
    let features = dsp::features(&pcm, &mut |f| progress(f * 0.9), cancel)?;
    let env = beats::onset_envelope(&features);
    let (period, bpm) = beats::tempo(&env).unwrap_or((beats::fps() * 0.5, 120.0));
    let mut found = beats::track(&env, period, 100.0);
    if found.len() < 8 {
        // no clear beat (ambient music): a grid at the tempo's period
        found = (0..features.frames).step_by(period.round().max(1.0) as usize).collect();
    }
    progress(1.0);
    Ok(Analysis { features, bpm, period, beats: found, pcm: Arc::new(pcm), seconds })
}

fn norm(a: &[f32; 12]) -> f32 {
    a.iter().map(|v| v * v).sum::<f32>().sqrt()
}

fn distance(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

fn cosine(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot / (norm(a) * norm(b)).max(1e-10)
}

/// Weighted mean cosine similarity of the chroma following (`forward`) or leading up to the
/// two frames, over `len` frames, near frames counting most.
fn sequence_similarity(chroma: &[[f32; 12]], a: usize, b: usize, len: usize, forward: bool, weights: &[f32]) -> f32 {
    let (mut sum, mut wsum) = (0.0, 0.0);
    for k in 0..len {
        let w = weights[if forward { k } else { len - 1 - k }];
        wsum += w;
        let (i, j) = if forward { (a + k, b + k) } else { (a.wrapping_sub(len - k), b.wrapping_sub(len - k)) };
        if i < chroma.len() && j < chroma.len() {
            sum += w * cosine(&chroma[i], &chroma[j]);
        }
    }
    sum / wsum.max(1e-9)
}

/// Weights falling geometrically from `first` to 1 over `len` values.
fn weights(len: usize, first: f32) -> Vec<f32> {
    if len <= 1 {
        return vec![1.0; len];
    }
    let ratio = (1.0 / first).powf(1.0 / (len - 1) as f32);
    (0..len).map(|i| first * ratio.powi(i as i32)).collect()
}

/// Moves `end` (a sample) within 20 ms so that the sound just before and after it matches the
/// sound around `start`; returns the new end.
fn align_end(pcm: &[f32], start: usize, end: usize) -> usize {
    const W: usize = 330; // 15 ms each side
    const R: isize = 441; // 20 ms
    if start < W || end < W + R as usize || end + W + R as usize >= pcm.len() || start + W >= pcm.len() {
        return end;
    }
    let a = &pcm[start - W..start + W];
    let na = a.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
    let mut best = (f32::MIN, 0isize);
    for d in -R..=R {
        let e = (end as isize + d) as usize;
        let b = &pcm[e - W..e + W];
        let (mut dot, mut nb) = (0.0f32, 0.0f32);
        for (x, y) in a.iter().zip(b) {
            dot += x * y;
            nb += y * y;
        }
        let v = dot / (na * nb.sqrt().max(1e-9));
        if v > best.0 {
            best = (v, d);
        }
    }
    (end as isize + best.1) as usize
}

impl Analysis {
    /// The best places to jump between, at least `min_len` and at most `max_len` seconds apart,
    /// best first (at most `limit`).
    pub fn loops(&self, min_len: f64, max_len: f64, limit: usize) -> Vec<Loop> {
        let f = &self.features;
        let fps = beats::fps() as f64;
        let (min_f, max_f) = ((min_len * fps).ceil() as usize, (max_len * fps) as usize);
        // beats and the points half-way between them
        let mut cand: Vec<usize> = Vec::with_capacity(self.beats.len() * 2);
        for w in self.beats.windows(2) {
            cand.push(w[0]);
            cand.push((w[0] + w[1]) / 2);
        }
        if let Some(l) = self.beats.last() {
            cand.push(*l);
        }
        cand.dedup();
        // loudness over three frames, so one hit does not decide
        let loud: Vec<f32> = (0..f.frames)
            .map(|i| {
                let (a, b) = (i.saturating_sub(1), (i + 1).min(f.frames - 1));
                f.loud[a..=b].iter().sum::<f32>() / (b - a + 1) as f32
            })
            .collect();
        let test = ((12.0 * self.period) as usize).clamp(8, f.frames / 4).max(1);
        let w = weights(test, (test / 12).max(2) as f32);
        let mut found: Vec<(usize, usize, f32)> = Vec::new();
        for (ie, &e) in cand.iter().enumerate() {
            let deviation = norm(&f.chroma[e]) * 0.0875;
            for &s in &cand[..ie] {
                let len = e - s;
                if len < min_f {
                    break;
                }
                if len > max_f {
                    continue;
                }
                if distance(&f.chroma[e], &f.chroma[s]) > deviation || (loud[e] - loud[s]).abs() > 1.5 {
                    continue;
                }
                let ahead = sequence_similarity(&f.chroma, s, e, test, true, &w);
                let behind = sequence_similarity(&f.chroma, s, e, test, false, &w);
                found.push((s, e, ahead.max(behind)));
            }
        }
        found.sort_by(|a, b| b.2.total_cmp(&a.2));
        found.truncate(limit);
        found
            .into_iter()
            .map(|(s, e, score)| {
                let start = s * HOP;
                let end = align_end(&self.pcm, start, e * HOP);
                Loop { start: start as f64 / RATE as f64, end: end as f64 / RATE as f64, score }
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Music-like test sound: `bars` bars of 2 s at 120 bpm. Each bar has a kick on the beats and
    /// a chord from `chords[bar % chords.len()]` (note frequencies in Hz).
    pub fn music(bars: usize, chords: &[&[f32]]) -> Vec<f32> {
        let bar = RATE * 2;
        let mut out = vec![0f32; bars * bar];
        for b in 0..bars {
            let chord = chords[b % chords.len()];
            for i in 0..bar {
                let t = i as f32 / RATE as f32;
                let mut v = 0.0;
                for f in chord {
                    v += (t * f * std::f32::consts::TAU).sin() * 0.12 * (1.0 - (t % 0.5) * 0.6);
                }
                let into = i % (RATE / 2);
                if into < 2000 {
                    // a kick: a falling sine
                    let k = into as f32 / RATE as f32;
                    v += (k * (150.0 - 700.0 * k) * std::f32::consts::TAU).sin() * 0.6 * (1.0 - into as f32 / 2000.0);
                }
                out[b * bar + i] = v;
            }
        }
        out
    }

    pub const A: &[f32] = &[220.0, 277.18, 329.63];
    pub const D: &[f32] = &[146.83, 220.0, 293.66, 369.99];
    pub const E: &[f32] = &[164.81, 207.65, 246.94, 329.63];
    pub const F: &[f32] = &[174.61, 220.0, 261.63, 349.23];

    #[test]
    fn repeats_are_found_in_music_that_repeats() {
        // four bars of chords, played five times: any whole number of 4-bar phrases joins
        let pcm = music(20, &[A, D, E, F]);
        let a = analyze(pcm, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert!((a.bpm - 120.0).abs() < 4.0, "{}", a.bpm);
        let loops = a.loops(4.0, 40.0, 50);
        assert!(!loops.is_empty());
        let best = loops[0];
        // the loop is a whole number of bars (2 s) and, being the same music, scores high
        let bars = best.length() / 2.0;
        assert!((bars - bars.round()).abs() < 0.03, "{best:?}");
        assert!(best.score > 0.9, "{best:?}");
        // it is a whole number of 4-bar phrases at the tempo the chords repeat (8 s) or one of
        // its multiples; a bar-length loop between different chords is not accepted
        let phrase = best.length() / 8.0;
        assert!((phrase - phrase.round()).abs() < 0.03, "{best:?}");
    }

    #[test]
    fn short_sound_is_refused() {
        let pcm = vec![0.0; RATE * 5];
        assert_eq!(analyze(pcm, &mut |_| {}, &AtomicBool::new(false)).err(), Some(AnalyzeError::TooShort));
    }
}
