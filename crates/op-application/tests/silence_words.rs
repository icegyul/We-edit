//! Cutting by what was said, with a real speech model (run with `OP_SPEECH_MODELS=<folder with
//! whisper-tiny> OP_SPEECH_WAV=<16 kHz speech wav> cargo test -- --ignored --nocapture`).

use std::time::{Duration, Instant};

use op_application::captions::ModelSize;
use op_application::silence::{Scope, SilenceSettings};
use op_application::{Dirs, Editor, Focus, Preferences};
use op_core::*;

#[test]
#[ignore]
fn words_are_found_in_real_speech() {
    let models = std::path::PathBuf::from(std::env::var("OP_SPEECH_MODELS").unwrap());
    let wav = std::path::PathBuf::from(std::env::var("OP_SPEECH_WAV").unwrap());
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let src = ModelSize::Tiny.dir(&models);
    let dst = ModelSize::Tiny.dir(&op_application::captions::models_dir(&ed));
    std::fs::create_dir_all(&dst).unwrap();
    for f in std::fs::read_dir(&src).unwrap().flatten() {
        std::fs::copy(f.path(), dst.join(f.file_name())).unwrap();
    }
    let root = ed.project.root;
    let probed = op_media::probe(&wav).unwrap();
    let (_, item) = ed
        .edit("Import", |p| Ok(p.add_asset(root, probed)))
        .unwrap();
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    ed.load_source(item);
    ed.execute("cmd.clip.overlay", Focus::Timeline);

    let mut s = SilenceSettings {
        scope: Scope::Sequence,
        by_words: true,
        model: ModelSize::Tiny,
        language: Some("en".into()),
        ..Default::default()
    };
    s.words.retakes = true;
    s.words.aggressive_fillers = true;
    ed.analyze_silence(&s).unwrap();
    let start = Instant::now();
    while ed.silencing.is_some() && start.elapsed() < Duration::from_secs(300) {
        ed.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
    let d = ed.silence_result.clone().expect("an analysis");
    let words = d.words.clone().expect("a transcript");
    eprintln!(
        "said: {}",
        words
            .iter()
            .map(|w| w.text.trim())
            .collect::<Vec<_>>()
            .join(" ")
    );
    for c in d.cuts(&s) {
        eprintln!(
            "{:6.2} - {:6.2}  {:8}  {}",
            c.range.start,
            c.range.end,
            c.kind.label(),
            c.text
        );
    }
    assert!(!words.is_empty());
}
