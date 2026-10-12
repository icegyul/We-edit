//! Model packs for ComfyUI: the files each generator needs, which of them are missing in the
//! user's ComfyUI, and the download of the missing ones straight into ComfyUI's model folders.
//! The download addresses are the ones ComfyUI's own workflow templates point to.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::editor::Editor;

pub struct PackFile {
    /// Folder under ComfyUI's `models`.
    pub dir: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub megabytes: u32,
}

pub struct Pack {
    pub id: &'static str,
    pub label: &'static str,
    /// What it makes (English source text of the interface).
    pub note: &'static str,
    pub files: &'static [PackFile],
}

pub const PACKS: &[Pack] = &[Pack {
    id: "krea2-turbo",
    label: "Krea 2 Turbo",
    note: "Image, fast (8 steps)",
    files: &[
        PackFile {
            dir: "diffusion_models",
            name: "krea2_turbo_int8_convrot.safetensors",
            url: "https://huggingface.co/Comfy-Org/Krea-2/resolve/main/diffusion_models/krea2_turbo_int8_convrot.safetensors",
            megabytes: 13000,
        },
        PackFile {
            dir: "text_encoders",
            name: "qwen3vl_4b_fp8_scaled.safetensors",
            url: "https://huggingface.co/Comfy-Org/Krea-2/resolve/main/text_encoders/qwen3vl_4b_fp8_scaled.safetensors",
            megabytes: 4900,
        },
        PackFile {
            dir: "vae",
            name: "qwen_image_vae.safetensors",
            url: "https://huggingface.co/Comfy-Org/Krea-2/resolve/main/vae/qwen_image_vae.safetensors",
            megabytes: 254,
        },
    ],
}];

pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

fn models_dir(comfy: &Path) -> PathBuf {
    comfy.join("ComfyUI").join("models")
}

/// The files of `pack` that ComfyUI does not have yet.
pub fn missing(pack: &Pack, comfy: &Path) -> Vec<&'static PackFile> {
    pack.files
        .iter()
        .filter(|f| !models_dir(comfy).join(f.dir).join(f.name).is_file())
        .collect()
}

pub struct Install {
    pub id: &'static str,
    /// Bytes received and bytes to receive.
    progress: Arc<Mutex<(u64, u64)>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}

impl Install {
    pub fn progress(&self) -> (u64, u64) {
        *self.progress.lock()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Editor {
    /// Downloads the missing files of a pack into the ComfyUI folder (`dir` or the folder found).
    pub fn comfy_install(&mut self, id: &str, dir: &str) -> Result<(), String> {
        let pack = pack(id).ok_or("unknown model pack")?;
        if self.comfy.install.is_some() {
            return Err("A model is already being installed".into());
        }
        let comfy = crate::comfy::find_dir(dir)
            .ok_or("ComfyUI was not found: choose its folder (the one with python_embeded)")?;
        let todo = missing(pack, &comfy);
        if todo.is_empty() {
            return Ok(());
        }
        let total = todo.iter().map(|f| u64::from(f.megabytes) * 1_000_000).sum();
        let progress = Arc::new(Mutex::new((0u64, total)));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r) = (progress.clone(), cancel.clone(), result.clone());
        std::thread::Builder::new()
            .name("comfy-install".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    for f in todo {
                        let dest = models_dir(&comfy).join(f.dir).join(f.name);
                        crate::download::fetch_file(f.url, &dest, &c, &mut |n| p.lock().0 += n)?;
                    }
                    Ok(())
                }))
                .unwrap_or_else(|_| Err("the download stopped unexpectedly".into()));
                *r.lock() = Some(res);
            })
            .map_err(|_| "The download could not start".to_string())?;
        self.comfy.install = Some(Install { id: pack.id, progress, cancel, result });
        Ok(())
    }

    pub(crate) fn poll_comfy_install(&mut self) {
        let finished = self.comfy.install.as_ref().and_then(|i| i.result.lock().take());
        if let Some(res) = finished {
            let id = self.comfy.install.take().map(|i| i.id).unwrap_or("");
            match res {
                Ok(()) => self.info(format!("{id} is installed")),
                Err(e) if e == "cancelled" => {}
                Err(e) => self.error(format!("Install failed: {e}")),
            }
        }
    }
}
