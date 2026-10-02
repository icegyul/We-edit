//! Proxies end to end: created in the background, attached to the asset, played while enabled,
//! never used for export, and removed again.

use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::media::{ExactFrames, PreviewFrames};
use op_application::{Dirs, Editor, Preferences};
use op_core::*;
use op_media::{Muxer, VideoCodec, VideoSettings};
use op_render::FrameSource;

#[test]
fn preview_plays_the_proxy_and_export_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("camera.mp4");
    let v = VideoSettings {
        codec: VideoCodec::H264,
        width: 1920,
        height: 1080,
        rate: Rate::FPS_25,
        bitrate_kbps: None,
        quality: 23,
        hardware: false,
    };
    let mut m = Muxer::create(&src, Some(&v), None).unwrap();
    let luma = vec![150u8; 1920 * 1080];
    let chroma = vec![128u8; 960 * 540];
    for _ in 0..25 {
        m.push_video(&[&luma, &chroma, &chroma]).unwrap();
    }
    m.finish().unwrap();

    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let root = ed.project.root;
    let probed = op_media::probe(&src).unwrap();
    let (asset, _) = ed
        .edit("Import", |p| Ok(p.add_asset(root, probed)))
        .unwrap();

    ed.create_proxies(&[asset]);
    let start = Instant::now();
    while ed.project.asset(asset).unwrap().proxy.is_none()
        && start.elapsed() < Duration::from_secs(60)
    {
        ed.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    let proxy = ed
        .project
        .asset(asset)
        .unwrap()
        .proxy
        .clone()
        .expect("the proxy is attached");
    assert!(proxy.contains("Proxies"), "{proxy}");
    assert!(std::path::Path::new(&proxy).is_file());

    let a = ed.project.asset(asset).unwrap().clone();
    ed.media.set_assets(ed.project.assets.iter());
    let preview = PreviewFrames {
        service: ed.media.clone(),
        wait: Duration::from_secs(20),
        ahead: 0,
    };
    let exact = ExactFrames {
        service: ed.media.clone(),
    };
    let t = SrcTime::from_seconds(0.4);
    let f = preview.video_frame(&a, t).expect("a preview frame");
    assert_eq!((f.width, f.height), (960, 540), "preview plays the proxy");
    let f = exact.video_frame(&a, t).expect("an export frame");
    assert_eq!(
        (f.width, f.height),
        (1920, 1080),
        "export reads the original"
    );

    ed.set_use_proxies(false);
    let f = preview.video_frame(&a, t).expect("a preview frame");
    assert_eq!((f.width, f.height), (1920, 1080), "proxies off");
    ed.set_use_proxies(true);

    // asking again does not queue a second conversion
    ed.create_proxies(&[asset]);
    assert!(!ed.proxies.busy());

    ed.remove_proxies(&[asset]);
    assert!(ed.project.asset(asset).unwrap().proxy.is_none());
    assert!(
        !std::path::Path::new(&proxy).exists(),
        "our proxy file is deleted"
    );
    let a = Arc::new(ed.project.asset(asset).unwrap().clone());
    let f = preview.video_frame(&a, t).expect("a preview frame");
    assert_eq!((f.width, f.height), (1920, 1080));
}
