//! The export queue panel (Window > 내보내기 대기열): exports added with "대기열에 추가" wait
//! here and are made one after the other when the queue is started, like in a media encoder.

use egui::{RichText, Ui};
use op_application::QueueState;

use crate::app::State;
use crate::theme;

pub fn show(s: &mut State, ui: &mut Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| body(s, ui));
}

fn fmt_time(d: std::time::Duration) -> String {
    let t = d.as_secs();
    if t >= 3600 {
        format!("{}시간 {}분", t / 3600, (t % 3600) / 60)
    } else if t >= 60 {
        format!("{}분 {}초", t / 60, t % 60)
    } else {
        format!("{t}초")
    }
}

fn body(s: &mut State, ui: &mut Ui) {
    let started = s.ed.export_queue.started;
    let waiting = s.ed.export_queue.waiting();
    ui.horizontal(|ui| {
        if started {
            if ui.button("■ 중지").on_hover_text("지금 내보내는 것을 멈추고(만든 파일은 지움) 대기열을 멈춥니다").clicked() {
                s.ed.export_queue.stop();
            }
        } else if ui
            .add_enabled(waiting > 0, egui::Button::new("▶ 대기열 시작"))
            .on_hover_text("위에서부터 차례로 한 개씩 내보냅니다")
            .clicked()
        {
            s.ed.export_queue.start();
        }
        if ui.button("끝난 항목 지우기").clicked() {
            s.ed.export_queue.clear_finished();
        }
        ui.label(RichText::new(op_application::export_queue::summary(&s.ed.export_queue)).size(11.0).color(theme::TEXT_DIM));
    });
    ui.separator();
    if s.ed.export_queue.items.is_empty() {
        ui.label(
            RichText::new("내보내기 창에서 '대기열에 추가'를 눌러 쌓아 두세요. '대기열 시작'을 누르면 위에서부터 차례로 한 개씩 내보내고, 그동안 편집을 계속할 수 있습니다. 대기열에는 추가한 순간의 편집 상태가 저장됩니다.")
                .size(11.5)
                .color(theme::TEXT_DIM),
        );
        return;
    }
    if started || s.ed.export_queue.busy() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    let mut remove = None;
    let mut open_dir = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for item in &s.ed.export_queue.items {
            egui::Frame::new()
                .fill(theme::RAISED)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&item.title).strong().color(theme::TEXT_BRIGHT));
                        ui.label(RichText::new(&item.preset).size(11.0).color(theme::TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("대기열에서 지웁니다 (내보내는 중이면 멈춥니다)").clicked() {
                                remove = Some(item.id);
                            }
                            if matches!(item.state, QueueState::Done)
                                && let Some(dir) = item.settings.path.parent()
                                && ui.small_button("폴더 열기").clicked()
                            {
                                open_dir = Some(dir.to_path_buf());
                            }
                        });
                    });
                    ui.label(RichText::new(item.settings.path.display().to_string()).size(11.0).color(theme::TEXT_DIM));
                    match &item.state {
                        QueueState::Waiting => {
                            ui.label(RichText::new("대기 중").color(theme::TEXT_DIM));
                        }
                        QueueState::Running(_) => {
                            let p = item.progress().unwrap_or_default();
                            let text = if p.preparing {
                                "소리를 준비하는 중…".to_string()
                            } else {
                                let eta = p.remaining.map(|r| format!(" · 남은 시간 {}", fmt_time(r))).unwrap_or_default();
                                format!("{}/{} {:.0}% · {:.1} fps{eta}", p.frame, p.total, p.fraction() * 100.0, p.fps)
                            };
                            ui.add(egui::ProgressBar::new(p.fraction()).text(text));
                        }
                        QueueState::Done => {
                            ui.label(RichText::new("완료").color(egui::Color32::from_rgb(0x5d, 0xc0, 0x80)));
                        }
                        QueueState::Failed(e) => {
                            ui.label(RichText::new(format!("실패: {e}")).color(theme::ERROR));
                        }
                        QueueState::Cancelled => {
                            ui.label(RichText::new("멈춤").color(theme::WARN));
                        }
                    }
                });
            ui.add_space(4.0);
        }
    });
    if let Some(id) = remove {
        s.ed.export_queue.remove(id);
    }
    if let Some(dir) = open_dir {
        crate::app::open_folder(&dir);
    }
}
