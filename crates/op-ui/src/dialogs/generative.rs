//! 생성형 AI settings: what this PC can run, the local models (ComfyUI) to install, the API keys
//! of the cloud image services, and a box to make a picture with them.

use std::sync::Arc;

use op_application::cloud_images::{Aspect, Provider, check_key};
use op_application::comfy_packs::{PACKS, missing};
use op_application::generative::{Gpu, Verdict, detect_gpu, free_disk_gb, memory_gb, verdict};
use op_application::secrets;
use parking_lot::Mutex;

use super::*;

type Check = Arc<Mutex<Option<Result<(), String>>>>;

pub struct GenForm {
    /// Answers of the checks of what uses the graphics card (run once, off the interface thread).
    cuda: Arc<Mutex<Option<GpuUse>>>,
    cuda_started: bool,
    gpu: Option<Option<Gpu>>,
    memory: f64,
    keys: [String; 2],
    checks: [Option<Check>; 2],
    /// Last answer of a key check: (ok, text).
    notes: [Option<(bool, String)>; 2],
    provider: usize,
    aspect: usize,
    prompt: String,
}

impl Default for GenForm {
    fn default() -> Self {
        GenForm {
            cuda: Arc::new(Mutex::new(None)),
            cuda_started: false,
            gpu: None,
            memory: memory_gb(),
            keys: [String::new(), String::new()],
            checks: [None, None],
            notes: [None, None],
            provider: 0,
            aspect: 0,
            prompt: String::new(),
        }
    }
}

const GOOD: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x9d, 0x4f);

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong().family(crate::fonts::strong()).color(theme::TEXT_BRIGHT));
    ui.separator();
}

/// What the checks found.
struct GpuUse {
    /// CUDA works for the video decoder (NVDEC) and encoder (NVENC).
    media: bool,
    /// The speech helper: (CUDA usable, cards); None without the helper.
    speech: Option<(bool, usize)>,
}

fn gb(megabytes: u64) -> String {
    format!("{:.1} GB", megabytes as f64 / 1000.0)
}

pub(super) fn generative_dialog(s: &mut State, ctx: &egui::Context, f: &mut GenForm) -> bool {
    let mut open = true;
    if f.gpu.is_none() {
        f.gpu = Some(detect_gpu());
    }
    // answers of key checks that finished
    for i in 0..2 {
        let done = f.checks[i].as_ref().and_then(|c| c.lock().take());
        if let Some(res) = done {
            f.checks[i] = None;
            f.notes[i] = Some(match res {
                Ok(()) => (true, "키가 맞습니다".to_string()),
                Err(e) => (false, e),
            });
        }
    }
    if f.checks.iter().any(Option::is_some) || s.ed.cloud.is_some() || s.ed.comfy.install.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
    if !f.cuda_started {
        f.cuda_started = true;
        let slot = f.cuda.clone();
        std::thread::spawn(move || {
            let media = op_media::cuda_available();
            let speech = op_application::generative::helper_cuda();
            *slot.lock() = Some(GpuUse { media, speech });
        });
    }
    window(ctx, "생성형 AI")
        .open(&mut open)
        .resizable(true)
        .default_width(660.0)
        .default_height(620.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                pc(s, ui, f);
                gpu_use(s, ui, f);
                local_models(s, ui, f);
                cloud_keys(s, ui, f);
                cloud_make(s, ui, f);
            });
        });
    open
}

fn pc(s: &mut State, ui: &mut Ui, f: &mut GenForm) {
    heading(ui, "내 PC");
    let gpu = f.gpu.as_ref().and_then(Option::as_ref);
    egui::Grid::new("gen-pc").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.label("그래픽 카드");
        match gpu {
            Some(g) => match g.vram_mb {
                Some(mb) => ui.label(format!("{} · 비디오 메모리 {:.0} GB", g.name, mb as f64 / 1024.0)),
                None => ui.label(g.name.clone()),
            },
            None => ui.label(RichText::new("NVIDIA 카드를 찾지 못했습니다 (AMD·Intel 카드나 드라이버 없음): 모델 사양 판정은 '알 수 없음'").color(theme::WARN)),
        };
        ui.end_row();
        ui.label("시스템 메모리");
        ui.label(format!("{:.0} GB", f.memory));
        ui.end_row();
        ui.label("ComfyUI");
        let dir = op_application::comfy::find_dir(&s.ed.prefs.comfy_dir);
        match &dir {
            Some(d) => {
                let free = free_disk_gb(d).map_or(String::new(), |g| format!(" · 디스크 여유 {g:.0} GB"));
                ui.label(format!("{}{free}", d.display()));
            }
            None => {
                ui.label(RichText::new("찾지 못함: ComfyUI 패널에서 폴더를 지정하세요").color(theme::WARN));
            }
        }
        ui.end_row();
    });
}

/// Which parts of the program run on the NVIDIA card, as found on this PC just now.
fn gpu_use(s: &mut State, ui: &mut Ui, f: &mut GenForm) {
    heading(ui, "이 프로그램이 그래픽 카드(CUDA)를 쓰는 곳");
    let found = f.cuda.lock();
    let row = |ui: &mut Ui, what: &str, ok: Option<bool>, detail: String| {
        ui.horizontal_wrapped(|ui| {
            let (mark, color) = match ok {
                Some(true) => ("✔", GOOD),
                Some(false) => ("✘", theme::WARN),
                None => ("…", theme::TEXT_DIM),
            };
            ui.label(RichText::new(mark).color(color));
            ui.label(RichText::new(what).strong());
            ui.label(RichText::new(detail).size(11.5).color(theme::TEXT_DIM));
        });
    };
    row(ui, "화면 합성·색보정·효과", Some(true), format!("그래픽 카드(GPU)로 처리: {}", s.gpu.description()));
    let (decode, encode) = (s.ed.prefs.hardware_decoding.as_str(), s.ed.prefs.hardware_encoding);
    match found.as_ref() {
        None => {
            row(ui, "영상 디코딩 (NVDEC)", None, "확인하는 중…".into());
            row(ui, "내보내기·프록시 인코딩 (NVENC)", None, "확인하는 중…".into());
            row(ui, "음성 인식 (위스퍼)", None, "확인하는 중…".into());
        }
        Some(u) => {
            let dec = u.media && decode != "never";
            row(ui, "영상 디코딩 (NVDEC)", Some(dec), if u.media { if decode == "never" { "환경 설정에서 꺼져 있음".into() } else { "CUDA로 재생·썸네일·프록시 디코딩 (지원하지 않는 영상은 자동으로 CPU)".into() } } else { "CUDA를 쓸 수 없어 CPU·DirectX로 디코딩".into() });
            let enc = u.media && encode;
            row(ui, "내보내기·프록시 인코딩 (NVENC)", Some(enc), if u.media { if encode { "H.264·HEVC 내보내기와 프록시를 NVENC로 (안 되면 자동으로 CPU)".into() } else { "환경 설정에서 꺼져 있음".into() } } else { "CUDA를 쓸 수 없어 CPU로 인코딩".into() });
            match u.speech {
                Some((true, n)) => row(ui, "음성 인식 (위스퍼)", Some(true), format!("CUDA로 실행 (카드 {n}개): 무음 제거·자막·문장 편집 모두")),
                Some((false, _)) => row(ui, "음성 인식 (위스퍼)", Some(false), "도우미가 CUDA를 찾지 못해 CPU로 실행 (느림)".into()),
                None => row(ui, "음성 인식 (위스퍼)", Some(false), "음성 인식 도우미(helper)가 없어 내장 CPU 위스퍼로 실행".into()),
            }
        }
    }
    row(ui, "이미지·영상 생성 (ComfyUI)", Some(true), "ComfyUI가 NVIDIA 카드로 실행".into());
}

fn local_models(s: &mut State, ui: &mut Ui, f: &mut GenForm) {
    heading(ui, "내 컴퓨터에서 만들기 (ComfyUI)");
    ui.label(
        RichText::new("모델 파일은 처음 설치할 때 Hugging Face에서 ComfyUI 폴더로 내려받습니다. 크기가 크니 디스크 여유를 확인하세요.")
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    let gpu = f.gpu.as_ref().and_then(Option::as_ref);
    let dir = op_application::comfy::find_dir(&s.ed.prefs.comfy_dir);
    let free = dir.as_deref().and_then(free_disk_gb);
    let installing = s.ed.comfy.install.as_ref().map(|i| (i.id, i.progress()));
    let mut action: Option<(&'static str, bool)> = None;
    for pack in PACKS {
        let todo = dir.as_deref().map(|d| missing(pack, d));
        let installed = todo.as_ref().is_some_and(Vec::is_empty);
        let need: u64 = todo.as_ref().map_or(0, |t| t.iter().map(|f| u64::from(f.megabytes)).sum());
        let total: u64 = pack.files.iter().map(|f| u64::from(f.megabytes)).sum();
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(pack.label).strong());
            ui.label(RichText::new(format!("{} · {}", pack.kind, gb(total))).color(theme::TEXT_DIM));
            let (text, color) = match verdict(pack.vram_gb, gpu) {
                Verdict::Good => (format!("✔ 이 PC에서 쓸 수 있음 (메모리 {} GB 이상 권장)", pack.vram_gb), GOOD),
                Verdict::Tight => (format!("△ 메모리 {} GB 이상 권장: 느리거나 실패할 수 있음", pack.vram_gb), theme::WARN),
                Verdict::No => (format!("✘ 메모리 {} GB 이상 필요: 이 PC에는 부족", pack.vram_gb), theme::ERROR),
                Verdict::Unknown => (format!("? 메모리 {} GB 이상 권장 (이 PC의 메모리를 알 수 없음)", pack.vram_gb), theme::TEXT_DIM),
            };
            ui.label(RichText::new(text).color(color));
        });
        ui.horizontal_wrapped(|ui| {
            match installing {
                Some((id, (done, all))) if id == pack.id => {
                    let frac = (done as f32 / all.max(1) as f32).min(1.0);
                    ui.add(egui::ProgressBar::new(frac).desired_width(180.0).text(format!("{:.0}%", frac * 100.0)));
                    if ui.small_button("취소").clicked() {
                        action = Some((pack.id, false));
                    }
                }
                _ if installed => {
                    ui.label(RichText::new("✔ 설치됨").color(GOOD));
                }
                _ => {
                    let room = free.is_none_or(|g| g * 1000.0 > need as f64 + 2000.0);
                    let can = dir.is_some() && installing.is_none() && room;
                    if ui.add_enabled(can, egui::Button::new(format!("설치 ({} 내려받기)", gb(need)))).clicked() {
                        action = Some((pack.id, true));
                    }
                    if dir.is_some() && !room {
                        ui.label(RichText::new("디스크 여유가 부족합니다").color(theme::ERROR));
                    }
                }
            }
        });
        ui.label(RichText::new(pack.license).size(11.0).color(if pack.license_warning { theme::WARN } else { theme::TEXT_DIM }));
    }
    match action {
        Some((id, true)) => {
            let folder = s.ed.prefs.comfy_dir.clone();
            match s.ed.comfy_install(id, &folder) {
                Ok(()) => s.ed.info("모델을 내려받는 중입니다"),
                Err(e) => s.ed.error(e),
            }
        }
        Some((_, false)) => {
            if let Some(i) = &s.ed.comfy.install {
                i.cancel();
            }
        }
        None => {}
    }
}

fn cloud_keys(s: &mut State, ui: &mut Ui, f: &mut GenForm) {
    heading(ui, "클라우드 이미지 (내 API 키)");
    ui.label(
        RichText::new("키는 Windows 자격 증명 관리자에만 저장되고, 설정 파일·프로젝트·로그에는 남지 않습니다. 요금은 각 서비스에서 청구됩니다.")
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    for (i, p) in Provider::ALL.into_iter().enumerate() {
        ui.add_space(4.0);
        let saved = secrets::get(p.secret()).is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(p.label()).strong());
            if saved {
                ui.label(RichText::new("● 키 저장됨").color(GOOD));
            } else {
                ui.label(RichText::new("키 없음").color(theme::WARN));
            }
            ui.hyperlink_to("키 만드는 곳", p.key_page());
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.keys[i]).password(true).hint_text("API 키를 붙여넣기").desired_width(260.0));
            if ui.add_enabled(!f.keys[i].trim().is_empty(), egui::Button::new("저장")).clicked() {
                match secrets::set(p.secret(), &f.keys[i]) {
                    Ok(()) => {
                        f.keys[i].clear();
                        f.notes[i] = Some((true, "저장했습니다".into()));
                    }
                    Err(e) => f.notes[i] = Some((false, e)),
                }
            }
            if ui.add_enabled(saved, egui::Button::new("삭제")).clicked() {
                match secrets::delete(p.secret()) {
                    Ok(()) => f.notes[i] = Some((true, "키를 지웠습니다".into())),
                    Err(e) => f.notes[i] = Some((false, e)),
                }
            }
            if ui.add_enabled(saved && f.checks[i].is_none(), egui::Button::new("연결 확인")).clicked()
                && let Some(key) = secrets::get(p.secret())
            {
                let slot: Check = Arc::new(Mutex::new(None));
                let s2 = slot.clone();
                std::thread::spawn(move || *s2.lock() = Some(check_key(p, &key)));
                f.checks[i] = Some(slot);
                f.notes[i] = None;
            }
            if f.checks[i].is_some() {
                ui.label(RichText::new("확인하는 중…").color(theme::TEXT_DIM));
            }
        });
        ui.horizontal(|ui| {
            ui.label("모델");
            let current = if p == Provider::Gemini { s.ed.prefs.gemini_model.clone() } else { s.ed.prefs.openai_model.clone() };
            let mut chosen = current.clone();
            egui::ComboBox::from_id_salt(("cloud-model", i)).selected_text(chosen.clone()).show_ui(ui, |ui| {
                for m in p.models() {
                    ui.selectable_value(&mut chosen, (*m).to_string(), *m);
                }
            });
            if chosen != current {
                if p == Provider::Gemini {
                    s.ed.prefs.gemini_model = chosen;
                } else {
                    s.ed.prefs.openai_model = chosen;
                }
                let _ = s.ed.prefs.save(&s.ed.dirs);
            }
        });
        if let Some((ok, text)) = &f.notes[i] {
            ui.label(RichText::new(text).size(11.5).color(if *ok { GOOD } else { theme::ERROR }));
        }
    }
}

fn cloud_make(s: &mut State, ui: &mut Ui, f: &mut GenForm) {
    heading(ui, "클라우드로 그림 만들기");
    let ready: Vec<Provider> = Provider::ALL.into_iter().filter(|p| secrets::get(p.secret()).is_some()).collect();
    if ready.is_empty() {
        ui.label(RichText::new("위에서 API 키를 하나 이상 저장하면 여기서 그림을 만들 수 있습니다").color(theme::TEXT_DIM));
        return;
    }
    if !ready.iter().any(|p| Provider::ALL[f.provider] == *p) {
        f.provider = Provider::ALL.iter().position(|p| *p == ready[0]).unwrap_or(0);
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("cloud-provider").selected_text(Provider::ALL[f.provider].label()).show_ui(ui, |ui| {
            for p in &ready {
                let i = Provider::ALL.iter().position(|x| x == p).unwrap_or(0);
                ui.selectable_value(&mut f.provider, i, p.label());
            }
        });
        egui::ComboBox::from_id_salt("cloud-aspect").selected_text(Aspect::ALL[f.aspect].label()).show_ui(ui, |ui| {
            for (i, a) in Aspect::ALL.iter().enumerate() {
                ui.selectable_value(&mut f.aspect, i, a.label());
            }
        });
    });
    ui.add(egui::TextEdit::multiline(&mut f.prompt).desired_rows(3).desired_width(f32::INFINITY).hint_text("만들 그림을 설명하세요"));
    ui.horizontal(|ui| {
        let busy = s.ed.cloud.is_some();
        if ui.add_enabled(!busy && !f.prompt.trim().is_empty(), egui::Button::new("그림 만들기")).clicked() {
            match s.ed.cloud_image(Provider::ALL[f.provider], &f.prompt, Aspect::ALL[f.aspect]) {
                Ok(()) => s.ed.info("그림을 만드는 중입니다 (수십 초)"),
                Err(e) => s.ed.error(e),
            }
        }
        if busy {
            ui.label(RichText::new("만드는 중…").color(theme::TEXT_DIM));
        } else if let Some(p) = &s.ed.cloud_last {
            ui.label(RichText::new(format!("마지막 그림: {}", p.display())).size(11.0).color(theme::TEXT_DIM));
        }
    });
    ui.label(RichText::new("만든 그림은 프로젝트 패널에 자동으로 들어옵니다. 비용은 사용한 서비스에서 청구됩니다.").size(11.0).color(theme::TEXT_DIM));
}
