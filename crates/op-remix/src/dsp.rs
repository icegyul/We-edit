//! Signal processing for the analysis: a short-time Fourier transform of 22.05 kHz mono sound and
//! from it, per frame, a chroma vector (how much of each of the 12 notes sounds), the loudness
//! (A-weighted, in dB) and a 128-band mel spectrum in dB (for the beat tracker).
//!
//! The choices follow librosa (n_fft 2048, hop 512, Slaney mel bands up to 8 kHz, chroma
//! weighted around 880 Hz), as PyMusicLooper uses them.

use std::sync::atomic::{AtomicBool, Ordering};

use realfft::RealFftPlanner;

/// Analysis sample rate.
pub const RATE: usize = 22_050;
pub const N_FFT: usize = 2048;
/// Samples between frames (23 ms).
pub const HOP: usize = 512;
const BINS: usize = N_FFT / 2 + 1;
pub const MELS: usize = 128;

/// The analysis was cancelled.
#[derive(Debug)]
pub struct Cancelled;

/// What is measured in every frame.
pub struct Features {
    /// Note content, each frame scaled so its strongest note is 1.
    pub chroma: Vec<[f32; 12]>,
    /// A-weighted loudness in dB (only differences matter).
    pub loud: Vec<f32>,
    /// Mel bands in dB, `MELS` values per frame, relative to the loudest value of the sound.
    pub mel_db: Vec<f32>,
    pub frames: usize,
}

fn hann() -> Vec<f32> {
    (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
        .collect()
}

/// A-weighting of frequency `f` in dB (IEC 61672).
fn a_weight_db(f: f32) -> f32 {
    let f2 = f * f;
    let ra = 12194.0f32.powi(2) * f2 * f2
        / ((f2 + 20.6f32.powi(2)) * ((f2 + 107.7f32.powi(2)) * (f2 + 737.9f32.powi(2))).sqrt() * (f2 + 12194.0f32.powi(2)));
    20.0 * ra.max(1e-12).log10() + 2.0
}

/// Slaney mel scale.
fn hz_to_mel(f: f32) -> f32 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f32).ln() / 27.0;
    if f >= min_log_hz { min_log_mel + (f / min_log_hz).ln() / logstep } else { f / f_sp }
}

fn mel_to_hz(m: f32) -> f32 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f32).ln() / 27.0;
    if m >= min_log_mel { min_log_hz * (logstep * (m - min_log_mel)).exp() } else { m * f_sp }
}

/// Triangular mel filters (area-normalised like Slaney's): (first bin, weights).
fn mel_filters() -> Vec<(usize, Vec<f32>)> {
    let fmax = 8000.0f32;
    let freqs: Vec<f32> = (0..BINS).map(|k| k as f32 * RATE as f32 / N_FFT as f32).collect();
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(fmax));
    let points: Vec<f32> = (0..MELS + 2).map(|i| mel_to_hz(lo + (hi - lo) * i as f32 / (MELS + 1) as f32)).collect();
    (0..MELS)
        .map(|m| {
            let (l, c, r) = (points[m], points[m + 1], points[m + 2]);
            let norm = 2.0 / (r - l);
            let mut first = None;
            let mut w = Vec::new();
            for (k, f) in freqs.iter().enumerate() {
                let v = if *f >= l && *f <= c {
                    (*f - l) / (c - l).max(1e-6)
                } else if *f > c && *f <= r {
                    (r - *f) / (r - c).max(1e-6)
                } else {
                    0.0
                };
                if v > 0.0 {
                    first.get_or_insert(k);
                    w.push(v * norm);
                } else if first.is_some() && *f > r {
                    break;
                }
            }
            (first.unwrap_or(0), w)
        })
        .collect()
}

/// For each FFT bin from 30 Hz to 8 kHz: its note (0 = C ... 11 = B) and its weight (a bell
/// around 880 Hz, two octaves wide).
fn chroma_map() -> Vec<(usize, usize, f32)> {
    (1..BINS)
        .filter_map(|k| {
            let f = k as f32 * RATE as f32 / N_FFT as f32;
            if !(30.0..=8000.0).contains(&f) {
                return None;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2();
            let class = (midi.round() as i32).rem_euclid(12) as usize;
            let octs = (f / 27.5).log2();
            let w = (-0.5 * ((octs - 5.0) / 2.0).powi(2)).exp();
            Some((k, class, w))
        })
        .collect()
}

/// Features of `pcm` (mono, `RATE` Hz).
pub fn features(pcm: &[f32], progress: &mut dyn FnMut(f32), cancel: &AtomicBool) -> Result<Features, Cancelled> {
    let frames = 1 + pcm.len() / HOP;
    // the signal is mirrored at both ends so every frame is centred on its sample
    let pad = N_FFT / 2;
    let at = |i: isize| -> f32 {
        let n = pcm.len() as isize;
        let mut j = i;
        if j < 0 {
            j = -j;
        }
        if j >= n {
            j = 2 * (n - 1) - j;
        }
        if n == 0 { 0.0 } else { pcm[j.clamp(0, n - 1) as usize] }
    };
    let window = hann();
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let aw: Vec<f32> = (0..BINS).map(|k| 10f32.powf(a_weight_db(k as f32 * RATE as f32 / N_FFT as f32) / 10.0)).collect();
    let cmap = chroma_map();
    let mels = mel_filters();
    let mut chroma = Vec::with_capacity(frames);
    let mut loud = Vec::with_capacity(frames);
    let mut mel_db = vec![0f32; frames * MELS];
    let mut power = vec![0f32; BINS];
    for f in 0..frames {
        if f % 256 == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(Cancelled);
            }
            progress(f as f32 / frames as f32);
        }
        let centre = (f * HOP) as isize;
        for (i, v) in input.iter_mut().enumerate() {
            *v = at(centre - pad as isize + i as isize) * window[i];
        }
        fft.process(&mut input, &mut spectrum).expect("matching sizes");
        for (k, c) in spectrum.iter().enumerate() {
            power[k] = c.norm_sqr();
        }
        let weighted: f32 = power.iter().zip(&aw).map(|(p, w)| p * w).sum();
        loud.push(10.0 * (weighted / (N_FFT * N_FFT) as f32 + 1e-12).log10());
        let mut c = [0f32; 12];
        for &(k, class, w) in &cmap {
            c[class] += power[k] * w;
        }
        let top = c.iter().copied().fold(0.0f32, f32::max);
        if top > 1e-9 {
            for v in &mut c {
                *v /= top;
            }
        }
        chroma.push(c);
        for (m, (first, w)) in mels.iter().enumerate() {
            let s: f32 = w.iter().enumerate().map(|(i, wt)| wt * power[first + i]).sum();
            mel_db[f * MELS + m] = 10.0 * s.max(1e-10).log10();
        }
    }
    // relative to the loudest band of the whole sound, never more than 80 dB below it
    let top = mel_db.iter().copied().fold(f32::MIN, f32::max);
    for v in &mut mel_db {
        *v = (*v - top).max(-80.0);
    }
    progress(1.0);
    Ok(Features { chroma, loud, mel_db, frames })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize).map(|i| (i as f32 / RATE as f32 * freq * std::f32::consts::TAU).sin() * 0.5).collect()
    }

    #[test]
    fn a_pure_note_lights_one_chroma_bin() {
        let f = features(&tone(440.0, 1.0), &mut |_| {}, &AtomicBool::new(false)).unwrap();
        let c = f.chroma[f.frames / 2];
        // 440 Hz is A: index 9
        let best = c.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(best, 9, "{c:?}");
        assert!((c[9] - 1.0).abs() < 1e-6);
        assert!(c.iter().enumerate().filter(|(i, _)| *i != 9).all(|(_, v)| *v < 0.3), "{c:?}");
    }

    #[test]
    fn loudness_follows_the_level() {
        let quiet: Vec<f32> = tone(1000.0, 1.0).iter().map(|v| v * 0.1).collect();
        let loud: Vec<f32> = tone(1000.0, 1.0).iter().map(|v| v * 1.0).collect();
        let (a, b) = (features(&quiet, &mut |_| {}, &AtomicBool::new(false)).unwrap(), features(&loud, &mut |_| {}, &AtomicBool::new(false)).unwrap());
        let diff = b.loud[b.frames / 2] - a.loud[a.frames / 2];
        assert!((diff - 20.0).abs() < 0.5, "{diff}");
    }
}
