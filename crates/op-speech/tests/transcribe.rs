//! Real transcription with a downloaded model (network and about 150 MB; run with
//! `OP_SPEECH_MODELS=<folder> OP_SPEECH_WAV=<16 kHz mono 16-bit wav> cargo test -- --ignored`).

use std::sync::atomic::AtomicBool;

use op_speech::{ModelSize, Task, Transcriber};

fn read_wav(path: &std::path::Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    // find the "data" chunk
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return bytes[i + 8..(i + 8 + len).min(bytes.len())]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
                .collect();
        }
        i += 8 + len;
    }
    panic!("no data chunk");
}

#[test]
#[ignore]
fn transcribes_speech_with_word_times() {
    let models =
        std::path::PathBuf::from(std::env::var("OP_SPEECH_MODELS").expect("OP_SPEECH_MODELS"));
    let wav = std::path::PathBuf::from(std::env::var("OP_SPEECH_WAV").expect("OP_SPEECH_WAV"));
    let size = ModelSize::Tiny;
    if !size.is_downloaded(&models) {
        size.download(&models, &mut |_, _| true).unwrap();
    }
    let pcm = read_wav(&wav);
    let start = std::time::Instant::now();
    let mut t = Transcriber::load(&size.dir(&models)).unwrap();
    let (lang, segs) = t
        .transcribe(
            &pcm,
            None,
            Task::Transcribe,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    eprintln!("{lang} in {:.1} s", start.elapsed().as_secs_f32());
    for s in &segs {
        eprintln!("[{:.2} - {:.2}] {}", s.start, s.end, s.text);
        for w in &s.words {
            eprintln!("    {:.2}-{:.2} {}", w.start, w.end, w.text);
        }
    }
    assert_eq!(lang, "en");
    let all: String = segs
        .iter()
        .map(|s| s.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(all.contains("video"), "{all}");
    assert!(all.contains("caption"), "{all}");
    let dur = pcm.len() as f64 / 16000.0;
    for s in &segs {
        assert!(
            s.start >= 0.0 && s.end <= dur + 1.0 && s.start < s.end,
            "{s:?}"
        );
    }
}
