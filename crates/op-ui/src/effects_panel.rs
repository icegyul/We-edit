//! The Effects panel: every implemented effect and transition by category, with search. Items are
//! dragged onto clips, cuts or Effect Controls; double-click applies to the selection.

use egui::{RichText, Sense, Ui, vec2};
use op_core::catalog::{self, EffectDef, EffectKind};
use op_core::presets::{self, PresetDef};

use crate::app::{Drag, State};
use crate::i18n::{t, tn};
use crate::icons::{self, Icon};
use crate::theme;

#[derive(Default)]
pub struct EffectsView {
    search: String,
}

const GROUPS: [(EffectKind, &str); 4] = [
    (EffectKind::VideoEffect, "Video Effects"),
    (EffectKind::VideoTransition, "Video Transitions"),
    (EffectKind::AudioEffect, "Audio Effects"),
    (EffectKind::AudioTransition, "Audio Transitions"),
];

fn is_default(s: &State, d: &EffectDef) -> bool {
    d.id == s.ed.project.settings.video_transition || d.id == s.ed.project.settings.audio_transition
}

pub fn show(s: &mut State, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut s.fx.search)
                .hint_text(t("Search"))
                .desired_width(ui.available_width() - 8.0),
        );
    });
    ui.add_space(4.0);
    let q = s.fx.search.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            presets(s, ui, &q);
            for (kind, title) in GROUPS {
                let defs: Vec<&'static EffectDef> = catalog::by_kind(kind)
                    .filter(|d| {
                        q.is_empty()
                            || tn(d.name).to_lowercase().contains(&q)
                            || d.name.to_lowercase().contains(&q)
                    })
                    .collect();
                if defs.is_empty() {
                    continue;
                }
                let id = ui.id().with(("fx-group", title));
                let header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    !q.is_empty(),
                );
                header
                    .show_header(ui, |ui| {
                        icons::draw(
                            ui.painter(),
                            egui::Rect::from_min_size(ui.cursor().min, vec2(16.0, 16.0)),
                            Icon::Folder,
                            theme::TEXT_DIM,
                        );
                        ui.add_space(18.0);
                        ui.label(RichText::new(t(title)).color(theme::TEXT_BRIGHT));
                    })
                    .body(|ui| {
                        let mut cats: Vec<&str> = defs.iter().map(|d| d.category).collect();
                        cats.dedup();
                        let mut seen = Vec::new();
                        for cat in cats {
                            if seen.contains(&cat) {
                                continue;
                            }
                            seen.push(cat);
                            let cid = ui.id().with(("fx-cat", title, cat));
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                cid,
                                !q.is_empty(),
                            )
                            .show_header(ui, |ui| {
                                ui.label(RichText::new(tn(cat)).color(theme::TEXT));
                            })
                            .body(|ui| {
                                for d in defs.iter().filter(|d| d.category == cat) {
                                    entry(s, ui, d);
                                }
                            });
                        }
                    });
            }
        });
}

/// Animation presets, by category; they are applied like video effects.
fn presets(s: &mut State, ui: &mut Ui, q: &str) {
    let list: Vec<&'static PresetDef> = presets::PRESETS
        .iter()
        .filter(|p| {
            q.is_empty()
                || tn(p.name).to_lowercase().contains(q)
                || p.name.to_lowercase().contains(q)
        })
        .collect();
    if list.is_empty() {
        return;
    }
    let id = ui.id().with("fx-presets");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, !q.is_empty())
        .show_header(ui, |ui| {
            icons::draw(
                ui.painter(),
                egui::Rect::from_min_size(ui.cursor().min, vec2(16.0, 16.0)),
                Icon::Folder,
                theme::TEXT_DIM,
            );
            ui.add_space(18.0);
            ui.label(RichText::new(t("Presets")).color(theme::TEXT_BRIGHT));
        })
        .body(|ui| {
            let mut cats: Vec<&str> = Vec::new();
            for p in &list {
                if !cats.contains(&p.category) {
                    cats.push(p.category);
                }
            }
            for cat in cats {
                let cid = ui.id().with(("fx-preset-cat", cat));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    cid,
                    !q.is_empty(),
                )
                .show_header(ui, |ui| {
                    ui.label(RichText::new(tn(cat)).color(theme::TEXT));
                })
                .body(|ui| {
                    for p in list.iter().filter(|p| p.category == cat) {
                        preset_entry(s, ui, p);
                    }
                });
            }
        });
}

fn preset_entry(s: &mut State, ui: &mut Ui, p: &'static PresetDef) {
    let (r, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click_and_drag());
    if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, theme::RAISED);
    }
    icons::draw(
        ui.painter(),
        egui::Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(16.0, 16.0)),
        Icon::Stopwatch,
        theme::TEXT_DIM,
    );
    ui.painter().text(
        r.min + vec2(22.0, 10.0),
        egui::Align2::LEFT_CENTER,
        tn(p.name),
        egui::FontId::proportional(12.0),
        theme::TEXT,
    );
    if resp.drag_started() {
        s.drag = Some(Drag::Effect(p.id));
    }
    if resp.double_clicked() {
        s.ed.apply_effect(p.id, None);
    }
    resp.on_hover_text(t(
        "Drag onto a clip, or double-click to apply to the selected clips",
    ))
    .context_menu(|ui| {
        if ui.button(t("Apply to Selected Clips")).clicked() {
            s.ed.apply_effect(p.id, None);
            ui.close();
        }
    });
}

fn entry(s: &mut State, ui: &mut Ui, d: &'static EffectDef) {
    let default = is_default(s, d);
    let (r, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click_and_drag());
    if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, theme::RAISED);
    }
    let icon = if d.kind.is_transition() {
        Icon::Sequence
    } else {
        Icon::Fx
    };
    icons::draw(
        ui.painter(),
        egui::Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(16.0, 16.0)),
        icon,
        if d.accelerated {
            theme::ACCENT
        } else {
            theme::TEXT_DIM
        },
    );
    let mut text = tn(d.name);
    if default {
        text = format!("{text}  \u{2605}");
    }
    ui.painter().text(
        r.min + vec2(22.0, 10.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        if default {
            theme::TEXT_BRIGHT
        } else {
            theme::TEXT
        },
    );
    if resp.drag_started() {
        s.drag = Some(Drag::Effect(d.id));
    }
    if resp.double_clicked() {
        if d.kind.is_transition() {
            let clips = s.ed.selection.clips.clone();
            let id = d.id;
            let dur = if d.kind.is_video() {
                s.ed.project.settings.video_transition_duration
            } else {
                s.ed.project.settings.audio_transition_duration
            };
            let kind = if d.kind.is_video() {
                op_core::TrackKind::Video
            } else {
                op_core::TrackKind::Audio
            };
            // add at both ends of each selected clip of the matching kind
            s.ed.seq_edit("Add Transition", |p, sid, opts| {
                let seq = p.sequence(sid).unwrap().clone();
                let mut any = false;
                for c in &clips {
                    if let Some((r, clip)) = seq.find_clip(*c)
                        && r.kind == kind
                    {
                        for cut in [clip.start, clip.end()] {
                            if op_timeline::add_transition(p, sid, r, cut, id, dur, None, opts)
                                .is_ok()
                            {
                                any = true;
                            }
                        }
                    }
                }
                if any {
                    Ok(())
                } else {
                    Err(op_core::EditError::Nothing)
                }
            });
        } else {
            s.ed.apply_effect(d.id, None);
        }
    }
    resp.on_hover_text(tn(d.category)).context_menu(|ui| {
        if d.kind.is_transition() {
            if ui.button(t("Set Selected as Default Transition")).clicked() {
                let (id, video) = (d.id.to_string(), d.kind.is_video());
                s.ed.edit("Default Transition", |p| {
                    if video {
                        p.settings.video_transition = id;
                    } else {
                        p.settings.audio_transition = id;
                    }
                    Ok(())
                });
                ui.close();
            }
        } else if ui.button(t("Apply to Selected Clips")).clicked() {
            s.ed.apply_effect(d.id, None);
            ui.close();
        }
    });
}
