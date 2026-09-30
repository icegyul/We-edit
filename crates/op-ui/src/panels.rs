//! Smaller panels: History, Tools, Audio Meters, Audio Track Mixer, Markers, Info, Lumetri Color,
//! Lumetri Scopes and Essential Graphics.

use std::collections::HashMap;
use std::time::Instant;

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use op_application::{Editor, Monitor, Selection, Tool};
use op_core::catalog;
use op_core::*;

use crate::app::State;
use crate::effect_controls;
use crate::i18n::{t, tf, tn};
use crate::icons::{self, Icon};
use crate::theme;

// ---------------------------------------------------------------------------------- history

/// Translates a history step label ("Add Gaussian Blur" keeps the effect name translated).
pub fn step_label(l: &str) -> String {
    if let Some(rest) = l.strip_prefix("Add ")
        && catalog::CATALOG.iter().any(|d| d.name == rest)
    {
        return tf("Add {}", &[&tn(rest)]);
    }
    tn(l)
}

pub fn history(s: &mut State, ui: &mut Ui) {
    let (labels, current) = {
        let (l, c) = s.ed.history.labels();
        (l.into_iter().map(step_label).collect::<Vec<String>>(), c)
    };
    let mut jump = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            let rows = std::iter::once(t("Open").to_string()).chain(labels);
            for (i, label) in rows.enumerate() {
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
                let fill = if i == current {
                    theme::ACCENT_DIM
                } else if resp.hovered() {
                    theme::RAISED
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(r, 0.0, fill);
                let color = if i > current {
                    theme::TEXT_DIM
                } else {
                    theme::TEXT
                };
                ui.painter().text(
                    r.min + vec2(10.0, 10.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    color,
                );
                if resp.clicked() {
                    jump = Some(i);
                }
            }
        });
    if let Some(i) = jump {
        s.ed.history_jump(i);
    }
}

// ------------------------------------------------------------------------------------ tools

pub fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Selection => Icon::Selection,
        Tool::TrackSelectForward => Icon::TrackForward,
        Tool::TrackSelectBackward => Icon::TrackBackward,
        Tool::Ripple => Icon::Ripple,
        Tool::Roll => Icon::Roll,
        Tool::RateStretch => Icon::RateStretch,
        Tool::Razor => Icon::Razor,
        Tool::Slip => Icon::Slip,
        Tool::Slide => Icon::Slide,
        Tool::Pen => Icon::Pen,
        Tool::Hand => Icon::Hand,
        Tool::Zoom => Icon::Zoom,
        Tool::Type => Icon::Type,
    }
}

pub fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Selection => "Selection Tool",
        Tool::TrackSelectForward => "Track Select Forward Tool",
        Tool::TrackSelectBackward => "Track Select Backward Tool",
        Tool::Ripple => "Ripple Edit Tool",
        Tool::Roll => "Rolling Edit Tool",
        Tool::RateStretch => "Rate Stretch Tool",
        Tool::Razor => "Razor Tool",
        Tool::Slip => "Slip Tool",
        Tool::Slide => "Slide Tool",
        Tool::Pen => "Pen Tool",
        Tool::Hand => "Hand Tool",
        Tool::Zoom => "Zoom Tool",
        Tool::Type => "Type Tool",
    }
}

pub fn tools(s: &mut State, ui: &mut Ui) {
    let full = ui.max_rect();
    let vertical = full.height() > full.width();
    // the strip has no tab: its top edge is the handle that moves the panel
    let r = if vertical {
        Rect::from_min_max(pos2(full.min.x, full.min.y + 10.0), full.max)
    } else {
        full
    };
    let layout = if vertical {
        egui::Layout::top_down(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(r.shrink(3.0))
            .layout(layout),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            for tool in Tool::ALL {
                let key =
                    s.ed.keymap
                        .keys_for(tool.command())
                        .map(|k| format!(" ({k})"))
                        .unwrap_or_default();
                if icons::button(
                    ui,
                    tool_icon(tool),
                    26.0,
                    s.ed.tool == tool,
                    &format!("{}{key}", t(tool_name(tool))),
                )
                .clicked()
                {
                    s.ed.tool = tool;
                }
            }
        },
    );
}

// ----------------------------------------------------------------------------------- meters

fn to_db(v: f32) -> f32 {
    if v <= 1e-6 { -96.0 } else { 20.0 * v.log10() }
}

#[derive(Default)]
pub struct MeterState {
    master: Vec<f32>,
    hold: Vec<(f32, Option<Instant>)>,
    tracks: HashMap<TrackId, [f32; 2]>,
    last: Option<Instant>,
}

impl MeterState {
    pub fn update(&mut self, ed: &Editor) {
        let now = Instant::now();
        let dt = self
            .last
            .map(|l| (now - l).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.5);
        self.last = Some(now);
        let fall = 30.0 * dt;
        let channels = ed
            .active_seq()
            .map(|q| q.settings.master.channels())
            .unwrap_or(2);
        let peaks = ed.playback.meters.take_master(channels);
        self.master.resize(channels, -96.0);
        self.hold.resize(channels, (-96.0, None));
        for (i, p) in peaks.iter().enumerate() {
            let db = to_db(*p);
            self.master[i] = db.max(self.master[i] - fall);
            if db >= self.hold[i].0
                || self.hold[i]
                    .1
                    .is_none_or(|t| t.elapsed().as_secs_f32() > 1.5)
            {
                self.hold[i] = (db.max(self.master[i]), Some(now));
            }
        }
        let tr = ed.playback.meters.take_tracks();
        for v in self.tracks.values_mut() {
            v[0] -= fall;
            v[1] -= fall;
        }
        for (id, [l, r]) in tr {
            let e = self.tracks.entry(id).or_insert([-96.0; 2]);
            e[0] = to_db(l).max(e[0]);
            e[1] = to_db(r).max(e[1]);
        }
    }

    pub fn active(&self) -> bool {
        self.master.iter().any(|d| *d > -70.0)
            || self.tracks.values().any(|v| v[0] > -70.0 || v[1] > -70.0)
    }
}

fn meter_color(db: f32) -> Color32 {
    if db > -3.0 {
        Color32::from_rgb(230, 70, 60)
    } else if db > -12.0 {
        Color32::from_rgb(230, 200, 60)
    } else {
        Color32::from_rgb(70, 200, 100)
    }
}

/// A vertical meter bar from -60 dB (bottom) to 0 dB (top).
fn meter_bar(p: &egui::Painter, r: Rect, db: f32, hold: Option<f32>) {
    p.rect_filled(r, 1.0, Color32::from_gray(14));
    let f = |d: f32| ((d + 60.0) / 60.0).clamp(0.0, 1.0);
    let top = r.max.y - f(db) * r.height();
    // segments so the colors band like hardware meters
    let mut y = r.max.y;
    while y > top {
        let seg_top = (y - 3.0).max(top);
        let d = -60.0 + (r.max.y - seg_top) / r.height() * 60.0;
        p.rect_filled(
            Rect::from_min_max(pos2(r.min.x, seg_top), pos2(r.max.x, y - 1.0)),
            0.0,
            meter_color(d),
        );
        y -= 4.0;
    }
    if let Some(h) = hold
        && h > -60.0
    {
        let hy = r.max.y - f(h) * r.height();
        p.line_segment(
            [pos2(r.min.x, hy), pos2(r.max.x, hy)],
            Stroke::new(1.5, meter_color(h)),
        );
    }
}

pub fn meters(s: &mut State, ui: &mut Ui) {
    let r = ui.max_rect().shrink(4.0);
    let n = s.meters.master.len().max(1);
    let scale_w = 22.0;
    let bar_w = ((r.width() - scale_w) / n as f32 - 2.0).clamp(4.0, 18.0);
    let p = ui.painter();
    let area = Rect::from_min_max(pos2(r.min.x, r.min.y + 4.0), pos2(r.max.x, r.max.y - 18.0));
    for db in [0, -6, -12, -18, -24, -30, -36, -42, -48, -54] {
        let y = area.max.y - ((db as f32 + 60.0) / 60.0) * area.height();
        p.text(
            pos2(area.min.x + scale_w - 3.0, y),
            Align2::RIGHT_CENTER,
            db.to_string(),
            FontId::proportional(9.0),
            theme::TEXT_DIM,
        );
    }
    for i in 0..n {
        let x = area.min.x + scale_w + i as f32 * (bar_w + 2.0);
        let br = Rect::from_min_max(pos2(x, area.min.y), pos2(x + bar_w, area.max.y));
        meter_bar(
            p,
            br,
            s.meters.master.get(i).copied().unwrap_or(-96.0),
            s.meters.hold.get(i).map(|h| h.0),
        );
    }
    let names = ["L", "R", "C", "LFE", "Ls", "Rs"];
    for (i, name) in names.iter().enumerate().take(n.min(6)) {
        let x = area.min.x + scale_w + i as f32 * (bar_w + 2.0) + bar_w / 2.0;
        p.text(
            pos2(x, r.max.y - 8.0),
            Align2::CENTER_CENTER,
            if n == 1 { "M" } else { name },
            FontId::proportional(9.0),
            theme::TEXT_DIM,
        );
    }
}

// ------------------------------------------------------------------------------------ mixer

fn fader(ui: &mut Ui, id: egui::Id, db: &mut f64, h: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, h), Sense::click_and_drag());
    let p = ui.painter();
    let f = |d: f64| ((d + 60.0) / 75.0).clamp(0.0, 1.0) as f32;
    p.rect_filled(
        Rect::from_center_size(r.center(), vec2(4.0, r.height())),
        2.0,
        Color32::from_gray(20),
    );
    let zero = r.max.y - f(0.0) * r.height();
    p.line_segment(
        [pos2(r.min.x, zero), pos2(r.max.x, zero)],
        Stroke::new(1.0, theme::LINE),
    );
    let y = r.max.y - f(*db) * r.height();
    let knob = Rect::from_center_size(pos2(r.center().x, y), vec2(24.0, 10.0));
    p.rect_filled(
        knob,
        2.0,
        if resp.dragged() {
            theme::ACCENT
        } else {
            Color32::from_gray(170)
        },
    );
    p.line_segment(
        [knob.left_center(), knob.right_center()],
        Stroke::new(1.0, Color32::from_gray(60)),
    );
    let mut changed = false;
    if resp.dragged() {
        let dy = resp.drag_delta().y as f64;
        *db = (*db - dy * 75.0 / r.height() as f64).clamp(-96.0, 15.0);
        if *db < -60.0 && dy > 0.0 {
            *db = -96.0;
        }
        changed = true;
    }
    if resp.double_clicked() {
        *db = 0.0;
        changed = true;
    }
    let _ = id;
    changed
}

fn knob(ui: &mut Ui, v: &mut f64) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click_and_drag());
    let p = ui.painter();
    let c = r.center();
    p.circle_filled(c, 13.0, Color32::from_gray(45));
    p.circle_stroke(c, 13.0, Stroke::new(1.0, theme::LINE));
    let a = (*v as f32 / 100.0) * 2.35 - std::f32::consts::FRAC_PI_2;
    p.line_segment(
        [c, c + vec2(a.cos(), a.sin()) * 11.0],
        Stroke::new(2.0, theme::ACCENT),
    );
    let mut changed = false;
    if resp.dragged() {
        *v = (*v + resp.drag_delta().x as f64 - resp.drag_delta().y as f64).clamp(-100.0, 100.0);
        changed = true;
    }
    if resp.double_clicked() {
        *v = 0.0;
        changed = true;
    }
    changed
}

fn track_param(seq: &Sequence, r: TrackRef, effect: &str, key: &str, t: SeqTime) -> f64 {
    seq.track(r)
        .and_then(|tr| tr.components.iter().find(|c| c.effect == effect))
        .map(|c| c.f64_at(key, t.cast()))
        .unwrap_or(0.0)
}

fn set_track_param(
    s: &mut State,
    sid: SequenceId,
    r: TrackRef,
    effect: &'static str,
    key: &'static str,
    v: f64,
) {
    let t = s.ed.playhead();
    let merge = Some(format!("track-{}-{}-{key}", r.index, effect));
    s.ed.edit_merge("Track Volume", merge, move |p| {
        let missing = p
            .sequence(sid)
            .and_then(|q| q.track(r))
            .is_none_or(|tr| !tr.components.iter().any(|c| c.effect == effect));
        let new = if missing {
            let def = catalog::find(effect).ok_or(EditError::Nothing)?;
            let mut ids = p.ids.clone();
            let c = Component::new(def, &mut ids);
            p.ids = ids;
            Some(c)
        } else {
            None
        };
        let tr = p
            .sequence_mut(sid)
            .and_then(|q| q.track_mut(r))
            .ok_or(EditError::Nothing)?;
        if let Some(c) = new {
            tr.components.push(c);
        }
        let c = tr
            .components
            .iter_mut()
            .find(|c| c.effect == effect)
            .ok_or(EditError::Nothing)?;
        let prm = c.param_mut(key).ok_or(EditError::Nothing)?;
        prm.set_at(t.cast(), Value::Float(v), Interp::Linear);
        Ok(())
    });
}

pub fn mixer(s: &mut State, ui: &mut Ui) {
    let Some(sid) = s.ed.active else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t("No sequence open")).color(theme::TEXT_DIM))
        });
        return;
    };
    let seq = s.ed.project.sequence(sid).unwrap().clone();
    let t0 = s.ed.playhead();
    let h = (ui.max_rect().height() - 110.0).max(60.0);
    egui::ScrollArea::horizontal()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (i, tr) in seq.audio.iter().enumerate() {
                    let r = TrackRef::audio(i);
                    ui.vertical(|ui| {
                        ui.set_width(64.0);
                        let name = if tr.name.is_empty() {
                            format!("A{}", i + 1)
                        } else {
                            tr.name.clone()
                        };
                        ui.label(RichText::new(name).color(theme::TEXT_BRIGHT).size(11.5));
                        let mut pan = track_param(&seq, r, catalog::PANNER, "balance", t0);
                        if knob(ui, &mut pan) {
                            set_track_param(s, sid, r, catalog::PANNER, "balance", pan);
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (label, on) in [("M", tr.muted), ("S", tr.solo)] {
                                let fill = if on {
                                    if label == "M" {
                                        Color32::from_rgb(0x2f, 0x9e, 0x6a)
                                    } else {
                                        Color32::from_rgb(0xd4, 0xa0, 0x2a)
                                    }
                                } else {
                                    theme::RAISED
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(label).size(10.5))
                                            .fill(fill)
                                            .min_size(vec2(22.0, 18.0)),
                                    )
                                    .clicked()
                                {
                                    s.ed.edit(
                                        if label == "M" {
                                            "Mute Track"
                                        } else {
                                            "Solo Track"
                                        },
                                        |p| {
                                            let t = p
                                                .sequence_mut(sid)
                                                .and_then(|q| q.track_mut(r))
                                                .ok_or(EditError::Nothing)?;
                                            if label == "M" {
                                                t.muted = !t.muted;
                                            } else {
                                                t.solo = !t.solo;
                                            }
                                            Ok(())
                                        },
                                    );
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut db = track_param(&seq, r, catalog::VOLUME, "level", t0);
                            if fader(ui, ui.id().with(("fader", i)), &mut db, h) {
                                set_track_param(s, sid, r, catalog::VOLUME, "level", db);
                            }
                            let lv = s.meters.tracks.get(&tr.id).copied().unwrap_or([-96.0; 2]);
                            let (mr, _) = ui.allocate_exact_size(vec2(14.0, h), Sense::hover());
                            meter_bar(
                                ui.painter(),
                                Rect::from_min_max(mr.min, pos2(mr.center().x - 1.0, mr.max.y)),
                                lv[0],
                                None,
                            );
                            meter_bar(
                                ui.painter(),
                                Rect::from_min_max(pos2(mr.center().x + 1.0, mr.min.y), mr.max),
                                lv[1],
                                None,
                            );
                        });
                        let db = track_param(&seq, r, catalog::VOLUME, "level", t0);
                        ui.label(
                            RichText::new(format!("{db:.1} dB"))
                                .size(10.5)
                                .color(theme::VALUE),
                        );
                    });
                    ui.separator();
                }
                // master
                ui.vertical(|ui| {
                    ui.set_width(70.0);
                    ui.label(
                        RichText::new(t("Master"))
                            .color(theme::TEXT_BRIGHT)
                            .size(11.5),
                    );
                    ui.add_space(52.0);
                    ui.horizontal(|ui| {
                        let mut db = seq
                            .master
                            .iter()
                            .find(|c| c.effect == catalog::VOLUME)
                            .map(|c| c.f64_at("level", t0.cast()))
                            .unwrap_or(0.0);
                        if fader(ui, ui.id().with("master"), &mut db, h) {
                            let merge = Some("master-volume".to_string());
                            s.ed.edit_merge("Master Volume", merge, move |p| {
                                let q = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                                let c = q
                                    .master
                                    .iter_mut()
                                    .find(|c| c.effect == catalog::VOLUME)
                                    .ok_or(EditError::Nothing)?;
                                c.param_mut("level").ok_or(EditError::Nothing)?.set_at(
                                    t0.cast(),
                                    Value::Float(db),
                                    Interp::Linear,
                                );
                                Ok(())
                            });
                        }
                        let (mr, _) = ui.allocate_exact_size(vec2(16.0, h), Sense::hover());
                        let l = s.meters.master.first().copied().unwrap_or(-96.0);
                        let rr = s.meters.master.get(1).copied().unwrap_or(l);
                        meter_bar(
                            ui.painter(),
                            Rect::from_min_max(mr.min, pos2(mr.center().x - 1.0, mr.max.y)),
                            l,
                            None,
                        );
                        meter_bar(
                            ui.painter(),
                            Rect::from_min_max(pos2(mr.center().x + 1.0, mr.min.y), mr.max),
                            rr,
                            None,
                        );
                    });
                });
            });
        });
}

// ---------------------------------------------------------------------------------- markers

pub fn markers(s: &mut State, ui: &mut Ui) {
    let Some(sid) = s.ed.active else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(t("No sequence open")).color(theme::TEXT_DIM))
        });
        return;
    };
    let seq = s.ed.project.sequence(sid).unwrap().clone();
    let (fmt, offset) = s.timecode(Monitor::Program);
    if seq.markers.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t("No markers. Press M to add one at the playhead."))
                    .color(theme::TEXT_DIM),
            )
        });
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(4.0);
            for m in &seq.markers {
                let id = m.id;
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let [r, g, b] = m.color.rgb();
                    let color = Color32::from_rgb(r, g, b);
                    ui.menu_button(RichText::new("\u{25A0}").color(color), |ui| {
                        for c in MarkerColor::ALL {
                            let [r, g, b] = c.rgb();
                            if ui
                                .button(
                                    RichText::new("\u{25A0}")
                                        .color(Color32::from_rgb(r, g, b))
                                        .size(16.0),
                                )
                                .clicked()
                            {
                                s.ed.seq_edit("Marker Color", |p, sid, _| {
                                    let q = p.sequence_mut(sid).unwrap();
                                    q.markers
                                        .iter_mut()
                                        .find(|x| x.id == id)
                                        .ok_or(EditError::Nothing)?
                                        .color = c;
                                    Ok(())
                                });
                                ui.close();
                            }
                        }
                    });
                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new(fmt.format(m.start.since_zero() + offset))
                                    .monospace()
                                    .color(theme::VALUE),
                            )
                            .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        s.ed.stop();
                        s.ed.set_playhead(m.start);
                    }
                    let mut name = m.name.clone();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text(t("Name"))
                                .desired_width(120.0),
                        )
                        .changed()
                    {
                        let merge = Some(format!("marker-name-{}", id.0));
                        s.ed.edit_merge("Marker", merge, |p| {
                            let q = p.sequence_mut(sid).unwrap();
                            q.markers
                                .iter_mut()
                                .find(|x| x.id == id)
                                .ok_or(EditError::Nothing)?
                                .name = name;
                            Ok(())
                        });
                    }
                    let mut comment = m.comment.clone();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut comment)
                                .hint_text(t("Comment"))
                                .desired_width((ui.available_width() - 30.0).max(60.0)),
                        )
                        .changed()
                    {
                        let merge = Some(format!("marker-comment-{}", id.0));
                        s.ed.edit_merge("Marker", merge, |p| {
                            let q = p.sequence_mut(sid).unwrap();
                            q.markers
                                .iter_mut()
                                .find(|x| x.id == id)
                                .ok_or(EditError::Nothing)?
                                .comment = comment;
                            Ok(())
                        });
                    }
                    if icons::button(ui, Icon::Trash, 18.0, false, t("Clear")).clicked() {
                        s.ed.seq_edit("Clear Marker", |p, sid, _| {
                            let q = p.sequence_mut(sid).unwrap();
                            q.markers.retain(|x| x.id != id);
                            Ok(())
                        });
                    }
                });
            }
        });
}

// ------------------------------------------------------------------------------------- info

pub fn info(s: &mut State, ui: &mut Ui) {
    let (fmt, offset) = s.timecode(Monitor::Program);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(6.0);
            egui::Grid::new("info")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    let mut row = |k: &str, v: String| {
                        ui.label(RichText::new(k).color(theme::TEXT_DIM));
                        ui.add(egui::Label::new(v.clone()).truncate())
                            .on_hover_text(v);
                        ui.end_row();
                    };
                    if let Some(seq) = s.ed.active_seq() {
                        row(t("Sequence"), seq.name.clone());
                        row(
                            t("Frame Size"),
                            format!("{} x {}", seq.settings.width, seq.settings.height),
                        );
                        row(t("Frame Rate"), seq.rate().label());
                        row(t("Duration"), fmt.format(seq.duration()));
                        row(
                            t("Playhead"),
                            fmt.format(s.ed.playhead().since_zero() + offset),
                        );
                        if let Some(c) = s.ed.selection.clips.first().and_then(|c| seq.clip(*c)) {
                            row("", String::new());
                            row(t("Clip"), c.name.clone());
                            row(t("Source"), s.ed.project.source_name(&c.source));
                            row(t("Start"), fmt.format(c.start.since_zero() + offset));
                            row(t("End"), fmt.format(c.end().since_zero() + offset));
                            row(t("Duration"), fmt.format(c.duration));
                            row(
                                t("Speed"),
                                format!(
                                    "{:.2} %{}",
                                    c.speed.percent(),
                                    if c.reverse {
                                        format!(" ({})", t("reversed"))
                                    } else {
                                        String::new()
                                    }
                                ),
                            );
                            if !c.is_video() {
                                row(t("Gain"), format!("{:.1} dB", c.gain_db));
                            }
                        }
                    }
                    if let Some(item) = s.ed.items.first().and_then(|i| s.ed.project.item(*i)) {
                        row("", String::new());
                        row(t("Item"), item.name.clone());
                        if let ItemKind::Media { asset, .. } = item.kind
                            && let Some(a) = s.ed.project.asset(asset)
                        {
                            row(t("File"), a.path.clone());
                            if let Some(v) = &a.video {
                                row(
                                    t("Video"),
                                    format!(
                                        "{} {}x{} {} {}-bit",
                                        v.codec,
                                        v.width,
                                        v.height,
                                        v.rate.label(),
                                        v.bit_depth
                                    ),
                                );
                            }
                            for st in &a.audio {
                                row(
                                    t("Audio"),
                                    format!(
                                        "{} {} Hz, {} ch",
                                        st.codec,
                                        st.sample_rate,
                                        st.layout.channels()
                                    ),
                                );
                            }
                            row(
                                t("Size"),
                                format!("{:.1} MB", a.file_size as f64 / 1_048_576.0),
                            );
                        }
                    }
                    let (dev, rate, _) = s.ed.playback.device_info();
                    row("", String::new());
                    row(t("Audio Device"), format!("{dev} ({rate} Hz)"));
                    row(t("GPU"), s.gpu.description());
                    row(t("FFmpeg"), op_media::ffmpeg_version());
                });
        });
}

// ---------------------------------------------------------------------------------- lumetri

fn selected_video_clip(s: &State) -> Option<(SequenceId, ClipId)> {
    let sid = s.ed.active?;
    let seq = s.ed.project.sequence(sid)?;
    let ph = s.ed.playhead();
    let clips: Vec<&Clip> =
        s.ed.selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .filter(|c| c.is_video())
            .collect();
    let c = clips
        .iter()
        .find(|c| c.range().contains(ph))
        .or(clips.first())?;
    Some((sid, c.id))
}

pub fn lumetri(s: &mut State, ui: &mut Ui) {
    let Some((sid, clip)) = selected_video_clip(s) else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t("Select a video clip to color correct it")).color(theme::TEXT_DIM),
            )
        });
        return;
    };
    let comp =
        s.ed.project
            .sequence(sid)
            .and_then(|q| q.clip(clip))
            .and_then(|c| c.component("op.video.lumetri"))
            .map(|c| c.id);
    match comp {
        None => {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                if ui.button(t("Add Lumetri Color")).clicked() {
                    s.ed.apply_effect("op.video.lumetri", Some(vec![clip]));
                }
            });
        }
        Some(comp) => {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    effect_controls::component_panel(s, ui, sid, clip, comp);
                });
        }
    }
}

// ----------------------------------------------------------------------------------- scopes

pub struct ScopesView {
    pub kind: op_render::ScopeKind,
    pub gain: f32,
}

impl Default for ScopesView {
    fn default() -> Self {
        ScopesView {
            kind: op_render::ScopeKind::Waveform,
            gain: 1.0,
        }
    }
}

fn scope_name(k: op_render::ScopeKind) -> &'static str {
    match k {
        op_render::ScopeKind::Waveform => "Waveform (Luma)",
        op_render::ScopeKind::Parade => "RGB Parade",
        op_render::ScopeKind::Vectorscope => "Vectorscope YUV",
        op_render::ScopeKind::Histogram => "Histogram",
    }
}

pub fn scopes(s: &mut State, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let before = (s.scopes.kind, s.scopes.gain);
        egui::ComboBox::from_id_salt("scope-kind")
            .selected_text(t(scope_name(s.scopes.kind)))
            .show_ui(ui, |ui| {
                for k in op_render::ScopeKind::ALL {
                    ui.selectable_value(&mut s.scopes.kind, k, t(scope_name(k)));
                }
            });
        ui.label(t("Brightness"));
        ui.add(
            egui::Slider::new(&mut s.scopes.gain, 0.25..=4.0)
                .logarithmic(true)
                .show_value(false),
        );
        if before != (s.scopes.kind, s.scopes.gain) {
            s.ed.frame_generation += 1;
        }
    });
    let r = ui.available_rect_before_wrap().shrink(6.0);
    ui.painter().rect_filled(r, 2.0, Color32::BLACK);
    match s.program.scope_texture() {
        Some((id, [w, h])) => {
            let aspect = w as f32 / h as f32;
            let size = if r.width() / r.height() > aspect {
                vec2(r.height() * aspect, r.height())
            } else {
                vec2(r.width(), r.width() / aspect)
            };
            let img = Rect::from_center_size(r.center(), size);
            ui.painter().image(
                id,
                img,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            // graticule labels
            if matches!(
                s.scopes.kind,
                op_render::ScopeKind::Waveform | op_render::ScopeKind::Parade
            ) {
                for v in [0, 25, 50, 75, 100] {
                    let y = img.max.y - v as f32 / 100.0 * img.height();
                    ui.painter().line_segment(
                        [pos2(img.min.x, y), pos2(img.max.x, y)],
                        Stroke::new(1.0, Color32::from_white_alpha(25)),
                    );
                    ui.painter().text(
                        pos2(img.min.x + 2.0, y),
                        Align2::LEFT_BOTTOM,
                        v.to_string(),
                        FontId::proportional(9.0),
                        Color32::from_white_alpha(120),
                    );
                }
            }
            ui.painter()
                .rect_stroke(img, 0.0, Stroke::new(1.0, theme::LINE), StrokeKind::Outside);
        }
        None => {
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                t("Scopes show the Program Monitor frame"),
                FontId::proportional(12.0),
                theme::TEXT_DIM,
            );
        }
    }
}

// --------------------------------------------------------------------------------- graphics

#[derive(Default)]
pub struct GraphicsView {
    layer: Option<ComponentId>,
}

pub fn graphics(s: &mut State, ui: &mut Ui) {
    let sel = selected_video_clip(s).filter(|(sid, c)| {
        s.ed.project
            .sequence(*sid)
            .and_then(|q| q.clip(*c))
            .is_some_and(|c| matches!(c.source, ClipSource::Graphic))
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(t("New Layer")).color(theme::TEXT_DIM));
        for (label, effect, shape) in [
            ("Text", catalog::TEXT, None),
            ("Rectangle", catalog::SHAPE, Some(0u32)),
            ("Ellipse", catalog::SHAPE, Some(1)),
        ] {
            if ui.button(t(label)).clicked() {
                match sel {
                    Some((sid, clip)) => {
                        let default_font = op_render::text::Fonts::default_family().to_string();
                        let new = s.ed.seq_edit("New Layer", |p, _, _| {
                            let def = catalog::find(effect).ok_or(EditError::Nothing)?;
                            let mut ids = p.ids.clone();
                            let mut comp = Component::new(def, &mut ids);
                            if let Some(v) = shape
                                && let Some(prm) = comp.param_mut("shape")
                            {
                                prm.value = Value::Choice(v);
                            }
                            if let Some(f) = comp.param_mut("font") {
                                f.value = Value::Text(default_font);
                            }
                            let id = comp.id;
                            p.ids = ids;
                            p.sequence_mut(sid)
                                .unwrap()
                                .clip_mut(clip)
                                .ok_or(EditError::Nothing)?
                                .components
                                .push(comp);
                            Ok(id)
                        });
                        if new.is_some() {
                            s.graphics.layer = new;
                        }
                    }
                    None => {
                        s.ed.add_graphic(effect, shape);
                        s.graphics.layer = None;
                    }
                }
            }
        }
    });
    ui.separator();
    let Some((sid, clip)) = sel else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t("Select a graphic clip, or add a new layer"))
                    .color(theme::TEXT_DIM),
            )
        });
        return;
    };
    let Some(c) =
        s.ed.project
            .sequence(sid)
            .and_then(|q| q.clip(clip))
            .cloned()
    else {
        return;
    };
    let layers: Vec<&Component> = c
        .components
        .iter()
        .filter(|x| x.effect == catalog::TEXT || x.effect == catalog::SHAPE)
        .collect();
    if s.graphics
        .layer
        .is_none_or(|l| !layers.iter().any(|x| x.id == l))
    {
        s.graphics.layer = layers.last().map(|x| x.id);
    }
    ui.label(RichText::new(t("Layers")).color(theme::TEXT_DIM));
    for l in layers.iter().rev() {
        let name = if l.effect == catalog::TEXT {
            let text = l.value_at("text", SrcTime::ZERO);
            let first = text.as_text().lines().next().unwrap_or("").to_string();
            if first.is_empty() {
                t("Text").to_string()
            } else {
                first
            }
        } else if l.value_at("shape", SrcTime::ZERO).as_choice() == 1 {
            t("Ellipse").to_string()
        } else {
            t("Rectangle").to_string()
        };
        let on = s.graphics.layer == Some(l.id);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            icons::draw(
                ui.painter(),
                Rect::from_min_size(ui.cursor().min, vec2(16.0, 16.0)),
                if l.effect == catalog::TEXT {
                    Icon::Type
                } else {
                    Icon::Grid
                },
                theme::TEXT_DIM,
            );
            ui.add_space(20.0);
            if ui.selectable_label(on, name).clicked() {
                s.graphics.layer = Some(l.id);
            }
        });
    }
    ui.separator();
    if let Some(layer) = s.graphics.layer {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(t("Align")).color(theme::TEXT_DIM));
            let pos = c
                .component_by_id(layer)
                .map(|x| {
                    x.value_at(
                        "position",
                        c.to_source(s.ed.playhead().clamp(c.start, c.end() - Dur(1))),
                    )
                    .as_point()
                })
                .unwrap_or([0.5, 0.5]);
            let set = |s: &mut State, v: [f64; 2]| {
                let ts = c.to_source(s.ed.playhead().clamp(c.start, c.end() - Dur(1)));
                s.ed.edit("Align", |p| {
                    effect_controls::write_param(
                        p,
                        sid,
                        clip,
                        layer,
                        "position",
                        Value::Point(v),
                        ts,
                    )
                });
            };
            if ui.button(t("Center Horizontally")).clicked() {
                set(s, [0.5, pos[1]]);
            }
            if ui.button(t("Center Vertically")).clicked() {
                set(s, [pos[0], 0.5]);
            }
            if ui.button(t("Remove Layer")).clicked() {
                s.ed.seq_edit("Remove Layer", |p, _, _| {
                    let cl = p
                        .sequence_mut(sid)
                        .unwrap()
                        .clip_mut(clip)
                        .ok_or(EditError::Nothing)?;
                    cl.components.retain(|x| x.id != layer);
                    Ok(())
                });
            }
        });
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                effect_controls::component_panel(s, ui, sid, clip, layer);
            });
    }
    let _ = Selection::default();
}
