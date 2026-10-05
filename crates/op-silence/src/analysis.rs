//! Measuring audio: loudness, speech-band energy and a neural voice detector, one value every
//! 10 ms. Everything the detectors need is computed once here, so changing a threshold in the
//! interface re-runs only the cheap detection on the stored curves.

use std::sync::atomic::{AtomicBool, Ordering};

/// Sample rate the analysis works at.
pub const RATE: u32 = 16_000;

/// Seconds between two values of the curves.
pub const HOP: f64 = 0.01;

const HOP_SAMPLES: usize = (RATE as f64 * HOP) as usize;
const WINDOW: usize = 320;
const FFT: usize = 512;

/// The curves of one stretch of audio.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// RMS level in dBFS (about -100 for digital silence).
    pub db: Vec<f32>,
    /// How much like speech each moment sounds, 0..1: voice-band energy over the recording's
    /// own noise floor, discounted when the spectrum is flat like noise.
    pub speech: Vec<f32>,
    /// The neural detector's voice probability, 0..1 (empty when it was not run).
    pub neural: Vec<f32>,
}

impl Analysis {
    pub fn duration(&self) -> f64 {
        self.db.len() as f64 * HOP
    }

    fn percentile(values: &[f32], q: f32) -> f32 {
        if values.is_empty() {
            return -100.0;
        }
        let mut v: Vec<f32> = values.to_vec();
        v.sort_by(f32::total_cmp);
        v[((v.len() - 1) as f32 * q.clamp(0.0, 1.0)) as usize]
    }

    /// The loud end of the recording (99th percentile, so a single click does not set it).
    pub fn peak_db(&self) -> f32 {
        Self::percentile(&self.db, 0.99)
    }

    /// The level of the quiet parts: the 10th percentile.
    pub fn noise_floor_db(&self) -> f32 {
        Self::percentile(&self.db, 0.10)
    }

    pub fn has_neural(&self) -> bool {
        !self.neural.is_empty()
    }
}

/// An in-place radix-2 FFT of `re`/`im` (length a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

fn to_db(power: f32) -> f32 {
    10.0 * power.max(1e-10).log10()
}

/// Cancellation: the caller set the flag.
#[derive(Debug)]
pub struct Cancelled;

/// Measures 16 kHz mono samples. `neural` also runs the neural voice detector (a small pure
/// Rust network, thousands of times faster than real time).
pub fn analyze(
    pcm: &[f32],
    neural: bool,
    progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<Analysis, Cancelled> {
    let frames = pcm.len().div_ceil(HOP_SAMPLES);
    let mut db = Vec::with_capacity(frames);
    let mut band_db = Vec::with_capacity(frames);
    let mut flatness = Vec::with_capacity(frames);
    let hann: Vec<f32> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT as f32).cos())
        .collect();
    let (mut re, mut im) = (vec![0f32; FFT], vec![0f32; FFT]);
    // 300 .. 3400 Hz in bins of 31.25 Hz
    let (lo, hi) = (10usize, 109usize);
    for f in 0..frames {
        if f % 512 == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(Cancelled);
            }
            progress(f as f32 / frames as f32 * 0.8);
        }
        let centre = f * HOP_SAMPLES + HOP_SAMPLES / 2;
        // level
        let (a, b) = (
            centre.saturating_sub(WINDOW / 2),
            (centre + WINDOW / 2).min(pcm.len()),
        );
        let power = if b > a {
            pcm[a..b].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32
        } else {
            0.0
        };
        db.push(to_db(power));
        // spectrum around the same moment
        let start = centre as isize - (FFT / 2) as isize;
        for i in 0..FFT {
            let idx = start + i as isize;
            let v = if idx >= 0 && (idx as usize) < pcm.len() {
                pcm[idx as usize]
            } else {
                0.0
            };
            re[i] = v * hann[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let mut sum = 0.0f32;
        let mut log_sum = 0.0f32;
        for k in lo..=hi {
            let p = re[k] * re[k] + im[k] * im[k] + 1e-12;
            sum += p;
            log_sum += p.ln();
        }
        let n = (hi - lo + 1) as f32;
        band_db.push(to_db(sum / n / (FFT as f32)));
        // geometric over arithmetic mean: near 1 for noise, small for voiced speech
        flatness.push(((log_sum / n).exp() / (sum / n)).clamp(0.0, 1.0));
    }
    let floor = Analysis::percentile(&band_db, 0.10);
    let speech: Vec<f32> = band_db
        .iter()
        .zip(&flatness)
        .map(|(b, flat)| {
            let level = ((b - floor - 6.0) / 12.0).clamp(0.0, 1.0);
            let tonal = 1.0 - 0.7 * ((flat - 0.2) / 0.4).clamp(0.0, 1.0);
            level * tonal
        })
        .collect();
    progress(0.85);
    let neural = if neural {
        neural_curve(pcm, frames)
    } else {
        Vec::new()
    };
    progress(1.0);
    Ok(Analysis { db, speech, neural })
}

/// Earshot's voice probability for every 16 ms frame, placed on the 10 ms grid.
fn neural_curve(pcm: &[f32], frames: usize) -> Vec<f32> {
    const FRAME: usize = 256;
    let mut detector = earshot::Detector::default();
    let mut scores = Vec::with_capacity(pcm.len() / FRAME + 1);
    let mut buf = [0f32; FRAME];
    for chunk in pcm.chunks(FRAME) {
        buf[..chunk.len()].copy_from_slice(chunk);
        buf[chunk.len()..].fill(0.0);
        scores.push(detector.predict_f32(&buf));
    }
    (0..frames)
        .map(|f| {
            let t = (f * HOP_SAMPLES + HOP_SAMPLES / 2) / FRAME;
            scores
                .get(t.min(scores.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.0)
        })
        .collect()
}
