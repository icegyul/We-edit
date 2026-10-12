//! Editing by sentence: what the sequence says, sentence by sentence, with times.
//!
//! A background job mixes the sequence's audio, hands it to the speech helper (Whisper large-v3
//! on the graphics card, see `helper/transcribe.py`) and keeps the sentences. They are in
//! sequence seconds, so a sentence can be cut out of the timeline directly with the same ripple
//! cut as Remove Silence, as one undo step.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use op_core::*;
use op_silence::Range;
use parking_lot::Mutex;
use serde_json::Value as Json;

use crate::captions::{CaptionProgress, CaptionStage, audio_16k};
use crate::editor::Editor;
use crate::silence::Action;

/// One spoken sentence.
#[derive(Clone, Debug)]
pub struct Sentence {
    /// Stays the same when other sentences are cut, so "sentence 7" always means the same words.
    pub id: usize,
    /// Sequence seconds.
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Word by word (어절), in sequence seconds.
    pub words: Vec<op_core::captions::TimedWord>,
}

#[derive(Clone, Debug)]
pub struct Transcript {
    pub sequence: SequenceId,
    pub language: String,
    pub sentences: Vec<Sentence>,
    /// Undo position the times belong to: any other edit makes them untrustworthy.
    stamp: usize,
}

/// A transcription running in the background.
pub struct SentenceJob {
    sequence: SequenceId,
    progress: Arc<Mutex<CaptionProgress>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<(String, Vec<Sentence>), String>>>>,
}

impl SentenceJob {
    pub fn progress(&self) -> CaptionProgress {
        self.progress.lock().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The helper's Python and script: `WEEDIT_HELPER` names the folder, else it is the
/// `helper` folder beside the program's own (`<folder>\app\OpenPremier.exe`).
pub fn helper_dir() -> Option<PathBuf> {
    std::env::var_os("WEEDIT_HELPER")
        .map(PathBuf::from)
        .or_else(|| Some(std::env::current_exe().ok()?.parent()?.parent()?.join("helper")))
}

pub fn helper_files() -> Option<(PathBuf, PathBuf)> {
    let dir = helper_dir()?;
    let python = if cfg!(windows) {
        dir.join("venv").join("Scripts").join("python.exe")
    } else {
        dir.join("venv").join("bin").join("python")
    };
    let script = dir.join("transcribe.py");
    (python.is_file() && script.is_file()).then_some((python, script))
}

/// Runs the helper on 16 kHz mono sound; returns the language and the sentences.
fn run_helper(
    python: &Path,
    script: &Path,
    pcm: &[f32],
    language: &str,
    model: &Path,
    scratch: &Path,
    progress: &Arc<Mutex<CaptionProgress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(String, Vec<Sentence>), String> {
    let file = scratch.join("sentences.f32");
    let bytes: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(&file, bytes).map_err(|e| format!("cannot write the sound for the helper: {e}"))?;
    let mut cmd = Command::new(python);
    cmd.arg(script).arg("--pcm").arg(&file).arg("--model").arg(model);
    if !language.is_empty() {
        cmd.arg("--language").arg(language);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("cannot start the speech helper: {e}"))?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let child = Arc::new(Mutex::new(child));
    // progress lines ("progress 0.42") update the bar; anything else is kept for an error message
    let tail = Arc::new(Mutex::new(String::new()));
    let reader = stderr.map(|err| {
        let (p, tail) = (progress.clone(), tail.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                match line.strip_prefix("progress ").and_then(|v| v.trim().parse::<f32>().ok()) {
                    Some(f) => p.lock().fraction = f,
                    None => {
                        let mut t = tail.lock();
                        t.push_str(&line);
                        t.push('\n');
                        let keep = t.len().saturating_sub(1500);
                        t.drain(..keep);
                    }
                }
            }
        })
    });
    // a watcher ends the helper when the job is cancelled
    let finished = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (child, cancel, finished) = (child.clone(), cancel.clone(), finished.clone());
        std::thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.lock().kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };
    let mut out = Vec::new();
    if let Some(mut o) = stdout {
        let _ = o.read_to_end(&mut out);
    }
    finished.store(true, Ordering::Relaxed);
    let status = child.lock().wait();
    let _ = watcher.join();
    if let Some(r) = reader {
        let _ = r.join();
    }
    let _ = std::fs::remove_file(&file);
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if !status.map(|s| s.success()).unwrap_or(false) {
        return Err(format!("the speech helper failed: {}", tail.lock().trim()));
    }
    let json: Json = serde_json::from_slice(&out).map_err(|e| format!("unreadable helper answer: {e}"))?;
    let language = json["language"].as_str().unwrap_or("").to_string();
    let sentences = json["sentences"]
        .as_array()
        .ok_or("the helper returned no sentences")?
        .iter()
        .enumerate()
        .map(|(id, s)| {
            let (start, end) = (s["start"].as_f64().unwrap_or(0.0), s["end"].as_f64().unwrap_or(0.0));
            let text = s["text"].as_str().unwrap_or("").to_string();
            let mut words: Vec<op_core::captions::TimedWord> = s["words"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|w| op_core::captions::TimedWord {
                    text: w["text"].as_str().unwrap_or("").to_string(),
                    start: w["start"].as_f64().unwrap_or(start),
                    end: w["end"].as_f64().unwrap_or(end),
                })
                .collect();
            if words.is_empty() {
                words = op_core::captions::spread(&text, start, end);
            }
            Sentence { id, start, end, text, words }
        })
        .collect();
    Ok((language, sentences))
}

impl Editor {
    /// Starts transcribing the active sequence's audio into sentences (`language` like "ko", or
    /// empty to detect it).
    pub fn transcribe_sentences(&mut self, language: &str) -> Result<(), String> {
        if self.sentencing.is_some() {
            return Err("A sentence transcription is already running".into());
        }
        let sid = self.active.ok_or("Open a sequence first")?;
        let (python, script) = helper_files()
            .ok_or("The speech helper is missing: it belongs in the helper folder beside the program's folder")?;
        let model = crate::whisper_models::chosen_dir(&self.prefs.whisper_model)
            .ok_or("No speech model is installed: download one in Preferences > Speech Recognition")?;
        let (project, media) = (self.project.clone(), self.media.clone());
        let scratch = self.dirs.cache.clone();
        let language = language.to_string();
        let progress = Arc::new(Mutex::new(CaptionProgress {
            stage: CaptionStage::MixingAudio,
            fraction: 0.0,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r) = (progress.clone(), cancel.clone(), result.clone());
        let spawned = std::thread::Builder::new().name("sentences".into()).spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let pcm = audio_16k(&project, sid, &media, None, &p, &c)?;
                *p.lock() = CaptionProgress {
                    stage: CaptionStage::Transcribing,
                    fraction: 0.0,
                };
                let _ = std::fs::create_dir_all(&scratch);
                run_helper(&python, &script, &pcm, &language, &model, &scratch, &p, &c)
            }))
            .unwrap_or_else(|_| Err("the transcription stopped unexpectedly".into()));
            *r.lock() = Some(res);
        });
        if spawned.is_err() {
            return Err("The transcription could not start".into());
        }
        self.sentencing = Some(SentenceJob {
            sequence: sid,
            progress,
            cancel,
            result,
        });
        Ok(())
    }

    /// Takes a finished transcription out of its job.
    pub(crate) fn poll_sentences(&mut self) {
        let Some(job) = &self.sentencing else { return };
        let Some(res) = job.result.lock().take() else {
            return;
        };
        let sequence = job.sequence;
        self.sentencing = None;
        match res {
            Ok((language, sentences)) => {
                let stamp = self.history.labels().1;
                self.transcript = Some(Arc::new(Transcript {
                    sequence,
                    language,
                    sentences,
                    stamp,
                }));
                if let Some(opts) = self.captions_after.take() {
                    match self.captions_from_transcript(&opts, None) {
                        Ok(n) => self.info(format!("{n} captions made from the transcript")),
                        Err(e) => self.error(e),
                    }
                }
            }
            Err(e) if e == "cancelled" => {}
            Err(e) => self.error(format!("Sentences not transcribed: {e}")),
        }
    }

    /// The transcript still matches the timeline (nothing else was edited since).
    pub fn transcript_is_current(&self) -> bool {
        self.transcript
            .as_ref()
            .is_some_and(|t| Some(t.sequence) == self.active && t.stamp == self.history.labels().1)
    }

    /// Cuts the sentences `ids` out of the active sequence (every track, gaps closed, one undo
    /// step) and moves the remaining sentences up. Returns how many were cut.
    pub fn delete_sentences(&mut self, ids: &[usize]) -> Result<usize, String> {
        let t = self
            .transcript
            .clone()
            .ok_or("There is no transcript: transcribe the sequence first")?;
        let mut ranges = Vec::new();
        for id in ids {
            let s = t.sentences.iter().find(|s| s.id == *id).ok_or_else(|| format!("There is no sentence {id}"))?;
            ranges.push((s.start, s.end));
        }
        self.delete_ranges(&ranges)
    }

    /// Cuts the given stretches (sequence seconds) out of every track with the gaps closed, as
    /// one undo step, and brings the transcript along: the words inside are gone and the
    /// others move up. Returns how many stretches were cut.
    pub fn delete_ranges(&mut self, ranges: &[(f64, f64)]) -> Result<usize, String> {
        let t = self
            .transcript
            .clone()
            .ok_or("There is no transcript: transcribe the sequence first")?;
        if !self.transcript_is_current() {
            return Err("The sequence was edited after it was transcribed: transcribe it again".into());
        }
        // sorted, and touching stretches joined
        let mut cut: Vec<(f64, f64)> = ranges.iter().copied().filter(|r| r.1 > r.0).collect();
        cut.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for r in cut {
            match merged.last_mut() {
                Some(last) if r.0 <= last.1 + 1e-6 => last.1 = last.1.max(r.1),
                _ => merged.push(r),
            }
        }
        let cuts: Vec<Range> = merged.iter().map(|r| Range { start: r.0, end: r.1 }).collect();
        let n = self.apply_cuts(SeqTime::ZERO, &cuts, Action::Remove, 12.0);
        if n == 0 {
            return Err("Nothing was cut".into());
        }
        // the words that were inside are gone; those after move up by what was cut before them
        let mid = |w: &op_core::captions::TimedWord| (w.start + w.end) / 2.0;
        let mut kept = Vec::new();
        for s in &t.sentences {
            let words: Vec<op_core::captions::TimedWord> = s
                .words
                .iter()
                .filter(|w| !merged.iter().any(|r| mid(w) >= r.0 && mid(w) <= r.1))
                .map(|w| {
                    let before: f64 = merged.iter().filter(|r| r.1 <= mid(w)).map(|r| r.1 - r.0).sum();
                    op_core::captions::TimedWord { text: w.text.clone(), start: w.start - before, end: w.end - before }
                })
                .collect();
            let (Some(first), Some(last)) = (words.first(), words.last()) else { continue };
            kept.push(Sentence {
                id: s.id,
                start: first.start,
                end: last.end,
                text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
                words,
            });
        }
        let stamp = self.history.labels().1;
        self.transcript = Some(Arc::new(Transcript {
            sequence: t.sequence,
            language: t.language.clone(),
            sentences: kept,
            stamp,
        }));
        Ok(n)
    }

    /// Makes captions from the transcript (all of it, or the part between two times) with the
    /// chosen design: Whisper's words timed to the sound. Returns how many captions were made.
    pub fn captions_from_transcript(
        &mut self,
        opts: &crate::captions::CaptionOptions,
        part: Option<(f64, f64)>,
    ) -> Result<usize, String> {
        let t = self
            .transcript
            .clone()
            .ok_or("There is no transcript: transcribe the sequence first")?;
        if !self.transcript_is_current() {
            return Err("The sequence was edited after it was transcribed: transcribe it again".into());
        }
        let words: Vec<op_core::captions::TimedWord> = t
            .sentences
            .iter()
            .flat_map(|s| s.words.iter().cloned())
            .filter(|w| part.is_none_or(|(a, b)| w.start >= a - 1e-6 && w.end <= b + 1e-6))
            .collect();
        if words.is_empty() {
            return Err("There are no words there".into());
        }
        let cues = op_core::captions::chunk(&words, opts.max_words.max(1), 0.8, 0.3);
        let n = self.create_captions(&cues, opts);
        if n == 0 {
            return Err("No captions could be made".into());
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_is_found_by_environment_variable() {
        let dir = tempfile::tempdir().unwrap();
        let py = if cfg!(windows) { dir.path().join("venv/Scripts") } else { dir.path().join("venv/bin") };
        std::fs::create_dir_all(&py).unwrap();
        std::fs::write(py.join(if cfg!(windows) { "python.exe" } else { "python" }), b"").unwrap();
        std::fs::write(dir.path().join("transcribe.py"), b"").unwrap();
        // SAFETY: tests of this module are the only ones that touch this variable
        unsafe { std::env::set_var("WEEDIT_HELPER", dir.path()) };
        let (python, script) = helper_files().expect("helper");
        assert!(python.ends_with(if cfg!(windows) { "python.exe" } else { "python" }));
        assert!(script.ends_with("transcribe.py"));
        unsafe { std::env::remove_var("WEEDIT_HELPER") };
    }
}
