//! Remix for music clips (like Remix in Premiere's Essential Sound): the music is made longer or
//! shorter by playing a part that repeats once more, or skipping it, so the result keeps its
//! natural flow. The analysis is `op-remix`; here are its job, the plan for a wanted length and the
//! edit that lays the result on the timeline.
//!
//! The remixed clip becomes a group of clips (the pieces of the original, in order) joined by short
//! constant-power crossfades. Revert puts them back into the one clip.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use op_core::catalog;
use op_core::*;
use op_media::ConformedAudio;
use op_timeline::add_transition;
use parking_lot::Mutex;

use crate::editor::Editor;

pub use op_remix::Plan;

/// Shortest repeated or skipped stretch the panel starts with, in seconds.
pub const DEFAULT_MIN_LOOP: f64 = 4.0;

/// What Remix works on: one music clip, or the pieces of one that was remixed before.
#[derive(Clone, Debug)]
pub struct RemixTarget {
    pub track: usize,
    /// The clips (one, or the pieces in order).
    pub pieces: Vec<ClipId>,
    /// The first piece: its effects and settings go to every new piece.
    pub first: Clip,
    pub asset: AssetId,
    pub stream: usize,
    /// The original, in the source file.
    pub src_in: SrcTime,
    pub src_out: SrcTime,
    /// Where it starts on the timeline and how long it plays now.
    pub start: SeqTime,
    pub length: Dur,
    pub group: Option<GroupId>,
}

impl RemixTarget {
    /// Seconds of the original.
    pub fn original(&self) -> f64 {
        (self.src_out - self.src_in).seconds()
    }
}

/// A finished analysis of one piece of music.
pub struct RemixAnalysis {
    pub asset: AssetId,
    pub stream: usize,
    pub src_in: SrcTime,
    pub src_out: SrcTime,
    pub analysis: Arc<op_remix::Analysis>,
    /// The places the music repeats, found once per shortest stretch asked for.
    loops: Mutex<Option<(i64, Arc<Vec<op_remix::Loop>>)>>,
}

impl RemixAnalysis {
    fn loops(&self, min_loop: f64) -> Arc<Vec<op_remix::Loop>> {
        let min = min_loop.max(1.0);
        let key = (min * 10.0).round() as i64;
        let mut cache = self.loops.lock();
        match &*cache {
            Some((k, l)) if *k == key => l.clone(),
            _ => {
                let l = Arc::new(self.analysis.loops(min, self.analysis.seconds * 0.9, 400));
                *cache = Some((key, l.clone()));
                l
            }
        }
    }

    fn matches(&self, t: &RemixTarget) -> bool {
        self.asset == t.asset && self.stream == t.stream && self.src_in == t.src_in && self.src_out == t.src_out
    }

    pub fn bpm(&self) -> f32 {
        self.analysis.bpm
    }

    /// Seconds of the analysed music.
    pub fn seconds(&self) -> f64 {
        self.analysis.seconds
    }
}

/// The analysis running in the background.
pub struct RemixJob {
    pub clip: ClipId,
    progress: Arc<Mutex<f32>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<RemixAnalysis, String>>>>,
}

impl RemixJob {
    pub fn progress(&self) -> f32 {
        *self.progress.lock()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct RemixState {
    pub job: Option<RemixJob>,
    pub analysis: Option<RemixAnalysis>,
}

/// `seconds` of `audio` from `start` as mono at the analysis rate (each output sample averages the
/// input frames it covers).
fn mono_22k(audio: &ConformedAudio, start: f64, seconds: f64) -> Vec<f32> {
    let rate = u64::from(audio.info.rate.max(1));
    let ch = usize::from(audio.info.channels.max(1));
    let out_rate = op_remix::RATE as u64;
    let first_frame = (start.max(0.0) * rate as f64) as u64;
    let available = (audio.info.samples.max(0) as u64).saturating_sub(first_frame);
    let frames = available.min((seconds * rate as f64) as u64);
    let len = (frames * out_rate / rate) as usize;
    let at = |j: u64| first_frame + j * rate / out_rate;
    let mut out = Vec::with_capacity(len);
    let mut buf = Vec::new();
    let mut j = 0usize;
    while j < len {
        let n = (1usize << 16).min(len - j);
        let (first, last) = (at(j as u64), at((j + n) as u64));
        let here = (last - first) as usize;
        buf.resize(here.max(1) * ch, 0.0);
        audio.read(first as i64, &mut buf);
        for k in 0..n as u64 {
            let a = (at(j as u64 + k) - first) as usize;
            let b = ((at(j as u64 + k + 1) - first) as usize).max(a + 1).min(here.max(1));
            let sum: f32 = (a..b).map(|f| buf[f * ch..(f + 1) * ch].iter().sum::<f32>() / ch as f32).sum();
            out.push(sum / (b - a).max(1) as f32);
        }
        j += n;
    }
    out
}

impl Editor {
    /// The music clip (or remix group) that `clip` belongs to, with everything an edit needs.
    pub fn remix_target(&self, clip: ClipId) -> Result<RemixTarget, String> {
        let seq = self.active_seq().ok_or("먼저 시퀀스를 여세요")?;
        let (index, track) = seq
            .audio
            .iter()
            .enumerate()
            .find(|(_, t)| t.clip(clip).is_some())
            .ok_or("리믹스는 오디오 트랙의 음악 클립에만 쓸 수 있습니다")?;
        let this = track.clip(clip).ok_or("클립을 찾을 수 없습니다")?;
        let group = this.group.filter(|g| self.project.remixes.contains(g));
        let mut pieces: Vec<&Clip> = match group {
            Some(g) => track.clips.iter().filter(|c| c.group == Some(g)).collect(),
            None => vec![this],
        };
        pieces.sort_by_key(|c| c.start);
        let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
        let ClipSource::Asset { asset, stream, .. } = first.source else {
            return Err("리믹스는 음악 파일 클립에만 쓸 수 있습니다".into());
        };
        if first.link.is_some() {
            return Err("영상과 연결된 클립은 리믹스할 수 없습니다: 연결을 해제한 오디오 클립에 쓰세요".into());
        }
        if pieces.iter().any(|c| c.speed != Speed::NORMAL || c.reverse || c.hold.is_some()) {
            return Err("속도를 바꾸었거나 거꾸로·정지 프레임인 클립은 리믹스할 수 없습니다".into());
        }
        Ok(RemixTarget {
            track: index,
            pieces: pieces.iter().map(|c| c.id).collect(),
            first: first.clone(),
            asset,
            stream,
            src_in: first.source_in,
            src_out: last.source_in + last.source_duration(),
            start: first.start,
            length: last.end() - first.start,
            group,
        })
    }

    /// Whether the music of `clip` is remixed (it is a group made by Remix).
    pub fn is_remixed(&self, clip: ClipId) -> bool {
        self.active_seq()
            .and_then(|s| s.audio.iter().find_map(|t| t.clip(clip)))
            .and_then(|c| c.group)
            .is_some_and(|g| self.project.remixes.contains(&g))
    }

    /// Whether an analysis of this very music is ready.
    pub fn remix_ready_for(&self, clip: ClipId) -> bool {
        match (&self.remix.analysis, self.remix_target(clip)) {
            (Some(a), Ok(t)) => a.matches(&t),
            _ => false,
        }
    }

    pub fn remix_analysis(&self) -> Option<&RemixAnalysis> {
        self.remix.analysis.as_ref()
    }

    /// Starts analysing the music of `clip` (tempo, notes, where it repeats) in the background.
    pub fn remix_analyze(&mut self, clip: ClipId) -> Result<(), String> {
        if self.remix.job.is_some() {
            return Err("다른 음악을 분석하는 중입니다".into());
        }
        let target = self.remix_target(clip)?;
        if self.remix_ready_for(clip) {
            return Ok(());
        }
        let media = self.media.clone();
        let progress = Arc::new(Mutex::new(0f32));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r) = (progress.clone(), cancel.clone(), result.clone());
        let (asset, stream, src_in, src_out) = (target.asset, target.stream, target.src_in, target.src_out);
        std::thread::Builder::new()
            .name("remix".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // the sound of the file is made ready first (decoded once, kept in the cache)
                    let started = Instant::now();
                    let audio = loop {
                        if c.load(Ordering::Relaxed) {
                            return Err("cancelled".to_string());
                        }
                        if let Some(a) = media.conformed(asset, stream) {
                            break a;
                        }
                        if let Some(why) = media.audio_error(asset, stream) {
                            return Err(format!("음악을 읽을 수 없습니다: {why}"));
                        }
                        if started.elapsed() > Duration::from_secs(600) {
                            return Err("음악을 읽는 데 너무 오래 걸립니다".into());
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    };
                    let pcm = mono_22k(&audio, src_in.seconds(), (src_out - src_in).seconds());
                    let analysis = op_remix::analyze(pcm, &mut |f| *p.lock() = f, &c).map_err(|e| match e {
                        op_remix::AnalyzeError::Cancelled => "cancelled".to_string(),
                        op_remix::AnalyzeError::TooShort => "10초보다 짧은 음악은 리믹스할 수 없습니다".to_string(),
                    })?;
                    // the places it repeats are found here, so the panel does not wait for them
                    let found = Arc::new(analysis.loops(DEFAULT_MIN_LOOP, analysis.seconds * 0.9, 400));
                    let loops = Mutex::new(Some(((DEFAULT_MIN_LOOP * 10.0).round() as i64, found)));
                    Ok(RemixAnalysis { asset, stream, src_in, src_out, analysis: Arc::new(analysis), loops })
                }))
                .unwrap_or_else(|_| Err("분석이 예기치 않게 멈췄습니다".into()));
                *r.lock() = Some(res);
            })
            .map_err(|_| "분석을 시작할 수 없습니다".to_string())?;
        self.remix.job = Some(RemixJob { clip, progress, cancel, result });
        Ok(())
    }

    pub(crate) fn poll_remix(&mut self) {
        let Some(job) = &self.remix.job else { return };
        let Some(res) = job.result.lock().take() else { return };
        self.remix.job = None;
        match res {
            Ok(a) => {
                self.info(format!("음악 분석 끝: 템포 {:.0} bpm, 박자 {}개", a.analysis.bpm, a.analysis.beats.len()));
                self.remix.analysis = Some(a);
            }
            Err(e) if e == "cancelled" => {}
            Err(e) => self.error(e),
        }
    }

    /// The plan for a version of the music of `clip` that plays `target` seconds. Repeated or skipped
    /// stretches are at least `min_loop` seconds.
    pub fn remix_plan(&self, clip: ClipId, target: f64, min_loop: f64) -> Result<Plan, String> {
        let t = self.remix_target(clip)?;
        let a = self.remix.analysis.as_ref().filter(|a| a.matches(&t)).ok_or("먼저 음악을 분석하세요")?;
        let loops = a.loops(min_loop);
        Ok(op_remix::plan(&loops, &op_remix::PlanOptions { range: (0.0, a.seconds()), target, min_loop }))
    }

    /// The lengths (seconds, with how well the join sounds) the music can grow or shrink by, best
    /// joins first: the steps of this very piece of music.
    pub fn remix_steps(&self, clip: ClipId, min_loop: f64) -> Result<Vec<(f64, f32)>, String> {
        let t = self.remix_target(clip)?;
        let a = self.remix.analysis.as_ref().filter(|a| a.matches(&t)).ok_or("먼저 음악을 분석하세요")?;
        let mut out: Vec<(f64, f32)> = Vec::new();
        for l in a.loops(min_loop).iter() {
            if out.iter().all(|(len, _)| (len - l.length()).abs() > 0.25) {
                out.push((l.length(), l.score));
            }
            if out.len() == 4 {
                break;
            }
        }
        Ok(out)
    }

    /// Lays `plan` on the timeline in place of the music clip (or its pieces): one undo step.
    pub fn remix_apply(&mut self, clip: ClipId, plan: &Plan) -> Result<f64, String> {
        let target = self.remix_target(clip)?;
        let original = target.original();
        let single = plan.segments.len() <= 1;
        let duration = plan.duration;
        let plan = plan.clone();
        let track_index = target.track;
        let (target_start, old_pieces) = (target.start, target.pieces.clone());
        let crossfade = Dur::from_seconds(0.04);
        let result = self.seq_edit("Remix", move |p, sid, opts| {
            let mut ids = p.ids.clone();
            let group = if single { None } else { Some(target.group.unwrap_or_else(|| ids.group())) };
            // the pieces of the result
            let mut pieces: Vec<Clip> = Vec::new();
            let mut at = target.start;
            for (k, (s, e)) in plan.segments.iter().enumerate() {
                let src_in = if k == 0 && *s <= 1e-9 { target.src_in } else { SrcTime::from_seconds(target.src_in.seconds() + s) };
                let dur = if (e - original).abs() < 1e-6 { target.src_out - src_in } else { Dur::from_seconds(e - s) };
                let mut c = target.first.clone();
                c.id = ids.clip();
                c.start = at;
                c.duration = dur;
                c.source_in = src_in;
                c.group = group;
                c.link = None;
                for comp in &mut c.components {
                    comp.id = ids.component();
                }
                at = at + dur;
                pieces.push(c);
            }
            let new_end = at;
            let seq = p.sequence_mut(sid).ok_or(EditError::NotFound("sequence".into()))?;
            let track = Arc::make_mut(seq.audio.get_mut(track_index).ok_or(EditError::NotFound("track".into()))?);
            if track.locked {
                return Err(EditError::Invalid("잠긴 트랙입니다".into()));
            }
            track.clips.retain(|c| !target.pieces.contains(&c.id));
            track.transitions.retain(|t| !t.from.is_some_and(|f| target.pieces.contains(&f)) && !t.to.is_some_and(|f| target.pieces.contains(&f)));
            if let Some(next) = track.clips.iter().find(|c| c.start < new_end && c.end() > target.start) {
                return Err(EditError::Invalid(format!(
                    "뒤에 다른 클립이 있어 {:.1}초까지 늘릴 수 없습니다 (그 클립은 {:.1}초에 시작): 음악 뒤 자리를 비우세요",
                    (new_end - target.start).seconds(),
                    next.start.seconds()
                )));
            }
            for c in &pieces {
                let pos = track.clips.partition_point(|x| x.start <= c.start);
                track.clips.insert(pos, c.clone());
            }
            p.ids = ids;
            if let Some(g) = group {
                if !p.remixes.contains(&g) {
                    p.remixes.push(g);
                }
            } else if let Some(g) = target.group {
                p.remixes.retain(|x| *x != g);
            }
            // a short crossfade at every join, so the cuts are not heard
            let mut cut = target.start;
            for c in &pieces[..pieces.len().saturating_sub(1)] {
                cut = cut + c.duration;
                let _ = add_transition(p, sid, TrackRef::audio(track_index), cut, catalog::CONSTANT_POWER, crossfade, Some(Alignment::CenterAtCut), opts);
            }
            Ok(())
        });
        if result.is_some() {
            // the selection follows the new pieces
            let end = target_start + Dur::from_seconds(duration);
            let now: Vec<ClipId> = self
                .active_seq()
                .and_then(|q| q.audio.get(track_index))
                .map(|t| t.clips.iter().filter(|c| c.start >= target_start && c.start < end).map(|c| c.id).collect())
                .unwrap_or_default();
            if !now.is_empty() && self.selection.clips.iter().any(|c| old_pieces.contains(c)) {
                self.selection = crate::Selection::only(now);
            }
        }
        match result {
            Some(()) => Ok(duration),
            None => Err(self.status.as_ref().filter(|s| s.error).map(|s| s.text.clone()).unwrap_or_else(|| "리믹스를 적용할 수 없습니다".into())),
        }
    }

    /// Puts a remixed music clip back into the one clip it was made from.
    pub fn remix_revert(&mut self, clip: ClipId) -> Result<(), String> {
        let target = self.remix_target(clip)?;
        if target.group.is_none() {
            return Err("이 음악은 리믹스되어 있지 않습니다".into());
        }
        let original = target.original();
        let plan = Plan { moves: Vec::new(), segments: vec![(0.0, original)], duration: original, original };
        self.remix_apply(clip, &plan).map(|_| ())
    }
}
