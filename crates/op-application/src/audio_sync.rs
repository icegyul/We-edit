//! Sound sync: how far apart in time two recordings of the same scene started, found by
//! comparing their sound (GCC-PHAT cross-correlation, which keeps working when the two
//! microphones colour the sound differently). Used by Clip > Synchronize and the
//! `sync_by_audio` tool.

use op_media::ConformedAudio;
use realfft::RealFftPlanner;

/// Analysis rate: speech and most room sound live below 4 kHz.
pub const RATE: u32 = 8000;

/// Which channel of a recording is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    One,
    Two,
    MixDown,
}

/// `seconds` of a recording from `start` (seconds into the file) as mono at `RATE` (each output
/// sample averages the input frames it covers, whatever the file's own rate).
pub fn mono(audio: &ConformedAudio, start: f64, seconds: f64, channel: Channel) -> Vec<f32> {
    let rate = u64::from(audio.info.rate.max(1));
    let ch = usize::from(audio.info.channels.max(1));
    let first_frame = (start.max(0.0) * rate as f64) as u64;
    let available = (audio.info.samples.max(0) as u64).saturating_sub(first_frame);
    let frames = available.min((seconds * rate as f64) as u64);
    let len = (frames * u64::from(RATE) / rate) as usize;
    let at = |j: u64| first_frame + j * rate / u64::from(RATE); // first input frame of output sample j
    let pick = |frame: &[f32]| match channel {
        Channel::One => frame[0],
        Channel::Two => frame[1.min(ch - 1)],
        Channel::MixDown => frame.iter().sum::<f32>() / ch as f32,
    };
    let mut out = Vec::with_capacity(len);
    let mut buf = Vec::new();
    let mut j = 0usize;
    while j < len {
        let n = (1usize << 16).min(len - j);
        let (first, last) = (at(j as u64), at((j + n) as u64));
        let frames_here = (last - first) as usize;
        buf.resize(frames_here * ch, 0.0);
        audio.read(first as i64, &mut buf);
        for k in 0..n as u64 {
            let a = (at(j as u64 + k) - first) as usize;
            let b = ((at(j as u64 + k + 1) - first) as usize).max(a + 1).min(frames_here);
            let sum: f32 = (a..b).map(|f| pick(&buf[f * ch..(f + 1) * ch])).sum();
            out.push(sum / (b - a).max(1) as f32);
        }
        j += n;
    }
    out
}

/// Where the scene is in `a` relative to `b`.
pub struct Lag {
    /// Positive: the same moment comes later in `a`, so `b` was started that much later.
    pub seconds: f64,
    /// Height of the correlation peak over its background; a few units mean a chance match.
    pub confidence: f64,
}

/// Below this confidence a match is a chance one and must not be used.
pub const MIN_CONFIDENCE: f64 = 8.0;

pub fn lag(a: &[f32], b: &[f32]) -> Option<Lag> {
    if a.len() < RATE as usize || b.len() < RATE as usize {
        return None; // under a second of sound cannot be matched
    }
    let n = (a.len() + b.len()).next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let spectrum = |x: &[f32]| {
        let mut t = fwd.make_input_vec();
        t[..x.len()].copy_from_slice(x);
        let mut s = fwd.make_output_vec();
        fwd.process(&mut t, &mut s).ok().map(|_| s)
    };
    let (mut sa, sb) = (spectrum(a)?, spectrum(b)?);
    for (x, y) in sa.iter_mut().zip(&sb) {
        let r = *x * y.conj();
        *x = r / (r.norm() + 1e-9); // phase transform: only the timing of each frequency counts
    }
    sa[0].im = 0.0;
    if let Some(last) = sa.last_mut() {
        last.im = 0.0;
    }
    let mut c = inv.make_output_vec();
    inv.process(&mut sa, &mut c).ok()?;
    let (k, best) = c
        .iter()
        .enumerate()
        .fold((0, f32::MIN), |m, (i, &v)| if v > m.1 { (i, v) } else { m });
    let mean = c.iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let var = c.iter().map(|&v| (f64::from(v) - mean).powi(2)).sum::<f64>() / n as f64;
    let k = if k > n / 2 { k as i64 - n as i64 } else { k as i64 };
    Some(Lag {
        seconds: k as f64 / f64::from(RATE),
        confidence: (f64::from(best) - mean) / (var.sqrt() + 1e-12),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gated noise, like speech: sound and pauses, never periodic.
    fn scene(seconds: usize) -> Vec<f32> {
        let mut x = 12345u32;
        let mut noise = || {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        (0..seconds * RATE as usize)
            .map(|i| {
                let gate = if (i as f32 / RATE as f32 * 2.3).sin() > 0.0 { 1.0 } else { 0.0 };
                noise() * gate
            })
            .collect()
    }

    /// The scene as a microphone starting `start` seconds in: quieter, with its own hiss.
    fn recording(scene: &[f32], start: f64, gain: f32, hiss: f32) -> Vec<f32> {
        let mut x = 777u32;
        let from = (start * f64::from(RATE)) as usize;
        scene[from..]
            .iter()
            .map(|&v| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                v * gain + ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * hiss
            })
            .collect()
    }

    #[test]
    fn finds_how_much_later_each_recording_started() {
        let s = scene(30);
        let a = recording(&s, 0.0, 1.0, 0.02);
        let b = recording(&s, 3.37, 0.4, 0.2); // started later, quieter, noisier
        let c = recording(&s, 2.0, 0.7, 0.05);
        let lb = lag(&a, &b).unwrap();
        let lc = lag(&a, &c).unwrap();
        assert!((lb.seconds - 3.37).abs() < 0.002, "{}", lb.seconds);
        assert!((lc.seconds - 2.0).abs() < 0.002, "{}", lc.seconds);
        assert!(lb.confidence > MIN_CONFIDENCE && lc.confidence > MIN_CONFIDENCE);
        // the other way round the sign flips
        assert!((lag(&b, &a).unwrap().seconds + 3.37).abs() < 0.002);
    }

    #[test]
    fn unrelated_sound_has_low_confidence() {
        let a = scene(20);
        let mut x = 99u32;
        let b: Vec<f32> = (0..20 * RATE as usize)
            .map(|_| {
                x = x.wrapping_mul(22695477).wrapping_add(1);
                (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        assert!(lag(&a, &b).unwrap().confidence < MIN_CONFIDENCE);
    }

    #[test]
    fn too_short_sound_is_refused() {
        assert!(lag(&[0.0; 100], &[0.0; 100]).is_none());
    }
}
