//! Transcribes a 16 kHz mono 16-bit WAV: `transcribe <model folder> <wav> [language] [translate]`.

use std::sync::atomic::AtomicBool;

use op_speech::{Task, Transcriber};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let pcm: Vec<f32> = bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
        .collect();
    let task = if args.get(3).is_some_and(|t| t == "translate") {
        Task::Translate
    } else {
        Task::Transcribe
    };
    let mut t = Transcriber::load(std::path::Path::new(&args[0])).unwrap();
    let (lang, segs) = t
        .transcribe(
            &pcm,
            args.get(2).map(String::as_str),
            task,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    println!("[{lang}]");
    for s in segs.iter().take(6) {
        println!("{}", s.text);
    }
}
