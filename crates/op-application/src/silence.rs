//! Cutting silences, filler words and repeated takes (Clip > Remove Silence).
//!
//! A background job mixes the audio the user chose, measures it once with `op-silence` and, when
//! the settings need words, transcribes it. The result stays in the dialog, which can change any
//! setting and see the cuts at once; Apply then edits the timeline as one undo step.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use op_core::*;
use op_silence::{self as sil, Analysis, Cut, Kind, Range};
use op_timeline::{razor, set_speed};
use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::captions::{
    CaptionProgress, CaptionStage, ModelSize, Task, TranscribeOptions, audio_16k, ensure_model,
    models_dir, transcribe_words,
};
use crate::editor::Editor;

/// What the cuts apply to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The selected clips' audio (the sound of a selected video clip counts).
    Selection,
    /// The whole sequence.
    Sequence,
    /// Between the sequence's In and Out points.
    InOut,
}

/// What happens to each stretch found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Take it out and close the gap (ripple).
    Remove,
    /// Take it out and leave an empty gap.
    LeaveGap,
    /// Play it faster instead (percent, like Speed/Duration).
    SpeedUp(f64),
    /// Only mark it on the timeline.
    Markers,
}

impl Action {
    pub const NAMES: [&'static str; 4] = [
        "Remove and Close Gaps",
        "Remove and Leave Gaps",
        "Speed Up",
        "Add Markers",
    ];

    pub fn index(&self) -> usize {
        match self {
            Action::Remove => 0,
            Action::LeaveGap => 1,
            Action::SpeedUp(_) => 2,
            Action::Markers => 3,
        }
    }
}

/// Everything the dialog sets.
#[derive(Clone, Debug)]
pub struct SilenceSettings {
    pub scope: Scope,
    /// Look for silences in the sound.
    pub by_sound: bool,
    pub sound: sil::Params,
    /// Look for words (needs a transcription).
    pub by_words: bool,
    pub words: sil::TextParams,
    pub model: ModelSize,
    /// Spoken language code; None detects it.
    pub language: Option<String>,
    pub action: Action,
    /// Move word cuts to the quietest moment next to them.
    pub snap: bool,
    /// Milliseconds of audio crossfade at every cut (0 off).
    pub smooth_ms: f64,
}

impl Default for SilenceSettings {
    fn default() -> Self {
        SilenceSettings {
            scope: Scope::Selection,
            by_sound: true,
            sound: sil::Params::default(),
            by_words: false,
            words: sil::TextParams::default(),
            model: ModelSize::Base,
            language: None,
            action: Action::Remove,
            snap: true,
            smooth_ms: 12.0,
        }
    }
}

impl SilenceSettings {
    /// Whether a setting uses the neural curve. It is measured with every analysis (it is cheap),
    /// so switching to it later does not ask to analyze again.
    pub fn needs_neural(&self) -> bool {
        matches!(self.sound.method, sil::Method::Neural { .. }) || self.sound.voice_only
    }
}

/// An analysed stretch of audio.
pub struct Detection {
    /// Sequence time of the first sample.
    pub origin: SeqTime,
    pub analysis: Arc<Analysis>,
    /// Transcribed words, seconds from `origin` (None when not transcribed).
    pub words: Option<Arc<Vec<sil::Word>>>,
    pub language: String,
    /// The audio clips the analysis heard.
    pub clips: Vec<ClipId>,
}

impl Detection {
    /// The stretches to cut under `s`, sorted and without overlaps, seconds from `origin`.
    pub fn cuts(&self, s: &SilenceSettings) -> Vec<Cut> {
        let mut all: Vec<Cut> = Vec::new();
        if s.by_sound {
            all.extend(
                sil::detect(&self.analysis, &s.sound)
                    .into_iter()
                    .map(|range| Cut {
                        range,
                        kind: Kind::Silence,
                        text: String::new(),
                    }),
            );
        }
        if s.by_words
            && let Some(words) = &self.words
        {
            let mut p = s.words.clone();
            p.language = self.language.clone();
            let mut found = sil::text::find_all(words, self.analysis.duration(), &p);
            if s.snap {
                for c in &mut found {
                    if c.kind != Kind::Gap {
                        c.range = sil::snap_to_quiet(c.range, &self.analysis, 0.12);
                    }
                }
            }
            all.extend(found);
        }
        sil::text::merge(all)
    }
}

/// A running analysis.
pub struct SilenceJob {
    progress: Arc<Mutex<CaptionProgress>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<Detection, String>>>>,
}

impl SilenceJob {
    pub fn progress(&self) -> CaptionProgress {
        self.progress.lock().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Editor {
    /// The audio clips and sequence range a scope covers.
    pub fn silence_targets(&self, scope: Scope) -> Result<(Vec<ClipId>, SeqRange), String> {
        let seq = self.active_seq().ok_or("Open a sequence first")?;
        let audio: Vec<&Clip> = seq
            .audio
            .iter()
            .flat_map(|t| t.clips.iter())
            .filter(|c| c.enabled)
            .collect();
        let end = SeqTime::ZERO + seq.duration();
        let (clips, range): (Vec<&Clip>, SeqRange) = match scope {
            Scope::Selection => {
                let chosen: Vec<&Clip> = self
                    .selection
                    .clips
                    .iter()
                    .filter_map(|id| seq.clip(*id))
                    .collect();
                let links: Vec<LinkId> = chosen.iter().filter_map(|c| c.link).collect();
                let picked: Vec<&Clip> = audio
                    .iter()
                    .copied()
                    .filter(|c| {
                        chosen.iter().any(|x| x.id == c.id)
                            || c.link.is_some_and(|l| links.contains(&l))
                    })
                    .collect();
                let start = picked.iter().map(|c| c.start).min();
                let stop = picked.iter().map(|c| c.end()).max();
                match (start, stop) {
                    (Some(a), Some(b)) => (picked, SeqRange::new(a, b)),
                    _ => return Err("Select clips that have audio first".into()),
                }
            }
            Scope::Sequence => (audio, SeqRange::new(SeqTime::ZERO, end)),
            Scope::InOut => {
                let a = seq.mark_in.unwrap_or(SeqTime::ZERO);
                let b = seq.mark_out.unwrap_or(end);
                (audio, SeqRange::new(a, b.max(a)))
            }
        };
        if clips.is_empty() || range.is_empty() {
            return Err("There is no audio to analyze".into());
        }
        Ok((clips.iter().map(|c| c.id).collect(), range))
    }

    /// Starts measuring (and, if the settings use words, transcribing) the audio of `s.scope`.
    pub fn analyze_silence(&mut self, s: &SilenceSettings) -> Result<(), String> {
        if self.silencing.is_some() {
            return Err("An analysis is already running".into());
        }
        let sid = self.active.ok_or("Open a sequence first")?;
        let (clips, range) = self.silence_targets(s.scope)?;
        // the mix of the chosen clips only
        let mut project = self.project.clone();
        if let Some(seq) = project.sequence_mut(sid) {
            for t in &mut seq.audio {
                let t = Arc::make_mut(t);
                for c in &mut t.clips {
                    if !clips.contains(&c.id) {
                        c.enabled = false;
                    }
                }
            }
        }
        let media = self.media.clone();
        let models = models_dir(self);
        let progress = Arc::new(Mutex::new(CaptionProgress {
            stage: CaptionStage::MixingAudio,
            fraction: 0.0,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r) = (progress.clone(), cancel.clone(), result.clone());
        let (neural, words, model, language) = (true, s.by_words, s.model, s.language.clone());
        let origin = range.start;
        let secs = range.duration().seconds();
        let spawned = std::thread::Builder::new()
            .name("silence".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let pcm = audio_16k(&project, sid, &media, Some((origin, secs)), &p, &c)?;
                    *p.lock() = CaptionProgress {
                        stage: CaptionStage::Analyzing,
                        fraction: 0.0,
                    };
                    let analysis = sil::analyze(&pcm, neural, &mut |f| p.lock().fraction = f, &c)
                        .map_err(|_| "cancelled".to_string())?;
                    let (language, words) = if words {
                        ensure_model(&models, model, &p, &c)?;
                        let opts = TranscribeOptions {
                            model,
                            language,
                            task: Task::Transcribe,
                            verbatim: true,
                            captions: Default::default(),
                        };
                        let (lang, w) = transcribe_words(&pcm, &models, &opts, &p, &c)?;
                        let w: Vec<sil::Word> = w
                            .into_iter()
                            .map(|w| sil::Word {
                                text: w.text,
                                start: w.start,
                                end: w.end,
                            })
                            .collect();
                        (lang, Some(Arc::new(w)))
                    } else {
                        (String::new(), None)
                    };
                    Ok::<_, String>(Detection {
                        origin,
                        analysis: Arc::new(analysis),
                        words,
                        language,
                        clips,
                    })
                }))
                .unwrap_or_else(|_| Err("the analysis stopped unexpectedly".into()));
                *r.lock() = Some(res);
            });
        if spawned.is_err() {
            return Err("The analysis could not start".into());
        }
        self.silencing = Some(SilenceJob {
            progress,
            cancel,
            result,
        });
        Ok(())
    }

    /// Takes a finished analysis out of its job.
    pub(crate) fn poll_silence(&mut self) {
        let Some(job) = &self.silencing else { return };
        let Some(res) = job.result.lock().take() else {
            return;
        };
        self.silencing = None;
        match res {
            Ok(d) => self.silence_result = Some(Arc::new(d)),
            Err(e) if e == "cancelled" => {}
            Err(e) => self.error(format!("Silence not analyzed: {e}")),
        }
    }

    /// Applies `cuts` (seconds from `origin`) to the active sequence as one undo step. Returns
    /// how many stretches were cut, sped up or marked.
    pub fn apply_cuts(
        &mut self,
        origin: SeqTime,
        cuts: &[Range],
        action: Action,
        smooth_ms: f64,
    ) -> usize {
        let Some(seq) = self.active_seq() else {
            return 0;
        };
        let rate = seq.rate();
        let snap = |s: f64| -> SeqTime {
            let t = origin + Dur::from_seconds(s);
            SeqTime::ZERO + rate.frames_to_dur(rate.dur_to_frames_round(t - SeqTime::ZERO))
        };
        // on the frame grid, without empty or overlapping stretches
        let mut ranges: Vec<SeqRange> = Vec::new();
        for c in cuts {
            let r = SeqRange::new(snap(c.start), snap(c.end));
            if r.is_empty() || ranges.last().is_some_and(|l| r.start < l.end) {
                continue;
            }
            ranges.push(r);
        }
        if ranges.is_empty() {
            return 0;
        }
        let all: Vec<TrackRef> = (0..seq.video.len())
            .map(TrackRef::video)
            .chain((0..seq.audio.len()).map(TrackRef::audio))
            .collect();
        let audio: Vec<TrackRef> = (0..seq.audio.len()).map(TrackRef::audio).collect();
        let n = ranges.len();
        let label = match action {
            Action::Remove => "Remove Silence",
            Action::LeaveGap => "Remove Silence (Leave Gaps)",
            Action::SpeedUp(_) => "Speed Up Silence",
            Action::Markers => "Mark Silence",
        };
        let done = self.seq_edit(label, |p, sid, opts| {
            match action {
                Action::Remove => {
                    // from the end, so earlier stretches keep their places
                    for r in ranges.iter().rev() {
                        op_timeline::extract(p, sid, *r, &all, opts)?;
                    }
                    if smooth_ms > 0.0 {
                        // a short constant-power crossfade where audio was joined
                        let mut removed = Dur::ZERO;
                        for r in &ranges {
                            let at = r.start - removed;
                            removed += r.duration();
                            for t in &audio {
                                let _ = op_timeline::add_transition(
                                    p,
                                    sid,
                                    *t,
                                    at,
                                    catalog::CONSTANT_POWER,
                                    Dur::from_seconds(smooth_ms / 1000.0),
                                    Some(Alignment::CenterAtCut),
                                    opts,
                                );
                            }
                        }
                    }
                }
                Action::LeaveGap => {
                    for r in ranges.iter().rev() {
                        op_timeline::lift(p, sid, *r, &all, opts)?;
                    }
                }
                Action::SpeedUp(percent) => {
                    let speed = Speed::from_percent(percent.clamp(101.0, 100_000.0));
                    for r in ranges.iter().rev() {
                        for t in [r.end, r.start] {
                            let ids: Vec<ClipId> = p
                                .sequence(sid)
                                .map(|s| {
                                    s.video
                                        .iter()
                                        .chain(&s.audio)
                                        .flat_map(|tr| &tr.clips)
                                        .filter(|c| c.start < t && c.end() > t)
                                        .map(|c| c.id)
                                        .collect()
                                })
                                .unwrap_or_default();
                            if !ids.is_empty() {
                                let _ = razor(p, sid, &ids, t, opts);
                            }
                        }
                        let ids: Vec<ClipId> = p
                            .sequence(sid)
                            .map(|s| {
                                s.video
                                    .iter()
                                    .chain(&s.audio)
                                    .flat_map(|tr| &tr.clips)
                                    .filter(|c| c.start >= r.start && c.end() <= r.end)
                                    .map(|c| c.id)
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !ids.is_empty() {
                            set_speed(p, sid, &ids, speed, false, true, opts)?;
                        }
                    }
                }
                Action::Markers => {
                    for (i, r) in ranges.iter().enumerate() {
                        let id = p.ids.marker();
                        let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                        let mut m = Marker::new(id, r.start);
                        m.duration = r.duration();
                        m.name = format!("Silence {} ({:.1} s)", i + 1, r.duration().seconds());
                        s.markers.push(m);
                    }
                    if let Some(s) = p.sequence_mut(sid) {
                        s.markers.sort_by_key(|m| m.start);
                    }
                }
            }
            Ok(())
        });
        if done.is_some() { n } else { 0 }
    }
}

/// What cutting one file did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileCut {
    pub count: usize,
    /// Seconds removed (or sped up, or marked).
    pub removed: f64,
    pub before: f64,
    pub after: f64,
}

/// The output file for `input` inside `dir`: the same name plus `suffix`, in a container the
/// program writes (video: mp4, mov or mxf; audio: wav or m4a).
pub fn output_name(
    dir: &std::path::Path,
    input: &std::path::Path,
    suffix: &str,
) -> std::path::PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let out_ext = match ext.as_str() {
        "mov" | "mxf" | "wav" | "m4a" => ext.as_str(),
        "mp3" | "flac" | "ogg" | "opus" | "aac" | "aif" | "aiff" | "wma" => "wav",
        _ => "mp4",
    };
    dir.join(format!("{stem}{suffix}.{out_ext}"))
}

/// Waits for background work of `ed`, giving up after `secs` or when `cancel` is set.
fn wait_for(
    ed: &mut Editor,
    secs: u64,
    cancel: &AtomicBool,
    busy: impl Fn(&Editor) -> bool,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    while busy(ed) {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if start.elapsed().as_secs() > secs {
            return Err("it took too long".into());
        }
        ed.tick();
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    Ok(())
}

/// Cuts the silences out of one media file and writes the result: imports it, analyzes and cuts
/// it as a sequence of its own and exports that. The format follows `output`'s extension (mp4,
/// mkv and others H.264, mov ProRes, mxf DNxHR; wav and m4a for audio). `ed` is a fresh editor,
/// usually one with `background` set.
pub fn cut_file(
    ed: &mut Editor,
    gpu: &Arc<op_render::Gpu>,
    input: &std::path::Path,
    output: &std::path::Path,
    s: &SilenceSettings,
    cancel: &AtomicBool,
) -> Result<FileCut, String> {
    use op_media::{AudioCodec, AudioSettings, VideoCodec, VideoSettings};
    let root = ed.project.root;
    ed.import(vec![input.to_path_buf()], root);
    wait_for(ed, 600, cancel, |e| {
        e.importing > 0 || !e.imports.is_empty()
    })?;
    let item = *ed
        .items
        .first()
        .ok_or_else(|| format!("{} could not be read", input.display()))?;
    let sid = ed
        .sequence_from_item(item)
        .ok_or("the file cannot start a sequence")?;
    ed.open_sequence(sid);
    let mut settings = s.clone();
    settings.scope = Scope::Sequence;
    ed.silence_result = None;
    ed.analyze_silence(&settings)?;
    wait_for(ed, 7200, cancel, |e| e.silencing.is_some())?;
    let found = ed
        .silence_result
        .clone()
        .ok_or("the audio could not be analyzed")?;
    let cuts = found.cuts(&settings);
    let before = ed
        .active_seq()
        .map(|q| q.duration().seconds())
        .unwrap_or(0.0);
    let ranges: Vec<Range> = cuts.iter().map(|c| c.range).collect();
    let count = ed.apply_cuts(found.origin, &ranges, settings.action, settings.smooth_ms);
    let seq = ed.active_seq().ok_or("no sequence")?.clone();
    let after = seq.duration().seconds();
    let has_video = seq.video.iter().any(|t| !t.clips.is_empty());
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (video, audio) = if has_video {
        let codec = match ext.as_str() {
            "mov" => VideoCodec::ProRes422Hq,
            "mxf" => VideoCodec::DnxhrHq,
            _ => VideoCodec::H264,
        };
        let audio = AudioSettings {
            codec: if matches!(codec, VideoCodec::H264 | VideoCodec::Hevc) {
                AudioCodec::Aac
            } else {
                AudioCodec::Pcm24
            },
            rate: 48_000,
            channels: 2,
            bitrate_kbps: 192,
        };
        (
            Some(VideoSettings {
                codec,
                width: seq.settings.width & !1,
                height: seq.settings.height & !1,
                rate: seq.rate(),
                bitrate_kbps: None,
                quality: 20,
                hardware: true,
            }),
            audio,
        )
    } else {
        (
            None,
            AudioSettings {
                codec: if matches!(ext.as_str(), "m4a" | "aac") {
                    AudioCodec::Aac
                } else {
                    AudioCodec::Pcm24
                },
                rate: 48_000,
                channels: 2,
                bitrate_kbps: 192,
            },
        )
    };
    if let Some(dir) = output.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let job = crate::export::ExportJob::start(
        ed.snapshot(),
        crate::export::ExportSettings {
            path: output.to_path_buf(),
            sequence: sid,
            range: SeqRange::new(SeqTime::ZERO, SeqTime::ZERO + seq.duration()),
            video,
            audio: Some(audio),
        },
        gpu.clone(),
        ed.media.clone(),
    );
    while !job.finished() {
        if cancel.load(Ordering::Relaxed) {
            job.cancel();
            return Err("cancelled".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    if let Some(e) = job.progress.lock().error.clone() {
        return Err(e);
    }
    Ok(FileCut {
        count,
        removed: cuts.iter().map(|c| c.range.len()).sum(),
        before,
        after,
    })
}

/// The detection settings as JSON for assistants and the log.
pub fn describe(cuts: &[Cut]) -> Json {
    let total: f64 = cuts.iter().map(|c| c.range.len()).sum();
    json!({
        "count": cuts.len(),
        "seconds": (total * 1000.0).round() / 1000.0,
        "cuts": cuts.iter().take(500).map(|c| json!({
            "start": (c.range.start * 1000.0).round() / 1000.0,
            "end": (c.range.end * 1000.0).round() / 1000.0,
            "kind": c.kind.label(),
            "text": c.text,
        })).collect::<Vec<_>>(),
    })
}
