//! End-to-end export on every platform: a camera-like clip with sound and effects is exported
//! in each delivery format (hardware encoders first where they apply), and every file is read
//! back. CI sets OPENPREMIER_REQUIRE_GPU so a missing adapter fails instead of skipping.

use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::{ExportJob, ExportSettings, MediaService};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, Muxer, VideoCodec, VideoSettings};
use op_render::Gpu;
use op_timeline::{EditOptions, Patch, SourceClip, overwrite};

fn gpu() -> Option<Arc<Gpu>> {
    match Gpu::headless() {
        Ok(g) => Some(g),
        Err(e) if std::env::var("OPENPREMIER_REQUIRE_GPU").is_ok_and(|v| v == "1") => {
            panic!("no GPU adapter: {e}")
        }
        Err(e) => {
            eprintln!("no GPU adapter, skipped: {e}");
            None
        }
    }
}

/// A 956x720 H.264 + AAC clip (an odd-looking size, like phone footage), 1 second at 25 fps.
fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("camera.mp4");
    let (w, h) = (956u32, 720u32);
    let v = VideoSettings {
        codec: VideoCodec::H264,
        width: w,
        height: h,
        rate: Rate::FPS_25,
        bitrate_kbps: None,
        quality: 23,
        hardware: false,
    };
    let a = AudioSettings {
        codec: AudioCodec::Aac,
        rate: 48_000,
        channels: 2,
        bitrate_kbps: 160,
    };
    let mut m = Muxer::create(&path, Some(&v), Some(&a)).unwrap();
    let chroma = vec![128u8; (w / 2 * h / 2) as usize];
    for i in 0..25u32 {
        let luma: Vec<u8> = (0..w * h)
            .map(|p| (40 + (p % w) * 160 / w + i * 2) as u8)
            .collect();
        m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        let samples: Vec<f32> = (0..1920 * 2)
            .map(|k| {
                let t = (i * 1920 + k / 2) as f32 / 48_000.0;
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.3
            })
            .collect();
        m.push_audio(&samples).unwrap();
    }
    m.finish().unwrap();
    path
}

#[test]
fn every_format_with_effects_and_sound() {
    let Some(gpu) = gpu() else { return };
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let mut p = Project::new("Export");
    let root = p.root;
    let (sid, _) = p.add_sequence(
        root,
        "S",
        SequenceSettings {
            width: 956,
            height: 720,
            rate: Rate::FPS_25,
            ..Default::default()
        },
    );
    let (_, item) = p.add_asset(root, op_media::probe(&src).unwrap());
    let spec = SourceClip::from_item(&p, item).unwrap();
    let patch = Patch {
        video: Some(0),
        audio: vec![Some(0)],
    };
    let (mut p, ids) = p
        .transact(|p| overwrite(p, sid, &spec, SeqTime::ZERO, &patch, EditOptions::default()))
        .unwrap();
    // looks that run several GPU passes
    for effect in [
        "op.video.glow",
        "op.video.gaussian_blur",
        "op.video.vignette",
    ] {
        let mut next = p.ids.clone();
        let comp = Component::new(catalog::find(effect).unwrap(), &mut next);
        p.ids = next;
        let clip = p
            .sequence_mut(sid)
            .unwrap()
            .clip_mut(ids[0])
            .expect("the video clip");
        clip.components.push(comp);
    }
    let project = Arc::new(p);

    for codec in VideoCodec::ALL {
        let path = dir
            .path()
            .join(format!("out-{codec:?}.{}", codec.extension()));
        let audio = match codec {
            VideoCodec::H264 | VideoCodec::Hevc => Some(AudioSettings {
                codec: AudioCodec::Aac,
                rate: 48_000,
                channels: 2,
                bitrate_kbps: 192,
            }),
            VideoCodec::Png => None,
            _ => Some(AudioSettings {
                codec: AudioCodec::Pcm24,
                rate: 48_000,
                channels: 2,
                bitrate_kbps: 0,
            }),
        };
        let settings = ExportSettings {
            path: path.clone(),
            sequence: sid,
            range: SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(0.6)),
            video: Some(VideoSettings {
                codec,
                width: 956,
                height: 720,
                rate: Rate::FPS_25,
                bitrate_kbps: None,
                quality: 23,
                // the default of the export dialog; a failing hardware encoder falls back
                hardware: true,
            }),
            audio: audio.clone(),
        };
        let media = MediaService::new(dir.path().join(format!("cache-{codec:?}")), 64 << 20);
        let job = ExportJob::start(project.clone(), settings, gpu.clone(), media);
        let start = Instant::now();
        while !job.finished() && start.elapsed() < Duration::from_secs(180) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let pr = job.progress.lock().clone();
        assert!(pr.done, "{codec:?}: the export did not finish");
        assert!(
            pr.error.is_none(),
            "{codec:?} ({}): {:?}",
            pr.encoder,
            pr.error
        );
        eprintln!("{codec:?}: exported with {}", pr.encoder);

        let mut out = op_media::probe(&path).unwrap();
        let v = out.video.clone().expect("a video stream");
        assert_eq!((v.width, v.height), (956, 720), "{codec:?}");
        assert!((v.frames - 15).abs() <= 1, "{codec:?}: {} frames", v.frames);
        assert_eq!(
            out.audio.is_empty(),
            audio.is_none(),
            "{codec:?}: audio stream"
        );

        // the file decodes and shows the picture, not black or garbage
        out.id = AssetId(1);
        let out = Arc::new(out);
        let check = MediaService::new(dir.path().join(format!("check-{codec:?}")), 64 << 20);
        check.set_assets(std::iter::once((&out.id, &out)));
        let f = check
            .frame(
                &out,
                SrcTime::from_seconds(0.2),
                Duration::from_secs(30),
                false,
                0,
            )
            .unwrap_or_else(|| panic!("{codec:?}: no decoded frame"));
        let px = f.to_rgba8_scaled(1, 1);
        assert!(
            px[0] > 30 && px[0] < 250 && px[3] > 200,
            "{codec:?}: average color {px:?}"
        );
    }
}
