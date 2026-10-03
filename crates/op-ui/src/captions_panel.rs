//! The Captions panel (Window > Captions): every caption of the sequence with its text, and one
//! place to style them all, or only the selected ones, at once.

use egui::{Color32, RichText, Ui};
use op_application::Monitor;
use op_core::catalog;
use op_core::*;

use crate::app::State;
use crate::i18n::{t, tf};
use crate::theme;

fn value<'a>(c: &'a Component, key: &str) -> Option<&'a Value> {
    c.param(key).map(|p| &p.value)
}

fn f64_of(c: &Component, key: &str, default: f64) -> f64 {
    match value(c, key) {
        Some(Value::Float(v)) => *v,
        Some(Value::Int(v)) => *v as f64,
        _ => default,
    }
}

fn bool_of(c: &Component, key: &str) -> bool {
    matches!(value(c, key), Some(Value::Bool(true)))
}

fn color_of(c: &Component, key: &str) -> [f32; 3] {
    match value(c, key) {
        Some(Value::Color(c)) => [c.r, c.g, c.b],
        _ => [1.0, 1.0, 1.0],
    }
}

pub fn captions(s: &mut State, ui: &mut Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| body(s, ui));
}

fn body(s: &mut State, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(t("Transcribe..."))
            .on_hover_text(t("Transcribe and Create Captions"))
            .clicked()
        {
            s.command("op.captions.transcribe");
        }
        if ui.button(t("New Caption")).clicked() {
            s.command("op.captions.new");
        }
        if ui.button(t("Import...")).clicked() {
            s.command("op.captions.import");
        }
        if ui.button(t("Export...")).clicked() {
            s.command("op.captions.export");
        }
    });
    ui.separator();
    if s.ed.active.is_none() {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t("Open a sequence first")).color(theme::TEXT_DIM));
        });
        return;
    }
    let all = s.ed.caption_clips();
    if all.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(t(
                        "This sequence has no captions yet. Transcribe its audio, import an SRT or WebVTT file, or add a caption by hand.",
                    ))
                    .color(theme::TEXT_DIM),
                )
                .wrap(),
            );
        });
        return;
    }
    let selected: Vec<ClipId> = all
        .iter()
        .copied()
        .filter(|id| s.ed.selection.clips.contains(id))
        .collect();
    // style changes go to the selected captions when there are some, else to all of them
    ui.horizontal(|ui| {
        ui.label(t("Apply to"));
        ui.radio_value(
            &mut s.captions_all,
            true,
            tf("All captions ({})", &[&all.len()]),
        );
        ui.add_enabled_ui(!selected.is_empty(), |ui| {
            ui.radio_value(
                &mut s.captions_all,
                false,
                tf("Selected ({})", &[&selected.len()]),
            );
        });
    });
    if selected.is_empty() {
        s.captions_all = true;
    }
    let targets: Vec<ClipId> = if s.captions_all {
        all.clone()
    } else {
        selected.clone()
    };
    // the look shown is the first target's
    let first = s.ed.active_seq().and_then(|q| {
        targets
            .first()
            .and_then(|id| q.clip(*id))
            .and_then(|c| c.component(catalog::CAPTION).cloned())
    });
    let Some(first) = first else { return };
    let mut changes: Vec<(String, Value)> = Vec::new();
    let mut merge: Option<String> = None;
    // side by side when the panel is wide, one under the other (scrolling together) when not
    let height = ui.available_height();
    if ui.available_width() >= 640.0 {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(390.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("caption-style-scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| style(s, ui, &first, &mut changes, &mut merge));
                },
            );
            ui.add_space(8.0);
            ui.vertical(|ui| caption_list(s, ui, &all, true));
        });
    } else {
        egui::ScrollArea::vertical()
            .id_salt("caption-panel-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                style(s, ui, &first, &mut changes, &mut merge);
                caption_list(s, ui, &all, false);
            });
    }
    if !changes.is_empty() {
        s.ed.set_caption_values(&targets, changes, merge);
    }
    if ui.input(|i| i.pointer.any_released()) {
        s.ed.seal();
    }
}

/// The style controls, showing the first target's look; changes are collected for all targets.
fn style(
    s: &mut State,
    ui: &mut Ui,
    first: &Component,
    changes: &mut Vec<(String, Value)>,
    merge: &mut Option<String>,
) {
    let first = first.clone();
    let mut set = |k: &str, v: Value, drag: bool, merge: &mut Option<String>| {
        if drag {
            *merge = Some(format!("caption-{k}"));
        }
        changes.push((k.to_string(), v));
    };

    egui::Frame::new()
        .fill(theme::PANEL_DARK)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("caption-style")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t("Caption Style"));
                    let style = match value(&first, "style") {
                        Some(Value::Choice(c)) => *c as usize,
                        _ => 2,
                    };
                    egui::ComboBox::from_id_salt("cap-panel-style")
                        .selected_text(t(
                            catalog::CAPTION_STYLES[style % catalog::CAPTION_STYLES.len()]
                        ))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in catalog::CAPTION_STYLES.iter().enumerate() {
                                if ui.selectable_label(i == style, t(name)).clicked() {
                                    set("style", Value::Choice(i as u32), false, merge);
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(t("Font"));
                    if s.ec.fonts.is_empty() {
                        s.ec.fonts = op_render::text::Fonts::global().families();
                    }
                    let cur = match value(&first, "font") {
                        Some(Value::Text(f)) if !f.is_empty() => f.clone(),
                        _ => op_render::text::Fonts::default_family().to_string(),
                    };
                    egui::ComboBox::from_id_salt("cap-panel-font")
                        .selected_text(&cur)
                        .width(170.0)
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            for f in &s.ec.fonts {
                                if ui.selectable_label(*f == cur, f).clicked() {
                                    set("font", Value::Text(f.clone()), false, merge);
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(t("Font Style"));
                    let fs = match value(&first, "font_style") {
                        Some(Value::Choice(c)) => *c as usize,
                        _ => 1,
                    };
                    egui::ComboBox::from_id_salt("cap-panel-fs")
                        .selected_text(t(catalog::FONT_STYLES[fs.min(3)]))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in catalog::FONT_STYLES.iter().enumerate() {
                                if ui.selectable_label(i == fs, t(name)).clicked() {
                                    set("font_style", Value::Choice(i as u32), false, merge);
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(t("Font Size"));
                    let mut size = f64_of(&first, "font_size", 72.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut size)
                                .range(8.0..=400.0)
                                .speed(0.5),
                        )
                        .changed()
                    {
                        set("font_size", Value::Float(size), true, merge);
                    }
                    ui.end_row();

                    ui.label(t("Text Color"));
                    let mut c = color_of(&first, "fill");
                    if ui.color_edit_button_rgb(&mut c).changed() {
                        set(
                            "fill",
                            Value::Color(Rgba::new(c[0], c[1], c[2], 1.0)),
                            true,
                            merge,
                        );
                    }
                    ui.end_row();

                    ui.label(t("Highlight Color"));
                    let mut c = color_of(&first, "highlight");
                    if ui.color_edit_button_rgb(&mut c).changed() {
                        set(
                            "highlight",
                            Value::Color(Rgba::new(c[0], c[1], c[2], 1.0)),
                            true,
                            merge,
                        );
                    }
                    ui.end_row();

                    ui.label(t("Stroke"));
                    ui.horizontal(|ui| {
                        let mut on = bool_of(&first, "stroke");
                        if ui.checkbox(&mut on, "").changed() {
                            set("stroke", Value::Bool(on), false, merge);
                        }
                        let mut c = color_of(&first, "stroke_color");
                        if ui.color_edit_button_rgb(&mut c).changed() {
                            set(
                                "stroke_color",
                                Value::Color(Rgba::new(c[0], c[1], c[2], 1.0)),
                                true,
                                merge,
                            );
                        }
                        let mut w = f64_of(&first, "stroke_width", 6.0);
                        if ui
                            .add(
                                egui::DragValue::new(&mut w)
                                    .range(0.0..=60.0)
                                    .speed(0.2)
                                    .suffix(" px"),
                            )
                            .changed()
                        {
                            set("stroke_width", Value::Float(w), true, merge);
                        }
                    });
                    ui.end_row();

                    ui.label(t("Box Color"));
                    ui.horizontal(|ui| {
                        let mut c = color_of(&first, "background_color");
                        if ui.color_edit_button_rgb(&mut c).changed() {
                            set(
                                "background_color",
                                Value::Color(Rgba::new(c[0], c[1], c[2], 1.0)),
                                true,
                                merge,
                            );
                        }
                        let mut o = f64_of(&first, "background_opacity", 75.0);
                        if ui
                            .add(egui::DragValue::new(&mut o).range(0.0..=100.0).suffix(" %"))
                            .changed()
                        {
                            set("background_opacity", Value::Float(o), true, merge);
                        }
                    });
                    ui.end_row();

                    ui.label(t("Vertical Position"));
                    let pos = match value(&first, "position") {
                        Some(Value::Point(p)) => *p,
                        _ => [0.5, 0.8],
                    };
                    let mut y = (pos[1] * 100.0) as f32;
                    if ui
                        .add(egui::Slider::new(&mut y, 5.0..=95.0).suffix(" %"))
                        .changed()
                    {
                        set(
                            "position",
                            Value::Point([pos[0], y as f64 / 100.0]),
                            true,
                            merge,
                        );
                    }
                    ui.end_row();

                    ui.label(t("Maximum Width"));
                    let mut mw = f64_of(&first, "max_width", 80.0) as f32;
                    if ui
                        .add(egui::Slider::new(&mut mw, 20.0..=100.0).suffix(" %"))
                        .changed()
                    {
                        set("max_width", Value::Float(mw as f64), true, merge);
                    }
                    ui.end_row();

                    ui.label(t("Animation Strength"));
                    let mut a = f64_of(&first, "animation", 100.0) as f32;
                    if ui
                        .add(egui::Slider::new(&mut a, 0.0..=200.0).suffix(" %"))
                        .changed()
                    {
                        set("animation", Value::Float(a as f64), true, merge);
                    }
                    ui.end_row();

                    ui.label(t("All Caps"));
                    let mut up = bool_of(&first, "uppercase");
                    if ui.checkbox(&mut up, "").changed() {
                        set("uppercase", Value::Bool(up), false, merge);
                    }
                    ui.end_row();

                    ui.label(t("Shadow"));
                    let mut sh = bool_of(&first, "shadow");
                    if ui.checkbox(&mut sh, "").changed() {
                        set("shadow", Value::Bool(sh), false, merge);
                    }
                    ui.end_row();
                });
        });
}

/// Every caption: start time and text, editable; clicking the time selects the caption and moves
/// the playhead to it. `scroll` gives the list its own scroll area.
fn caption_list(s: &mut State, ui: &mut Ui, all: &[ClipId], scroll: bool) {
    ui.label(
        RichText::new(tf("{} captions", &[&all.len()]))
            .strong()
            .family(crate::fonts::strong())
            .color(theme::TEXT_BRIGHT),
    );
    // every caption: start time and text, editable; clicking the time selects the caption and
    // moves the playhead to it
    let rows: Vec<(ClipId, SeqTime, String)> =
        s.ed.active_seq()
            .map(|q| {
                all.iter()
                    .filter_map(|id| q.clip(*id))
                    .map(|c| {
                        let text = c
                            .component(catalog::CAPTION)
                            .and_then(|x| match value(x, "text") {
                                Some(Value::Text(t)) => Some(t.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        (c.id, c.start, text)
                    })
                    .collect()
            })
            .unwrap_or_default();
    let (format, offset) = s.timecode(Monitor::Program);
    let rows_ui = |ui: &mut Ui| {
        for (id, start, text) in rows {
            let sel = s.ed.selection.clips.contains(&id);
            ui.horizontal(|ui| {
                let tc = format.format(start - SeqTime::ZERO + offset);
                let label = RichText::new(tc).monospace().size(11.0).color(if sel {
                    theme::ACCENT
                } else {
                    theme::TEXT_DIM
                });
                if ui
                    .add(egui::Button::new(label).frame(false))
                    .on_hover_text(t("Select and go to this caption"))
                    .clicked()
                {
                    let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    if add {
                        if !s.ed.selection.clips.contains(&id) {
                            s.ed.selection.clips.push(id);
                        }
                    } else {
                        s.ed.selection = op_application::Selection::only(vec![id]);
                    }
                    s.ed.set_playhead(start);
                }
                let mut edited = text.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut edited)
                        .desired_width(ui.available_width() - 4.0)
                        .text_color(if sel { Color32::WHITE } else { theme::TEXT }),
                );
                if resp.changed() {
                    s.ed.set_caption_values(
                        &[id],
                        vec![("text".into(), Value::Text(edited))],
                        Some(format!("caption-text-{}", id.0)),
                    );
                }
                if resp.lost_focus() {
                    s.ed.seal();
                }
            });
        }
    };
    if scroll {
        egui::ScrollArea::vertical()
            .id_salt("caption-rows")
            .auto_shrink([false, false])
            .show(ui, rows_ui);
    } else {
        rows_ui(ui);
    }
}
