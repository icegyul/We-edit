//! The Effects panel: every implemented effect and transition by category, with search. Items are
//! dragged onto clips, cuts or Effect Controls; double-click applies to the selection.

use std::collections::{HashMap, VecDeque};

use egui::{RichText, Sense, Ui, vec2};
use op_application::effect_preview::{self, FRAMES};
use op_application::performance::PreviewMode;
use op_core::catalog::{self, EffectDef, EffectKind};
use op_core::presets::{self, PresetDef};

use crate::app::{Drag, State};
use crate::i18n::{t, tn};
use crate::icons::{self, Icon};
use crate::theme;

/// Pictures kept at most (each is a short run of small textures).
const CACHE: usize = 90;
/// Previews rendered per frame.
const PER_FRAME: usize = 2;
/// Frames per second of an animated preview.
const FPS: f64 = 12.0;

#[derive(Default)]
pub struct EffectsView {
    search: String,
    /// Icon view (cards with pictures); the list of names is the default.
    grid: Option<bool>,
    cache: HashMap<&'static str, Vec<egui::TextureHandle>>,
    order: VecDeque<&'static str>,
    /// Previews cards asked for this frame and do not have yet.
    wanted: Vec<&'static EffectDef>,
    animating: bool,
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
    let mode = s.ed.prefs.performance().previews;
    let grid = mode != PreviewMode::Off && s.fx.grid.unwrap_or(false);
    s.fx.wanted.clear();
    s.fx.animating = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut s.fx.search)
                .hint_text(t("Search"))
                .desired_width(ui.available_width() - 36.0),
        );
        if mode != PreviewMode::Off
            && icons::button(
                ui,
                if grid { Icon::List } else { Icon::Grid },
                20.0,
                false,
                if grid {
                    t("Show a list")
                } else {
                    t("Show pictures")
                },
            )
            .clicked()
        {
            s.fx.grid = Some(!grid);
        }
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
                                if grid {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                                        for d in defs.iter().filter(|d| d.category == cat) {
                                            card(s, ui, d, mode);
                                        }
                                    });
                                } else {
                                    for d in defs.iter().filter(|d| d.category == cat) {
                                        entry(s, ui, d);
                                    }
                                }
                            });
                        }
                    });
            }
        });
    render_wanted(s, ui.ctx());
}

/// Renders the previews cards asked for (a couple per frame, so scrolling stays smooth).
fn render_wanted(s: &mut State, ctx: &egui::Context) {
    let wanted: Vec<&'static EffectDef> = std::mem::take(&mut s.fx.wanted);
    let mut made = 0;
    for d in wanted {
        if made >= PER_FRAME {
            break;
        }
        if s.fx.cache.contains_key(d.id) {
            continue;
        }
        made += 1;
        let frames = effect_preview::render(&mut s.renderer, d)
            .map(|p| {
                p.frames
                    .iter()
                    .enumerate()
                    .map(|(i, rgba)| {
                        ctx.load_texture(
                            format!("fx-preview-{}-{i}", d.id),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [p.width as usize, p.height as usize],
                                rgba,
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        s.fx.cache.insert(d.id, frames);
        s.fx.order.push_back(d.id);
        while s.fx.order.len() > CACHE {
            if let Some(old) = s.fx.order.pop_front() {
                s.fx.cache.remove(old);
            }
        }
    }
    if made > 0 || s.fx.animating {
        ctx.request_repaint_after(std::time::Duration::from_millis(if made > 0 {
            1
        } else {
            (1000.0 / FPS) as u64
        }));
    }
}

/// One effect as a card: its picture over its name. The picture moves while the pointer is on it
/// (or always, at the highest performance setting).
fn card(s: &mut State, ui: &mut Ui, d: &'static EffectDef, mode: PreviewMode) {
    const W: f32 = 104.0;
    let pic = vec2(W, W * 9.0 / 16.0);
    let (r, resp) = ui.allocate_exact_size(vec2(W, pic.y + 30.0), Sense::click_and_drag());
    if !ui.is_rect_visible(r) {
        return;
    }
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(r.expand(2.0), 4.0, theme::RAISED);
    }
    let image = egui::Rect::from_min_size(r.min, pic);
    painter.rect_filled(image, 3.0, theme::PANEL_DARK);
    if effect_preview::has_preview(d.kind) {
        match s.fx.cache.get(d.id) {
            Some(frames) if !frames.is_empty() => {
                let moving =
                    mode == PreviewMode::Always || (mode == PreviewMode::Hover && resp.hovered());
                let i = if moving {
                    s.fx.animating = true;
                    ((ui.input(|i| i.time) * FPS) as usize) % FRAMES
                } else {
                    FRAMES / 2
                };
                painter.image(
                    frames[i.min(frames.len() - 1)].id(),
                    image,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            Some(_) => {}
            None => {
                s.fx.wanted.push(d);
                ui.ctx().request_repaint();
            }
        }
    } else {
        icons::draw(
            painter,
            egui::Rect::from_center_size(image.center(), vec2(22.0, 22.0)),
            Icon::Music,
            theme::TEXT_DIM,
        );
    }
    painter.rect_stroke(
        image,
        3.0,
        egui::Stroke::new(
            1.0,
            if resp.hovered() {
                theme::ACCENT
            } else {
                theme::LINE
            },
        ),
        egui::StrokeKind::Inside,
    );
    let mut name = tn(d.name);
    if is_default(s, d) {
        name = format!("{name} \u{2605}");
    }
    let galley = painter.layout(name, egui::FontId::proportional(11.0), theme::TEXT, W);
    let pos = egui::pos2(
        r.min.x + (W - galley.size().x.min(W)) / 2.0,
        r.min.y + pic.y + 2.0,
    );
    painter
        .with_clip_rect(egui::Rect::from_min_size(
            r.min + vec2(0.0, pic.y),
            vec2(W, 30.0),
        ))
        .galley(pos, galley, theme::TEXT);
    finish(s, resp, d);
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
    finish(s, resp, d);
}

/// Dragging, double-click and the context menu of an effect (list row or card).
fn finish(s: &mut State, resp: egui::Response, d: &'static EffectDef) {
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
