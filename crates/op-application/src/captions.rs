//! Captions in the editor: caption clips from timed phrases, caption files (SRT, WebVTT), one
//! style for every caption, and automatic captions transcribed from the sequence's audio with a
//! local speech model (Graphics > Captions).
//!
//! As in Premiere Pro, captions sit on their own track ("Captions") above the video, one clip per
//! caption, and are styled together. Each clip carries a Caption component with the words and
//! their times, so styles can light up, pop or reveal each word as it is spoken.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use op_core::captions::{self, Cue, TimedWord};
use op_core::catalog::{self, EffectKind};
use op_core::*;
use parking_lot::Mutex;

use crate::editor::Editor;
use crate::media::MediaService;

pub use op_speech::{ModelSize, Task};

/// How new captions look and how many words each one holds.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionOptions {
    /// Index into `catalog::CAPTION_STYLES`.
    pub style: u32,
    pub max_words: usize,
    pub uppercase: bool,
}

impl Default for CaptionOptions {
    fn default() -> Self {
        CaptionOptions {
            style: 2,
            max_words: 4,
            uppercase: false,
        }
    }
}

/// Settings of an automatic transcription.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscribeOptions {
    pub model: ModelSize,
    /// Whisper language code of the speech, or None to detect it.
    pub language: Option<String>,
    /// Captions in the language spoken, or translated into English.
    pub task: Task,
    pub captions: CaptionOptions,
}

/// English names of the languages offered in the transcription dialog, for messages.
pub fn language_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "es" => "Spanish",
        "en" => "English",
        "pt" => "Portuguese",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "ca" => "Catalan",
        "nl" => "Dutch",
        "pl" => "Polish",
        "ru" => "Russian",
        "tr" => "Turkish",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionStage {
    Downloading,
    MixingAudio,
    Transcribing,
}

#[derive(Clone, Debug)]
pub struct CaptionProgress {
    pub stage: CaptionStage,
    pub fraction: f32,
}

/// A transcription running in the background.
pub struct CaptionJob {
    pub sequence: SequenceId,
    pub options: TranscribeOptions,
    progress: Arc<Mutex<CaptionProgress>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<(String, Vec<Cue>), String>>>>,
}

impl CaptionJob {
    pub fn progress(&self) -> CaptionProgress {
        self.progress.lock().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn take(&self) -> Option<Result<(String, Vec<Cue>), String>> {
        self.result.lock().take()
    }
}

/// Where speech models are kept.
pub fn models_dir(ed: &Editor) -> PathBuf {
    ed.dirs.data.join("SpeechModels")
}

/// The sequence's mix as 16 kHz mono, for speech recognition: rendered at 48 kHz and reduced
/// by three with a short low-pass, so nothing above 8 kHz folds into the speech band.
fn speech_audio(
    project: &Project,
    sid: SequenceId,
    media: &Arc<MediaService>,
    progress: &Mutex<CaptionProgress>,
    cancel: &AtomicBool,
) -> Result<Vec<f32>, String> {
    let seq = project
        .sequence(sid)
        .ok_or("the sequence no longer exists")?;
    let secs = seq.duration().seconds();
    if secs <= 0.0 {
        return Err("the sequence is empty".into());
    }
    let mut streams = Vec::new();
    for t in &seq.audio {
        for c in &t.clips {
            if let ClipSource::Asset { asset, stream, .. } = &c.source
                && !streams.contains(&(*asset, *stream))
            {
                streams.push((*asset, *stream));
            }
        }
    }
    if streams.is_empty() {
        return Err("the sequence has no audio".into());
    }
    media.set_assets(project.assets.iter());
    media.wait_audio(&streams, std::time::Duration::from_secs(600));
    let rate = 48_000u32;
    let total = (secs * rate as f64).ceil() as usize;
    // the mixer works in stereo like playback and export; the channels are averaged
    let mut mixer = op_audio::Mixer::new(rate, 2);
    let source: &dyn op_audio::AudioSource = &**media;
    let mut out = Vec::with_capacity(total / 3 + 1);
    let block = rate as usize * 5;
    let mut done = 0usize;
    // the low-pass keeps its state across blocks
    let (mut z1, mut z2) = (0f32, 0f32);
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = block.min(total - done);
        let mut buf = vec![0f32; n * 2];
        let t = SeqTime::ZERO + Dur::from_seconds(done as f64 / rate as f64);
        mixer.render(project, sid, t, 1.0, &mut buf, source);
        let mono: Vec<f32> = buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| (f[0] + f[1]) * 0.5)
            .collect();
        for chunk in mono.chunks(3) {
            // two one-pole stages around 6 kHz, then every third sample
            let mut v = 0.0;
            for s in chunk {
                z1 += 0.55 * (s - z1);
                z2 += 0.55 * (z1 - z2);
                v = z2;
            }
            out.push(v);
        }
        done += n;
        progress.lock().fraction = done as f32 / total as f32;
    }
    Ok(out)
}

impl Editor {
    /// Starts automatic captions for the active sequence: downloads the model if needed, mixes
    /// the audio, transcribes it and, when done, places the captions (see `poll_captions`).
    pub fn transcribe_captions(&mut self, options: TranscribeOptions) -> bool {
        let Some(sid) = self.active else {
            self.error("Open a sequence first");
            return false;
        };
        if self.captioning.is_some() {
            return false;
        }
        let project = Arc::new(self.project.clone());
        let media = self.media.clone();
        let models = models_dir(self);
        let progress = Arc::new(Mutex::new(CaptionProgress {
            stage: CaptionStage::MixingAudio,
            fraction: 0.0,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r, opts) = (
            progress.clone(),
            cancel.clone(),
            result.clone(),
            options.clone(),
        );
        log::info!(
            "transcribing sequence with {} ({}, {:?})",
            opts.model.id(),
            opts.language.as_deref().unwrap_or("auto"),
            opts.task
        );
        let spawned = std::thread::Builder::new()
            .name("captions".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_transcription(&project, sid, &media, &models, &opts, &p, &c)
                }))
                .unwrap_or_else(|_| Err("the transcription stopped unexpectedly".into()));
                *r.lock() = Some(res);
            });
        if spawned.is_err() {
            return false;
        }
        self.captioning = Some(CaptionJob {
            sequence: sid,
            options,
            progress,
            cancel,
            result,
        });
        true
    }

    /// Places finished automatic captions.
    pub(crate) fn poll_captions(&mut self) {
        let Some(job) = &self.captioning else { return };
        let Some(res) = job.take() else { return };
        let (sid, opts, task) = (job.sequence, job.options.captions.clone(), job.options.task);
        self.captioning = None;
        match res {
            Ok((_, cues)) if cues.is_empty() => self.info("No speech was found".to_string()),
            Ok((lang, cues)) => {
                if self.active != Some(sid) {
                    self.open_sequence(sid);
                }
                let n = self.create_captions(&cues, &opts);
                // say which language was heard, so a wrong guess is easy to spot and redo
                let lang = language_name(&lang).map(str::to_string).unwrap_or(lang);
                match task {
                    Task::Transcribe => self.info(format!("Created {n} captions in {lang}")),
                    Task::Translate => {
                        self.info(format!("Created {n} captions translated from {lang}"))
                    }
                }
            }
            Err(e) if e == "cancelled" => {}
            Err(e) => self.error(format!("Captions not created: {e}")),
        }
    }

    /// Caption clips for `cues` (times in sequence seconds) on a Captions track at the top of
    /// the active sequence. Returns how many were made (one undo step).
    pub fn create_captions(&mut self, cues: &[Cue], opts: &CaptionOptions) -> usize {
        let Some(seq) = self.active_seq().cloned() else {
            return 0;
        };
        let rate = seq.rate();
        let def = match catalog::find(catalog::CAPTION) {
            Some(d) => d,
            None => return 0,
        };
        let font = op_render::text::Fonts::default_family().to_string();
        // about 6.5 % of the picture height, 70 pixels in HD
        let size = (seq.settings.height as f64 * 0.065).round().max(12.0);
        let y = if seq.settings.height > seq.settings.width {
            0.7
        } else {
            0.8
        };
        let mut placed: Vec<(SeqTime, Dur, Cue)> = Vec::new();
        let mut prev_end = SeqTime::ZERO;
        for cue in cues {
            if cue.words.is_empty() {
                continue;
            }
            let start = SeqTime::ZERO + Dur::from_seconds(cue.start.max(0.0)).round_frames(rate);
            let start = if start < prev_end { prev_end } else { start };
            let end = SeqTime::ZERO + Dur::from_seconds(cue.end.max(0.0)).round_frames(rate);
            let len = if end > start {
                end - start
            } else {
                rate.frame_duration()
            };
            prev_end = start + len;
            placed.push((start, len, cue.clone()));
        }
        if placed.is_empty() {
            return 0;
        }
        let first = placed[0].0;
        let last = placed.last().map(|(s, d, _)| *s + *d).unwrap_or(first);
        let range = SeqRange::new(first, last);
        // an empty Captions track is reused; otherwise a new one goes on top
        let reuse = seq
            .video
            .iter()
            .rposition(|t| t.name == "Captions" && !t.locked && t.clips_in(range).next().is_none());
        let opts = opts.clone();
        let made = self.seq_edit("Create Captions", move |p, sid, _| {
            let mut ids = p.ids.clone();
            let mut clips = Vec::new();
            for (start, len, cue) in &placed {
                let offset = start.seconds();
                let words: Vec<TimedWord> = cue
                    .words
                    .iter()
                    .map(|w| TimedWord {
                        text: w.text.clone(),
                        start: (w.start - offset).max(0.0),
                        end: (w.end - offset).max(0.0),
                    })
                    .collect();
                let mut comp = Component::new(def, &mut ids);
                let set = |c: &mut Component, k: &str, v: Value| {
                    if let Some(prm) = c.param_mut(k) {
                        prm.value = v;
                    }
                };
                set(&mut comp, "text", Value::Text(cue.text()));
                set(
                    &mut comp,
                    "timing",
                    Value::Text(captions::encode_timing(&words)),
                );
                set(&mut comp, "style", Value::Choice(opts.style));
                set(&mut comp, "font", Value::Text(font.clone()));
                set(&mut comp, "font_size", Value::Float(size));
                set(&mut comp, "uppercase", Value::Bool(opts.uppercase));
                set(&mut comp, "position", Value::Point([0.5, y]));
                let mut components = default_components(EffectKind::VideoFixed, &mut ids);
                components.push(comp);
                let name: String = cue.text().chars().take(40).collect();
                clips.push(Clip {
                    id: ids.clip(),
                    name,
                    kind: TrackKind::Video,
                    source: ClipSource::Graphic,
                    start: *start,
                    duration: *len,
                    source_in: SrcTime::ZERO,
                    speed: Speed::NORMAL,
                    reverse: false,
                    hold: None,
                    enabled: true,
                    link: None,
                    group: None,
                    label: Label::Lavender,
                    components,
                    gain_db: 0.0,
                    scale_to_frame: false,
                    channels: None,
                });
            }
            let index = match reuse {
                Some(i) => i,
                None => {
                    let tid = ids.track();
                    let mut track = Track::new(tid, TrackKind::Video);
                    track.name = "Captions".into();
                    let s = p
                        .sequence_mut(sid)
                        .ok_or(EditError::NotFound("sequence".into()))?;
                    s.video.push(Arc::new(track));
                    s.video.len() - 1
                }
            };
            p.ids = ids;
            let s = p
                .sequence_mut(sid)
                .ok_or(EditError::NotFound("sequence".into()))?;
            let tr = s
                .track_mut(TrackRef::video(index))
                .ok_or(EditError::NotFound("track".into()))?;
            let made: Vec<ClipId> = clips.iter().map(|c| c.id).collect();
            for clip in clips {
                let pos = tr.clips.partition_point(|c| c.start <= clip.start);
                tr.clips.insert(pos, clip);
            }
            Ok(made)
        });
        match made {
            Some(ids) => {
                let n = ids.len();
                self.selection = crate::session::Selection::only(ids);
                n
            }
            None => 0,
        }
    }

    /// Captions of the active sequence as cues in sequence seconds, in order.
    pub fn caption_cues(&self) -> Vec<Cue> {
        let Some(seq) = self.active_seq() else {
            return Vec::new();
        };
        let mut cues: Vec<Cue> = seq
            .video
            .iter()
            .filter(|t| t.enabled)
            .flat_map(|t| &t.clips)
            .filter(|c| c.enabled)
            .filter_map(|c| {
                let comp = c.component(catalog::CAPTION).filter(|x| x.enabled)?;
                let text = match comp.param("text").map(|p| &p.value) {
                    Some(Value::Text(t)) => t.clone(),
                    _ => return None,
                };
                let timing = match comp.param("timing").map(|p| &p.value) {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                let start = c.start.seconds();
                let dur = c.duration.seconds();
                let words = captions::words(&text, &timing, dur)
                    .into_iter()
                    .map(|w| TimedWord {
                        text: w.text,
                        start: start + w.start.min(dur),
                        end: start + w.end.min(dur),
                    })
                    .collect();
                Some(Cue {
                    start,
                    end: start + dur,
                    words,
                })
            })
            .collect();
        cues.sort_by(|a, b| a.start.total_cmp(&b.start));
        cues
    }

    /// Reads an SRT or WebVTT file into caption clips.
    pub fn import_captions(&mut self, path: &Path, opts: &CaptionOptions) -> Result<usize, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let cues = captions::parse(&text);
        if cues.is_empty() {
            return Err("the file has no captions".into());
        }
        if self.active.is_none() {
            return Err("Open a sequence first".into());
        }
        Ok(self.create_captions(&cues, opts))
    }

    /// Writes the active sequence's captions as SRT, or WebVTT for a `.vtt` path.
    pub fn export_captions(&self, path: &Path) -> Result<usize, String> {
        let cues = self.caption_cues();
        if cues.is_empty() {
            return Err("the sequence has no captions".into());
        }
        let vtt = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("vtt"));
        let text = if vtt {
            captions::to_vtt(&cues)
        } else {
            captions::to_srt(&cues)
        };
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(cues.len())
    }

    /// Caption clips of the active sequence, in time order.
    pub fn caption_clips(&self) -> Vec<ClipId> {
        let Some(seq) = self.active_seq() else {
            return Vec::new();
        };
        let mut v: Vec<(SeqTime, ClipId)> = seq
            .video
            .iter()
            .flat_map(|t| &t.clips)
            .filter(|c| c.component(catalog::CAPTION).is_some())
            .map(|c| (c.start, c.id))
            .collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Sets Caption parameters on several captions at once (one undo step; `merge` joins the
    /// steps of a slider drag).
    pub fn set_caption_values(
        &mut self,
        clips: &[ClipId],
        values: Vec<(String, Value)>,
        merge: Option<String>,
    ) {
        if clips.is_empty() || values.is_empty() {
            return;
        }
        let clips = clips.to_vec();
        let Some(sid) = self.active else { return };
        self.edit_merge("Caption Style", merge, move |p| {
            let s = p
                .sequence_mut(sid)
                .ok_or(EditError::NotFound("sequence".into()))?;
            let mut n = 0;
            for t in s.video.iter_mut() {
                let t = Arc::make_mut(t);
                for c in t.clips.iter_mut().filter(|c| clips.contains(&c.id)) {
                    let Some(comp) = c
                        .components
                        .iter_mut()
                        .find(|x| x.effect == catalog::CAPTION)
                    else {
                        continue;
                    };
                    for (k, v) in &values {
                        if let Some(prm) = comp.param_mut(k) {
                            prm.value = v.clone();
                            n += 1;
                        }
                    }
                    if values.iter().any(|(k, _)| k == "text")
                        && let Some(Value::Text(t)) = values
                            .iter()
                            .find(|(k, _)| k == "text")
                            .map(|(_, v)| v.clone())
                    {
                        c.name = t.chars().take(40).collect();
                    }
                }
            }
            if n == 0 {
                return Err(EditError::Nothing);
            }
            Ok(())
        });
    }

    /// Gives every caption in the active sequence the look of the selected caption (style,
    /// font, colors, position; the words stay).
    pub fn apply_caption_style_to_all(&mut self) -> usize {
        let Some(seq) = self.active_seq() else {
            return 0;
        };
        let source = seq
            .video
            .iter()
            .flat_map(|t| &t.clips)
            .filter(|c| self.selection.clips.contains(&c.id))
            .find_map(|c| c.component(catalog::CAPTION).cloned());
        let Some(source) = source else {
            self.info("Select a caption first".to_string());
            return 0;
        };
        let changed = self.seq_edit("Apply Caption Style", move |p, sid, _| {
            let s = p
                .sequence_mut(sid)
                .ok_or(EditError::NotFound("sequence".into()))?;
            let mut n = 0;
            for t in s.video.iter_mut() {
                let t = Arc::make_mut(t);
                for c in t.clips.iter_mut() {
                    let Some(comp) = c
                        .components
                        .iter_mut()
                        .find(|x| x.effect == catalog::CAPTION)
                    else {
                        continue;
                    };
                    for prm in &source.params {
                        if prm.key == "text" || prm.key == "timing" {
                            continue;
                        }
                        if let Some(dst) = comp.param_mut(&prm.key) {
                            *dst = prm.clone();
                        }
                    }
                    n += 1;
                }
            }
            Ok(n)
        });
        changed.unwrap_or(0)
    }
}

fn run_transcription(
    project: &Project,
    sid: SequenceId,
    media: &Arc<MediaService>,
    models: &Path,
    opts: &TranscribeOptions,
    progress: &Mutex<CaptionProgress>,
    cancel: &AtomicBool,
) -> Result<(String, Vec<Cue>), String> {
    if !opts.model.is_downloaded(models) {
        *progress.lock() = CaptionProgress {
            stage: CaptionStage::Downloading,
            fraction: 0.0,
        };
        opts.model
            .download(models, &mut |done, total| {
                progress.lock().fraction = done as f32 / total.max(1) as f32;
                !cancel.load(Ordering::Relaxed)
            })
            .map_err(|e| match e {
                op_speech::SpeechError::Cancelled => "cancelled".to_string(),
                e => e.to_string(),
            })?;
    }
    *progress.lock() = CaptionProgress {
        stage: CaptionStage::MixingAudio,
        fraction: 0.0,
    };
    let pcm = speech_audio(project, sid, media, progress, cancel)?;
    *progress.lock() = CaptionProgress {
        stage: CaptionStage::Transcribing,
        fraction: 0.0,
    };
    let started = std::time::Instant::now();
    let mut t = op_speech::Transcriber::load(&opts.model.dir(models)).map_err(|e| e.to_string())?;
    let (lang, segments) = t
        .transcribe(
            &pcm,
            opts.language.as_deref(),
            opts.task,
            &mut |f| progress.lock().fraction = f,
            cancel,
        )
        .map_err(|e| match e {
            op_speech::SpeechError::Cancelled => "cancelled".to_string(),
            e => e.to_string(),
        })?;
    log::info!(
        "transcribed {:.0} s of audio ({lang}) in {:.1} s: {} segments",
        pcm.len() as f64 / 16_000.0,
        started.elapsed().as_secs_f32(),
        segments.len()
    );
    let words: Vec<TimedWord> = segments
        .iter()
        .flat_map(|s| &s.words)
        .map(|w| TimedWord {
            text: w.text.clone(),
            start: w.start,
            end: w.end,
        })
        .collect();
    Ok((
        lang,
        captions::chunk(&words, opts.captions.max_words, 0.7, 0.6),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::{Dirs, Preferences};

    #[test]
    fn captions_are_created_restyled_and_exported() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let dir = tempfile::tempdir().unwrap();
        let mut e = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
        e.new_sequence("S", SequenceSettings::default());
        let srt = dir.path().join("in.srt");
        std::fs::write(
            &srt,
            "1\n00:00:01,000 --> 00:00:02,000\nHola a todos\n\n2\n00:00:02,000 --> 00:00:03,500\nBienvenidos\n",
        )
        .unwrap();
        let n = e.import_captions(&srt, &CaptionOptions::default()).unwrap();
        assert_eq!(n, 2);
        let seq = e.active_seq().unwrap();
        let track = seq.video.iter().find(|t| t.name == "Captions").unwrap();
        assert_eq!(track.clips.len(), 2);
        let c = &track.clips[0];
        assert_eq!(c.start, SeqTime::from_seconds(1.0));
        let comp = c.component(catalog::CAPTION).unwrap();
        assert_eq!(
            comp.param("text").unwrap().value,
            Value::Text("Hola a todos".into())
        );
        // word times are relative to the clip
        match &comp.param("timing").unwrap().value {
            Value::Text(t) => assert!(t.starts_with("0.000-"), "{t}"),
            v => panic!("{v:?}"),
        }
        // restyle everything after the first caption
        let first = c.id;
        let comp_id = comp.id;
        e.edit("Style", |p| {
            let s = Arc::make_mut(p.sequences.values_mut().next().unwrap());
            for t in s.video.iter_mut() {
                for c in Arc::make_mut(t).clips.iter_mut() {
                    if c.id == first
                        && let Some(x) = c.components.iter_mut().find(|x| x.id == comp_id)
                    {
                        x.param_mut("style").unwrap().value = Value::Choice(9);
                    }
                }
            }
            Ok(())
        });
        e.selection = crate::session::Selection::only(vec![first]);
        assert_eq!(e.apply_caption_style_to_all(), 2);
        let seq = e.active_seq().unwrap();
        let track = seq.video.iter().find(|t| t.name == "Captions").unwrap();
        for c in &track.clips {
            let comp = c.component(catalog::CAPTION).unwrap();
            assert_eq!(comp.param("style").unwrap().value, Value::Choice(9));
        }
        assert_eq!(
            track.clips[1]
                .component(catalog::CAPTION)
                .unwrap()
                .param("text")
                .unwrap()
                .value,
            Value::Text("Bienvenidos".into()),
            "the words stay"
        );
        // the Captions panel: one change for every caption, one undo step; text edits rename
        let all = e.caption_clips();
        assert_eq!(all.len(), 2);
        let undo_before = e.history.labels().0.len();
        e.set_caption_values(
            &all,
            vec![("font_size".into(), Value::Float(90.0))],
            Some("drag".into()),
        );
        e.set_caption_values(
            &all,
            vec![("font_size".into(), Value::Float(96.0))],
            Some("drag".into()),
        );
        assert_eq!(
            e.history.labels().0.len(),
            undo_before + 1,
            "a drag is one step"
        );
        let seq = e.active_seq().unwrap();
        for id in &all {
            let c = seq.clip(*id).unwrap();
            let size = &c
                .component(catalog::CAPTION)
                .unwrap()
                .param("font_size")
                .unwrap()
                .value;
            assert_eq!(*size, Value::Float(96.0));
        }
        e.seal();
        e.set_caption_values(
            &all[..1],
            vec![("text".into(), Value::Text("Hola".into()))],
            None,
        );
        assert_eq!(e.active_seq().unwrap().clip(all[0]).unwrap().name, "Hola");
        e.undo();
        assert_ne!(e.active_seq().unwrap().clip(all[0]).unwrap().name, "Hola");
        // round trip through a file
        let out = dir.path().join("out.vtt");
        assert_eq!(e.export_captions(&out).unwrap(), 2);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.starts_with("WEBVTT"));
        let back = captions::parse(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].text(), "Hola a todos");
        assert!((back[1].end - 3.5).abs() < 0.05);
        // a second import reuses nothing that is busy: it lands on a new Captions track
        e.import_captions(&srt, &CaptionOptions::default()).unwrap();
        let tracks = e
            .active_seq()
            .unwrap()
            .video
            .iter()
            .filter(|t| t.name == "Captions")
            .count();
        assert_eq!(tracks, 2);
    }
}
