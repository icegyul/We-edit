//! The Sounds panel (Window > 효과음·배경음): public-domain (CC0) sound effects and background
//! music. A sound is downloaded the first time it is played or added, never before.

use egui::{RichText, Ui};
use op_application::sound_library::{SoundAction, catalog};

use crate::app::State;
use crate::theme;

#[derive(Default)]
pub struct SoundsView {
    /// 0 sound effects, 1 background music.
    kind: usize,
    query: String,
    /// Chosen category; empty for all.
    category: String,
}

const KINDS: [(&str, &str); 2] = [("sfx", "효과음"), ("bgm", "배경음")];

pub fn show(s: &mut State, ui: &mut Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| body(s, ui));
}

fn body(s: &mut State, ui: &mut Ui) {
    let kind = KINDS[s.sounds.kind].0;
    ui.horizontal_wrapped(|ui| {
        for (i, (_, name)) in KINDS.iter().enumerate() {
            if ui.selectable_label(s.sounds.kind == i, *name).clicked() && s.sounds.kind != i {
                s.sounds.kind = i;
                s.sounds.category.clear();
            }
        }
        ui.add(egui::TextEdit::singleline(&mut s.sounds.query).hint_text("검색 (예: 휘익, click, 에어혼)").desired_width(170.0));
        let mut cats: Vec<(&str, usize)> = Vec::new();
        for snd in catalog().sounds.iter().filter(|x| x.kind == kind) {
            match cats.iter_mut().find(|c| c.0 == snd.cat) {
                Some(c) => c.1 += 1,
                None => cats.push((&snd.cat, 1)),
            }
        }
        cats.sort_by(|a, b| b.1.cmp(&a.1));
        let shown = if s.sounds.category.is_empty() { "모든 분류" } else { s.sounds.category.as_str() };
        egui::ComboBox::from_id_salt("sound-category").selected_text(shown).width(130.0).show_ui(ui, |ui| {
            if ui.selectable_label(s.sounds.category.is_empty(), "모든 분류").clicked() {
                s.sounds.category.clear();
            }
            for (name, n) in &cats {
                if ui.selectable_label(s.sounds.category == *name, format!("{name} ({n})")).clicked() {
                    s.sounds.category = name.to_string();
                }
            }
        });
    });
    ui.label(
        RichText::new("모두 CC0(퍼블릭 도메인): 저작권 걱정·크레딧 없이 상업 영상에도 쓸 수 있습니다. 소리는 처음 듣거나 추가할 때 한 번만 내려받습니다.")
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    ui.separator();

    let list: Vec<_> = catalog()
        .sounds
        .iter()
        .filter(|x| x.kind == kind)
        .filter(|x| s.sounds.category.is_empty() || x.cat == s.sounds.category)
        .filter(|x| x.matches(&s.sounds.query))
        .collect();
    let fetching = s.ed.sound.fetching();
    let previewing = s.ed.sound.previewing();
    if fetching.is_some() || previewing.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    if list.is_empty() {
        ui.label(RichText::new("찾는 소리가 없습니다").color(theme::TEXT_DIM));
        return;
    }
    let can_add = s.ed.active.is_some();
    let row_h = 24.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, list.len(), |ui, range| {
        for snd in &list[range] {
            ui.horizontal(|ui| {
                ui.set_height(row_h);
                let playing = previewing == Some(snd.id.as_str());
                let loading = fetching == Some(snd.id.as_str());
                let (icon, tip) = if playing { ("■", "멈춤") } else { ("▶", "듣기") };
                if ui.add_enabled(!loading, egui::Button::new(icon)).on_hover_text(tip).clicked() {
                    if playing {
                        s.ed.sound_stop();
                    } else if let Err(e) = s.ed.sound_request(&snd.id, SoundAction::Preview) {
                        s.ed.error(e);
                    }
                }
                if ui
                    .add_enabled(can_add && !loading, egui::Button::new("＋"))
                    .on_hover_text("재생 헤드 위치의 빈 오디오 트랙에 넣기")
                    .clicked()
                    && let Err(e) = s.ed.sound_request(&snd.id, SoundAction::Add)
                {
                    s.ed.error(e);
                }
                ui.label(RichText::new(snd.title()).color(theme::TEXT));
                if !snd.ko.is_empty() {
                    ui.label(RichText::new(&snd.name).size(11.0).color(theme::TEXT_DIM));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:.1}초", snd.sec)).monospace().color(theme::VALUE));
                    if loading {
                        ui.label(RichText::new("내려받는 중…").color(theme::WARN));
                    }
                });
            });
        }
    });
}
