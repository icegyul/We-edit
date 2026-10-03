//! Export jobs: render every frame of a sequence range on the GPU, convert to the delivery
//! format on the GPU, encode with FFmpeg, and mix audio with the same mixer as playback. Jobs
//! run on their own thread from an immutable project snapshot (architecture 9). They can be
//! paused and cancelled, report progress with a small preview of the frame being rendered, and
//! a failure inside a job ends that job with an error instead of ending the program.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use op_core::*;
use op_media::{AudioSettings, Muxer, VideoSettings};
use op_render::{Gpu, Renderer, Request};
use parking_lot::Mutex;

use crate::media::{ExactFrames, MediaService};

#[derive(Clone, Debug, PartialEq)]
pub struct ExportSettings {
    pub path: PathBuf,
    pub sequence: SequenceId,
    pub range: SeqRange,
    pub video: Option<VideoSettings>,
    pub audio: Option<AudioSettings>,
}

/// A reduced copy of the frame an export is rendering (RGBA8, straight alpha).
#[derive(Clone, Debug)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Output frame number it shows.
    pub frame: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub frame: u64,
    pub total: u64,
    pub fps: f32,
    pub remaining: Option<Duration>,
    /// Time spent working, pauses excluded.
    pub elapsed: Duration,
    pub encoder: String,
    pub done: bool,
    pub error: Option<String>,
    pub cancelled: bool,
    pub paused: bool,
    /// Waiting for audio to be conformed before the first frame.
    pub preparing: bool,
    pub preview: Option<Arc<PreviewImage>>,
    /// Average milliseconds per frame spent waiting for decoded media, rendering on the GPU,
    /// reading the result back and encoding (diagnostics).
    pub stage_ms: [f32; 4],
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.frame as f32 / self.total as f32
        }
    }
}

pub struct ExportJob {
    pub settings: ExportSettings,
    pub progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    reported: bool,
}

impl ExportJob {
    pub fn start(
        project: Arc<Project>,
        settings: ExportSettings,
        gpu: Arc<Gpu>,
        media: Arc<MediaService>,
    ) -> ExportJob {
        let progress = Arc::new(Mutex::new(Progress::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let (p, c, pa, s) = (
            progress.clone(),
            cancel.clone(),
            pause.clone(),
            settings.clone(),
        );
        log::info!(
            "export started: {} ({} to {})",
            settings.path.display(),
            settings.range.start.seconds(),
            settings.range.end.seconds()
        );
        let thread = std::thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                let started = Instant::now();
                let attempt = |s: &ExportSettings| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(&project, s, gpu.clone(), media.clone(), &p, &c, &pa)
                    }))
                    .unwrap_or_else(|panic| {
                        let what = panic
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_else(|| "internal error".into());
                        Err(format!("the export stopped unexpectedly ({what})"))
                    })
                };
                let mut result = attempt(&s);
                // a hardware encoder that fails (driver, media engine, unusual size) must not
                // stop the export: start again with the software encoder
                let encoder = p.lock().encoder.clone();
                if result.is_err()
                    && !c.load(Ordering::Relaxed)
                    && op_media::encode::is_hardware_encoder(&encoder)
                {
                    log::warn!(
                        "hardware encoder {encoder} failed ({}); exporting again with the software encoder",
                        result.as_ref().err().map(String::as_str).unwrap_or_default()
                    );
                    let mut soft = s.clone();
                    if let Some(v) = soft.video.as_mut() {
                        v.hardware = false;
                    }
                    {
                        let mut pr = p.lock();
                        *pr = Progress {
                            paused: pr.paused,
                            ..Progress::default()
                        };
                    }
                    result = attempt(&soft);
                }
                let mut pr = p.lock();
                pr.done = true;
                pr.preparing = false;
                match result {
                    Ok(()) => log::info!(
                        "export finished in {:.1} s: {} ({} frames, {})",
                        started.elapsed().as_secs_f32(),
                        s.path.display(),
                        pr.total,
                        pr.encoder
                    ),
                    Err(e) if c.load(Ordering::Relaxed) => {
                        pr.cancelled = true;
                        let _ = std::fs::remove_file(&s.path);
                        log::info!("export cancelled: {e}");
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&s.path);
                        log::error!("export failed: {e}");
                        pr.error = Some(e);
                    }
                }
            })
            .ok();
        ExportJob {
            settings,
            progress,
            cancel,
            pause,
            thread,
            reported: false,
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause.store(paused, Ordering::Relaxed);
        self.progress.lock().paused = paused;
        log::info!(
            "export {}: {}",
            if paused { "paused" } else { "resumed" },
            self.settings.path.display()
        );
    }

    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    pub fn finished(&self) -> bool {
        self.progress.lock().done
    }

    /// A completion message, once.
    pub fn take_message(&mut self) -> Option<(String, bool)> {
        if self.reported || !self.finished() {
            return None;
        }
        self.reported = true;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let p = self.progress.lock().clone();
        Some(if let Some(e) = p.error {
            (format!("Export failed: {e}"), true)
        } else if p.cancelled {
            ("Export cancelled".into(), false)
        } else {
            (
                format!("Exported {} ({})", self.settings.path.display(), p.encoder),
                false,
            )
        })
    }
}

impl Drop for ExportJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Audio streams a sequence plays, including nested sequences.
fn audio_streams(p: &Project, sid: SequenceId, out: &mut Vec<(AssetId, usize)>, depth: usize) {
    let Some(seq) = p.sequence(sid) else { return };
    for t in &seq.audio {
        for c in &t.clips {
            match &c.source {
                ClipSource::Asset { asset, stream, .. } if !out.contains(&(*asset, *stream)) => {
                    out.push((*asset, *stream))
                }
                ClipSource::Sequence { sequence, .. } if depth < 8 => {
                    audio_streams(p, *sequence, out, depth + 1)
                }
                _ => {}
            }
        }
    }
}

fn run(
    project: &Project,
    s: &ExportSettings,
    gpu: Arc<Gpu>,
    media: Arc<MediaService>,
    progress: &Mutex<Progress>,
    cancel: &AtomicBool,
    pause: &AtomicBool,
) -> Result<(), String> {
    let seq = project
        .sequence(s.sequence)
        .ok_or("the sequence no longer exists")?;
    // frames are sampled at the output rate, which may differ from the sequence rate
    let rate = s.video.as_ref().map(|v| v.rate).unwrap_or(seq.rate());
    let total = rate.dur_to_frames_round(s.range.duration()).max(0) as u64;
    if total == 0 && s.video.is_some() {
        return Err("the export range is empty".into());
    }
    media.set_assets(project.assets.iter());
    if s.audio.is_some() {
        let mut streams = Vec::new();
        audio_streams(project, s.sequence, &mut streams, 0);
        progress.lock().preparing = true;
        media.wait_audio(&streams, Duration::from_secs(600));
        progress.lock().preparing = false;
    }
    if let Some(dir) = s.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot write {}: {e}", dir.display()))?;
    }
    // FFmpeg reports a protected or missing destination vaguely ("No such file or directory"
    // when Windows Controlled folder access blocks it); check it first for a clear message
    std::fs::File::create(&s.path)
        .map_err(|e| format!("cannot write {}: {e}", s.path.display()))?;
    let (mut writer, encoder) = Writer::start(s)?;
    log::info!("export encoder: {encoder}");
    progress.lock().encoder = encoder;
    progress.lock().total = total;
    let mut renderer = s.video.as_ref().map(|_| Renderer::new(gpu));
    let frames = ExactFrames {
        service: media.clone(),
    };
    let source: &dyn op_audio::AudioSource = &*media;
    let mut mixer = s
        .audio
        .as_ref()
        .map(|a| op_audio::Mixer::new(a.rate, a.channels as usize));
    let mut audio_done: i64 = 0;
    let started = Instant::now();
    let mut paused_for = Duration::ZERO;
    let mut preview_at: Option<Instant> = None;
    // seconds spent per stage: media and render commands, readback, encoding
    let mut stage = [0f64; 4];
    let audio_total = s
        .audio
        .as_ref()
        .map(|a| Rate::fps(a.rate).dur_to_frames_round(s.range.duration()))
        .unwrap_or(0);
    let video_frames = if s.video.is_some() { total } else { 0 };
    let steps = video_frames.max(1);
    let mut in_flight: Option<op_render::output::PendingRead> = None;
    for n in 0..steps {
        if pause.load(Ordering::Relaxed) {
            let since = Instant::now();
            while pause.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            paused_for += since.elapsed();
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if let (Some(r), Some(v)) = (renderer.as_mut(), s.video.as_ref()) {
            let t = s.range.start + rate.frames_to_dur(n as i64);
            let scale = v.width as f32 / seq.settings.width.max(1) as f32;
            let t0 = Instant::now();
            let frame = r.render(&Request {
                project,
                sequence: s.sequence,
                time: t,
                scale,
                source: &frames,
            });
            stage[0] += t0.elapsed().as_secs_f64();
            // a small copy of the frame a few times per second, for the progress preview
            if preview_at.is_none_or(|p| p.elapsed() >= Duration::from_millis(200)) {
                preview_at = Some(Instant::now());
                let k = (480.0 / frame.width as f32)
                    .min(270.0 / frame.height as f32)
                    .min(1.0);
                let (pw, ph) = (
                    ((frame.width as f32 * k) as u32).max(1),
                    ((frame.height as f32 * k) as u32).max(1),
                );
                let target = r.display_target(pw, ph);
                r.present_into(&frame, &target, false);
                let rgba = r.read_texture(&target, 4);
                progress.lock().preview = Some(Arc::new(PreviewImage {
                    width: pw,
                    height: ph,
                    rgba,
                    frame: n,
                }));
            }
            let t1 = Instant::now();
            // the frame's planes are read back while the next frame renders
            let pending = if (frame.width, frame.height) == (v.width, v.height) {
                r.begin_delivery(&frame, v.codec.input())
            } else {
                // odd sizes: render at the exact output size
                let fixed = r.work(v.width, v.height);
                r.pass(
                    "fs_copy",
                    &[&frame.view],
                    op_render::params::P::new(),
                    frame.size(),
                    &fixed,
                );
                let p = r.begin_delivery(&fixed, v.codec.input());
                r.recycle(fixed);
                p
            };
            r.recycle(frame);
            let previous = in_flight.replace(pending);
            if let Some(prev) = previous {
                let planes = r.end_read(prev);
                stage[1] += t1.elapsed().as_secs_f64();
                // time blocked here means the encoder is the slowest stage
                let t2 = Instant::now();
                writer.send(Packet::Video(planes))?;
                stage[2] += t2.elapsed().as_secs_f64();
            }
        }
        // audio up to the end of this frame (or everything, for audio-only exports)
        if let (Some(m), Some(a)) = (mixer.as_mut(), s.audio.as_ref()) {
            let upto = if video_frames > 0 {
                Rate::fps(a.rate)
                    .dur_to_frames_round(rate.frames_to_dur(n as i64 + 1))
                    .min(audio_total)
            } else {
                audio_total
            };
            while audio_done < upto {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                let chunk = ((upto - audio_done) as usize).min(8192);
                let mut buf = vec![0f32; chunk * a.channels as usize];
                let t = s.range.start + Rate::fps(a.rate).frames_to_dur(audio_done);
                m.render(project, s.sequence, t, 1.0, &mut buf, source);
                writer.send(Packet::Audio(buf))?;
                audio_done += chunk as i64;
                if video_frames == 0 {
                    let mut pr = progress.lock();
                    pr.total = audio_total as u64;
                    pr.frame = audio_done as u64;
                }
            }
        }
        let mut pr = progress.lock();
        pr.elapsed = started.elapsed().saturating_sub(paused_for);
        if video_frames > 0 {
            pr.frame = n + 1;
            let k = 1000.0 / (n + 1) as f64;
            pr.stage_ms = [
                (stage[0] * k) as f32,
                (stage[1] * k) as f32,
                (stage[2] * k) as f32,
                (stage[3] * k) as f32,
            ];
            let secs = pr.elapsed.as_secs_f32().max(1e-3);
            pr.fps = (n + 1) as f32 / secs;
            let left = (total - n - 1) as f32 / pr.fps.max(1e-3);
            pr.remaining = Some(Duration::from_secs_f32(left));
        }
    }
    if let (Some(r), Some(prev)) = (renderer.as_mut(), in_flight.take()) {
        writer.send(Packet::Video(r.end_read(prev)))?;
    }
    writer.finish()
}

enum Packet {
    Video(Vec<Vec<u8>>),
    Audio(Vec<f32>),
}

/// Encodes and writes on its own thread, so the next frame renders while this one encodes.
struct Writer {
    tx: Option<crossbeam_channel::Sender<Packet>>,
    thread: Option<std::thread::JoinHandle<Result<(), String>>>,
}

impl Writer {
    /// Opens the output on the writer thread (FFmpeg contexts stay on one thread). Returns the
    /// writer and the video encoder's name.
    fn start(s: &ExportSettings) -> Result<(Writer, String), String> {
        let (tx, rx) = crossbeam_channel::bounded::<Packet>(3);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<String, String>>(1);
        let (path, video, audio) = (s.path.clone(), s.video.clone(), s.audio.clone());
        let thread = std::thread::Builder::new()
            .name("export-writer".into())
            .spawn(move || {
                let mut muxer = match Muxer::create(&path, video.as_ref(), audio.as_ref()) {
                    Ok(m) => {
                        let name = m.video_encoder().unwrap_or("audio").to_string();
                        let _ = ready_tx.send(Ok(name));
                        m
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return Err(e.to_string());
                    }
                };
                for p in rx {
                    match p {
                        Packet::Video(planes) => {
                            let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_slice()).collect();
                            muxer.push_video(&refs)
                        }
                        Packet::Audio(buf) => muxer.push_audio(&buf),
                    }
                    .map_err(|e| e.to_string())?;
                }
                muxer.finish().map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        let mut writer = Writer {
            tx: Some(tx),
            thread: Some(thread),
        };
        match ready_rx.recv() {
            Ok(Ok(name)) => Ok((writer, name)),
            Ok(Err(e)) => {
                let _ = writer.join();
                Err(e)
            }
            Err(_) => Err(writer
                .join()
                .err()
                .unwrap_or_else(|| "the encoder did not start".into())),
        }
    }

    fn send(&mut self, p: Packet) -> Result<(), String> {
        let sent = self.tx.as_ref().is_some_and(|tx| tx.send(p).is_ok());
        if sent {
            return Ok(());
        }
        // the writer stopped: report its error
        Err(self
            .join()
            .err()
            .unwrap_or_else(|| "the encoder stopped".into()))
    }

    fn join(&mut self) -> Result<(), String> {
        self.tx.take();
        match self.thread.take() {
            Some(t) => t
                .join()
                .unwrap_or_else(|_| Err("the encoder stopped unexpectedly".into())),
            None => Ok(()),
        }
    }

    /// Flushes the encoders and closes the file.
    fn finish(mut self) -> Result<(), String> {
        self.join()
    }
}

impl Drop for Writer {
    // cancelled or failed exports still close the file before it is removed
    fn drop(&mut self) {
        let _ = self.join();
    }
}

/// Renders one frame to an image file (PNG, JPEG or TIFF by extension).
pub fn export_frame(
    project: &Project,
    sequence: SequenceId,
    t: SeqTime,
    path: &std::path::Path,
    gpu: Arc<Gpu>,
    media: Arc<MediaService>,
) -> Result<(), String> {
    let seq = project.sequence(sequence).ok_or("no sequence")?;
    media.set_assets(project.assets.iter());
    let mut r = Renderer::new(gpu);
    let frames = ExactFrames { service: media };
    let frame = r.render(&Request {
        project,
        sequence,
        time: t,
        scale: 1.0,
        source: &frames,
    });
    let rgba = r.rgba8(&frame);
    r.recycle(frame);
    let v = VideoSettings {
        codec: op_media::VideoCodec::Png,
        width: seq.settings.width,
        height: seq.settings.height,
        rate: seq.rate(),
        bitrate_kbps: None,
        quality: 0,
        hardware: false,
    };
    let mut m = Muxer::create(path, Some(&v), None).map_err(|e| e.to_string())?;
    m.push_video(&[&rgba]).map_err(|e| e.to_string())?;
    m.finish().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_preview_and_output_rate() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let mut p = Project::new("E");
        let root = p.root;
        let (sid, _) = p.add_sequence(
            root,
            "S",
            SequenceSettings {
                width: 64,
                height: 36,
                rate: Rate::FPS_25,
                ..Default::default()
            },
        );
        let item = p.add_item(
            root,
            "Red",
            ItemKind::Synthetic {
                generator: Generator::ColorMatte {
                    color: Rgba::new(1.0, 0.0, 0.0, 1.0),
                },
                duration: Dur::from_seconds(2.0),
            },
        );
        let spec = op_timeline::SourceClip::from_item(&p, item).unwrap();
        let patch = op_timeline::Patch {
            video: Some(0),
            audio: vec![],
        };
        let opts = op_timeline::EditOptions {
            linked_selection: true,
            ripple_markers: false,
        };
        let (p, _) = p
            .transact(|p| op_timeline::overwrite(p, sid, &spec, SeqTime::ZERO, &patch, opts))
            .unwrap();
        let media = MediaService::new(dir.path().join("cache"), 64 << 20);
        let path = dir.path().join("out.mov");
        let settings = ExportSettings {
            path: path.clone(),
            sequence: sid,
            range: SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(2.0)),
            // the file is written at 50 fps although the sequence runs at 25
            video: Some(VideoSettings {
                codec: op_media::VideoCodec::Png,
                width: 64,
                height: 36,
                rate: Rate::FPS_50,
                bitrate_kbps: None,
                quality: 0,
                hardware: false,
            }),
            audio: None,
        };
        let job = ExportJob::start(Arc::new(p), settings, gpu, media);
        job.set_paused(true);
        // a frame already being rendered may still finish (slow software adapters)
        let mut frozen = job.progress.lock().frame;
        let start = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let now = job.progress.lock().frame;
            if now == frozen || start.elapsed() > Duration::from_secs(20) {
                break;
            }
            frozen = now;
        }
        assert!(!job.finished() && frozen < 100, "the export did not pause");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            job.progress.lock().frame,
            frozen,
            "a paused export advanced"
        );
        job.set_paused(false);
        let start = Instant::now();
        while !job.finished() && start.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let pr = job.progress.lock().clone();
        assert!(pr.error.is_none(), "{:?}", pr.error);
        assert_eq!(pr.total, 100);
        assert_eq!(pr.frame, 100);
        let preview = pr.preview.expect("a preview frame");
        assert_eq!((preview.width, preview.height), (64, 36));
        assert_eq!(preview.rgba.len(), 64 * 36 * 4);
        // the preview shows the red matte
        assert!(preview.rgba[0] > 200 && preview.rgba[1] < 40);
        let probed = op_media::probe(&path).unwrap();
        let v = probed.video.unwrap();
        assert_eq!(v.rate, Rate::FPS_50);
        assert!((v.frames - 100).abs() <= 1, "{} frames", v.frames);
    }
}
