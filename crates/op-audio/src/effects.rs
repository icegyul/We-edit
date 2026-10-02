//! Audio effect processing for catalog components. State (filter memories, delay lines) lives
//! per effect instance across blocks.

use op_core::plan::EvalComponent;

use crate::dsp::*;

#[derive(Debug, Default)]
pub enum FxState {
    #[default]
    None,
    Filters(Vec<Biquad>),
    Delay(Delay),
    Compressor(Compressor),
    Limiter(Limiter),
    Reverb(Box<Reverb>),
    Modulation(Modulation),
    Lfo(Lfo),
    Phaser(Box<Phaser>),
    Crusher(Crusher),
    Gate(Gate),
    Pitch(Box<PitchShifter>),
}

impl FxState {
    pub fn reset(&mut self) {
        match self {
            FxState::Filters(f) => f.iter_mut().for_each(Biquad::reset),
            FxState::Delay(d) => d.reset(),
            FxState::Compressor(c) => c.reset(),
            FxState::Limiter(l) => l.reset(),
            _ => *self = FxState::None,
        }
    }
}

fn filters(state: &mut FxState, n: usize) -> &mut Vec<Biquad> {
    if !matches!(state, FxState::Filters(f) if f.len() == n) {
        *state = FxState::Filters(vec![Biquad::default(); n]);
    }
    match state {
        FxState::Filters(f) => f,
        _ => unreachable!(),
    }
}

/// Processes one standard audio effect in place (interleaved samples).
pub fn process(
    state: &mut FxState,
    fx: &EvalComponent,
    buf: &mut [f32],
    channels: usize,
    rate: u32,
) {
    let ch = channels.max(1);
    match fx.effect.as_str() {
        "op.audio.amplify" => {
            let gl = db_to_gain(fx.f64("left")) as f32;
            let gr = db_to_gain(fx.f64("right")) as f32;
            for frame in buf.chunks_exact_mut(ch) {
                frame[0] *= gl;
                if ch > 1 {
                    frame[1] *= gr;
                }
            }
        }
        "op.audio.bass" => {
            let f = filters(state, 1);
            f[0].set(FilterKind::LowShelf, 200.0, 0.707, fx.f64("boost"), rate);
            f[0].process(buf, ch);
        }
        "op.audio.treble" => {
            let f = filters(state, 1);
            f[0].set(FilterKind::HighShelf, 4000.0, 0.707, fx.f64("boost"), rate);
            f[0].process(buf, ch);
        }
        "op.audio.highpass" => {
            let f = filters(state, 1);
            f[0].set(
                FilterKind::HighPass,
                fx.f64("cutoff"),
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
                rate,
            );
            f[0].process(buf, ch);
        }
        "op.audio.lowpass" => {
            let f = filters(state, 1);
            f[0].set(
                FilterKind::LowPass,
                fx.f64("cutoff"),
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
                rate,
            );
            f[0].process(buf, ch);
        }
        "op.audio.bandpass" => {
            let f = filters(state, 1);
            f[0].set(
                FilterKind::BandPass,
                fx.f64("center"),
                fx.f64("q"),
                0.0,
                rate,
            );
            f[0].process(buf, ch);
        }
        "op.audio.notch" => {
            let f = filters(state, 1);
            f[0].set(FilterKind::Notch, fx.f64("center"), fx.f64("q"), 0.0, rate);
            f[0].process(buf, ch);
        }
        "op.audio.simple_eq" => {
            let f = filters(state, 1);
            f[0].set(
                FilterKind::Peak,
                fx.f64("center"),
                fx.f64("q"),
                fx.f64("boost"),
                rate,
            );
            f[0].process(buf, ch);
        }
        "op.audio.parametric_eq" => {
            let f = filters(state, 7);
            f[0].set(
                FilterKind::LowShelf,
                fx.f64("low_freq"),
                0.707,
                fx.f64("low_gain"),
                rate,
            );
            f[1].set(
                FilterKind::Peak,
                fx.f64("b1_freq"),
                fx.f64("b1_q"),
                fx.f64("b1_gain"),
                rate,
            );
            f[2].set(
                FilterKind::Peak,
                fx.f64("b2_freq"),
                fx.f64("b2_q"),
                fx.f64("b2_gain"),
                rate,
            );
            f[3].set(
                FilterKind::Peak,
                fx.f64("b3_freq"),
                fx.f64("b3_q"),
                fx.f64("b3_gain"),
                rate,
            );
            f[4].set(
                FilterKind::HighShelf,
                fx.f64("high_freq"),
                0.707,
                fx.f64("high_gain"),
                rate,
            );
            f[5].set(
                FilterKind::HighPass,
                fx.f64("hp_freq"),
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
                rate,
            );
            f[6].set(
                FilterKind::LowPass,
                fx.f64("lp_freq"),
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
                rate,
            );
            // pass filters only when moved off the band edges
            let hp_on = fx.f64("hp_freq") > 20.5;
            let lp_on = fx.f64("lp_freq") < 19999.0;
            for (i, filter) in f.iter_mut().enumerate() {
                if (i == 5 && !hp_on) || (i == 6 && !lp_on) {
                    continue;
                }
                filter.process(buf, ch);
            }
            let g = db_to_gain(fx.f64("output")) as f32;
            if g != 1.0 {
                buf.iter_mut().for_each(|s| *s *= g);
            }
        }
        "op.audio.delay" => {
            if !matches!(state, FxState::Delay(_)) {
                *state = FxState::Delay(Delay::default());
            }
            if let FxState::Delay(d) = state {
                let frames = (fx.f64("time") * rate as f64).round() as usize;
                d.process(
                    buf,
                    ch,
                    frames,
                    (fx.f64("feedback") / 100.0).min(0.98) as f32,
                    (fx.f64("mix") / 100.0) as f32,
                );
            }
        }
        "op.audio.hard_limiter" => {
            if !matches!(state, FxState::Limiter(_)) {
                *state = FxState::Limiter(Limiter::default());
            }
            if let FxState::Limiter(l) = state {
                l.process(
                    buf,
                    ch,
                    rate,
                    fx.f64("ceiling"),
                    fx.f64("input"),
                    fx.f64("lookahead"),
                    fx.f64("release"),
                );
            }
        }
        "op.audio.compressor" => {
            if !matches!(state, FxState::Compressor(_)) {
                let mut c = Compressor::default();
                c.reset();
                *state = FxState::Compressor(c);
            }
            if let FxState::Compressor(c) = state {
                let threshold = fx.f64("threshold");
                let ratio = fx.f64("ratio");
                let makeup = if fx.bool("auto_gain") {
                    -threshold * (1.0 - 1.0 / ratio.max(1.0)) * 0.5
                } else {
                    0.0
                } + fx.f64("output");
                c.process(
                    buf,
                    ch,
                    rate,
                    threshold,
                    ratio,
                    fx.f64("attack"),
                    fx.f64("release"),
                    makeup,
                );
            }
        }
        "op.audio.reverb" => {
            if !matches!(state, FxState::Reverb(_)) {
                *state = FxState::Reverb(Box::default());
            }
            if let FxState::Reverb(r) = state {
                let p = |k: &str| (fx.f64(k) / 100.0) as f32;
                r.process(
                    buf,
                    ch,
                    rate,
                    p("room"),
                    p("damping"),
                    p("width"),
                    p("dry"),
                    p("wet"),
                );
            }
        }
        "op.audio.chorus" => {
            if !matches!(state, FxState::Modulation(_)) {
                *state = FxState::Modulation(Modulation::default());
            }
            if let FxState::Modulation(m) = state {
                m.process(
                    buf,
                    ch,
                    rate,
                    fx.choice("mode") == 1,
                    fx.f64("rate"),
                    fx.f64("depth") / 100.0,
                    (fx.f64("feedback") / 100.0).min(0.95) as f32,
                    (fx.f64("mix") / 100.0) as f32,
                );
            }
        }
        "op.audio.tremolo" => {
            if !matches!(state, FxState::Lfo(_)) {
                *state = FxState::Lfo(Lfo::default());
            }
            if let FxState::Lfo(l) = state {
                let depth = fx.f64("depth") / 100.0;
                let (speed, shape) = (fx.f64("rate"), fx.choice("shape"));
                for frame in buf.chunks_exact_mut(ch) {
                    let v = l.next(speed, rate, shape);
                    let g = (1.0 - depth * (0.5 - 0.5 * v)) as f32;
                    frame.iter_mut().for_each(|s| *s *= g);
                }
            }
        }
        "op.audio.autopan" if ch > 1 => {
            if !matches!(state, FxState::Lfo(_)) {
                *state = FxState::Lfo(Lfo::default());
            }
            if let FxState::Lfo(l) = state {
                let depth = fx.f64("depth") / 100.0;
                let speed = fx.f64("rate");
                for frame in buf.chunks_exact_mut(ch) {
                    // equal power pan, unity in the center
                    let pan = l.next(speed, rate, 0) * depth;
                    let a = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
                    frame[0] *= (a.cos() * std::f64::consts::SQRT_2) as f32;
                    frame[1] *= (a.sin() * std::f64::consts::SQRT_2) as f32;
                }
            }
        }
        "op.audio.phaser" => {
            if !matches!(state, FxState::Phaser(_)) {
                *state = FxState::Phaser(Box::default());
            }
            if let FxState::Phaser(p) = state {
                p.process(
                    buf,
                    ch,
                    rate,
                    fx.f64("rate"),
                    fx.f64("depth") / 100.0,
                    (fx.f64("feedback") / 100.0).min(0.95) as f32,
                    fx.f64("stages").round() as usize,
                    (fx.f64("mix") / 100.0) as f32,
                );
            }
        }
        "op.audio.bitcrusher" => {
            if !matches!(state, FxState::Crusher(_)) {
                *state = FxState::Crusher(Crusher::default());
            }
            if let FxState::Crusher(c) = state {
                c.process(
                    buf,
                    ch,
                    fx.f64("bits").round() as u32,
                    fx.f64("downsample").round() as usize,
                    (fx.f64("mix") / 100.0) as f32,
                );
            }
        }
        "op.audio.distortion" => {
            let drive = db_to_gain(fx.f64("drive")) as f32;
            let norm = drive.tanh().max(1e-3);
            let mix = (fx.f64("mix") / 100.0) as f32;
            let f = filters(state, 1);
            f[0].set(
                FilterKind::LowPass,
                fx.f64("tone"),
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
                rate,
            );
            let dry: Vec<f32> = buf.to_vec();
            for s in buf.iter_mut() {
                *s = (*s * drive).tanh() / norm;
            }
            f[0].process(buf, ch);
            for (s, d) in buf.iter_mut().zip(dry) {
                *s = d * (1.0 - mix) + *s * mix;
            }
        }
        "op.audio.noise_gate" => {
            if !matches!(state, FxState::Gate(_)) {
                *state = FxState::Gate(Gate::default());
            }
            if let FxState::Gate(g) = state {
                g.process(
                    buf,
                    ch,
                    rate,
                    fx.f64("threshold"),
                    fx.f64("attack"),
                    fx.f64("hold"),
                    fx.f64("release"),
                );
            }
        }
        "op.audio.pitch_shifter" => {
            let semis = fx.f64("semitones") + fx.f64("cents") / 100.0;
            if semis.abs() > 1e-6 {
                if !matches!(state, FxState::Pitch(_)) {
                    *state = FxState::Pitch(Box::default());
                }
                if let FxState::Pitch(p) = state {
                    p.process(
                        buf,
                        ch,
                        rate,
                        2f64.powf(semis / 12.0),
                        (fx.f64("mix") / 100.0) as f32,
                    );
                }
            }
        }
        "op.audio.radio_voice" => {
            // band limits, a presence peak and how hard the little speaker is driven
            let (hp, lp, peak_hz, peak_db, drive) = match fx.choice("mode") {
                1 => (450.0, 5000.0, 2500.0, 3.0, 1.6),
                2 => (650.0, 3200.0, 1600.0, 7.0, 4.0),
                3 => (400.0, 2600.0, 1800.0, 4.0, 6.0),
                4 => (40.0, 520.0, 300.0, 4.0, 1.0),
                _ => (300.0, 3400.0, 1800.0, 2.0, 2.0),
            };
            let mix = (fx.f64("mix") / 100.0) as f32;
            let f = filters(state, 3);
            let q = std::f64::consts::FRAC_1_SQRT_2;
            f[0].set(FilterKind::HighPass, hp, q, 0.0, rate);
            f[1].set(FilterKind::LowPass, lp, q, 0.0, rate);
            f[2].set(FilterKind::Peak, peak_hz, 1.2, peak_db, rate);
            let dry: Vec<f32> = buf.to_vec();
            for filter in f.iter_mut() {
                filter.process(buf, ch);
            }
            let d = drive as f32;
            let norm = d.tanh().max(1e-3);
            for (s, dr) in buf.iter_mut().zip(dry) {
                let wet = (*s * d).tanh() / norm;
                *s = dr * (1.0 - mix) + wet * mix;
            }
        }
        "op.audio.invert" => buf.iter_mut().for_each(|s| *s = -*s),
        "op.audio.swap_channels" if ch > 1 => {
            for frame in buf.chunks_exact_mut(ch) {
                frame.swap(0, 1);
            }
        }
        "op.audio.fill_left" if ch > 1 => {
            for frame in buf.chunks_exact_mut(ch) {
                frame[0] = frame[1];
            }
        }
        "op.audio.fill_right" if ch > 1 => {
            for frame in buf.chunks_exact_mut(ch) {
                frame[1] = frame[0];
            }
        }
        "op.audio.stereo_expander" if ch > 1 => {
            let w = (fx.f64("width") / 100.0) as f32;
            for frame in buf.chunks_exact_mut(ch) {
                let m = (frame[0] + frame[1]) * 0.5;
                let s = (frame[0] - frame[1]) * 0.5 * w;
                frame[0] = m + s;
                frame[1] = m - s;
            }
        }
        _ => {}
    }
}

/// Gain curves of the audio transitions for progress `p` (0..1): (outgoing, incoming).
pub fn crossfade(effect: &str, p: f64) -> (f64, f64) {
    let p = p.clamp(0.0, 1.0);
    match effect {
        "op.atr.constant_gain" => (1.0 - p, p),
        "op.atr.exponential_fade" => {
            let k = 4.0;
            let curve = |x: f64| ((k * x).exp() - 1.0) / (k.exp() - 1.0);
            (curve(1.0 - p), curve(p))
        }
        _ => (
            (p * std::f64::consts::FRAC_PI_2).cos(),
            (p * std::f64::consts::FRAC_PI_2).sin(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_core::{ComponentId, Value};

    const RATE: u32 = 48_000;

    fn fx(effect: &str, values: &[(&str, Value)]) -> EvalComponent {
        EvalComponent {
            id: ComponentId(1),
            effect: effect.to_string(),
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    /// One second of a stereo sine.
    fn sine(hz: f64, level: f32) -> Vec<f32> {
        (0..RATE as usize)
            .flat_map(|i| {
                let v = (std::f64::consts::TAU * hz * i as f64 / RATE as f64).sin() as f32 * level;
                [v, v]
            })
            .collect()
    }

    fn run(effect: &EvalComponent, mut buf: Vec<f32>) -> Vec<f32> {
        let mut state = FxState::default();
        // in blocks, as the mixer does
        for block in buf.chunks_mut(1024 * 2) {
            process(&mut state, effect, block, 2, RATE);
        }
        buf
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|s| s * s).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    /// Rising zero crossings of the left channel over the last half second.
    fn crossings(v: &[f32]) -> usize {
        let left: Vec<f32> = v.iter().step_by(2).copied().collect();
        let half = &left[left.len() / 2..];
        half.windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count()
    }

    #[test]
    fn new_effects_stay_finite_and_bounded() {
        for id in [
            "op.audio.tremolo",
            "op.audio.autopan",
            "op.audio.phaser",
            "op.audio.bitcrusher",
            "op.audio.distortion",
            "op.audio.noise_gate",
            "op.audio.pitch_shifter",
            "op.audio.radio_voice",
        ] {
            let out = run(&fx(id, &[]), sine(440.0, 0.5));
            assert!(out.iter().all(|s| s.is_finite()), "{id}");
            let peak = out.iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!(peak <= 1.5, "{id}: peak {peak}");
        }
    }

    #[test]
    fn pitch_shifter_moves_an_octave() {
        let up = run(
            &fx(
                "op.audio.pitch_shifter",
                &[("semitones", Value::Float(12.0))],
            ),
            sine(220.0, 0.5),
        );
        let down = run(
            &fx(
                "op.audio.pitch_shifter",
                &[("semitones", Value::Float(-12.0))],
            ),
            sine(440.0, 0.5),
        );
        // half a second of 440 Hz has 220 rising crossings
        let (u, d) = (crossings(&up), crossings(&down));
        assert!((200..=240).contains(&u), "octave up: {u} crossings");
        assert!((95..=125).contains(&d), "octave down: {d} crossings");
        assert!(rms(&up) > 0.2, "the level holds");
    }

    #[test]
    fn gate_silences_quiet_signals_and_passes_loud_ones() {
        let quiet = run(&fx("op.audio.noise_gate", &[]), sine(440.0, 0.002));
        let tail = &quiet[quiet.len() / 2..];
        assert!(rms(tail) < 1e-4, "quiet hum is gated");
        let loud = run(&fx("op.audio.noise_gate", &[]), sine(440.0, 0.5));
        assert!(rms(&loud[loud.len() / 2..]) > 0.3);
    }

    #[test]
    fn bitcrusher_reduces_levels() {
        let out = run(
            &fx(
                "op.audio.bitcrusher",
                &[("bits", Value::Int(2)), ("downsample", Value::Int(1))],
            ),
            sine(440.0, 0.9),
        );
        let mut levels: Vec<i32> = out.iter().map(|s| (s * 1000.0).round() as i32).collect();
        levels.sort();
        levels.dedup();
        assert!(levels.len() <= 5, "{levels:?}");
    }

    #[test]
    fn tremolo_and_telephone_shape_the_sound() {
        let t = run(
            &fx("op.audio.tremolo", &[("depth", Value::Float(100.0))]),
            sine(440.0, 0.5),
        );
        // the gain dips to silence once per cycle: some 10 ms windows are nearly silent
        let quietest = t.chunks(960).map(rms).fold(f32::MAX, f32::min);
        assert!(quietest < 0.05, "{quietest}");
        // a telephone does not carry 80 Hz
        let low = run(&fx("op.audio.radio_voice", &[]), sine(80.0, 0.5));
        assert!(rms(&low[low.len() / 2..]) < 0.1, "{}", rms(&low));
    }

    #[test]
    fn constant_power_keeps_power() {
        for i in 0..=10 {
            let (a, b) = crossfade("op.atr.constant_power", i as f64 / 10.0);
            assert!((a * a + b * b - 1.0).abs() < 1e-9);
        }
        assert_eq!(crossfade("op.atr.constant_gain", 0.25), (0.75, 0.25));
        let (a, b) = crossfade("op.atr.exponential_fade", 0.0);
        assert!((a - 1.0).abs() < 1e-9 && b.abs() < 1e-9);
    }
}
