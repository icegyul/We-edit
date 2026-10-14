//! A picture that cannot be read back from the graphics card must not end up in the file as zeros
//! (a green frame): the export renders that frame again, and stops with an error when it keeps
//! failing. (Seen once in a real export: from one failed read on, every other frame was green to the
//! end of the file.)

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use op_application::{ExportJob, ExportOptions, ExportSettings, MediaService};
use op_core::*;
use op_media::{Muxer, VideoCodec, VideoDecoder, VideoSettings};
use op_render::Gpu;
use op_timeline::{EditOptions, Patch, SourceClip, overwrite};

const W: u32 = 640;
const H: u32 = 360;
const FRAMES: u32 = 24;

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("source.mp4");
    let v = VideoSettings { codec: VideoCodec::H264, width: W, height: H, rate: Rate::FPS_25, bitrate_kbps: None, quality: 23, hardware: false };
    let mut m = Muxer::create(&path, Some(&v), None).unwrap();
    let chroma = vec![140u8; (W / 2 * H / 2) as usize];
    for i in 0..FRAMES {
        let luma: Vec<u8> = (0..W * H).map(|p| (60 + ((p % W) + i * 5) % 150) as u8).collect();
        m.push_video(&[&luma, &chroma, &chroma]).unwrap();
    }
    m.finish().unwrap();
    path
}

fn export(gpu: &Arc<Gpu>, dir: &std::path::Path, name: &str, src: &std::path::Path) -> (Result<(), String>, std::path::PathBuf) {
    let mut p = Project::new("Readback");
    let root = p.root;
    let (sid, _) = p.add_sequence(root, "S", SequenceSettings { width: W, height: H, rate: Rate::FPS_25, ..Default::default() });
    let (_, item) = p.add_asset(root, op_media::probe(src).unwrap());
    let spec = SourceClip::from_item(&p, item).unwrap();
    let patch = Patch { video: Some(0), audio: vec![] };
    let (next, _) = p.transact(|p| overwrite(p, sid, &spec, SeqTime::ZERO, &patch, EditOptions::default())).unwrap();
    let path = dir.join(format!("{name}.mp4"));
    let settings = ExportSettings {
        path: path.clone(),
        sequence: sid,
        range: SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(FRAMES as f64 / 25.0)),
        video: Some(VideoSettings { codec: VideoCodec::H264, width: W, height: H, rate: Rate::FPS_25, bitrate_kbps: None, quality: 20, hardware: false }),
        audio: None,
    };
    let media = MediaService::new(dir.join(format!("cache-{name}")), 64 << 20);
    let job = ExportJob::start_with(Arc::new(next), settings, ExportOptions { preview: false, ..Default::default() }, gpu.clone(), media);
    let began = Instant::now();
    while !job.finished() && began.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pr = job.progress.lock().clone();
    assert!(pr.done, "{name}: the export did not finish");
    (pr.error.map_or(Ok(()), Err), path)
}

/// Frames of `path` whose chroma is far from neutral (all-zero planes decode to 0 / 0 here).
fn green_frames(path: &std::path::Path) -> (usize, usize) {
    let mut d = VideoDecoder::open(path, None, None, Default::default(), false).unwrap();
    let (mut total, mut green) = (0, 0);
    for i in 0..FRAMES as i64 {
        let f = d.frame(i).unwrap();
        let mean = |p: &op_media::Plane| p.data.iter().map(|b| *b as u64).sum::<u64>() as f64 / p.data.len().max(1) as f64;
        total += 1;
        if mean(&f.planes[1]) < 60.0 || mean(&f.planes[2]) < 60.0 {
            green += 1;
        }
    }
    (total, green)
}

#[test]
fn a_failed_readback_is_rendered_again_and_never_written_as_zeros() {
    let Ok(gpu) = Gpu::headless() else {
        eprintln!("no graphics adapter: skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());

    // the first two reads fail: the frame comes out right on the third try
    op_render::output::FAIL_NEXT_READS.store(2, Ordering::Relaxed);
    let (result, path) = export(&gpu, dir.path(), "retried", &src);
    assert_eq!(op_render::output::FAIL_NEXT_READS.load(Ordering::Relaxed), 0, "the failures were not used");
    result.expect("the export should survive failed reads");
    assert_eq!(green_frames(&path), (FRAMES as usize, 0), "frames written as zeros");

    // reads that keep failing stop the export; no file of zeros is left behind
    op_render::output::FAIL_NEXT_READS.store(1000, Ordering::Relaxed);
    let (result, path) = export(&gpu, dir.path(), "failing", &src);
    op_render::output::FAIL_NEXT_READS.store(0, Ordering::Relaxed);
    let e = result.expect_err("an export whose frames cannot be read must fail");
    assert!(e.contains("could not hand over"), "{e}");
    assert!(!path.exists(), "a file of empty frames was left");
}
