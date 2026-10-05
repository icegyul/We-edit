//! Silence, filler words and repeated takes for OpenPremier, in pure Rust.
//!
//! Detection comes in two families that can be combined:
//!
//! - **Sound**: [`analyze`] measures 16 kHz mono audio once (loudness, voice-band energy, a
//!   neural voice detector) and [`detect`] turns those curves into silences under any
//!   [`Params`]: five ways to tell sound from silence, a minimum silence and sound length, and the
//!   air to leave around the words.
//! - **Words**: [`text`] works on a transcript: dead air between words, filler words in several
//!   languages, words and phrases said twice, and takes said again (the last one stays).
//!
//! The references behind these choices: auto-editor (loudness margin and combining methods),
//! unsilence (speeding silence up instead of cutting it), tightcut (Whisper word gaps), Recut and
//! TimeBolt (threshold, padding and minimum length), Drift (voice-activity detection, cutting at
//! the quietest point, a short de-click at every cut) and Earshot (the neural detector).

pub mod analysis;
pub mod detect;
pub mod presets;
pub mod text;

pub use analysis::{Analysis, Cancelled, HOP, RATE, analyze};
pub use detect::{Method, Params, Range, activity, detect, snap_to_quiet, total};
pub use text::{Cut, Kind, TextParams, Word};

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    /// A voice-like signal: a 150 Hz tone with harmonics, wobbling in volume, `secs` long.
    fn voice(secs: f64, amp: f32) -> Vec<f32> {
        (0..(secs * RATE as f64) as usize)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let env = 0.6 + 0.4 * (t * 9.0 * std::f32::consts::TAU).sin();
                let tone: f32 = (1..=12)
                    .map(|h| (t * 150.0 * h as f32 * std::f32::consts::TAU).sin() / h as f32)
                    .sum();
                tone * env * amp * 0.3
            })
            .collect()
    }

    /// Steady hiss.
    fn hiss(secs: f64, amp: f32) -> Vec<f32> {
        let mut x = 12345u32;
        (0..(secs * RATE as f64) as usize)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }

    fn mix(parts: &[Vec<f32>]) -> Vec<f32> {
        parts.concat()
    }

    fn run(pcm: &[f32], neural: bool) -> Analysis {
        analyze(pcm, neural, &mut |_| {}, &AtomicBool::new(false)).unwrap()
    }

    /// Speech - 2 s of near silence - speech - 1 s - speech, with a bit of room hiss throughout.
    fn podcast() -> Vec<f32> {
        let noise = |s| hiss(s, 0.002);
        let with_noise = |v: Vec<f32>| {
            let n = hiss(v.len() as f64 / RATE as f64, 0.002);
            v.iter().zip(n).map(|(a, b)| a + b).collect::<Vec<f32>>()
        };
        mix(&[
            with_noise(voice(2.0, 1.0)),
            noise(2.0),
            with_noise(voice(2.0, 1.0)),
            noise(1.0),
            with_noise(voice(1.5, 1.0)),
        ])
    }

    #[test]
    fn loudness_finds_the_pauses_and_leaves_the_padding() {
        let a = run(&podcast(), false);
        let p = Params {
            method: Method::Loudness {
                threshold_db: -45.0,
            },
            min_silence: 0.5,
            pad_before: 0.1,
            pad_after: 0.1,
            ..Default::default()
        };
        let cuts = detect(&a, &p);
        assert_eq!(cuts.len(), 2, "{cuts:?}");
        // the first pause is 2.0 .. 4.0 s, less 0.1 s of air at each side
        assert!(
            (cuts[0].start - 2.1).abs() < 0.05 && (cuts[0].end - 3.9).abs() < 0.05,
            "{cuts:?}"
        );
        assert!(
            (cuts[1].start - 6.1).abs() < 0.05 && (cuts[1].end - 6.9).abs() < 0.05,
            "{cuts:?}"
        );
        // a longer minimum keeps the shorter pause
        let long = detect(
            &a,
            &Params {
                min_silence: 1.5,
                ..p
            },
        );
        assert_eq!(long.len(), 1);
    }

    #[test]
    fn adaptive_and_relative_follow_the_recordings_own_level() {
        // the same podcast, ten times quieter: a fixed -45 dB would call everything silence
        let quiet: Vec<f32> = podcast().iter().map(|v| v * 0.01).collect();
        let a = run(&quiet, false);
        let fixed = detect(
            &a,
            &Params {
                method: Method::Loudness {
                    threshold_db: -45.0,
                },
                ..Default::default()
            },
        );
        assert!(
            total(&fixed) > 6.0,
            "the fixed level cuts nearly everything: {fixed:?}"
        );
        for method in [
            Method::Adaptive { margin_db: 9.0 },
            Method::Relative {
                below_peak_db: 25.0,
            },
        ] {
            let cuts = detect(
                &a,
                &Params {
                    method,
                    min_silence: 0.5,
                    ..Default::default()
                },
            );
            assert_eq!(cuts.len(), 2, "{method:?}: {cuts:?}");
        }
    }

    #[test]
    fn the_speech_band_and_the_neural_detector_ignore_steady_noise() {
        // loud hiss where the pauses are: a level threshold hears sound, the detectors do not
        let loud_noise = |s| hiss(s, 0.05);
        let pcm = mix(&[voice(2.0, 1.0), loud_noise(2.0), voice(2.0, 1.0)]);
        let a = run(&pcm, true);
        let by_level = detect(
            &a,
            &Params {
                method: Method::Loudness {
                    threshold_db: -45.0,
                },
                min_silence: 0.5,
                ..Default::default()
            },
        );
        assert!(by_level.is_empty(), "{by_level:?}");
        let by_speech = detect(
            &a,
            &Params {
                method: Method::Speech { sensitivity: 0.5 },
                min_silence: 0.5,
                ..Default::default()
            },
        );
        assert_eq!(by_speech.len(), 1, "{by_speech:?}");
        assert!(by_speech[0].start > 1.9 && by_speech[0].end < 4.1);
        // the neural curve exists and the voice-only filter turns the noise into silence too
        assert!(a.has_neural());
        let voice_only = detect(
            &a,
            &Params {
                method: Method::Loudness {
                    threshold_db: -45.0,
                },
                min_silence: 0.5,
                voice_only: true,
                ..Default::default()
            },
        );
        assert!(
            !voice_only.is_empty(),
            "voice-only should cut the noisy pause: {voice_only:?}"
        );
    }

    #[test]
    fn blips_are_not_sound() {
        // a 30 ms click in the middle of a long silence
        let mut pcm = hiss(4.0, 0.001);
        for s in pcm[32_000..32_480].iter_mut() {
            *s = 0.5;
        }
        let a = run(&pcm, false);
        let p = Params {
            method: Method::Loudness {
                threshold_db: -45.0,
            },
            min_silence: 0.5,
            pad_before: 0.0,
            pad_after: 0.0,
            ..Default::default()
        };
        assert_eq!(
            detect(
                &a,
                &Params {
                    min_sound: 0.0,
                    ..p
                }
            )
            .len(),
            2
        );
        assert_eq!(
            detect(
                &a,
                &Params {
                    min_sound: 0.1,
                    ..p
                }
            )
            .len(),
            1
        );
    }

    #[test]
    fn snapping_moves_an_edge_to_the_quiet_moment() {
        let pcm = mix(&[voice(1.0, 1.0), hiss(1.0, 0.001), voice(1.0, 1.0)]);
        let a = run(&pcm, false);
        // an approximate word end, 80 ms into the speech
        let r = snap_to_quiet(
            Range {
                start: 0.92,
                end: 2.05,
            },
            &a,
            0.15,
        );
        assert!(r.start >= 0.99, "{r:?}");
        assert!(r.end <= 2.0 + 1e-6, "{r:?}");
    }

    fn words(text: &str) -> Vec<Word> {
        // 0.3 s per word, a 0.05 s gap; a '|' is a 1 s pause
        let mut t = 0.0;
        let mut out = Vec::new();
        for w in text.split_whitespace() {
            if w == "|" {
                t += 1.0;
                continue;
            }
            out.push(Word {
                text: w.to_string(),
                start: t,
                end: t + 0.3,
            });
            t += 0.35;
        }
        out
    }

    #[test]
    fn fillers_come_out_in_each_language() {
        let en = words("so um this is uh the plan you know");
        let p = TextParams {
            aggressive_fillers: false,
            ..Default::default()
        };
        let cuts = text::find_fillers(&en, &p);
        let said: Vec<&str> = cuts.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(said, ["um", "uh"]);
        let p = TextParams {
            aggressive_fillers: true,
            ..Default::default()
        };
        let said: Vec<String> = text::find_fillers(&en, &p)
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert!(said.contains(&"you know".to_string()), "{said:?}");
        let es = words("Eh, bueno, o sea, esto es el plan, éh");
        let p = TextParams {
            language: "es".into(),
            aggressive_fillers: true,
            ..Default::default()
        };
        let said: Vec<String> = text::find_fillers(&es, &p)
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert_eq!(said.len(), 4, "{said:?}");
        // a custom list
        let p = TextParams {
            fillers: false,
            custom: "plan, the".into(),
            ..Default::default()
        };
        assert_eq!(text::find_fillers(&en, &p).len(), 2);
    }

    #[test]
    fn repeated_words_and_phrases_keep_the_last_copy() {
        let w = words("I I think we we should go to the to the store");
        let cuts = text::find_repeats(&w, &TextParams::default());
        let said: Vec<&str> = cuts.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(said, ["I", "we", "to the"]);
        // each cut stops just before the word that stays
        assert!(cuts[0].range.end <= w[1].start && cuts[0].range.start == w[0].start);
        // words only: the phrase stays
        let p = TextParams {
            repeated_phrases: false,
            ..Default::default()
        };
        assert_eq!(text::find_repeats(&w, &p).len(), 2);
        // three in a row cost two copies
        let w = words("no no no way");
        assert_eq!(text::find_repeats(&w, &TextParams::default()).len(), 2);
    }

    #[test]
    fn a_sentence_said_again_is_a_retake() {
        let w = words(
            "Welcome to the channel today we talk about cooking | Welcome to the channel where today we talk about cooking eggs",
        );
        let p = TextParams {
            retakes: true,
            ..Default::default()
        };
        let cuts = text::find_retakes(&w, &p);
        assert_eq!(cuts.len(), 1, "{cuts:?}");
        assert_eq!(cuts[0].range.start, 0.0);
        assert!(cuts[0].text.starts_with("Welcome") && cuts[0].text.ends_with("cooking"));
        // different sentences are left alone
        let w = words("First we mix the flour | then we add the eggs and milk");
        assert!(text::find_retakes(&w, &p).is_empty());
    }

    #[test]
    fn dead_air_between_words_and_merging() {
        let w = words("hello there | and now the point");
        let p = TextParams {
            gap: 0.5,
            pad_before: 0.1,
            pad_after: 0.1,
            ..Default::default()
        };
        let gaps = text::gaps(&w, 3.1, &p);
        assert_eq!(gaps.len(), 1);
        assert!(
            (gaps[0].range.len() - (1.05 - 0.2)).abs() < 1e-6,
            "{gaps:?}"
        );
        // overlapping cuts become one
        let merged = text::merge(vec![
            Cut {
                range: Range {
                    start: 1.0,
                    end: 2.0,
                },
                kind: Kind::Filler,
                text: "um".into(),
            },
            Cut {
                range: Range {
                    start: 1.5,
                    end: 2.5,
                },
                kind: Kind::Gap,
                text: String::new(),
            },
            Cut {
                range: Range {
                    start: 4.0,
                    end: 5.0,
                },
                kind: Kind::Repeat,
                text: "we".into(),
            },
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[0].range,
            Range {
                start: 1.0,
                end: 2.5
            }
        );
    }
}
