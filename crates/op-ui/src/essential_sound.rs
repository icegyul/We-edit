//! The Essential Sound panel (Window > 기본 사운드): Remix of a music clip. The music is made
//! longer or shorter by repeating or skipping the parts where it repeats itself, so it stays
//! natural; the length changes in whole repeated parts, which differ from song to song.

use egui::{RichText, Ui};
use op_application::remix::Plan;
use op_core::{AssetId, ClipId, SeqTime, SrcTime};

use crate::app::State;
use crate::theme;

/// Which music the panel is showing: the same music keeps its settings while it is remixed.
type Key = (AssetId, SrcTime, SrcTime, usize, SeqTime);

/// A join that sounds worse than this is warned about.
const POOR_JOIN: f32 = 0.7;

pub struct EssentialView {
    key: Option<Key>,
    /// The length asked for, in seconds.
    target: f64,
    /// The target that was last applied.
    applied: f64,
    /// Shortest repeated or skipped stretch, in seconds.
    min_loop: f64,
    /// What the last change did.
    last: Option<Plan>,
}

impl Default for EssentialView {
    fn default() -> Self {
        EssentialView { key: None, target: 0.0, applied: 0.0, min_loop: 4.0, last: None }
    }
}

/// The first selected clip that sits on an audio track.
pub fn selected_audio(s: &State) -> Option<ClipId> {
    let seq = s.ed.active_seq()?;
    s.ed.selection.clips.iter().copied().find(|c| seq.audio.iter().any(|t| t.clip(*c).is_some()))
}

/// Right-click > Remix > Use Remix: analyses the selected music. False when nothing was started.
pub fn enable(s: &mut State) -> bool {
    let Some(clip) = selected_audio(s) else {
        s.ed.error("타임라인에서 음악 클립을 먼저 선택하세요");
        return false;
    };
    if let Err(e) = s.ed.remix_target(clip) {
        s.ed.error(e);
        return false;
    }
    if s.ed.remix_ready_for(clip) {
        s.ed.info("이 음악은 이미 분석되어 있습니다: 기본 사운드 패널에서 대상 지속 시간을 정하세요");
        return true;
    }
    match s.ed.remix_analyze(clip) {
        Ok(()) => {
            s.ed.info("음악을 분석하는 중입니다: 끝나면 기본 사운드 패널에서 대상 지속 시간을 정하세요");
            true
        }
        Err(e) => {
            s.ed.error(e);
            false
        }
    }
}

/// Right-click > Remix > Stop Using Remix: the music is as it was.
pub fn disable(s: &mut State) {
    let Some(clip) = selected_audio(s) else {
        s.ed.error("타임라인에서 음악 클립을 먼저 선택하세요");
        return;
    };
    match s.ed.remix_revert(clip) {
        Ok(()) => s.ed.info("리믹스를 끄고 원래 음악으로 되돌렸습니다"),
        Err(e) => s.ed.error(e),
    }
}

fn seconds(v: f64) -> String {
    if v >= 60.0 {
        format!("{}분 {:.1}초", (v / 60.0) as u32, v % 60.0)
    } else {
        format!("{v:.1}초")
    }
}

pub fn show(s: &mut State, ui: &mut Ui) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| body(s, ui));
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(theme::TEXT_DIM));
}

fn body(s: &mut State, ui: &mut Ui) {
    if let Some(job) = &s.ed.remix.job {
        let f = job.progress();
        ui.label("음악을 분석하는 중… (박자와 반복되는 부분을 찾습니다)");
        ui.add(egui::ProgressBar::new(f).text(format!("{:.0}%", f * 100.0)));
        if ui.button("취소").clicked() {
            job.cancel();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }
    let Some(clip) = selected_audio(s) else {
        hint(ui, "타임라인에서 음악 클립을 선택한 뒤 오른쪽 클릭 ▸ 리믹스 ▸ 리믹스 사용 을 누르세요.");
        hint(ui, "리믹스는 곡에서 반복되는 부분을 찾아 그 부분만 한 번 더 반복하거나 건너뛰어 음악 길이를 자연스럽게 바꿉니다.");
        return;
    };
    let target = match s.ed.remix_target(clip) {
        Ok(t) => t,
        Err(e) => {
            ui.label(RichText::new(e).color(theme::WARN));
            return;
        }
    };
    let key = (target.asset, target.src_in, target.src_out, target.track, target.start);
    if s.essential.key != Some(key) {
        s.essential = EssentialView { key: Some(key), target: target.length.seconds(), applied: target.length.seconds(), last: None, ..Default::default() };
    }
    let original = target.original();
    let now = target.length.seconds();
    let remixed = target.group.is_some();

    egui::Grid::new("remix-info").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.label("방법");
        ui.label(RichText::new("리믹스").strong());
        ui.end_row();
        ui.label("원본 길이");
        ui.label(RichText::new(seconds(original)).monospace().color(theme::VALUE));
        ui.end_row();
        ui.label("리믹스 지속 시간");
        let delta = now - original;
        let text = if remixed { format!("{} ({delta:+.1}초)", seconds(now)) } else { seconds(now) };
        ui.label(RichText::new(text).monospace().color(theme::VALUE));
        ui.end_row();
    });
    ui.add_space(6.0);

    if !s.ed.remix_ready_for(clip) {
        hint(ui, "곡에서 반복되는 부분을 찾아 그 부분만 한 번 더 반복하거나 건너뜁니다. 길이는 반복 구간 단위로 바뀌며, 곡마다 단위가 다릅니다.");
        if remixed {
            hint(ui, "이 음악은 리믹스되어 있습니다. 길이를 다시 바꾸려면 먼저 음악을 분석하세요.");
        }
        ui.horizontal(|ui| {
            let label = if remixed { "다시 분석" } else { "리믹스 사용" };
            if ui.button(label).clicked() {
                enable(s);
            }
            if remixed && ui.button("원래 음악으로 되돌리기").clicked() {
                disable(s);
            }
        });
        return;
    }

    let (mut want, mut min_loop) = (s.essential.target, s.essential.min_loop);
    let (mut apply, mut force) = (false, false);
    ui.horizontal(|ui| {
        ui.label("대상 지속 시간");
        if ui.button("−5초").on_hover_text("대상 길이를 5초 줄이기").clicked() {
            want = (want - 5.0).max(1.0);
            apply = true;
        }
        let r = ui.add(egui::DragValue::new(&mut want).speed(0.5).range(1.0..=3600.0).fixed_decimals(1).suffix(" 초"));
        if r.drag_stopped() || r.lost_focus() {
            apply = true;
        }
        if ui.button("＋5초").on_hover_text("대상 길이를 5초 늘리기").clicked() {
            want += 5.0;
            apply = true;
        }
    });
    hint(ui, "실제 길이는 대상에 가장 가까운 반복 구간 단위로 맞춰집니다.");
    ui.collapsing("사용자 지정", |ui| {
        ui.horizontal(|ui| {
            ui.label("반복·건너뛸 구간 최소 길이");
            let r = ui.add(egui::DragValue::new(&mut min_loop).speed(0.2).range(2.0..=30.0).fixed_decimals(1).suffix(" 초"));
            if r.drag_stopped() || r.lost_focus() {
                force = true;
                apply = true;
            }
        });
        hint(ui, "작을수록 곡을 잘게 늘릴 수 있지만, 큰 구간이 더 자연스럽게 이어집니다.");
    });
    s.essential.target = want;
    s.essential.min_loop = min_loop;

    if let Ok(steps) = s.ed.remix_steps(clip, min_loop) {
        if steps.is_empty() {
            ui.label(RichText::new("이 음악에서는 반복되는 부분을 찾지 못해 길이를 바꿀 수 없습니다.").color(theme::WARN));
        } else {
            let list: Vec<String> = steps.iter().map(|(len, score)| format!("{len:.1}초 (이음새 {:.0}%)", score * 100.0)).collect();
            ui.label(RichText::new(format!("이 곡이 늘어나는 단위: {}", list.join(" · "))).size(11.0).color(theme::TEXT_DIM));
        }
    }

    if apply && (force || (want - s.essential.applied).abs() > 1e-3) {
        match s.ed.remix_plan(clip, want, min_loop).and_then(|p| s.ed.remix_apply(clip, &p).map(|len| (p, len))) {
            Ok((p, len)) => {
                s.essential.last = Some(p);
                s.essential.applied = want;
                s.ed.info(format!("음악 길이를 {}로 맞췄습니다 (대상 {})", seconds(len), seconds(want)));
            }
            Err(e) => s.ed.error(e),
        }
    }

    if let Some(plan) = &s.essential.last {
        ui.add_space(4.0);
        if plan.moves.is_empty() {
            hint(ui, "원본 그대로입니다.");
        }
        for m in &plan.moves {
            let what = if m.times > 0 { format!("{}번 더 반복", m.times) } else { "건너뜀".to_string() };
            ui.label(
                RichText::new(format!("· {} ~ {} 구간 {what}", seconds(m.at.start), seconds(m.at.end)))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        }
        if plan.moves.iter().any(|m| m.at.score < POOR_JOIN) {
            ui.label(RichText::new("이 곡은 이음새가 어색할 수 있습니다: 다른 대상 길이를 시도해 보세요.").color(theme::WARN));
        }
    }

    ui.add_space(6.0);
    if remixed && ui.button("원래 음악으로 되돌리기").on_hover_text("리믹스를 끄고 클립 하나로 돌아갑니다").clicked() {
        disable(s);
    }
}
