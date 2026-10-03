//! Heavy-load benchmark: exports one second of 1080p with no effects, with 100 effects on one
//! clip, and with ten stacked half-transparent layers of ten effects each, and reports frames
//! per second. Run with `cargo test --release -- --ignored --nocapture`; OP_STRESS_SOFTWARE=1
//! renders with the processor's rasterizer instead, like a computer without a graphics card.

use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::{ExportJob, ExportSettings, MediaService};
use op_core::*;
use op_media::{Muxer, VideoCodec, VideoSettings};
use op_render::Gpu;
use op_timeline::{EditOptions, Patch, SourceClip, overwrite};

const W: u32 = 1920;
const H: u32 = 1080;
const FRAMES: u32 = 100;

/// Frames exported per case (OP_STRESS_FRAMES, at most FRAMES; few for the slow software runs).
fn frames() -> u32 {
    std::env::var("OP_STRESS_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(FRAMES)
        .clamp(1, FRAMES)
}

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("source.mp4");
    let v = VideoSettings {
        codec: VideoCodec::H264,
        width: W,
        height: H,
        rate: Rate::FPS_25,
        bitrate_kbps: None,
        quality: 23,
        hardware: false,
    };
    let mut m = Muxer::create(&path, Some(&v), None).unwrap();
    let chroma = vec![128u8; (W / 2 * H / 2) as usize];
    for i in 0..FRAMES {
        let luma: Vec<u8> = (0..W * H)
            .map(|p| (30 + ((p % W) + (p / W) + i * 8) % 200) as u8)
            .collect();
        m.push_video(&[&luma, &chroma, &chroma]).unwrap();
    }
    m.finish().unwrap();
    path
}

/// Every video effect of the catalog, repeated up to `n`.
fn effects(n: usize) -> Vec<&'static catalog::EffectDef> {
    let all: Vec<&catalog::EffectDef> = catalog::CATALOG
        .iter()
        .filter(|d| d.kind == catalog::EffectKind::VideoEffect)
        .collect();
    all.iter().cycle().take(n).copied().collect()
}

/// A sequence with `layers` stacked clips, each with `per_layer` effects.
fn project(src: &std::path::Path, layers: usize, per_layer: usize) -> (Arc<Project>, SequenceId) {
    let mut p = Project::new("Stress");
    let root = p.root;
    let (sid, _) = p.add_sequence(
        root,
        "S",
        SequenceSettings {
            width: W,
            height: H,
            video_tracks: layers.max(1),
            ..Default::default()
        },
    );
    let (_, item) = p.add_asset(root, op_media::probe(src).unwrap());
    let spec = SourceClip::from_item(&p, item).unwrap();
    let mut fx = effects(layers * per_layer).into_iter();
    for layer in 0..layers {
        let patch = Patch {
            video: Some(layer),
            audio: vec![],
        };
        let (next, ids) = p
            .transact(|p| overwrite(p, sid, &spec, SeqTime::ZERO, &patch, EditOptions::default()))
            .unwrap();
        p = next;
        let mut ids_gen = p.ids.clone();
        let clip = p.sequence_mut(sid).unwrap().clip_mut(ids[0]).unwrap();
        for _ in 0..per_layer {
            if let Some(def) = fx.next() {
                clip.components.push(Component::new(def, &mut ids_gen));
            }
        }
        // layers above the first let the ones below show through
        if layer > 0
            && let Some(o) = clip.component_mut(catalog::OPACITY)
            && let Some(prm) = o.params.iter_mut().find(|x| x.key == "opacity")
        {
            prm.value = Value::Float(50.0);
        }
        p.ids = ids_gen;
    }
    (Arc::new(p), sid)
}

fn export(
    gpu: &Arc<Gpu>,
    dir: &std::path::Path,
    name: &str,
    p: Arc<Project>,
    sid: SequenceId,
) -> (f64, f32, [f32; 4]) {
    let path = dir.join(format!("{name}.mov"));
    let settings = ExportSettings {
        path,
        sequence: sid,
        range: SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(frames() as f64 / 25.0)),
        video: Some(VideoSettings {
            // a light intra codec, so the time is the rendering's
            codec: VideoCodec::ProRes422Hq,
            width: W,
            height: H,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 23,
            hardware: false,
        }),
        audio: None,
    };
    let media = MediaService::new(dir.join(format!("cache-{name}")), 512 << 20);
    let start = Instant::now();
    let job = ExportJob::start(p, settings, gpu.clone(), media);
    while !job.finished() && start.elapsed() < Duration::from_secs(1800) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let secs = start.elapsed().as_secs_f64();
    let pr = job.progress.lock().clone();
    assert!(pr.done && pr.error.is_none(), "{name}: {:?}", pr.error);
    (frames() as f64 / secs, pr.fps, pr.stage_ms)
}

#[test]
#[ignore]
fn heavy_effect_stacks() {
    let software = std::env::var("OP_STRESS_SOFTWARE").is_ok_and(|v| v == "1");
    let gpu = if software {
        Gpu::software()
    } else {
        Gpu::headless()
    }
    .expect("a GPU adapter");
    eprintln!("adapter: {} ({:?})", gpu.info.name, gpu.info.device_type);
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let cases: [(&str, usize, usize); 4] = [
        ("plain", 1, 0),
        ("10 effects", 1, 10),
        ("100 effects", 1, 100),
        ("10 layers x 10 effects", 10, 10),
    ];
    for (name, layers, per) in cases {
        let (p, sid) = project(&src, layers, per);
        let (overall, running, ms) = export(&gpu, dir.path(), &name.replace(' ', "_"), p, sid);
        eprintln!(
            "{name:>24}: {overall:6.1} frames/s overall, {running:6.1} running; per frame: render {:.1} ms, readback {:.1} ms, encoder wait {:.1} ms",
            ms[0], ms[1], ms[2]
        );
    }
}
