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
    /// "이미지" or "영상".
    pub kind: &'static str,
    /// Video memory the model wants, in GB.
    pub vram_gb: u32,
    /// What its license says that matters here (Korean).
    pub license: &'static str,
    /// The license has limits the user must read before using the results.
    pub license_warning: bool,
    pub files: &'static [PackFile],
}

pub const PACKS: &[Pack] = &[
    Pack {
        id: "z-image-turbo",
        label: "Z-Image Turbo",
        note: "Image, very fast (8 steps)",
        kind: "이미지",
        vram_gb: 8,
        license: "Apache-2.0: 상업 이용 가능",
        license_warning: false,
        files: &[
            PackFile {
                dir: "diffusion_models",
                name: "z_image_turbo_int8_convrot.safetensors",
                url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/diffusion_models/z_image_turbo_int8_convrot.safetensors",
                megabytes: 6201,
            },
            PackFile {
                dir: "text_encoders",
                name: "qwen_3_4b_fp8_mixed.safetensors",
                url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/text_encoders/qwen_3_4b_fp8_mixed.safetensors",
                megabytes: 5632,
            },
            PackFile {
                dir: "vae",
                name: "ae.safetensors",
                url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/vae/ae.safetensors",
                megabytes: 335,
            },
        ],
    },
    Pack {
        id: "minimax-h3",
        label: "MiniMax H3 (영상+소리)",
        note: "Video with sound, a few seconds",
        kind: "영상",
        vram_gb: 24,
        license: "라이선스가 한국을 사용 제외 지역으로 두고 있다는 조사 결과가 있습니다. 쓰기 전에 모델 카드를 직접 확인하세요",
        license_warning: true,
        files: &[
            PackFile {
                dir: "diffusion_models",
                name: "minimax_h3_fl2va_pruned_int8_convrot.safetensors",
                url: "https://huggingface.co/Comfy-Org/MiniMax-H3/resolve/main/diffusion_models/minimax_h3_fl2va_pruned_int8_convrot.safetensors",
                megabytes: 20970,
            },
            PackFile {
                dir: "text_encoders",
                name: "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors",
                url: "https://huggingface.co/Comfy-Org/MiniMax-H3/resolve/main/text_encoders/qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors",
                megabytes: 15687,
            },
            PackFile {
                dir: "vae",
                name: "minimax_h3_video_vae_fp16.safetensors",
                url: "https://huggingface.co/Comfy-Org/MiniMax-H3/resolve/main/vae/minimax_h3_video_vae_fp16.safetensors",
                megabytes: 5208,
            },
            PackFile {
                dir: "vae",
                name: "minimax_h3_audio_vae_fp32.safetensors",
                url: "https://huggingface.co/Comfy-Org/MiniMax-H3/resolve/main/vae/minimax_h3_audio_vae_fp32.safetensors",
                megabytes: 605,
            },
            PackFile {
                dir: "loras",
                name: "minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16.safetensors",
                url: "https://huggingface.co/lightx2v/Minimax-h3-Turbo/resolve/main/minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16.safetensors",
                megabytes: 1956,
            },
        ],
    },
    Pack {
    id: "krea2-turbo",
    label: "Krea 2 Turbo",
    note: "Image, fast (8 steps)",
    kind: "이미지",
    vram_gb: 8,
    license: "상업 이용에 조건이 있습니다(연 매출 규모 제한). 쓰기 전에 모델 카드를 확인하세요",
    license_warning: true,
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
    },
];

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
