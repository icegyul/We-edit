//! The ComfyUI panel (Window > ComfyUI): starts and stops ComfyUI and generates images
//! (Z-Image Turbo) and videos (MiniMax H3) straight into the project.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui};
use op_application::comfy::{self, Kind, Request};
use op_application::comfy_packs;
use serde_json::{Value as Json, json};

use crate::app::State;
use crate::theme;

pub struct ComfyView {
    kind: u8,
    prompt: String,
    width: u32,
    height: u32,
    seconds: f32,
    vertical: bool,
    seed: u64,
    random_seed: bool,
    add_to_timeline: bool,
    /// ComfyUI's answer to the latest check (None: not running).
    up: Option<String>,
    checked: Option<Instant>,
    rx: Option<mpsc::Receiver<Option<String>>>,
    /// A finished generation waiting to be put on the timeline.
    place_pending: bool,
}

impl Default for ComfyView {
    fn default() -> Self {
        ComfyView {
            kind: 0,
            prompt: String::new(),
            width: 1920,
            height: 1080,
            seconds: 5.0,
            vertical: false,
            seed: new_seed(),
            random_seed: true,
            add_to_timeline: true,
            up: None,
            checked: None,
            rx: None,
            place_pending: false,
        }
    }
}

fn new_seed() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish() % 1_000_000_000_000
}

/// One line about ComfyUI's report of itself.
fn describe(stats: &Json) -> String {
    let gpu = stats["devices"][0]["name"].as_str().unwrap_or("").split(" : ").next().unwrap_or("");
    let free = stats["devices"][0]["vram_free"].as_f64().unwrap_or(0.0) / 1e9;
    let total = stats["devices"][0]["vram_total"].as_f64().unwrap_or(0.0) / 1e9;
    format!("{gpu} · VRAM {free:.1}/{total:.1} GB")
}

fn mmss(d: Duration) -> String {
    format!("{}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}

pub fn show(s: &mut State, ui: &mut Ui) {
    // ask ComfyUI every few seconds, without waiting for it in the interface thread
    if let Some(rx) = &s.comfy.rx
        && let Ok(answer) = rx.try_recv()
    {
        s.comfy.up = answer;
        s.comfy.rx = None;
    }
    if s.comfy.rx.is_none() && s.comfy.checked.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
        let (tx, rx) = mpsc::channel();
        let ctx = ui.ctx().clone();
        std::thread::spawn(move || {
            let _ = tx.send(comfy::probe().map(|v| describe(&v)));
            ctx.request_repaint();
        });
        s.comfy.rx = Some(rx);
        s.comfy.checked = Some(Instant::now());
    }
    ui.ctx().request_repaint_after(Duration::from_secs(1));
    place_when_ready(s);
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| body(s, ui)));
}

/// A generation that finished and was imported goes on the timeline when it was asked for.
fn place_when_ready(s: &mut State) {
    if !s.comfy.place_pending {
        return;
    }
    if let Some((_, Some(item))) = s.ed.comfy.last.clone() {
        s.comfy.place_pending = false;
        if s.ed.active.is_some()
            && let Err(e) = op_mcp::tools::call(&mut s.ed, "add_clip", &json!({"media_id": item.0}))
        {
            s.ed.error(e);
        }
    } else if s.ed.comfy.job.is_none() && s.ed.comfy.last_error.is_some() {
        s.comfy.place_pending = false;
    }
}

fn body(s: &mut State, ui: &mut Ui) {
    let green = Color32::from_rgb(0x2e, 0x9d, 0x4f);
    let running = s.comfy.up.is_some();
    let loading = !running && s.ed.comfy.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    ui.horizontal_wrapped(|ui| {
        match (&s.comfy.up, loading) {
            (Some(info), _) => ui.label(RichText::new(format!("● ComfyUI 실행 중 — {info}")).color(green)),
            (None, true) => ui.label(RichText::new("◐ ComfyUI 시작하는 중... (모델 준비에 1~2분)").color(theme::WARN)),
            (None, false) => ui.label(RichText::new("○ ComfyUI 꺼져 있음").color(theme::TEXT_DIM)),
        };
    });
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!running && !loading, egui::Button::new("시작")).clicked() {
            let dir = s.ed.prefs.comfy_dir.clone();
            if let Err(e) = s.ed.comfy_start(&dir) {
                s.ed.error(e);
            }
        }
        if ui.add_enabled(s.ed.comfy.child.is_some(), egui::Button::new("중지")).clicked()
            && let Err(e) = s.ed.comfy_stop()
        {
            s.ed.error(e);
        }
        if ui.add_enabled(running, egui::Button::new("웹 화면 열기")).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(comfy::url("/")));
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("포트").color(theme::TEXT_DIM));
        let mut port = s.ed.prefs.comfy_port;
        if ui.add(egui::DragValue::new(&mut port).range(1024..=65535)).changed() {
            s.ed.prefs.comfy_port = port;
            comfy::set_port(port);
            s.comfy.checked = None;
            let _ = s.ed.prefs.save(&s.ed.dirs);
        }
        ui.label(RichText::new("(8188은 다른 프로그램이 쓸 수 있어 8288을 기본으로 합니다)").size(11.0).color(theme::TEXT_DIM));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("폴더").color(theme::TEXT_DIM));
        let auto = comfy::find_dir(&s.ed.prefs.comfy_dir).map(|p| p.display().to_string()).unwrap_or_else(|| "(찾지 못함)".into());
        let mut text = s.ed.prefs.comfy_dir.clone();
        if ui
            .add(egui::TextEdit::singleline(&mut text).hint_text(format!("자동: {auto}")).desired_width(240.0))
            .changed()
        {
            s.ed.prefs.comfy_dir = text;
            let _ = s.ed.prefs.save(&s.ed.dirs);
        }
        if ui.button("찾기...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            s.ed.prefs.comfy_dir = dir.display().to_string();
            let _ = s.ed.prefs.save(&s.ed.dirs);
        }
    });
    let dir = comfy::find_dir(&s.ed.prefs.comfy_dir);
    egui::CollapsingHeader::new("필요한 모델 파일").default_open(false).show(ui, |ui| {
        for (title, kind) in [
            ("Z-Image Turbo (이미지)", Kind::Image),
            ("Krea 2 Turbo (이미지)", Kind::Krea2),
            ("MiniMax H3 (영상)", Kind::Video),
        ] {
            ui.label(RichText::new(title).strong());
            for (label, rel) in comfy::required_models(kind) {
                let present = dir.as_ref().is_some_and(|d| d.join("ComfyUI").join("models").join(rel).is_file());
                let (mark, color) = if present { ("✓", green) } else { ("✗", theme::WARN) };
                ui.label(RichText::new(format!("  {mark} {label}")).color(color));
            }
        }
        // Krea 2 can be installed from here (its files come from Hugging Face)
        if let Some(pack) = comfy_packs::pack("krea2-turbo") {
            let todo = dir.as_ref().map(|d| comfy_packs::missing(pack, d)).unwrap_or_default();
            if let Some(job) = &s.ed.comfy.install {
                let (done, total) = job.progress();
                let f = (done as f32 / total.max(1) as f32).min(1.0);
                ui.horizontal(|ui| {
                    ui.add(egui::ProgressBar::new(f).desired_width(160.0).text(format!("{} {:.0}%", pack.label, f * 100.0)));
                    if ui.small_button("취소").clicked() {
                        job.cancel();
                    }
                });
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            } else if !todo.is_empty() && dir.is_some() {
                let gb: f32 = todo.iter().map(|f| f.megabytes as f32).sum::<f32>() / 1000.0;
                if ui.button(format!("Krea 2 Turbo 설치 ({gb:.1} GB)")).clicked() {
                    let folder = s.ed.prefs.comfy_dir.clone();
                    if let Err(e) = s.ed.comfy_install(pack.id, &folder) {
                        s.ed.error(e);
                    }
                }
            }
        }
    });
    ui.separator();

    // what to make
    ui.horizontal(|ui| {
        ui.radio_value(&mut s.comfy.kind, 0, "이미지 (Z-Image Turbo)");
        ui.radio_value(&mut s.comfy.kind, 1, "이미지 (Krea 2 Turbo)");
        ui.radio_value(&mut s.comfy.kind, 2, "영상 (MiniMax H3)");
    });
    ui.add(
        egui::TextEdit::multiline(&mut s.comfy.prompt)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .hint_text("만들 장면을 설명하세요 (영어가 더 정확합니다)"),
    );
    if s.comfy.kind < 2 {
        ui.horizontal(|ui| {
            ui.label("크기");
            for (label, w, h) in [("16:9", 1920, 1080), ("9:16", 1080, 1920), ("1:1", 1024, 1024)] {
                if ui.selectable_label((s.comfy.width, s.comfy.height) == (w, h), label).clicked() {
                    s.comfy.width = w;
                    s.comfy.height = h;
                }
            }
            ui.add(egui::DragValue::new(&mut s.comfy.width).range(256..=2048).suffix(" px"));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut s.comfy.height).range(256..=2048).suffix(" px"));
        });
    } else {
        ui.horizontal(|ui| {
            ui.label("길이");
            ui.add(egui::Slider::new(&mut s.comfy.seconds, 2.0..=10.0).suffix(" 초").step_by(1.0));
            ui.checkbox(&mut s.comfy.vertical, "세로 (9:16)");
        });
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut s.comfy.random_seed, "시드 무작위");
        ui.add_enabled(!s.comfy.random_seed, egui::DragValue::new(&mut s.comfy.seed).speed(1.0));
        ui.checkbox(&mut s.comfy.add_to_timeline, "완료되면 타임라인에 넣기");
    });

    // run
    if let Some(job) = &s.ed.comfy.job {
        let p = job.progress();
        ui.horizontal(|ui| {
            ui.spinner();
            let note = match p.note.as_str() {
                "Generating" => "생성 중",
                "Queued in ComfyUI" => "ComfyUI 대기열에 넣는 중",
                "Fetching the file" => "파일을 가져오는 중",
                _ => "준비 중",
            };
            ui.label(format!("{note} · {}", mmss(p.started.elapsed())));
            if ui.button("취소").clicked() {
                job.cancel();
            }
        });
        ui.label(RichText::new("영상은 컴퓨터 사양에 따라 수 분이 걸립니다.").size(11.0).color(theme::TEXT_DIM));
    } else {
        let can = running && !s.comfy.prompt.trim().is_empty();
        if ui.add_enabled(can, egui::Button::new("생성")).clicked() {
            if s.comfy.random_seed {
                s.comfy.seed = new_seed();
            }
            let req = Request {
                kind: match s.comfy.kind {
                    0 => Kind::Image,
                    1 => Kind::Krea2,
                    _ => Kind::Video,
                },
                prompt: s.comfy.prompt.trim().to_string(),
                width: s.comfy.width,
                height: s.comfy.height,
                seconds: f64::from(s.comfy.seconds),
                vertical: s.comfy.vertical,
                seed: s.comfy.seed,
            };
            match s.ed.comfy_generate(req) {
                Ok(()) => s.comfy.place_pending = s.comfy.add_to_timeline,
                Err(e) => s.ed.error(e),
            }
        }
        if !running {
            ui.label(RichText::new("먼저 ComfyUI를 시작하세요.").size(11.0).color(theme::TEXT_DIM));
        }
    }
    if let Some(e) = &s.ed.comfy.last_error {
        ui.label(RichText::new(format!("⚠ {e}")).color(theme::WARN));
    }
    if let Some((path, item)) = s.ed.comfy.last.clone() {
        ui.separator();
        ui.label(RichText::new("마지막 결과").color(theme::TEXT_DIM));
        ui.add(egui::Label::new(RichText::new(path.display().to_string()).size(11.0)).wrap());
        if item.is_some() && ui.button("타임라인에 넣기").clicked() {
            s.comfy.place_pending = true;
        }
        if ui.button("폴더 열기").clicked()
            && let Some(dir) = path.parent().map(PathBuf::from)
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file:///{}", dir.display().to_string().replace('\\', "/"))));
        }
    }
}
