//! Export jobs: render every frame of a sequence range on the GPU, convert to the delivery
//! format on the GPU, encode with FFmpeg, and mix audio with the same mixer as playback. Jobs
//! run on their own thread from an immutable project snapshot (architecture 9).

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

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub frame: u64,
    pub total: u64,
    pub fps: f32,
    pub remaining: Option<Duration>,
    pub encoder: String,
    pub done: bool,
    pub error: Option<String>,
    pub cancelled: bool,
    pub paused: bool,
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
        let thread = std::thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                let result = run(&project, &s, gpu, media, &p, &c, &pa);
                let mut pr = p.lock();
                pr.done = true;
                match result {
                    Ok(()) => {}
                    Err(e) if c.load(Ordering::Relaxed) => {
                        pr.cancelled = true;
                        let _ = std::fs::remove_file(&s.path);
                        log::info!("export cancelled: {e}");
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&s.path);
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
    let rate = seq.rate();
    let total = rate.dur_to_frames_round(s.range.duration()).max(0) as u64;
    if total == 0 && s.video.is_some() {
        return Err("the export range is empty".into());
    }
    media.set_assets(project.assets.iter());
    if s.audio.is_some() {
        let mut streams = Vec::new();
        audio_streams(project, s.sequence, &mut streams, 0);
        media.wait_audio(&streams, Duration::from_secs(600));
    }
    if let Some(dir) = s.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut muxer =
        Muxer::create(&s.path, s.video.as_ref(), s.audio.as_ref()).map_err(|e| e.to_string())?;
    progress.lock().encoder = muxer.video_encoder().unwrap_or("audio").to_string();
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
    let audio_total = s
        .audio
        .as_ref()
        .map(|a| Rate::fps(a.rate).dur_to_frames_round(s.range.duration()))
        .unwrap_or(0);
    let video_frames = if s.video.is_some() { total } else { 0 };
    let steps = video_frames.max(1);
    for n in 0..steps {
        while pause.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if let (Some(r), Some(v)) = (renderer.as_mut(), s.video.as_ref()) {
            let t = s.range.start + rate.frames_to_dur(n as i64);
            let scale = v.width as f32 / seq.settings.width.max(1) as f32;
            let frame = r.render(&Request {
                project,
                sequence: s.sequence,
                time: t,
                scale,
                source: &frames,
            });
            let planes = if (frame.width, frame.height) == (v.width, v.height) {
                r.delivery_planes(&frame, v.codec.input())
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
                let p = r.delivery_planes(&fixed, v.codec.input());
                r.recycle(fixed);
                p
            };
            r.recycle(frame);
            let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_slice()).collect();
            muxer.push_video(&refs).map_err(|e| e.to_string())?;
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
                muxer.push_audio(&buf).map_err(|e| e.to_string())?;
                audio_done += chunk as i64;
                if video_frames == 0 {
                    let mut pr = progress.lock();
                    pr.total = audio_total as u64;
                    pr.frame = audio_done as u64;
                }
            }
        }
        let mut pr = progress.lock();
        if video_frames > 0 {
            pr.frame = n + 1;
            let secs = started.elapsed().as_secs_f32().max(1e-3);
            pr.fps = (n + 1) as f32 / secs;
            let left = (total - n - 1) as f32 / pr.fps.max(1e-3);
            pr.remaining = Some(Duration::from_secs_f32(left));
        }
    }
    muxer.finish().map_err(|e| e.to_string())?;
    Ok(())
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
