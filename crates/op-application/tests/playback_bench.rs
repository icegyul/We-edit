//! Playback benchmark on a real clip: renders frames of a sequence the way the program monitor
//! does (decoder read-ahead, GPU compositing at a reduced scale) and reports frames per second.
//! Needs PLAY_CLIP (a video file); PLAY_START seconds into it (default 30), PLAY_FRAMES frames
//! (default 90), PLAY_SCALE the render scale (default 0.25), PLAY_TRACKS how many tracks of the
//! same clip are stacked (default 1). Run with `cargo test --release --test playback_bench --
//! --nocapture`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::media::{HardwareDecoding, PreviewFrames};
use op_application::MediaService;
use op_core::*;
use op_render::{Gpu, Renderer, Request};
use op_timeline::{EditOptions, Patch, SourceClip, overwrite};

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn playback_speed_of_a_real_clip() {
    let Ok(path) = std::env::var("PLAY_CLIP") else { return };
    let start_s: f64 = env("PLAY_START", 30.0);
    let frames: u32 = env("PLAY_FRAMES", 90);
    let scale: f32 = env("PLAY_SCALE", 0.25);
    let tracks: usize = env("PLAY_TRACKS", 1);
    // the clip starts this many seconds before the playing begins (so playing starts inside it)
    let offset: f64 = env("PLAY_OFFSET", 5.0);
    let gpu = Gpu::headless().expect("a graphics adapter");
    eprintln!("adapter: {}", gpu.info.name);
    let probed = op_media::probe(std::path::Path::new(&path)).unwrap();
    let v = probed.video.clone().unwrap();
    eprintln!("clip: {}x{} {} {} {:.2} fps", v.width, v.height, v.codec, v.pixel_format, probed.frame_rate().unwrap_or(v.rate).as_f64());
    let rate = probed.frame_rate().unwrap_or(v.rate);

    let mut p = Project::new("Bench");
    let root = p.root;
    let (sid, _) = p.add_sequence(
        root,
        "S",
        SequenceSettings { width: v.width, height: v.height, rate, video_tracks: tracks.max(1), ..Default::default() },
    );
    let (_, item) = p.add_asset(root, probed);
    let spec = SourceClip::from_item(&p, item).unwrap();
    for t in 0..tracks.max(1) {
        let patch = Patch { video: Some(t), audio: vec![] };
        let (next, _) = p
            .transact(|p| overwrite(p, sid, &spec, SeqTime::from_seconds(start_s - offset), &patch, EditOptions::default()))
            .unwrap();
        p = next;
    }
    let project = Arc::new(p);

    for mode in [HardwareDecoding::Always, HardwareDecoding::Never] {
        let dir = tempfile::tempdir().unwrap();
        let service = MediaService::new(dir.path().join("cache"), 1536 << 20);
        service.set_hardware_decoding(mode);
        service.set_assets(project.assets.iter());
        let mut renderer = Renderer::new(gpu.clone());
        let frame_source = PreviewFrames { service: service.clone(), wait: Duration::from_secs(5), ahead: service.read_ahead() };
        // the first frame includes opening the file and seeking
        let t0 = Instant::now();
        let mut first = true;
        let mut first_ms = 0.0;
        let mut worst = 0.0f64;
        let begin = Instant::now();
        for i in 0..frames {
            let time = SeqTime::from_seconds(start_s + i as f64 / rate.as_f64());
            let t = Instant::now();
            let out = renderer.render(&Request { project: &project, sequence: sid, time, scale, source: &frame_source });
            let _ = renderer.rgba8(&out);
            renderer.recycle(out);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if first {
                first = false;
                first_ms = ms;
            } else {
                worst = worst.max(ms);
            }
        }
        let total = begin.elapsed().as_secs_f64();
        let steady = (frames - 1) as f64 / (total - first_ms / 1000.0).max(0.001);
        eprintln!(
            "PLAY {mode:?}: {frames} frames in {total:.2}s ({:.1} fps overall, {steady:.1} fps after the first), first {first_ms:.0} ms, worst later {worst:.0} ms, missing {}",
            frames as f64 / total,
            renderer.stats.missing_frames
        );
        let _ = t0;
    }
}


/// The same, on a saved project (PLAY_PROJECT, a copy of it): plays PLAY_FRAMES frames of its
/// first sequence from PLAY_START seconds, with every track it has.
#[test]
fn playback_speed_of_a_saved_project() {
    let Ok(path) = std::env::var("PLAY_PROJECT") else { return };
    let start_s: f64 = env("PLAY_START", 60.0);
    let frames: u32 = env("PLAY_FRAMES", 90);
    let scale: f32 = env("PLAY_SCALE", 0.25);
    let gpu = Gpu::headless().expect("a graphics adapter");
    let loaded = op_project::native::load(std::path::Path::new(&path)).unwrap();
    let project = Arc::new(loaded.project);
    let sid = *project.sequences.keys().next().unwrap();
    let seq = project.sequence(sid).unwrap();
    let rate = seq.rate();
    eprintln!(
        "project: {} sequences, first '{}' {}x{} {:.2} fps, {} video tracks, {} audio tracks, {} offline media",
        project.sequences.len(),
        seq.name,
        seq.settings.width,
        seq.settings.height,
        rate.as_f64(),
        seq.video.len(),
        seq.audio.len(),
        loaded.offline.len()
    );
    for mode in [HardwareDecoding::Always, HardwareDecoding::Never] {
        let dir = tempfile::tempdir().unwrap();
        let service = MediaService::new(dir.path().join("cache"), 1536 << 20);
        service.set_hardware_decoding(mode);
        service.set_assets(project.assets.iter());
        let mut renderer = Renderer::new(gpu.clone());
        let frame_source = PreviewFrames { service: service.clone(), wait: Duration::from_secs(5), ahead: service.read_ahead() };
        let (mut first_ms, mut worst) = (0.0f64, 0.0f64);
        let (mut stalls, mut stall_ms) = (0u32, 0.0f64);
        let begin = Instant::now();
        for i in 0..frames {
            let time = SeqTime::from_seconds(start_s + i as f64 / rate.as_f64());
            let t = Instant::now();
            let out = renderer.render(&Request { project: &project, sequence: sid, time, scale, source: &frame_source });
            let _ = renderer.rgba8(&out);
            renderer.recycle(out);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if i == 0 {
                first_ms = ms;
            } else {
                worst = worst.max(ms);
                // a frame later than 100 ms is a visible hitch at 30 fps
                if ms > 100.0 {
                    stalls += 1;
                    stall_ms += ms;
                }
            }
        }
        let total = begin.elapsed().as_secs_f64();
        eprintln!("PROJECT {mode:?}: {stalls} frames slower than 100 ms, {stall_ms:.0} ms in them, of {:.1} s of video", frames as f64 / rate.as_f64());
        eprintln!(
            "PROJECT {mode:?}: {frames} frames in {total:.2}s ({:.1} fps overall, {:.1} fps after the first), first {first_ms:.0} ms, worst later {worst:.0} ms, missing {}",
            frames as f64 / total,
            (frames - 1) as f64 / (total - first_ms / 1000.0).max(0.001),
            renderer.stats.missing_frames
        );
    }
}
