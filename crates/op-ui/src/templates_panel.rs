//! The Motion Templates panel (Window > 모션 템플릿): animated titles and graphics that drop onto
//! the timeline at the playhead, ready to be edited in Effect Controls.

use egui::{RichText, Ui};
use op_application::motion_templates::{TEMPLATES, TemplateOptions};

use crate::app::State;
use crate::theme;

pub struct TemplatesView {
    sel: usize,
    text: String,
    sub: String,
    accent: [f32; 3],
    seconds: f64,
}

impl Default for TemplatesView {
    fn default() -> Self {
        let mut v = TemplatesView { sel: 0, text: String::new(), sub: String::new(), accent: [1.0; 3], seconds: 3.0 };
        v.load_defaults();
        v
    }
}

impl TemplatesView {
    fn load_defaults(&mut self) {
        let t = &TEMPLATES[self.sel];
        self.text = t.default_text.to_string();
        self.sub = t.default_sub.to_string();
        self.accent = t.accent;
        self.seconds = t.seconds;
    }
}

pub fn show(s: &mut State, ui: &mut Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| body(s, ui));
}

fn body(s: &mut State, ui: &mut Ui) {
    let height = (ui.available_height() - 150.0).max(80.0);
    egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show(ui, |ui| {
        for (i, t) in TEMPLATES.iter().enumerate() {
            let on = s.templates.sel == i;
            if ui.selectable_label(on, RichText::new(t.name).strong()).clicked() && !on {
                s.templates.sel = i;
                s.templates.load_defaults();
            }
            ui.label(RichText::new(t.note).size(11.0).color(theme::TEXT_DIM));
            ui.add_space(2.0);
        }
    });
    ui.separator();
    let t = &TEMPLATES[s.templates.sel];
    let v = &mut s.templates;
    egui::Grid::new("template-fields").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        if t.lines >= 1 {
            ui.label("글자");
            ui.add(egui::TextEdit::singleline(&mut v.text).desired_width(180.0));
            ui.end_row();
        }
        if t.lines >= 2 {
            ui.label("부제");
            ui.add(egui::TextEdit::singleline(&mut v.sub).desired_width(180.0));
            ui.end_row();
        }
        ui.label("강조 색");
        ui.color_edit_button_rgb(&mut v.accent);
        ui.end_row();
        ui.label("길이");
        ui.add(egui::DragValue::new(&mut v.seconds).range(0.3..=60.0).speed(0.1).suffix(" 초"));
        ui.end_row();
    });
    ui.add_space(4.0);
    let can = s.ed.active.is_some();
    if ui
        .add_enabled(can, egui::Button::new("타임라인에 넣기"))
        .on_hover_text("재생 헤드 위치, 비어 있는 영상 트랙 위에 묶음으로 들어갑니다")
        .clicked()
    {
        let o = TemplateOptions { text: s.templates.text.clone(), sub: s.templates.sub.clone(), accent: s.templates.accent, seconds: s.templates.seconds };
        match s.ed.add_motion_template(t.id, &o) {
            Ok(n) => s.ed.info(format!("'{}' 템플릿을 넣었습니다 (클립 {n}개, 하나로 묶임)", t.name)),
            Err(e) => s.ed.error(e),
        }
    }
    if !can {
        ui.label(RichText::new("먼저 시퀀스를 여세요").color(theme::TEXT_DIM));
    }
}
