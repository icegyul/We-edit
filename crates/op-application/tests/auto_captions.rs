//! Automatic captions end to end with a real speech model (run with
//! `OP_SPEECH_MODELS=<folder with whisper-tiny> OP_SPEECH_WAV=<speech wav> cargo test -- --ignored`;
//! OP_CAPTION_PNG, if set, receives a rendered frame).

use std::time::{Duration, Instant};

use op_application::captions::{CaptionOptions, ModelSize, Task, TranscribeOptions};
use op_application::{Dirs, Editor, Focus, Preferences};
use op_core::*;

#[test]
#[ignore]
fn transcribes_the_sequence_into_animated_captions() {
    let models = std::path::PathBuf::from(std::env::var("OP_SPEECH_MODELS").unwrap());
    let wav = std::path::PathBuf::from(std::env::var("OP_SPEECH_WAV").unwrap());
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    // the model the user downloaded earlier
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
    assert!(ed.active_seq().unwrap().duration().seconds() > 5.0);

    assert!(ed.transcribe_captions(TranscribeOptions {
        model: ModelSize::Tiny,
        language: None,
        task: Task::Transcribe,
        captions: CaptionOptions {
            style: 2,
            max_words: 3,
            uppercase: false,
        },
    }));
    let start = Instant::now();
    while ed.captioning.is_some() && start.elapsed() < Duration::from_secs(240) {
        ed.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("status: {:?}", ed.status);
    // the status says which language was heard
    assert!(format!("{:?}", ed.status).contains("captions in English"));
    let cues = ed.caption_cues();
    let text: Vec<String> = cues.iter().map(|c| c.text()).collect();
    eprintln!("{text:?}");
    assert!(cues.len() >= 4, "{text:?}");
    let all = text.join(" ").to_lowercase();
    assert!(all.contains("video") && all.contains("caption"), "{all}");
    assert!(cues.iter().all(|c| c.words.len() <= 3));
    // captions follow each other without overlapping
    for w in cues.windows(2) {
        assert!(w[0].end <= w[1].start + 1e-6, "{:?} / {:?}", w[0], w[1]);
    }
    if let Ok(png) = std::env::var("OP_CAPTION_PNG") {
        let gpu = op_render::Gpu::headless().unwrap();
        // a moment in the middle of the third caption
        let t = (cues[2].start + cues[2].end) / 2.0;
        op_application::export::export_frame(
            &ed.project,
            sid,
            SeqTime::from_seconds(t),
            std::path::Path::new(&png),
            gpu,
            ed.media.clone(),
        )
        .unwrap();
    }
}
