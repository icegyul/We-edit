//! Tempo and beats: an onset envelope (how much new sound starts in each frame), the tempo from
//! its autocorrelation, and Ellis's dynamic-programming beat tracker (the one librosa uses).

use crate::dsp::{Features, HOP, MELS, RATE};

/// Frames per second.
pub fn fps() -> f32 {
    RATE as f32 / HOP as f32
}

/// How much sound begins in each frame: the rise of every mel band since the frame before,
/// averaged over the bands.
pub fn onset_envelope(f: &Features) -> Vec<f32> {
    let mut env = vec![0f32; f.frames];
    for i in 1..f.frames {
        let (a, b) = (&f.mel_db[(i - 1) * MELS..i * MELS], &f.mel_db[i * MELS..(i + 1) * MELS]);
        env[i] = a.iter().zip(b).map(|(x, y)| (y - x).max(0.0)).sum::<f32>() / MELS as f32;
    }
    env
}

/// The beat period in frames (fractional), and the tempo in beats per minute. A log-normal
/// preference for 120 bpm picks the usual octave when a tempo and its double both fit.
pub fn tempo(env: &[f32]) -> Option<(f32, f32)> {
    if env.len() < 64 {
        return None;
    }
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    let x: Vec<f32> = env.iter().map(|v| v - mean).collect();
    let fps = fps();
    // 50 .. 240 bpm
    let (min_lag, max_lag) = ((fps * 60.0 / 240.0) as usize, ((fps * 60.0 / 50.0) as usize).min(env.len() / 2));
    if max_lag <= min_lag + 2 {
        return None;
    }
    let ac: Vec<f32> = (0..=max_lag + 1)
        .map(|lag| {
            let n = x.len() - lag;
            x[..n].iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>() / n as f32
        })
        .collect();
    let zero = ac[0].max(1e-9);
    let mut best = (min_lag, f32::MIN);
    for lag in min_lag..=max_lag {
        let bpm = 60.0 * fps / lag as f32;
        let prior = (-0.5 * (bpm / 120.0).log2().powi(2)).exp();
        let v = ac[lag] / zero * prior;
        if v > best.1 {
            best = (lag, v);
        }
    }
    let lag = best.0;
    // parabola through the peak and its neighbours
    let (a, b, c) = (ac[lag - 1], ac[lag], ac[lag + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-9 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    let period = lag as f32 + shift;
    Some((period, 60.0 * fps / period))
}

/// Beat frames: each beat is placed where onsets are strong and about one period after the one
/// before (`tightness` is how strictly the period is kept).
pub fn track(env: &[f32], period: f32, tightness: f32) -> Vec<usize> {
    let n = env.len();
    if n < 4 || period < 2.0 {
        return Vec::new();
    }
    let mean = env.iter().sum::<f32>() / n as f32;
    let std = (env.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / (n - 1) as f32).sqrt().max(1e-9);
    let normed: Vec<f32> = env.iter().map(|v| v / std).collect();
    // smooth with a Gaussian a thirty-second of a beat wide... as librosa: width period / 32 * 32
    let half = period.round() as isize;
    let kernel: Vec<f32> = (-half..=half).map(|i| (-0.5 * (i as f32 * 32.0 / period).powi(2)).exp()).collect();
    let mut local = vec![0f32; n];
    for (i, l) in local.iter_mut().enumerate() {
        let mut s = 0.0;
        for (j, k) in kernel.iter().enumerate() {
            let idx = i as isize + j as isize - half;
            if idx >= 0 && (idx as usize) < n {
                s += normed[idx as usize] * k;
            }
        }
        *l = s;
    }
    let top = local.iter().copied().fold(f32::MIN, f32::max);
    let (lo, hi) = (-(2.0 * period).round() as isize, -(period / 2.0).round() as isize);
    let offsets: Vec<isize> = (lo..=hi).collect();
    let penalty: Vec<f32> = offsets.iter().map(|o| -tightness * (-(*o as f32) / period).ln().powi(2)).collect();
    let mut cum = vec![0f32; n];
    let mut back = vec![-1isize; n];
    let mut first = true;
    for i in 0..n {
        let mut best: Option<(f32, isize)> = None;
        for (o, pen) in offsets.iter().zip(&penalty) {
            let j = i as isize + o;
            if j >= 0 {
                let v = cum[j as usize] + pen;
                if best.is_none_or(|b| v > b.0) {
                    best = Some((v, j));
                }
            }
        }
        match best {
            Some((v, j)) => {
                cum[i] = local[i] + v;
                if first && local[i] < 0.01 * top {
                    back[i] = -1;
                } else {
                    back[i] = j;
                    first = false;
                }
            }
            None => cum[i] = local[i],
        }
    }
    // the last beat: the strongest local maximum of the cumulative score that is not weak
    let maxima: Vec<usize> = (1..n - 1).filter(|&i| cum[i] > cum[i - 1] && cum[i] >= cum[i + 1]).collect();
    if maxima.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<f32> = maxima.iter().map(|&i| cum[i]).collect();
    sorted.sort_by(f32::total_cmp);
    let threshold = 0.5 * sorted[sorted.len() / 2];
    let Some(&tail) = maxima.iter().rev().find(|&&i| cum[i] >= threshold) else { return Vec::new() };
    let mut beats = vec![tail];
    let mut at = tail as isize;
    while back[at as usize] >= 0 {
        at = back[at as usize];
        beats.push(at as usize);
    }
    beats.reverse();
    // leading beats on silence do not count
    let cut = 0.5 * beats.iter().map(|&b| local[b]).sum::<f32>() / beats.len() as f32;
    while beats.len() > 2 && local[beats[0]] < cut * 0.5 {
        beats.remove(0);
    }
    while beats.len() > 2 && local[*beats.last().unwrap()] < cut * 0.5 {
        beats.pop();
    }
    beats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::features;
    use std::sync::atomic::AtomicBool;

    /// A click track: a short noise burst every `period` seconds.
    fn clicks(period: f32, seconds: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        let every = (RATE as f32 * period) as usize;
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                let into = i % every;
                if into < 400 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    ((seed >> 16) as f32 / 32768.0 - 1.0) * (1.0 - into as f32 / 400.0)
                } else {
                    0.0
                }
            })
            .collect()
    }

    #[test]
    fn the_tempo_and_beats_of_a_click_track_are_found() {
        let pcm = clicks(0.5, 20.0); // 120 bpm
        let f = features(&pcm, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        let env = onset_envelope(&f);
        let (period, bpm) = tempo(&env).expect("a tempo");
        assert!((bpm - 120.0).abs() < 3.0, "{bpm}");
        let beats = track(&env, period, 100.0);
        assert!(beats.len() > 30, "{}", beats.len());
        // every beat is about half a second after the one before
        let gaps: Vec<f32> = beats.windows(2).map(|w| (w[1] - w[0]) as f32 / fps()).collect();
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
        // and they sit on the clicks (a click every 0.5 s starting at 0)
        let off: Vec<f32> = beats.iter().map(|&b| ((b as f32 / fps()) / 0.5).fract()).map(|p| p.min(1.0 - p)).collect();
        assert!(off.iter().filter(|o| **o < 0.08).count() * 10 > off.len() * 9, "{off:?}");
    }
}
