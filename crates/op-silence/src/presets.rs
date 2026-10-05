//! Ready-made settings for the usual jobs, so nobody starts from a blank form.

use crate::detect::{Method, Params};
use crate::text::TextParams;

/// A named set of detection settings.
pub struct Preset {
    pub name: &'static str,
    pub about: &'static str,
    pub params: Params,
    pub text: TextParams,
    /// The transcript is needed (fillers, repeats).
    pub words: bool,
}

pub fn all() -> Vec<Preset> {
    let base = Params::default();
    let text = TextParams::default();
    vec![
        Preset {
            name: "Gentle",
            about: "Only long pauses; a natural rhythm. For interviews and lectures.",
            params: Params {
                method: Method::Adaptive { margin_db: 9.0 },
                min_silence: 1.0,
                pad_before: 0.25,
                pad_after: 0.3,
                ..base
            },
            text: TextParams {
                fillers: false,
                repeated_words: false,
                repeated_phrases: false,
                ..text.clone()
            },
            words: false,
        },
        Preset {
            name: "Podcast",
            about: "Tightens pauses and takes out \"um\" and \"uh\".",
            params: Params {
                method: Method::Adaptive { margin_db: 9.0 },
                min_silence: 0.5,
                pad_before: 0.12,
                pad_after: 0.18,
                ..base
            },
            text: TextParams {
                fillers: true,
                repeated_words: true,
                repeated_phrases: false,
                ..text.clone()
            },
            words: true,
        },
        Preset {
            name: "Tutorial",
            about: "Short pauses go, words said twice go, a repeated sentence keeps its last take.",
            params: Params {
                method: Method::Speech { sensitivity: 0.5 },
                min_silence: 0.4,
                pad_before: 0.08,
                pad_after: 0.12,
                ..base
            },
            text: TextParams {
                retakes: true,
                ..text.clone()
            },
            words: true,
        },
        Preset {
            name: "Jump Cuts",
            about: "Fast, tight cuts for social video; keeps almost no air.",
            params: Params {
                method: Method::Speech { sensitivity: 0.55 },
                min_silence: 0.2,
                min_sound: 0.1,
                pad_before: 0.04,
                pad_after: 0.07,
                voice_only: false,
            },
            text: TextParams {
                aggressive_fillers: true,
                retakes: true,
                ..text.clone()
            },
            words: true,
        },
        Preset {
            name: "Noisy Room",
            about: "Voice detector: ignores music, hum and background noise.",
            params: Params {
                method: Method::Neural { sensitivity: 0.5 },
                min_silence: 0.4,
                pad_before: 0.1,
                pad_after: 0.15,
                voice_only: true,
                ..base
            },
            text: TextParams {
                fillers: false,
                repeated_words: false,
                repeated_phrases: false,
                ..text
            },
            words: false,
        },
    ]
}
