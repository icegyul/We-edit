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
