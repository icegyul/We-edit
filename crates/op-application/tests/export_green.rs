//! Regression check for green frames in an export: exports a stretch of a saved project the way the
//! export dialog does (4K H.264, preview off) and decodes the result, counting frames whose
//! chroma is far from neutral (all planes zero shows as dark green). Needs EXPORT_PROJECT (a copy
//! of a project); EXPORT_START / EXPORT_SECONDS pick the stretch (default 0 and 20), EXPORT_PREVIEW=1
//! turns the progress picture on, EXPORT_HW=0 uses the software encoder. Run with `cargo test
//! --release --test export_green -- --nocapture`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::{ExportJob, ExportOptions, ExportSettings, MediaService};
use op_core::*;
use op_media::{Tuning, VideoCodec, VideoDecoder, VideoSettings};
use op_render::Gpu;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn an_export_has_no_green_frames() {
    let Ok(path) = std::env::var("EXPORT_PROJECT") else { return };
    let start: f64 = env("EXPORT_START", 0.0);
    let seconds: f64 = env("EXPORT_SECONDS", 20.0);
    let preview = env("EXPORT_PREVIEW", 0) == 1;
    let hardware = env("EXPORT_HW", 1) == 1;
    let gpu = Gpu::headless().expect("a graphics adapter");
    eprintln!("adapter: {}", gpu.info.name);
    let loaded = op_project::native::load(std::path::Path::new(&path)).unwrap();
    let project = Arc::new(loaded.project);
    for (i, (id, q)) in project.sequences.iter().enumerate() {
        eprintln!("sequence {i}: '{}' {}x{} {:.1} s {id:?}", q.name, q.settings.width, q.settings.height, q.duration().seconds());
    }
    let sid = *project.sequences.keys().nth(env("EXPORT_SEQ", 0usize)).unwrap();
    let seq = project.sequence(sid).unwrap();
    let rate = seq.rate();
    let (w, h) = (seq.settings.width, seq.settings.height);
    eprintln!(
        "sequence '{}' {w}x{h} {:.2} fps, {} video tracks, {} offline media, ends at {:.1} s",
        seq.name,
        rate.as_f64(),
        seq.video.len(),
        loaded.offline.len(),
        seq.duration().seconds()
    );
    // EXPORT_POLLER=1: another thread submits to the graphics queue all the time, like the window
    // drawing its frames; map callbacks of the export's readbacks can then fire on that thread
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if env("EXPORT_POLLER", 0) == 1 {
        let (g, stop) = (gpu.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                g.queue.submit(std::iter::empty());
                std::thread::sleep(Duration::from_micros(100));
            }
        });
    }
    let dir = tempfile::tempdir().unwrap();
    // EXPORT_KEEP=<file> keeps the exported file there (to look at it with other decoders)
    let out = std::env::var("EXPORT_KEEP").map(std::path::PathBuf::from).unwrap_or_else(|_| dir.path().join("out.mp4"));
    let settings = ExportSettings {
        path: out.clone(),
        sequence: sid,
        range: SeqRange::new(SeqTime::from_seconds(start), SeqTime::from_seconds(start + seconds)),
        video: Some(VideoSettings { codec: VideoCodec::H264, width: w, height: h, rate, bitrate_kbps: Some(45_000), quality: 20, hardware }),
        audio: None,
    };
    let media = MediaService::new(dir.path().join("cache"), 4096 << 20);
    // EXPORT_DECODE=always|never forces the decoder kind (the program's default is automatic)
    match std::env::var("EXPORT_DECODE").as_deref() {
        Ok("always") => media.set_hardware_decoding(op_application::media::HardwareDecoding::Always),
        Ok("never") => media.set_hardware_decoding(op_application::media::HardwareDecoding::Never),
        _ => {}
    }
    let began = Instant::now();
    let job = ExportJob::start_with(project, settings, ExportOptions { preview, tuning: Tuning::YOUTUBE }, gpu, media);
    while !job.finished() && began.elapsed() < Duration::from_secs(1800) {
        std::thread::sleep(Duration::from_millis(50));
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let pr = job.progress.lock().clone();
    assert!(pr.done && pr.error.is_none(), "export: {:?}", pr.error);
    eprintln!("exported {} frames in {:.1} s with {} (stage ms {:?})", pr.total, began.elapsed().as_secs_f32(), pr.encoder, pr.stage_ms);

    let mut dec = VideoDecoder::open(&out, None, None, ColorInfo::default(), false).unwrap();
    let mut bad = Vec::new();
    for i in 0..pr.total as i64 {
        let f = dec.frame(i).unwrap();
        // mean of each plane (8-bit planes of yuv420p)
        let mean = |p: &op_media::Plane| p.data.iter().step_by(7).map(|b| *b as u64).sum::<u64>() as f64 / (p.data.len() / 7).max(1) as f64;
        let (y, u, v) = (mean(&f.planes[0]), mean(&f.planes[1]), mean(&f.planes[2]));
        if u < 60.0 || v < 60.0 {
            bad.push((i, y, u, v));
        }
    }
    for (i, y, u, v) in bad.iter().take(40) {
        eprintln!("GREEN frame {i}: y {y:.0} u {u:.0} v {v:.0}");
    }
    eprintln!("{} of {} frames are green", bad.len(), pr.total);
    assert!(bad.is_empty(), "{} green frames", bad.len());
}
