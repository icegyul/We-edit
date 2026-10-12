//! The sound library: sound effects and background music that are public domain (CC0), so they
//! can be used in any video without a copyright claim or a credit.
//!
//! Nothing is installed with the program. `data/sounds.json` lists the sounds (built by
//! `tools/build_sound_catalog.py` from repositories whose license text was read); a sound is
//! downloaded from GitHub, at a commit-pinned address, only when it is played or added, and kept
//! in the program's data folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use op_core::*;
use op_timeline::{Patch, SourceClip};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::editor::Editor;

#[derive(Deserialize)]
pub struct Source {
    pub repo: String,
    pub commit: String,
    pub name: String,
    pub license: String,
    pub proof: String,
}

#[derive(Deserialize)]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub ko: String,
    /// "sfx" or "bgm".
    pub kind: String,
    pub cat: String,
    pub tags: String,
    pub sec: f64,
    pub src: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct Catalog {
    pub sources: HashMap<String, Source>,
    pub sounds: Vec<Sound>,
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../data/sounds.json")).expect("the bundled sound catalog")
    })
}

pub fn find(id: &str) -> Option<&'static Sound> {
    catalog().sounds.iter().find(|s| s.id == id)
}

impl Sound {
    /// Name to show: Korean when there is one.
    pub fn title(&self) -> &str {
        if self.ko.is_empty() { &self.name } else { &self.ko }
    }

    pub fn source(&self) -> &'static Source {
        &catalog().sources[&self.src]
    }

    pub fn url(&self) -> String {
        let s = self.source();
        format!("https://raw.githubusercontent.com/{}/{}/{}", s.repo, s.commit, self.path)
    }

    fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Where the downloaded file is kept.
    pub fn cache_file(&self, data: &Path) -> PathBuf {
        data.join("Sounds").join(&self.src).join(self.file_name())
    }

    /// Whether `query` (words, any case) matches the name, the tags or the category.
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!("{} {} {} {}", self.name, self.ko, self.tags, self.cat).to_lowercase();
        query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
    }
}

/// What to do with a sound once its file is there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundAction {
    Preview,
    /// Put it on the timeline at the playhead.
    Add,
}

struct Fetch {
    id: &'static str,
    action: SoundAction,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}

struct Preview {
    id: &'static str,
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

/// Downloads and the preview of the sound library.
#[derive(Default)]
pub struct SoundState {
    fetch: Option<Fetch>,
    preview: Option<Preview>,
}

impl SoundState {
    /// Sound being downloaded.
    pub fn fetching(&self) -> Option<&'static str> {
        self.fetch.as_ref().map(|f| f.id)
    }

    /// Sound being played now.
    pub fn previewing(&self) -> Option<&'static str> {
        self.preview.as_ref().filter(|p| !p.done.load(Ordering::Relaxed)).map(|p| p.id)
    }
}

/// Plays `file` (decoded at once, resampled to the device) until it ends or `stop` is set.
fn play_file(file: &Path, cache: &Path, stop: &AtomicBool) -> Result<(), String> {
    let conformed = cache.with_extension("audio");
    if !conformed.is_file() {
        let peaks = cache.with_extension("peaks");
        op_media::audio::conform(file, 0, Dur::ZERO, &conformed, &peaks, &mut |_| !stop.load(Ordering::Relaxed))
            .map_err(|e| format!("{}: {e}", file.display()))?;
    }
    let audio = op_media::audio::ConformedAudio::open(&conformed).map_err(|e| e.to_string())?;
    let mut out = op_audio::device::Output::open();
    let (src_rate, src_ch) = (f64::from(audio.info.rate.max(1)), usize::from(audio.info.channels.max(1)));
    let ratio = src_rate / f64::from(out.rate.max(1));
    let total = (audio.info.samples as f64 / ratio) as usize;
    let mut pos = 0usize;
    let mut buf: Vec<f32> = Vec::new();
    while pos < total && !stop.load(Ordering::Relaxed) {
        let frames = out.free_frames().min(2048).min(total - pos);
        if frames == 0 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        buf.clear();
        for f in 0..frames {
            let at = (pos + f) as f64 * ratio;
            let i = at.floor() as i64;
            let frac = (at - i as f64) as f32;
            for c in 0..out.channels {
                let ch = c.min(src_ch - 1);
                let (a, b) = (audio.sample(i, ch), audio.sample(i + 1, ch));
                buf.push(a + (b - a) * frac);
            }
        }
        pos += out.write(&buf);
    }
    while out.queued_frames() > 0 && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(10));
    }
    out.flush();
    Ok(())
}

impl Editor {
    /// Plays a sound or puts it on the timeline; downloads it first when it is not on this PC.
    pub fn sound_request(&mut self, id: &str, action: SoundAction) -> Result<(), String> {
        let sound = find(id).ok_or("There is no such sound in the library")?;
        let file = sound.cache_file(&self.dirs.data);
        if file.is_file() {
            return self.sound_use(sound, action);
        }
        if let Some(f) = &self.sound.fetch {
            return Err(format!("'{}' is still downloading", find(f.id).map_or(f.id, |s| s.title())));
        }
        let url = sound.url();
        let result = Arc::new(Mutex::new(None));
        let r = result.clone();
        std::thread::Builder::new()
            .name("sound-download".into())
            .spawn(move || {
                let res = crate::download::fetch_file(&url, &file, &AtomicBool::new(false), &mut |_| {});
                *r.lock() = Some(res);
            })
            .map_err(|_| "The download could not start".to_string())?;
        self.sound.fetch = Some(Fetch { id: &sound.id, action, result });
        self.info(format!("'{}'을(를) 내려받는 중입니다 (처음 한 번만)", sound.title()));
        Ok(())
    }

    fn sound_use(&mut self, sound: &'static Sound, action: SoundAction) -> Result<(), String> {
        let file = sound.cache_file(&self.dirs.data);
        match action {
            SoundAction::Preview => {
                self.sound_stop();
                let stop = Arc::new(AtomicBool::new(false));
                let done = Arc::new(AtomicBool::new(false));
                let (s2, d2) = (stop.clone(), done.clone());
                let cache = self.dirs.cache.join("SoundPreview").join(format!("{}.{}", sound.src, sound.file_name()));
                if let Some(dir) = cache.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::thread::Builder::new()
                    .name("sound-preview".into())
                    .spawn(move || {
                        if let Err(e) = play_file(&file, &cache, &s2) {
                            log::warn!("sound preview: {e}");
                        }
                        d2.store(true, Ordering::Relaxed);
                    })
                    .map_err(|_| "The preview could not start".to_string())?;
                self.sound.preview = Some(Preview { id: &sound.id, stop, done });
                Ok(())
            }
            SoundAction::Add => {
                self.add_sound_file(&file, sound.title())?;
                self.info(format!("'{}'을(를) 타임라인에 넣었습니다", sound.title()));
                Ok(())
            }
        }
    }

    /// Stops the sound being previewed.
    pub fn sound_stop(&mut self) {
        if let Some(p) = self.sound.preview.take() {
            p.stop.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn poll_sound(&mut self) {
        let finished = self.sound.fetch.as_ref().and_then(|f| f.result.lock().take());
        let Some(res) = finished else { return };
        let Some(fetch) = self.sound.fetch.take() else { return };
        let Some(sound) = find(fetch.id) else { return };
        match res {
            Ok(()) => {
                if let Err(e) = self.sound_use(sound, fetch.action) {
                    self.error(e);
                }
            }
            Err(e) => self.error(format!("Could not download '{}': {e}", sound.title())),
        }
    }

    /// Imports an audio file and lays it on the first audio track that is free at the playhead
    /// (a new track when all are taken). One undo step for the import and one for the placing.
    pub fn add_sound_file(&mut self, file: &Path, name: &str) -> Result<(), String> {
        let sid = self.active.ok_or("Open a sequence first")?;
        let at = self.playhead();
        let probed = op_media::probe(file).map_err(|e| format!("{}: {e}", file.display()))?;
        if probed.audio.is_empty() {
            return Err(format!("{} has no sound", file.display()));
        }
        let bin = self.bin;
        let (_, item) = self
            .edit("Import", |p| Ok(p.add_asset(bin, probed)))
            .ok_or("The sound could not be imported")?;
        // a readable name in the project panel
        let label = name.to_string();
        self.edit("Rename", |p| {
            if let Some(it) = p.item_mut(item) {
                it.name = label;
            }
            Ok(())
        });
        let spec = SourceClip::from_item(&self.project, item).map_err(|e| e.to_string())?;
        let range = SeqRange::with_duration(at, spec.range.end - spec.range.start);
        let opts = self.opts();
        self.edit("Add Sound", move |p| {
            let s = p.sequence(sid).ok_or(EditError::NotFound("sequence".into()))?;
            let free = s
                .audio
                .iter()
                .position(|t| !t.locked && t.clips_in(range).next().is_none());
            let index = match free {
                Some(i) => i,
                None => {
                    let tid = p.ids.track();
                    let s = p.sequence_mut(sid).ok_or(EditError::NotFound("sequence".into()))?;
                    s.audio.push(Arc::new(Track::new(tid, TrackKind::Audio)));
                    s.audio.len() - 1
                }
            };
            let patch = Patch {
                video: None,
                audio: (0..spec.audio.len()).map(|i| Some(index + i)).collect(),
            };
            op_timeline::overwrite(p, sid, &spec, at, &patch, opts)
        })
        .ok_or("The sound could not be placed on the timeline")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_sound() {
        let c = catalog();
        assert!(c.sounds.len() > 500);
        let mut seen = std::collections::HashSet::new();
        for s in &c.sounds {
            assert!(seen.insert(&s.id), "duplicate id {}", s.id);
            assert!(c.sources.contains_key(&s.src), "{}: unknown source", s.id);
            assert_eq!(c.sources[&s.src].license, "CC0-1.0", "{}: only public-domain sounds", s.id);
            assert!(matches!(s.kind.as_str(), "sfx" | "bgm"), "{}", s.id);
            assert!(s.sec > 0.0, "{}: no duration", s.id);
            assert!(s.url().starts_with("https://raw.githubusercontent.com/"), "{}", s.id);
            assert!(s.url().contains(&c.sources[&s.src].commit), "{}: not pinned to a commit", s.id);
        }
        assert!(find("reza/air_horn_meme").is_some());
        assert!(find("reza/air_horn_meme").unwrap().matches("에어혼"));
    }
}
