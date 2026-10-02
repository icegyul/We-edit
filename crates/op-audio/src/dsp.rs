//! Signal processing building blocks. Filter coefficients follow the widely published "Audio EQ
//! Cookbook" formulas; the reverb is a Schroeder/Moorer design (parallel combs into series
//! all-passes).

use std::f64::consts::PI;

pub fn db_to_gain(db: f64) -> f64 {
    if db <= -95.9 {
        0.0
    } else {
        10f64.powf(db / 20.0)
    }
}

pub fn gain_to_db(g: f64) -> f64 {
    if g <= 1e-6 { -120.0 } else { 20.0 * g.log10() }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

/// A second-order section, transposed direct form II, one per channel.
#[derive(Clone, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: Vec<f64>,
    z2: Vec<f64>,
    key: Option<(u8, u64, u64, u64, u32)>,
}

impl Biquad {
    pub fn set(&mut self, kind: FilterKind, freq: f64, q: f64, gain_db: f64, rate: u32) {
        let key = Some((
            kind as u8,
            freq.to_bits(),
            q.to_bits(),
            gain_db.to_bits(),
            rate,
        ));
        if self.key == key {
            return;
        }
        self.key = key;
        let fs = rate as f64;
        let f = freq.clamp(10.0, fs * 0.49);
        let q = q.max(0.05);
        let w0 = 2.0 * PI * f / fs;
        let (sw, cw) = w0.sin_cos();
        let alpha = sw / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterKind::LowPass => (
                (1.0 - cw) / 2.0,
                1.0 - cw,
                (1.0 - cw) / 2.0,
                1.0 + alpha,
                -2.0 * cw,
                1.0 - alpha,
            ),
            FilterKind::HighPass => (
                (1.0 + cw) / 2.0,
                -(1.0 + cw),
                (1.0 + cw) / 2.0,
                1.0 + alpha,
                -2.0 * cw,
                1.0 - alpha,
            ),
            FilterKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterKind::Notch => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            FilterKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cw,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cw,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cw + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
                    a * ((a + 1.0) - (a - 1.0) * cw - s),
                    (a + 1.0) + (a - 1.0) * cw + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cw),
                    (a + 1.0) + (a - 1.0) * cw - s,
                )
            }
            FilterKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cw + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
                    a * ((a + 1.0) + (a - 1.0) * cw - s),
                    (a + 1.0) - (a - 1.0) * cw + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cw),
                    (a + 1.0) - (a - 1.0) * cw - s,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    pub fn process(&mut self, buf: &mut [f32], channels: usize) {
        if self.z1.len() != channels {
            self.z1 = vec![0.0; channels];
            self.z2 = vec![0.0; channels];
        }
        for frame in buf.chunks_exact_mut(channels) {
            for (c, s) in frame.iter_mut().enumerate() {
                let x = *s as f64;
                let y = self.b0 * x + self.z1[c];
                self.z1[c] = self.b1 * x - self.a1 * y + self.z2[c];
                self.z2[c] = self.b2 * x - self.a2 * y;
                // flush denormals
                if self.z1[c].abs() < 1e-30 {
                    self.z1[c] = 0.0;
                }
                if self.z2[c].abs() < 1e-30 {
                    self.z2[c] = 0.0;
                }
                *s = y as f32;
            }
        }
    }

    pub fn reset(&mut self) {
        self.z1.iter_mut().for_each(|z| *z = 0.0);
        self.z2.iter_mut().for_each(|z| *z = 0.0);
    }
}

/// Multi-channel delay line.
#[derive(Clone, Debug, Default)]
pub struct Delay {
    buf: Vec<f32>,
    pos: usize,
    channels: usize,
    feedback_state: bool,
}

impl Delay {
    fn ensure(&mut self, frames: usize, channels: usize) {
        if self.channels != channels || self.buf.len() != frames * channels {
            self.buf = vec![0.0; frames.max(1) * channels];
            self.pos = 0;
            self.channels = channels;
        }
    }

    /// Echo with feedback: out = dry * (1 - mix) + delayed * mix.
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        delay_frames: usize,
        feedback: f32,
        mix: f32,
    ) {
        let len = delay_frames.clamp(1, 10 * 192_000);
        self.ensure(len, channels);
        self.feedback_state = true;
        let frames = self.buf.len() / channels;
        for frame in buf.chunks_exact_mut(channels) {
            for (c, s) in frame.iter_mut().enumerate() {
                let i = self.pos * channels + c;
                let delayed = self.buf[i];
                self.buf[i] = *s + delayed * feedback;
                *s = *s * (1.0 - mix) + delayed * mix;
            }
            self.pos = (self.pos + 1) % frames;
        }
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// Feed-forward compressor with a smoothed level detector (dB domain).
#[derive(Clone, Debug, Default)]
pub struct Compressor {
    env_db: f64,
}

impl Compressor {
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_db: f64,
    ) {
        let fs = rate as f64;
        let att = (-1.0 / (attack_ms.max(0.01) * 0.001 * fs)).exp();
        let rel = (-1.0 / (release_ms.max(1.0) * 0.001 * fs)).exp();
        let ratio = ratio.max(1.0);
        let makeup = db_to_gain(makeup_db);
        for frame in buf.chunks_exact_mut(channels) {
            let peak = frame.iter().fold(0f32, |m, v| m.max(v.abs())) as f64;
            let level = gain_to_db(peak);
            let coef = if level > self.env_db { att } else { rel };
            self.env_db = level + coef * (self.env_db - level);
            let over = self.env_db - threshold_db;
            let reduction = if over > 0.0 { over - over / ratio } else { 0.0 };
            let g = (db_to_gain(-reduction) * makeup) as f32;
            for s in frame.iter_mut() {
                *s *= g;
            }
        }
    }

    pub fn reset(&mut self) {
        self.env_db = -120.0;
    }
}

/// Brick-wall limiter with look-ahead: the output never exceeds the ceiling.
#[derive(Clone, Debug, Default)]
pub struct Limiter {
    delay: Vec<f32>,
    peaks: Vec<f32>,
    pos: usize,
    channels: usize,
    gain: f32,
}

impl Limiter {
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        ceiling_db: f64,
        boost_db: f64,
        lookahead_ms: f64,
        release_ms: f64,
    ) {
        let la = ((lookahead_ms.clamp(0.5, 50.0) * 0.001 * rate as f64) as usize).max(1);
        if self.channels != channels || self.peaks.len() != la {
            self.delay = vec![0.0; la * channels];
            self.peaks = vec![0.0; la];
            self.pos = 0;
            self.channels = channels;
            self.gain = 1.0;
        }
        let ceiling = db_to_gain(ceiling_db) as f32;
        let boost = db_to_gain(boost_db) as f32;
        let rel = (-1.0 / (release_ms.max(1.0) * 0.001 * rate as f64)).exp() as f32;
        for frame in buf.chunks_exact_mut(channels) {
            let peak = frame.iter().fold(0f32, |m, v| m.max((v * boost).abs()));
            self.peaks[self.pos] = peak;
            // the loudest sample within the look-ahead window sets the target gain
            let window_peak = self.peaks.iter().fold(0f32, |m, v| m.max(*v));
            let target = if window_peak > ceiling {
                ceiling / window_peak
            } else {
                1.0
            };
            self.gain = if target < self.gain {
                target
            } else {
                target + rel * (self.gain - target)
            };
            for (c, s) in frame.iter_mut().enumerate() {
                let i = self.pos * channels + c;
                let delayed = self.delay[i];
                self.delay[i] = *s * boost;
                *s = (delayed * self.gain).clamp(-ceiling, ceiling);
            }
            self.pos = (self.pos + 1) % la;
        }
    }

    pub fn reset(&mut self) {
        self.delay.iter_mut().for_each(|v| *v = 0.0);
        self.peaks.iter_mut().for_each(|v| *v = 0.0);
        self.gain = 1.0;
    }
}

#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

#[derive(Clone, Debug)]
struct AllPass {
    buf: Vec<f32>,
    pos: usize,
}

impl AllPass {
    fn new(len: usize) -> AllPass {
        AllPass {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let y = -x + b;
        self.buf[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// Stereo room reverb.
#[derive(Clone, Debug, Default)]
pub struct Reverb {
    combs: Vec<[Comb; 2]>,
    allpasses: Vec<[AllPass; 2]>,
    rate: u32,
}

impl Reverb {
    fn build(&mut self, rate: u32) {
        let k = rate as f64 / 44100.0;
        let spread = 23;
        let comb_len = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let ap_len = [556, 441, 341, 225];
        self.combs = comb_len
            .iter()
            .map(|l| {
                [
                    Comb::new((*l as f64 * k) as usize),
                    Comb::new(((*l + spread) as f64 * k) as usize),
                ]
            })
            .collect();
        self.allpasses = ap_len
            .iter()
            .map(|l| {
                [
                    AllPass::new((*l as f64 * k) as usize),
                    AllPass::new(((*l + spread) as f64 * k) as usize),
                ]
            })
            .collect();
        self.rate = rate;
    }

    /// `room`, `damping`, `width`, `dry`, `wet` in 0..1.
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        room: f32,
        damping: f32,
        width: f32,
        dry: f32,
        wet: f32,
    ) {
        if self.rate != rate || self.combs.is_empty() {
            self.build(rate);
        }
        let feedback = 0.7 + room * 0.28;
        let damp = damping * 0.4;
        let wet1 = wet * (width / 2.0 + 0.5);
        let wet2 = wet * ((1.0 - width) / 2.0);
        for frame in buf.chunks_exact_mut(channels) {
            let l_in = frame[0];
            let r_in = if channels > 1 { frame[1] } else { frame[0] };
            let input = (l_in + r_in) * 0.015;
            let (mut l, mut r) = (0.0, 0.0);
            for c in &mut self.combs {
                l += c[0].tick(input, feedback, damp);
                r += c[1].tick(input, feedback, damp);
            }
            for a in &mut self.allpasses {
                l = a[0].tick(l);
                r = a[1].tick(r);
            }
            let out_l = l * wet1 + r * wet2;
            let out_r = r * wet1 + l * wet2;
            frame[0] = l_in * dry + out_l;
            if channels > 1 {
                frame[1] = r_in * dry + out_r;
            }
        }
    }
}

/// Modulated delay for chorus (longer delay) and flanger (short delay with feedback).
#[derive(Clone, Debug, Default)]
pub struct Modulation {
    buf: Vec<f32>,
    pos: usize,
    phase: f64,
    channels: usize,
}

impl Modulation {
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        flanger: bool,
        speed_hz: f64,
        depth: f64,
        feedback: f32,
        mix: f32,
    ) {
        let max = (rate as usize / 20).max(64);
        if self.channels != channels || self.buf.len() != max * channels {
            self.buf = vec![0.0; max * channels];
            self.pos = 0;
            self.channels = channels;
        }
        let (base_ms, range_ms) = if flanger {
            (2.0, 4.0 * depth)
        } else {
            (18.0, 10.0 * depth)
        };
        let fs = rate as f64;
        for frame in buf.chunks_exact_mut(channels) {
            self.phase = (self.phase + speed_hz / fs).fract();
            for (c, s) in frame.iter_mut().enumerate() {
                // opposite phase per channel widens the image
                let lfo = (2.0 * PI * (self.phase + c as f64 * 0.25)).sin() * 0.5 + 0.5;
                let d = ((base_ms + range_ms * lfo) * 0.001 * fs).min(max as f64 - 2.0);
                let read = self.pos as f64 - d;
                let read = if read < 0.0 { read + max as f64 } else { read };
                let i0 = read.floor() as usize % max;
                let i1 = (i0 + 1) % max;
                let f = (read - read.floor()) as f32;
                let delayed =
                    self.buf[i0 * channels + c] * (1.0 - f) + self.buf[i1 * channels + c] * f;
                self.buf[self.pos * channels + c] =
                    *s + delayed * if flanger { feedback } else { feedback * 0.5 };
                *s = *s * (1.0 - mix * 0.5) + delayed * mix;
            }
            self.pos = (self.pos + 1) % max;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / rate as f64).sin() as f32)
            .collect()
    }

    fn rms(v: &[f32]) -> f64 {
        (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn lowpass_attenuates_above_cutoff() {
        let mut f = Biquad::default();
        f.set(FilterKind::LowPass, 1000.0, 0.707, 0.0, 48000);
        let mut hi = sine(8000.0, 48000, 48000);
        f.process(&mut hi, 1);
        let mut f2 = Biquad::default();
        f2.set(FilterKind::LowPass, 1000.0, 0.707, 0.0, 48000);
        let mut lo = sine(100.0, 48000, 48000);
        f2.process(&mut lo, 1);
        assert!(rms(&hi[4800..]) < 0.03, "{}", rms(&hi[4800..]));
        assert!((rms(&lo[4800..]) - 0.707).abs() < 0.02);
    }

    #[test]
    fn peak_filter_boosts_its_band() {
        let mut f = Biquad::default();
        f.set(FilterKind::Peak, 1000.0, 1.0, 6.0, 48000);
        let mut s = sine(1000.0, 48000, 48000);
        f.process(&mut s, 1);
        let gain = rms(&s[4800..]) / std::f64::consts::FRAC_1_SQRT_2;
        assert!((gain_to_db(gain) - 6.0).abs() < 0.2, "{}", gain_to_db(gain));
    }

    #[test]
    fn limiter_holds_the_ceiling() {
        let mut l = Limiter::default();
        let mut s: Vec<f32> = sine(440.0, 48000, 48000).iter().map(|v| v * 2.0).collect();
        l.process(&mut s, 1, 48000, -1.0, 0.0, 5.0, 50.0);
        let ceiling = db_to_gain(-1.0) as f32 + 1e-6;
        assert!(s.iter().all(|v| v.abs() <= ceiling));
        assert!(rms(&s[4800..]) > 0.5);
    }

    #[test]
    fn compressor_reduces_loud_signals() {
        let mut c = Compressor::default();
        c.reset();
        let mut s = sine(440.0, 48000, 48000);
        c.process(&mut s, 1, 48000, -20.0, 4.0, 1.0, 50.0, 0.0);
        assert!(rms(&s[24000..]) < 0.3);
    }

    #[test]
    fn delay_and_reverb_produce_tails() {
        let mut impulse = vec![0f32; 48000 * 2];
        impulse[0] = 1.0;
        let mut d = Delay::default();
        let mut x = impulse.clone();
        d.process(&mut x, 2, 4800, 0.0, 1.0);
        assert!((x[4800 * 2] - 1.0).abs() < 1e-6);
        let mut r = Reverb::default();
        let mut y = impulse.clone();
        r.process(&mut y, 2, 48000, 0.5, 0.5, 1.0, 0.0, 1.0);
        let tail: f32 = y[2000..20000].iter().map(|v| v.abs()).sum();
        assert!(tail > 0.01);
    }
}

/// Low-frequency oscillator: phase 0..1 advanced per sample frame.
#[derive(Clone, Debug, Default)]
pub struct Lfo {
    phase: f64,
}

impl Lfo {
    /// The current value in -1..1 for `shape` (0 sine, 1 square with soft edges, 2 triangle),
    /// then one step at `rate` Hz.
    pub fn next(&mut self, rate_hz: f64, sample_rate: u32, shape: u32) -> f64 {
        let p = self.phase;
        self.phase = (self.phase + rate_hz / sample_rate.max(1) as f64).fract();
        match shape {
            1 => (std::f64::consts::TAU * p).sin().clamp(-0.2, 0.2) * 5.0,
            2 => 1.0 - 4.0 * (p - 0.5).abs(),
            _ => (std::f64::consts::TAU * p).sin(),
        }
    }
}

/// Phaser: a chain of first-order all-pass filters whose corner sweeps with an LFO, mixed with
/// the dry signal so the moving notches comb the spectrum.
#[derive(Clone, Debug, Default)]
pub struct Phaser {
    lfo: Lfo,
    /// Per channel and stage: previous input and output.
    state: Vec<Vec<(f32, f32)>>,
    last: Vec<f32>,
}

impl Phaser {
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        speed: f64,
        depth: f64,
        feedback: f32,
        stages: usize,
        mix: f32,
    ) {
        let stages = stages.clamp(2, 12);
        if self.state.len() != channels || self.state.first().is_some_and(|s| s.len() != stages) {
            self.state = vec![vec![(0.0, 0.0); stages]; channels];
            self.last = vec![0.0; channels];
        }
        let sr = rate.max(1) as f64;
        for frame in buf.chunks_exact_mut(channels) {
            let l = self.lfo.next(speed, rate, 0) * 0.5 + 0.5;
            // sweep 200 Hz .. 200 Hz * 2^(depth * 4) on a log scale
            let f = 200.0 * 2f64.powf(depth.clamp(0.0, 1.0) * 4.0 * l);
            let t = (std::f64::consts::PI * f.min(sr * 0.45) / sr).tan();
            let a = ((t - 1.0) / (t + 1.0)) as f32;
            for (c, s) in frame.iter_mut().enumerate() {
                let mut x = *s + self.last[c] * feedback;
                for st in self.state[c].iter_mut() {
                    let y = a * x + st.0 - a * st.1;
                    st.0 = x;
                    st.1 = y;
                    x = y;
                }
                self.last[c] = x;
                *s = *s * (1.0 - mix * 0.5) + x * mix * 0.5;
            }
        }
    }
}

/// Bit depth and sample rate reduction.
#[derive(Clone, Debug, Default)]
pub struct Crusher {
    held: Vec<f32>,
    count: usize,
}

impl Crusher {
    pub fn process(&mut self, buf: &mut [f32], channels: usize, bits: u32, hold: usize, mix: f32) {
        if self.held.len() != channels {
            self.held = vec![0.0; channels];
            self.count = 0;
        }
        let levels = (1u32 << (bits.clamp(1, 16) - 1)) as f32;
        let hold = hold.max(1);
        for frame in buf.chunks_exact_mut(channels) {
            let take = self.count == 0;
            self.count = (self.count + 1) % hold;
            for (c, s) in frame.iter_mut().enumerate() {
                if take {
                    self.held[c] = ((*s * levels).round() / levels).clamp(-1.0, 1.0);
                }
                *s = *s * (1.0 - mix) + self.held[c] * mix;
            }
        }
    }
}

/// Noise gate: silences the signal while its level stays below the threshold.
#[derive(Clone, Debug, Default)]
pub struct Gate {
    env: f64,
    gain: f64,
    held: usize,
}

impl Gate {
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        buf: &mut [f32],
        channels: usize,
        rate: u32,
        threshold_db: f64,
        attack_ms: f64,
        hold_ms: f64,
        release_ms: f64,
    ) {
        let sr = rate.max(1) as f64;
        let thr = db_to_gain(threshold_db);
        let coef = |ms: f64| (-1.0 / (ms.max(0.05) * 0.001 * sr)).exp();
        let (ka, kr) = (coef(attack_ms), coef(release_ms));
        let env_release = coef(20.0);
        let hold = (hold_ms.max(0.0) * 0.001 * sr) as usize;
        for frame in buf.chunks_exact_mut(channels) {
            let peak = frame.iter().fold(0f32, |m, s| m.max(s.abs())) as f64;
            self.env = if peak > self.env {
                peak
            } else {
                self.env * env_release + peak * (1.0 - env_release)
            };
            let open = if self.env >= thr {
                self.held = hold;
                true
            } else if self.held > 0 {
                self.held -= 1;
                true
            } else {
                false
            };
            let target = if open { 1.0 } else { 0.0 };
            let k = if target > self.gain { ka } else { kr };
            self.gain = target + (self.gain - target) * k;
            let g = self.gain as f32;
            frame.iter_mut().for_each(|s| *s *= g);
        }
    }
}

/// Pitch shifter: two read heads sweep through a short delay line at the pitch ratio and
/// cross-fade (sin^2 windows that always sum to one), the classic delay-line shifter. Good for
/// voices from deep to chipmunk; the duration does not change.
#[derive(Clone, Debug, Default)]
pub struct PitchShifter {
    ring: Vec<Vec<f32>>,
    pos: usize,
    phase: f64,
}

impl PitchShifter {
    pub fn process(&mut self, buf: &mut [f32], channels: usize, rate: u32, ratio: f64, mix: f32) {
        // a 40 ms window: long enough for low voices, short enough to avoid echoes
        let window = ((rate.max(8000) as f64 * 0.04) as usize).max(64);
        let len = window * 2 + 4;
        if self.ring.len() != channels || self.ring.first().is_some_and(|r| r.len() != len) {
            self.ring = vec![vec![0.0; len]; channels];
            self.pos = 0;
            self.phase = 0.0;
        }
        let step = (1.0 - ratio) / window as f64;
        for frame in buf.chunks_exact_mut(channels) {
            for (c, s) in frame.iter_mut().enumerate() {
                let ring = &mut self.ring[c];
                ring[self.pos] = *s;
                let mut out = 0.0f32;
                for k in 0..2 {
                    let f = (self.phase + k as f64 * 0.5).rem_euclid(1.0);
                    let d = 1.0 + f * window as f64;
                    let rp = (self.pos as f64 - d).rem_euclid(len as f64);
                    let i0 = rp.floor() as usize % len;
                    let i1 = (i0 + 1) % len;
                    let fr = (rp - rp.floor()) as f32;
                    let v = ring[i0] * (1.0 - fr) + ring[i1] * fr;
                    let w = (std::f64::consts::PI * f).sin();
                    out += v * (w * w) as f32;
                }
                *s = *s * (1.0 - mix) + out * mix;
            }
            self.phase = (self.phase + step).rem_euclid(1.0);
            self.pos = (self.pos + 1) % len;
        }
    }
}
