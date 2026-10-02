//! Mel filter bank as Whisper was trained with (librosa defaults: Slaney mel scale and area
//! normalization), laid out as `n_mels` rows of `n_fft / 2 + 1` frequency bins.

fn hz_to_mel(f: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f64.ln() / 27.0;
    if f >= min_log_hz {
        min_log_mel + (f / min_log_hz).ln() / logstep
    } else {
        f / f_sp
    }
}

fn mel_to_hz(m: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f64.ln() / 27.0;
    if m >= min_log_mel {
        min_log_hz * (logstep * (m - min_log_mel)).exp()
    } else {
        f_sp * m
    }
}

pub fn filters(n_mels: usize, n_fft: usize, sample_rate: u32) -> Vec<f32> {
    let bins = n_fft / 2 + 1;
    let sr = sample_rate as f64;
    let fft_freqs: Vec<f64> = (0..bins).map(|i| i as f64 * sr / n_fft as f64).collect();
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(sr / 2.0));
    let mel_f: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (n_mels + 1) as f64))
        .collect();
    let mut out = vec![0f32; n_mels * bins];
    for i in 0..n_mels {
        let (f0, f1, f2) = (mel_f[i], mel_f[i + 1], mel_f[i + 2]);
        let enorm = 2.0 / (f2 - f0);
        for (k, &f) in fft_freqs.iter().enumerate() {
            let lower = (f - f0) / (f1 - f0);
            let upper = (f2 - f) / (f2 - f1);
            let w = lower.min(upper).max(0.0);
            out[i * bins + k] = (w * enorm) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_scale_round_trips_and_filters_cover_the_band() {
        for f in [0.0, 300.0, 1000.0, 4000.0, 8000.0] {
            assert!((mel_to_hz(hz_to_mel(f)) - f).abs() < 1e-6);
        }
        let fb = filters(80, 400, 16000);
        assert_eq!(fb.len(), 80 * 201);
        // every filter has weight somewhere, and the bank has none above Nyquist
        for i in 0..80 {
            assert!(
                fb[i * 201..(i + 1) * 201].iter().any(|w| *w > 0.0),
                "filter {i}"
            );
        }
        // the first filter peaks near 0.0249 (40 Hz bins, Slaney area normalization)
        let peak = fb[..201].iter().copied().fold(0.0f32, f32::max);
        assert!((peak - 0.0249).abs() < 0.001, "{peak}");
    }
}
