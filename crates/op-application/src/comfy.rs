//! ComfyUI: image and video generation (Z-Image Turbo, MiniMax H3) in the user's own ComfyUI.
//!
//! This program starts and stops ComfyUI (the Windows portable build), sends it a workflow over
//! its local HTTP interface (127.0.0.1 only), waits for the result and imports the file into the
//! project. The models stay in ComfyUI's folders; nothing is downloaded here.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use op_core::*;
use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::editor::Editor;

/// The port of the ComfyUI this program talks to and starts.
static PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(8288);

pub fn port() -> u16 {
    PORT.load(Ordering::Relaxed)
}

pub fn set_port(p: u16) {
    PORT.store(if p < 1024 { 8288 } else { p }, Ordering::Relaxed);
}
const Z_IMAGE: &str = include_str!("../data/comfy/z_image.json");
const MINIMAX_T2V: &str = include_str!("../data/comfy/minimax_h3_t2v.json");
const KREA2: &str = include_str!("../data/comfy/krea2_turbo.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Z-Image Turbo, text to image.
    Image,
    /// Krea 2 Turbo, text to image.
    Krea2,
    /// MiniMax H3, text to video (with sound).
    Video,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub kind: Kind,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    /// Video length.
    pub seconds: f64,
    /// Video: 9:16 instead of 16:9.
    pub vertical: bool,
    pub seed: u64,
}

pub fn url(path: &str) -> String {
    format!("http://127.0.0.1:{}{path}", port())
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

fn get_json(path: &str, timeout: Duration) -> Option<Json> {
    let mut resp = agent(timeout).get(&url(path)).call().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    serde_json::from_str(&resp.body_mut().read_to_string().ok()?).ok()
}

/// ComfyUI's own report of itself when it answers (None: not running).
pub fn probe() -> Option<Json> {
    get_json("/system_stats", Duration::from_millis(1500))
}

/// The ComfyUI folder: the preference, else the usual places (the portable build has
/// `python_embeded` and `ComfyUI\main.py`).
pub fn find_dir(preferred: &str) -> Option<PathBuf> {
    let ok = |p: &Path| p.join("python_embeded").join("python.exe").is_file() && p.join("ComfyUI").join("main.py").is_file();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if !preferred.trim().is_empty() {
        candidates.push(PathBuf::from(preferred.trim()));
    }
    candidates.extend(["C:\\ai\\ComfyUI", "D:\\ComfyUI", "C:\\ComfyUI"].map(PathBuf::from));
    if let Ok(exe) = std::env::current_exe()
        && let Some(root) = exe.parent().and_then(Path::parent)
    {
        candidates.push(root.join("comfyui"));
    }
    candidates.into_iter().find(|p| ok(p))
}

/// Where generated files go: `generated` beside the program's folder, else the data folder.
pub fn generated_dir(data: &Path) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("generated")))
        .filter(|d| d.parent().is_some_and(|p| p.join("app").is_dir()))
        .unwrap_or_else(|| data.join("generated"))
}

/// The model files each generator needs: label and path under ComfyUI's `models` folder.
pub fn required_models(kind: Kind) -> &'static [(&'static str, &'static str)] {
    match kind {
        Kind::Krea2 => &[
            ("Krea 2 Turbo", "diffusion_models/krea2_turbo_int8_convrot.safetensors"),
            ("Qwen3-VL 4B", "text_encoders/qwen3vl_4b_fp8_scaled.safetensors"),
            ("VAE", "vae/qwen_image_vae.safetensors"),
        ],
        Kind::Image => &[
            ("Z-Image Turbo", "diffusion_models/z_image_turbo_int8_convrot.safetensors"),
            ("Qwen 3 4B", "text_encoders/qwen_3_4b_fp8_mixed.safetensors"),
            ("VAE", "vae/ae.safetensors"),
        ],
        Kind::Video => &[
            ("MiniMax H3", "diffusion_models/minimax_h3_fl2va_pruned_int8_convrot.safetensors"),
            ("Qwen3-VL 32B", "text_encoders/qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"),
            ("Video VAE", "vae/minimax_h3_video_vae_fp16.safetensors"),
            ("Audio VAE", "vae/minimax_h3_audio_vae_fp32.safetensors"),
            ("Turbo LoRA", "loras/minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16.safetensors"),
        ],
    }
}

/// Starts ComfyUI hidden, listening on this computer only.
pub fn start_process(dir: &Path, log: &Path) -> Result<Child, String> {
    let python = dir.join("python_embeded").join("python.exe");
    if !python.is_file() || !dir.join("ComfyUI").join("main.py").is_file() {
        return Err("This is not a ComfyUI folder (python_embeded and ComfyUI\\main.py are expected)".into());
    }
    let log_file = std::fs::File::create(log).ok();
    let mut cmd = Command::new(python);
    cmd.current_dir(dir)
        .args(["-s", "ComfyUI\\main.py", "--windows-standalone-build", "--listen", "127.0.0.1", "--port"])
        .arg(port().to_string())
        .stdin(Stdio::null());
    match log_file.and_then(|f| f.try_clone().ok().map(|g| (f, g))) {
        Some((out, err)) => cmd.stdout(out).stderr(err),
        None => cmd.stdout(Stdio::null()).stderr(Stdio::null()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.spawn().map_err(|e| format!("ComfyUI could not be started: {e}"))
}

fn percent(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The workflow for a request, with the user's words, size, length and seed put in.
fn workflow(req: &Request) -> Result<Json, String> {
    let mut wf: Json = serde_json::from_str(match req.kind {
        Kind::Image => Z_IMAGE,
        Kind::Krea2 => KREA2,
        Kind::Video => MINIMAX_T2V,
    })
    .map_err(|e| format!("the workflow is damaged: {e}"))?;
    let seed = req.seed % (1u64 << 53); // JSON numbers stay exact up to 2^53
    let set = |wf: &mut Json, node: &str, input: &str, v: Json| -> Result<(), String> {
        wf[node]["inputs"][input] = v;
        if wf[node]["inputs"].get(input).is_none() {
            return Err(format!("the workflow has no node {node}"));
        }
        Ok(())
    };
    match req.kind {
        Kind::Image => {
            let round = |v: u32| (v.clamp(256, 2048) / 16) * 16; // the model works in steps of 16
            set(&mut wf, "5", "text", json!(req.prompt))?;
            set(&mut wf, "7", "width", json!(round(req.width)))?;
            set(&mut wf, "7", "height", json!(round(req.height)))?;
            set(&mut wf, "8", "seed", json!(seed))?;
        }
        Kind::Krea2 => {
            let round = |v: u32| (v.clamp(256, 2048) / 16) * 16;
            set(&mut wf, "5", "text", json!(req.prompt))?;
            set(&mut wf, "7", "width", json!(round(req.width)))?;
            set(&mut wf, "7", "height", json!(round(req.height)))?;
            set(&mut wf, "8", "seed", json!(seed))?;
        }
        Kind::Video => {
            set(&mut wf, "140:131", "prompt", json!(req.prompt))?;
            set(&mut wf, "140:129", "noise_seed", json!(seed))?;
            set(&mut wf, "133", "value", json!(req.seconds.clamp(2.0, 15.0)))?;
            let ratio = if req.vertical { "9:16 (Portrait Widescreen)" } else { "16:9 (Widescreen)" };
            set(&mut wf, "115", "aspect_ratio", json!(ratio))?;
        }
    }
    Ok(wf)
}

/// What a running generation shows.
#[derive(Clone, Debug)]
pub struct Progress {
    pub note: String,
    pub started: Instant,
}

pub struct Job {
    pub kind: Kind,
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<PathBuf, String>>>>,
}

impl Job {
    pub fn progress(&self) -> Progress {
        self.progress.lock().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn post_json(path: &str, body: &Json) -> Result<Json, String> {
    let mut resp = agent(Duration::from_secs(30))
        .post(&url(path))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| format!("ComfyUI did not answer: {e}"))?;
    let status = resp.status();
    let text = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    if !status.is_success() {
        // a ComfyUI that lacks the model files names them in its answer
        if text.contains("value_not_in_list") {
            return Err(format!(
                "This ComfyUI (port {}) does not have the model files the workflow needs. Start the ComfyUI folder that holds them with the Start button, or choose another port. Details: {}",
                port(),
                text.chars().take(300).collect::<String>()
            ));
        }
        return Err(format!("ComfyUI refused the workflow ({status}): {}", text.chars().take(600).collect::<String>()));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Sends the workflow, waits for it and saves the file ComfyUI made into `out_dir`.
fn run(req: &Request, out_dir: &Path, progress: &Mutex<Progress>, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let say = |s: &str| progress.lock().note = s.to_string();
    let wf = workflow(req)?;
    say("Queued in ComfyUI");
    let client = format!(
        "we-edit-{:x}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) ^ u128::from(std::process::id())
    );
    let answer = post_json("/prompt", &json!({"prompt": wf, "client_id": client}))?;
    let id = answer["prompt_id"].as_str().ok_or("ComfyUI sent no job id")?.to_string();
    say("Generating");
    let started = Instant::now();
    let item = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = post_json("/interrupt", &json!({}));
            let _ = post_json("/queue", &json!({"delete": [id]}));
            return Err("cancelled".into());
        }
        if started.elapsed() > Duration::from_secs(3 * 3600) {
            return Err("ComfyUI took longer than three hours".into());
        }
        std::thread::sleep(Duration::from_millis(1200));
        let Some(history) = get_json(&format!("/history/{id}"), Duration::from_secs(10)) else {
            if probe().is_none() {
                return Err("ComfyUI stopped answering".into());
            }
            continue;
        };
        let Some(entry) = history.get(&id) else { continue };
        if entry["status"]["status_str"] == "error" {
            let why = entry["status"]["messages"].to_string();
            return Err(format!("ComfyUI reported an error: {}", why.chars().take(500).collect::<String>()));
        }
        // the first output file of any node
        let found = entry["outputs"].as_object().into_iter().flat_map(|o| o.values()).find_map(|node| {
            node.as_object()?
                .values()
                .filter_map(Json::as_array)
                .flatten()
                .find(|f| f["filename"].is_string() && f["type"] == "output")
                .cloned()
        });
        if let Some(f) = found {
            break f;
        }
        if entry["status"]["completed"] == true {
            return Err("ComfyUI finished without a file".into());
        }
    };
    say("Fetching the file");
    let name = item["filename"].as_str().unwrap_or("result").to_string();
    let query = format!(
        "/view?filename={}&subfolder={}&type=output",
        percent(&name),
        percent(item["subfolder"].as_str().unwrap_or(""))
    );
    let resp = ureq::get(&url(&query)).call().map_err(|e| format!("the file could not be fetched: {e}"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = out_dir.join(format!("{stamp}-{name}"));
    let mut file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::copy(&mut resp.into_body().into_reader(), &mut file).map_err(|e| e.to_string())?;
    Ok(path)
}

/// ComfyUI and the generations of this program.
#[derive(Default)]
pub struct State {
    /// ComfyUI as started by this program (not one that was already running).
    pub child: Option<Child>,
    pub job: Option<Job>,
    /// The latest finished generation and its media in the project (once imported).
    pub last: Option<(PathBuf, Option<ItemId>)>,
    pub last_error: Option<String>,
    importing: Option<PathBuf>,
    /// Model files being downloaded into ComfyUI.
    pub install: Option<crate::comfy_packs::Install>,
}

impl Editor {
    /// Starts ComfyUI (from `dir`, or the folder found) unless it already answers.
    pub fn comfy_start(&mut self, dir: &str) -> Result<(), String> {
        if probe().is_some() {
            return Ok(());
        }
        if self.comfy.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None))) {
            return Ok(()); // started and still loading
        }
        let dir = find_dir(dir).ok_or("ComfyUI was not found: choose its folder (the one with python_embeded)")?;
        let _ = std::fs::create_dir_all(&self.dirs.logs());
        let child = start_process(&dir, &self.dirs.logs().join("comfyui.log"))?;
        self.comfy.child = Some(child);
        Ok(())
    }

    /// Stops the ComfyUI this program started.
    pub fn comfy_stop(&mut self) -> Result<(), String> {
        match self.comfy.child.take() {
            Some(mut c) => {
                let _ = c.kill();
                let _ = c.wait();
                Ok(())
            }
            None => Err("ComfyUI was not started by this program: close it where it runs".into()),
        }
    }

    pub fn comfy_generate(&mut self, req: Request) -> Result<(), String> {
        if self.comfy.job.is_some() {
            return Err("A generation is already running".into());
        }
        if req.prompt.trim().is_empty() {
            return Err("Write what to generate first".into());
        }
        if probe().is_none() {
            return Err("ComfyUI is not running: start it first".into());
        }
        let out_dir = generated_dir(&self.dirs.data);
        let progress = Arc::new(Mutex::new(Progress { note: "Starting".into(), started: Instant::now() }));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (p, c, r, kind) = (progress.clone(), cancel.clone(), result.clone(), req.kind);
        std::thread::Builder::new()
            .name("comfy".into())
            .spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&req, &out_dir, &p, &c)))
                    .unwrap_or_else(|_| Err("the generation stopped unexpectedly".into()));
                *r.lock() = Some(res);
            })
            .map_err(|_| "The generation could not start".to_string())?;
        self.comfy.last_error = None;
        self.comfy.job = Some(Job { kind, progress, cancel, result });
        Ok(())
    }

    /// Takes a finished generation, imports its file and finds it in the project.
    pub(crate) fn poll_comfy(&mut self) {
        set_port(self.prefs.comfy_port);
        self.poll_comfy_install();
        let finished = self.comfy.job.as_ref().and_then(|j| j.result.lock().take());
        if let Some(res) = finished {
            self.comfy.job = None;
            match res {
                Ok(path) => {
                    let root = self.project.root;
                    self.import(vec![path.clone()], root);
                    self.comfy.importing = Some(path.clone());
                    self.comfy.last = Some((path, None));
                }
                Err(e) if e == "cancelled" => {}
                Err(e) => {
                    self.comfy.last_error = Some(e.clone());
                    self.error(format!("Generation failed: {e}"));
                }
            }
        }
        if let Some(path) = self.comfy.importing.clone() {
            let found = self.project.items.iter().find_map(|(id, it)| match &it.kind {
                ItemKind::Media { asset, .. } => self
                    .project
                    .asset(*asset)
                    .filter(|a| Path::new(&a.path) == path.as_path())
                    .map(|_| *id),
                _ => None,
            });
            if let Some(id) = found {
                self.comfy.last = Some((path, Some(id)));
                self.comfy.importing = None;
            }
        }
    }
}
