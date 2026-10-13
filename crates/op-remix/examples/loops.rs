//! Prints the repeats found in a 22.05 kHz mono f32le file and a plan for a target length.
//! `cargo run -p op-remix --release --example loops -- track.f32 [target_seconds]`

use std::sync::atomic::AtomicBool;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a .f32 file (ffmpeg -ac 1 -ar 22050 -f f32le)");
    let target: Option<f64> = args.next().and_then(|t| t.parse().ok());
    let bytes = std::fs::read(&path).expect("readable file");
    let pcm: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let started = std::time::Instant::now();
    let a = op_remix::analyze(pcm, &mut |_| {}, &AtomicBool::new(false)).expect("analysis");
    println!("{path}: {:.1} s, {:.1} bpm, {} beats, analysed in {:.2} s", a.seconds, a.bpm, a.beats.len(), started.elapsed().as_secs_f32());
    let t = std::time::Instant::now();
    let loops = a.loops(4.0, a.seconds * 0.9, 300);
    println!("{} loops in {:.2} s; best:", loops.len(), t.elapsed().as_secs_f32());
    for l in loops.iter().take(8) {
        println!("  {:.3} -> {:.3}  length {:.3}  score {:.4}", l.start, l.end, l.length(), l.score);
    }
    if let Some(target) = target {
        let p = op_remix::plan(&loops, &op_remix::PlanOptions { range: (0.0, a.seconds), target, min_loop: 4.0 });
        println!("target {target:.1} s -> {:.2} s with {} move(s):", p.duration, p.moves.len());
        for m in &p.moves {
            println!("  loop {:.3}-{:.3} (score {:.3}) x{}", m.at.start, m.at.end, m.at.score, m.times);
        }
        println!("  pieces: {:?}", p.segments.iter().map(|s| (((s.0 * 100.0).round()) / 100.0, ((s.1 * 100.0).round()) / 100.0)).collect::<Vec<_>>());
    }
}
