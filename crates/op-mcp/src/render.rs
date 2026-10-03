//! Pictures and files for assistants: a rendered frame as PNG, and exports.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use op_application::media::ExactFrames;
use op_application::{Editor, ExportJob, ExportSettings, MediaService};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, Muxer, VideoCodec, VideoSettings};
use op_render::{Gpu, Renderer, Request};
use serde_json::{Value as Json, json};

use crate::tools::Content;

/// One GPU device for the whole session.
fn gpu() -> Result<Arc<Gpu>, String> {
    static GPU: OnceLock<Result<Arc<Gpu>, String>> = OnceLock::new();
    GPU.get_or_init(|| Gpu::headless().map_err(|e| e.to_string()))
        .clone()
}

/// The sequence at `t`, `width` pixels wide, as PNG bytes.
pub fn frame_png(
    project: &Project,
    sid: SequenceId,
    t: SeqTime,
    width: u32,
    media: Arc<MediaService>,
) -> Result<Vec<u8>, String> {
    let seq = project.sequence(sid).ok_or("no sequence")?;
    media.set_assets(project.assets.iter());
    let scale = (width as f32 / seq.settings.width.max(1) as f32).min(1.0);
    let mut r = Renderer::new(gpu()?);
    let frames = ExactFrames { service: media };
    let frame = r.render(&Request {
        project,
        sequence: sid,
        time: t,
        scale,
        source: &frames,
    });
    let (w, h) = (frame.width, frame.height);
    let mut rgba = r.rgba8(&frame);
    r.recycle(frame);
    // over black, as the Program Monitor shows it (the image has straight alpha)
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = (*c as u16 * a / 255) as u8;
        }
        px[3] = 255;
    }
    let dir = std::env::temp_dir().join(format!("openpremier-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("frame.png");
    let v = VideoSettings {
        codec: VideoCodec::Png,
        width: w,
        height: h,
        rate: seq.rate(),
        bitrate_kbps: None,
        quality: 0,
        hardware: false,
    };
    let mut m = Muxer::create(&path, Some(&v), None).map_err(|e| e.to_string())?;
    m.push_video(&[&rgba]).map_err(|e| e.to_string())?;
    m.finish().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    Ok(bytes)
}

/// The `export` tool: writes the active sequence and waits for the file.
pub fn export(ed: &mut Editor, a: &Json) -> Result<Vec<Content>, String> {
    let (settings, path) = export_settings(ed, a)?;
    let began = Instant::now();
    let job = ExportJob::start(ed.snapshot(), settings, gpu()?, ed.media.clone());
    while !job.finished() {
        if began.elapsed() > Duration::from_secs(6 * 3600) {
            job.cancel();
            return Err("the export took more than six hours".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let pr = job.progress.lock().clone();
    if let Some(e) = pr.error {
        return Err(e);
    }
    Ok(vec![Content::Text(
        json!({
            "file": path.display().to_string(),
            "seconds": (began.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "encoder": pr.encoder,
        })
        .to_string(),
    )])
}

/// The `export` tool in the open program: the export runs like one started from its menu, with
/// its progress at the bottom right.
pub fn export_in_background(ed: &mut Editor, a: &Json) -> Result<Vec<Content>, String> {
    let (settings, path) = export_settings(ed, a)?;
    let job = ExportJob::start(ed.snapshot(), settings, gpu()?, ed.media.clone());
    ed.exports.push(job);
    Ok(vec![Content::Text(
        json!({"started": path.display().to_string(), "note": "the export runs in the open program; the file is complete when its progress reaches 100 %"})
            .to_string(),
    )])
}

fn export_settings(ed: &mut Editor, a: &Json) -> Result<(ExportSettings, PathBuf), String> {
    let sid = ed.active.ok_or("no sequence is open")?;
    let seq = ed.project.sequence(sid).ok_or("no sequence")?.clone();
    let path = PathBuf::from(
        a.get("path")
            .and_then(Json::as_str)
            .ok_or("path is required")?,
    );
    let format = a.get("format").and_then(Json::as_str).unwrap_or("h264");
    let codec = match format {
        "h264" | "mp4" => VideoCodec::H264,
        "hevc" | "h265" => VideoCodec::Hevc,
        "prores" => VideoCodec::ProRes422Hq,
        "prores4444" => VideoCodec::ProRes4444,
        "dnxhr" => VideoCodec::DnxhrHq,
        "png" => VideoCodec::Png,
        other => return Err(format!("unknown format {other}")),
    };
    let start = a.get("start").and_then(Json::as_f64).unwrap_or(0.0);
    let end = a
        .get("end")
        .and_then(Json::as_f64)
        .unwrap_or_else(|| seq.duration().seconds());
    if end <= start {
        return Err("the sequence is empty, or end is not after start".into());
    }
    let size = |k: &str, d: u32| {
        a.get(k)
            .and_then(Json::as_u64)
            .map(|v| v as u32)
            .unwrap_or(d)
    };
    let audio = match codec {
        VideoCodec::Png => None,
        VideoCodec::H264 | VideoCodec::Hevc => Some(AudioSettings {
            codec: AudioCodec::Aac,
            rate: 48_000,
            channels: 2,
            bitrate_kbps: 192,
        }),
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
        range: SeqRange::new(SeqTime::from_seconds(start), SeqTime::from_seconds(end)),
        video: Some(VideoSettings {
            codec,
            width: size("width", seq.settings.width) & !1,
            height: size("height", seq.settings.height) & !1,
            rate: seq.rate(),
            bitrate_kbps: None,
            quality: 20,
            hardware: true,
        }),
        audio,
    };
    Ok((settings, path))
}
