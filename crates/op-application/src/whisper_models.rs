//! Whisper speech models (faster-whisper / CTranslate2 format) for the sentence editor and the
//! automatic captions: the list to choose from, which are installed, and the download from
//! Hugging Face into the `helper\models` folder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde_json::Value as Json;

use crate::editor::Editor;

pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    /// Hugging Face repository.
    pub repo: &'static str,
    pub megabytes: u32,
    /// What it is good for (English source text of the interface).
    pub note: &'static str,
}

/// Every model offered, smallest first.
pub const MODELS: &[ModelInfo] = &[
    ModelInfo { id: "tiny", label: "tiny", repo: "Systran/faster-whisper-tiny", megabytes: 80, note: "Fastest, least accurate" },
    ModelInfo { id: "base", label: "base", repo: "Systran/faster-whisper-base", megabytes: 150, note: "Fast, for clear speech" },
    ModelInfo { id: "small", label: "small", repo: "Systran/faster-whisper-small", megabytes: 490, note: "Balanced" },
    ModelInfo { id: "medium", label: "medium", repo: "Systran/faster-whisper-medium", megabytes: 1530, note: "Accurate" },
    ModelInfo { id: "large-v3-turbo", label: "large-v3-turbo", repo: "mobiuslabsgmbh/faster-whisper-large-v3-turbo", megabytes: 1620, note: "Fast and accurate (recommended)" },
    ModelInfo { id: "large-v2", label: "large-v2", repo: "Systran/faster-whisper-large-v2", megabytes: 3090, note: "Very accurate" },
    ModelInfo { id: "large-v3", label: "large-v3", repo: "Systran/faster-whisper-large-v3", megabytes: 3090, note: "Most accurate" },
    ModelInfo { id: "korean-turbo", label: "korean-turbo", repo: "imTak/faster-whisper_Korean_L3turbo", megabytes: 820, note: "Tuned for Korean (community model)" },
];

pub fn info(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

/// The folder the models live in: `helper\models`.
pub fn models_root() -> Option<PathBuf> {
    crate::sentences::helper_dir().map(|d| d.join("models"))
}

/// Where one model is stored (the large-v3 that was fetched first keeps its folder name).
pub fn dir_of(id: &str) -> Option<PathBuf> {
    Some(models_root()?.join(format!("faster-whisper-{id}")))
}

/// A model is usable when its weights and settings are all there.
pub fn installed(id: &str) -> bool {
    dir_of(id).is_some_and(|d| d.join("model.bin").is_file() && d.join("config.json").is_file())
}

/// The model the preferences choose, or the best installed one when that is missing.
pub fn chosen_dir(preferred: &str) -> Option<PathBuf> {
    let pick = |id: &str| installed(id).then(|| dir_of(id)).flatten();
    pick(preferred).or_else(|| MODELS.iter().rev().find_map(|m| pick(m.id)))
}

pub struct Download {
    pub id: &'static str,
    /// Bytes received and bytes in all.
    progress: Arc<Mutex<(u64, u64)>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}

impl Download {
    pub fn progress(&self) -> (u64, u64) {
        *self.progress.lock()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn get_json(url: &str) -> Result<Json, String> {
    let text = ureq::get(url)
        .header("User-Agent", "We edit")
        .call()
        .map_err(|e| format!("{url}: {e}"))?
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn fetch(m: &ModelInfo, dest: &Path, progress: &Mutex<(u64, u64)>, cancel: &AtomicBool) -> Result<(), String> {
    let meta = get_json(&format!("https://huggingface.co/api/models/{}?blobs=true", m.repo))?;
    let mut files: Vec<(String, u64)> = meta["siblings"]
        .as_array()
        .ok_or("the model list is unreadable")?
        .iter()
        .filter_map(|s| Some((s["rfilename"].as_str()?.to_string(), s["size"].as_u64().unwrap_or(0))))
        .filter(|(n, _)| !n.starts_with('.') && !n.ends_with(".md"))
        .collect();
    // the weights last: a model with weights is a complete model
    files.sort_by_key(|(n, _)| n == "model.bin");
    progress.lock().1 = files.iter().map(|f| f.1).sum();
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for (name, _) in &files {
        let final_path = dest.join(name);
        if let Some(parent) = final_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let part = dest.join(format!("{name}.part"));
        let resp = ureq::get(&format!("https://huggingface.co/{}/resolve/main/{name}", m.repo))
            .header("User-Agent", "We edit")
            .call()
            .map_err(|e| format!("{name}: {e}"))?;
        let mut out = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut reader = resp.into_body().into_reader();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("{name}: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
            progress.lock().0 += n as u64;
        }
        drop(out);
        std::fs::rename(&part, &final_path).map_err(|e| format!("{}: {e}", final_path.display()))?;
    }
    Ok(())
}

impl Editor {
    pub fn whisper_download(&mut self, id: &str) -> Result<(), String> {
        let m = info(id).ok_or("unknown model")?;
        if self.whisper_dl.is_some() {
            return Err("A model is already downloading".into());
        }
        let dest = dir_of(id).ok_or("The helper folder is missing: models are stored in helper\\models")?;
        let progress = Arc::new(Mutex::new((0u64, u64::from(m.megabytes) * 1_000_000)));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r) = (progress.clone(), cancel.clone(), result.clone());
        std::thread::Builder::new()
            .name("whisper-download".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fetch(m, &dest, &p, &c)))
                    .unwrap_or_else(|_| Err("the download stopped unexpectedly".into()));
                *r.lock() = Some(res);
            })
            .map_err(|_| "The download could not start".to_string())?;
        self.whisper_dl = Some(Download { id: m.id, progress, cancel, result });
        Ok(())
    }

    pub(crate) fn poll_whisper_download(&mut self) {
        let finished = self.whisper_dl.as_ref().and_then(|d| d.result.lock().take());
        if let Some(res) = finished {
            let id = self.whisper_dl.take().map(|d| d.id).unwrap_or("");
            match res {
                Ok(()) => self.info(format!("Model {id} is ready")),
                Err(e) if e == "cancelled" => {}
                Err(e) => self.error(format!("Download failed: {e}")),
            }
        }
    }
}

// ------------------------------------------------------------------------- what is on the disk

/// A speech model folder found on this computer.
pub struct Folder {
    pub path: PathBuf,
    /// Folder name.
    pub name: String,
    pub bytes: u64,
    /// The model of `MODELS` it is, if it is one.
    pub known: Option<&'static ModelInfo>,
}

/// Bytes of the files in `dir` and one level below.
fn folder_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            match e.metadata() {
                Ok(m) if m.is_file() => total += m.len(),
                Ok(m) if m.is_dir() => {
                    if let Ok(inner) = std::fs::read_dir(e.path()) {
                        total += inner.flatten().filter_map(|f| f.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum::<u64>();
                    }
                }
                _ => {}
            }
        }
    }
    total
}

fn list(root: &Path, known: impl Fn(&str) -> Option<&'static ModelInfo>) -> Vec<Folder> {
    let mut out: Vec<Folder> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.metadata().is_ok_and(|m| m.is_dir()))
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            Folder { bytes: folder_bytes(&e.path()), known: known(&name), path: e.path(), name }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every folder in the speech helper's `models` folder.
pub fn folders() -> Vec<Folder> {
    let Some(root) = models_root() else { return Vec::new() };
    list(&root, |name| name.strip_prefix("faster-whisper-").and_then(info))
}

/// The built-in Whisper's models (`SpeechModels` in the program's data folder).
pub fn builtin_folders(models: &Path) -> Vec<Folder> {
    list(models, |_| None)
}

impl Editor {
    /// Deletes a downloaded speech model folder. Only folders directly inside the helper's
    /// `models` folder or the built-in models folder can go, and not while speech is being
    /// recognized or the model is being downloaded.
    pub fn whisper_remove(&mut self, path: &Path) -> Result<(), String> {
        if self.whisper_dl.is_some() || self.sentencing.is_some() || self.silencing.is_some() || self.captioning.is_some() {
            return Err("음성 인식이나 모델 내려받기가 끝난 뒤에 지울 수 있습니다".into());
        }
        let builtin = crate::captions::models_dir(self);
        let inside = |root: Option<PathBuf>| {
            root.and_then(|r| r.canonicalize().ok())
                .zip(path.canonicalize().ok())
                .is_some_and(|(root, p)| p.parent() == Some(root.as_path()) && p.is_dir())
        };
        if !inside(models_root()) && !inside(Some(builtin)) {
            return Err("이 폴더는 모델 폴더가 아니어서 지우지 않습니다".into());
        }
        std::fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.info(format!("'{}' 모델을 지웠습니다", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        Ok(())
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    #[test]
    fn folders_are_listed_with_their_size_and_only_model_folders_go() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("helper");
        let models = helper.join("models");
        std::fs::create_dir_all(models.join("faster-whisper-small")).unwrap();
        std::fs::write(models.join("faster-whisper-small").join("model.bin"), vec![0u8; 1000]).unwrap();
        std::fs::create_dir_all(models.join("whisper-old")).unwrap();
        std::fs::write(models.join("whisper-old").join("a.bin"), vec![0u8; 10]).unwrap();
        // SAFETY: tests of this module are the only ones that touch this variable
        unsafe { std::env::set_var("WEEDIT_HELPER", &helper) };
        let found = folders();
        assert_eq!(found.len(), 2);
        let small = found.iter().find(|f| f.name == "faster-whisper-small").unwrap();
        assert_eq!(small.bytes, 1000);
        assert_eq!(small.known.map(|m| m.id), Some("small"));
        assert!(found.iter().find(|f| f.name == "whisper-old").unwrap().known.is_none());

        let mut ed = Editor::new(crate::Dirs::portable(dir.path()), crate::Preferences::default(), false);
        // a folder outside the models folder is refused
        let other = dir.path().join("elsewhere");
        std::fs::create_dir_all(&other).unwrap();
        assert!(ed.whisper_remove(&other).is_err());
        assert!(other.is_dir());
        ed.whisper_remove(&models.join("whisper-old")).unwrap();
        assert!(!models.join("whisper-old").exists());
        assert!(models.join("faster-whisper-small").exists());
        unsafe { std::env::remove_var("WEEDIT_HELPER") };
    }
}
