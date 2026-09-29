//! Effect Controls: the components of the selected clip or transition, their parameters,
//! animation (stopwatch, keyframe navigation, a keyframe lane) and the parameter editors shared
//! with the Lumetri Color and Essential Graphics panels.

use egui::{
    Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use op_core::catalog::{self, EffectDef, ParamKind, ParamSpec, PointSpace};
use op_core::*;

use crate::app::{Drag, State};
use crate::i18n::{t, tn};
use crate::icons::{self, Icon};
use crate::{theme, widgets};

const ROW_H: f32 = 24.0;

#[derive(Default)]
pub struct EcView {
    fonts: Vec<String>,
    kf_drag: Option<(ComponentId, String, usize)>,
    curve_drag: Option<usize>,
}

/// Writes a parameter value at a source time: a keyframe when animated, the value otherwise.
pub fn write_param(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    comp: ComponentId,
    key: &str,
    v: Value,
    t: SrcTime,
) -> EditResult {
    let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
    let c = s.clip_mut(clip).ok_or(EditError::Nothing)?;
    let comp = c.component_by_id_mut(comp).ok_or(EditError::Nothing)?;
    let v = comp
        .def()
        .and_then(|d| d.param(key))
        .map(|sp| sp.clamp(v.clone()))
        .unwrap_or(v);
    if comp.param(key).is_none() {
        comp.complete_params();
    }
    let prm = comp.param_mut(key).ok_or(EditError::Nothing)?;
    if prm.value_at(t) == v {
        return Err(EditError::Nothing);
    }
    prm.set_at(t, v, Interp::Linear);
    Ok(())
}

fn with_component(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    comp: ComponentId,
    f: impl FnOnce(&mut Component) -> EditResult,
) -> EditResult {
    let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
    let c = s.clip_mut(clip).ok_or(EditError::Nothing)?;
    let comp = c.component_by_id_mut(comp).ok_or(EditError::Nothing)?;
    f(comp)
}

/// Everything parameter editors need to know about the clip.
pub struct ClipCtx {
    pub sid: SequenceId,
    pub clip: Clip,
    /// Source time the editors show and change.
    pub t: SrcTime,
    pub seq_size: [f64; 2],
    pub layer_size: [f64; 2],
}

impl ClipCtx {
    pub fn new(s: &State, sid: SequenceId, clip: ClipId) -> Option<ClipCtx> {
        let seq = s.ed.project.sequence(sid)?;
        let clip = seq.clip(clip)?.clone();
        let ph = s.ed.playhead();
        let t = clip.to_source(ph.clamp(clip.start, clip.end() - Dur(1)));
        let (w, h) = s.ed.project.source_size(&clip.source, &seq.settings);
        Some(ClipCtx {
            sid,
            t,
            seq_size: [seq.settings.width as f64, seq.settings.height as f64],
            layer_size: [w.max(1) as f64, h.max(1) as f64],
            clip,
        })
    }
}

/// The keyframe lane on the right side of the panel.
pub struct Lane {
    x0: f32,
    x1: f32,
    start: SeqTime,
    end: SeqTime,
}

impl Lane {
    fn x(&self, t: SeqTime) -> f32 {
        let len = (self.end - self.start).seconds().max(1e-9);
        self.x0 + ((t - self.start).seconds() / len) as f32 * (self.x1 - self.x0)
    }
    fn t(&self, x: f32) -> SeqTime {
        let len = (self.end - self.start).seconds();
        self.start
            + Dur::from_seconds(
                (((x - self.x0) / (self.x1 - self.x0)).clamp(0.0, 1.0) as f64) * len,
            )
    }
}

fn commit(s: &mut State, cx: &ClipCtx, comp: ComponentId, key: &str, v: Value) {
    let (sid, clip, t) = (cx.sid, cx.clip.id, cx.t);
    let merge = Some(format!("fx-{}-{}-{key}", clip.0, comp.0));
    let key = key.to_string();
    s.ed.edit_merge("Change Parameter", merge, move |p| {
        write_param(p, sid, clip, comp, &key, v, t)
    });
}

pub fn show(s: &mut State, ui: &mut Ui) {
    let full = ui.max_rect();
    // effects dropped on the panel go to the clip it shows
    let dropped = ui.rect_contains_pointer(full) && ui.input(|i| i.pointer.any_released());
    let Some(sid) = s.ed.active else {
        empty(ui, full, t("No sequence open"));
        return;
    };
    if let Some(tid) = s.ed.selection.transitions.first().copied() {
        transition_ui(s, ui, sid, tid);
        return;
    }
    let seq = s.ed.project.sequence(sid).unwrap();
    let ph = s.ed.playhead();
    let clip =
        s.ed.selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .find(|c| c.is_video() && c.range().contains(ph))
            .or_else(|| {
                s.ed.selection
                    .clips
                    .iter()
                    .filter_map(|c| seq.clip(*c))
                    .find(|c| c.is_video())
            })
            .or_else(|| s.ed.selection.clips.first().and_then(|c| seq.clip(*c)))
            .map(|c| c.id);
    let Some(clip) = clip else {
        empty(ui, full, t("Select a clip to see its effects"));
        return;
    };
    if dropped && let Some(Drag::Effect(e)) = s.drag.clone() {
        s.drag = None;
        s.ed.apply_effect(e, Some(vec![clip]));
    }
    let Some(cx) = ClipCtx::new(s, sid, clip) else {
        return;
    };
    let seq_name =
        s.ed.active_seq()
            .map(|q| q.name.clone())
            .unwrap_or_default();
    let split = full.min.x + (full.width() * 0.6).max(260.0).min(full.width() - 60.0);
    let lane = Lane {
        x0: split + 8.0,
        x1: full.max.x - 8.0,
        start: cx.clip.start,
        end: cx.clip.end(),
    };
    // header
    let header = Rect::from_min_size(full.min, vec2(full.width(), 22.0));
    ui.painter().rect_filled(header, 0.0, theme::PANEL_DARK);
    ui.painter()
        .with_clip_rect(Rect::from_min_max(header.min, pos2(split, header.max.y)))
        .text(
            header.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{seq_name} \u{2022} {}", cx.clip.name),
            FontId::proportional(12.0),
            theme::TEXT_BRIGHT,
        );
    lane_ruler(
        s,
        ui,
        &lane,
        Rect::from_min_max(pos2(split, header.min.y), header.max),
    );
    let body = Rect::from_min_max(pos2(full.min.x, header.max.y), full.max);
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(split, body.min.y), body.max),
        0.0,
        theme::PANEL_DARK,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.set_width(body.width());
                let comps: Vec<Component> = cx.clip.components.clone();
                let video = cx.clip.is_video();
                ui.add_space(2.0);
                widgets::dim_label(
                    ui,
                    if video {
                        format!("  {}", t("Video"))
                    } else {
                        format!("  {}", t("Audio"))
                    },
                );
                for (i, comp) in comps.iter().enumerate() {
                    component_ui(s, ui, &cx, comp, i, comps.len(), Some(&lane), split);
                }
                ui.add_space(40.0);
            });
    });
    // playhead in the lane
    let x = lane.x(ph.clamp(lane.start, lane.end));
    ui.painter()
        .with_clip_rect(Rect::from_min_max(pos2(split, full.min.y), full.max))
        .line_segment(
            [pos2(x, full.min.y), pos2(x, full.max.y)],
            Stroke::new(1.0, theme::PLAYHEAD),
        );
}

fn empty(ui: &Ui, r: Rect, text: &str) {
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.5),
        theme::TEXT_DIM,
    );
}

fn lane_ruler(s: &mut State, ui: &mut Ui, lane: &Lane, r: Rect) {
    let resp = ui.interact(r, ui.id().with("ec-ruler"), Sense::click_and_drag());
    if (resp.clicked() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos()
    {
        s.ed.stop();
        s.ed.set_playhead(lane.t(p.x));
    }
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(lane.x0, r.min.y + 4.0), pos2(lane.x1, r.max.y - 4.0)),
        2.0,
        theme::RAISED,
    );
}

/// One component with its header and parameters. `lane` adds keyframe lanes right of `split`.
fn component_ui(
    s: &mut State,
    ui: &mut Ui,
    cx: &ClipCtx,
    comp: &Component,
    index: usize,
    count: usize,
    lane: Option<&Lane>,
    split: f32,
) {
    let def = comp.def();
    let name = def
        .map(|d| tn(d.name))
        .unwrap_or_else(|| comp.foreign.clone().unwrap_or_else(|| comp.effect.clone()));
    let open_id = ui.id().with(("comp-open", comp.id.0));
    let mut open: bool = ui
        .data(|d| d.get_temp(open_id))
        .unwrap_or(comp.is_fixed() || count <= 4);
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    ui.painter().rect_filled(
        Rect::from_min_max(row.min, pos2(split, row.max.y)),
        0.0,
        theme::RAISED,
    );
    // fx toggle
    let fx = Rect::from_min_size(row.min + vec2(6.0, 4.0), vec2(18.0, 16.0));
    let fx_resp = ui.interact(fx, ui.id().with(("fx", comp.id.0)), Sense::click());
    ui.painter().rect_stroke(
        fx,
        3.0,
        Stroke::new(
            1.0,
            if comp.enabled {
                theme::TEXT
            } else {
                theme::LINE
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter().text(
        fx.center(),
        Align2::CENTER_CENTER,
        "fx",
        FontId::proportional(10.0),
        if comp.enabled {
            theme::TEXT_BRIGHT
        } else {
            theme::TEXT_DIM
        },
    );
    if fx_resp
        .on_hover_text(t("Toggle the effect on or off"))
        .clicked()
    {
        let (sid, clip, id) = (cx.sid, cx.clip.id, comp.id);
        s.ed.edit("Toggle Effect", |p| {
            with_component(p, sid, clip, id, |c| {
                c.enabled = !c.enabled;
                Ok(())
            })
        });
    }
    icons::draw(
        ui.painter(),
        Rect::from_min_size(row.min + vec2(28.0, 4.0), vec2(16.0, 16.0)),
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        theme::TEXT_DIM,
    );
    ui.painter().text(
        row.min + vec2(48.0, ROW_H / 2.0),
        Align2::LEFT_CENTER,
        &name,
        FontId::proportional(12.5),
        if comp.enabled {
            theme::TEXT_BRIGHT
        } else {
            theme::TEXT_DIM
        },
    );
    if def.is_none() {
        ui.painter().text(
            pos2(split - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            t("not supported"),
            FontId::proportional(11.0),
            theme::WARN,
        );
    }
    // reset
    let reset = Rect::from_center_size(pos2(split - 16.0, row.center().y), vec2(18.0, 18.0));
    if def.is_some() {
        let rr = ui.interact(reset, ui.id().with(("reset", comp.id.0)), Sense::click());
        icons::draw(
            ui.painter(),
            reset.shrink(2.0),
            Icon::Reset,
            if rr.hovered() {
                theme::TEXT_BRIGHT
            } else {
                theme::TEXT_DIM
            },
        );
        if rr.on_hover_text(t("Reset")).clicked() {
            let (sid, clip, id) = (cx.sid, cx.clip.id, comp.id);
            s.ed.edit("Reset Effect", |p| {
                with_component(p, sid, clip, id, |c| {
                    let Some(d) = c.def() else {
                        return Err(EditError::Nothing);
                    };
                    c.params = d
                        .params
                        .iter()
                        .map(|sp| Param::new(sp.key, sp.default_value()))
                        .collect();
                    Ok(())
                })
            });
        }
    }
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(open_id, open));
    }
    resp.context_menu(|ui| {
        let (sid, clip, id) = (cx.sid, cx.clip.id, comp.id);
        if !comp.is_fixed() {
            if ui.button(t("Remove Effect")).clicked() {
                s.ed.seq_edit("Remove Effect", |p, _, _| {
                    let c = p
                        .sequence_mut(sid)
                        .unwrap()
                        .clip_mut(clip)
                        .ok_or(EditError::Nothing)?;
                    c.components.retain(|x| x.id != id);
                    Ok(())
                });
                ui.close();
            }
            let fixed = cx
                .clip
                .components
                .iter()
                .take_while(|c| c.is_fixed())
                .count();
            for (label, dir) in [(t("Move Up"), -1i32), (t("Move Down"), 1)] {
                let to = index as i32 + dir;
                let enabled = to >= fixed as i32 && (to as usize) < count;
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    s.ed.seq_edit("Reorder Effects", |p, _, _| {
                        let c = p
                            .sequence_mut(sid)
                            .unwrap()
                            .clip_mut(clip)
                            .ok_or(EditError::Nothing)?;
                        c.components.swap(index, to as usize);
                        Ok(())
                    });
                    ui.close();
                }
            }
        }
        if ui.button(t("Reset")).clicked() {
            s.ed.edit("Reset Effect", |p| {
                with_component(p, sid, clip, id, |c| {
                    let Some(d) = c.def() else {
                        return Err(EditError::Nothing);
                    };
                    c.params = d
                        .params
                        .iter()
                        .map(|sp| Param::new(sp.key, sp.default_value()))
                        .collect();
                    Ok(())
                })
            });
            ui.close();
        }
    });
    if !open {
        return;
    }
    if let Some(def) = def {
        params_ui(s, ui, cx, comp, def, lane, split);
    }
}

/// The parameters of a component, folded by group.
pub fn params_ui(
    s: &mut State,
    ui: &mut Ui,
    cx: &ClipCtx,
    comp: &Component,
    def: &'static EffectDef,
    lane: Option<&Lane>,
    split: f32,
) {
    let mut i = 0;
    while i < def.params.len() {
        let group = def.params[i].group;
        let end = if group.is_empty() {
            i + 1
        } else {
            i + def.params[i..]
                .iter()
                .take_while(|p| p.group == group)
                .count()
        };
        let visible = if group.is_empty() {
            true
        } else {
            let id = ui.id().with(("group", comp.id.0, group));
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.set_width(split - ui.min_rect().min.x - 4.0);
                widgets::section(ui, id, &tn(group), def.id != "op.video.lumetri")
            })
            .inner
        };
        if visible {
            for spec in &def.params[i..end] {
                param_row(
                    s,
                    ui,
                    cx,
                    comp,
                    spec,
                    lane,
                    split,
                    if group.is_empty() { 20.0 } else { 34.0 },
                );
            }
        }
        i = end;
    }
}

fn row_height(spec: &ParamSpec) -> f32 {
    match spec.kind {
        ParamKind::Text {
            multiline: true, ..
        } => 70.0,
        ParamKind::Curve => 150.0,
        ParamKind::Point {
            space: PointSpace::Wheel,
            ..
        } => 96.0,
        _ => ROW_H,
    }
}

#[allow(clippy::too_many_arguments)]
fn param_row(
    s: &mut State,
    ui: &mut Ui,
    cx: &ClipCtx,
    comp: &Component,
    spec: &'static ParamSpec,
    lane: Option<&Lane>,
    split: f32,
    indent: f32,
) {
    let h = row_height(spec);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let left = Rect::from_min_max(row.min, pos2(split, row.max.y));
    let prm = comp
        .param(spec.key)
        .cloned()
        .unwrap_or_else(|| Param::new(spec.key, spec.default_value()));
    let value = prm.value_at(cx.t);
    let animatable = spec.animatable
        && !matches!(
            spec.kind,
            ParamKind::Text { .. } | ParamKind::File | ParamKind::Curve
        );
    // stopwatch
    let sw = Rect::from_min_size(
        pos2(left.min.x + indent - 16.0, left.min.y + 4.0),
        vec2(16.0, 16.0),
    );
    if animatable {
        let r = ui.interact(
            sw,
            ui.id().with(("sw", comp.id.0, spec.key)),
            Sense::click(),
        );
        icons::draw(
            ui.painter(),
            sw,
            Icon::Stopwatch,
            if prm.animated {
                theme::ACCENT
            } else if r.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            },
        );
        if r.on_hover_text(t("Toggle animation")).clicked() {
            let (sid, clip, id, t0) = (cx.sid, cx.clip.id, comp.id, cx.t);
            let key = spec.key;
            s.ed.edit("Toggle Animation", |p| {
                with_component(p, sid, clip, id, |c| {
                    if c.param(key).is_none() {
                        c.complete_params();
                    }
                    c.param_mut(key)
                        .ok_or(EditError::Nothing)?
                        .toggle_animation(t0, Interp::Linear);
                    Ok(())
                })
            });
        }
    }
    let label_w = 132.0f32.min((split - left.min.x) * 0.42);
    ui.painter().with_clip_rect(left).text(
        pos2(left.min.x + indent + 4.0, left.min.y + 12.0),
        Align2::LEFT_CENTER,
        tn(spec.label),
        FontId::proportional(12.0),
        theme::TEXT,
    );
    let nav_w = if prm.animated { 60.0 } else { 0.0 };
    let editor = Rect::from_min_max(
        pos2(left.min.x + indent + label_w, left.min.y),
        pos2(split - 6.0 - nav_w, left.max.y),
    );
    let id = ui.id().with(("val", comp.id.0, spec.key));
    let new = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(editor)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
            |ui| {
                ui.add_space(2.0);
                value_editor(s, ui, id, cx, spec, &value)
            },
        )
        .inner;
    if let Some(v) = new {
        commit(s, cx, comp.id, spec.key, v);
    }
    if prm.animated {
        keyframe_nav(
            s,
            ui,
            cx,
            comp,
            &prm,
            Rect::from_min_max(
                pos2(split - 6.0 - nav_w, left.min.y),
                pos2(split - 4.0, left.min.y + ROW_H),
            ),
        );
    }
    if let Some(lane) = lane
        && prm.animated
    {
        keyframe_lane(
            s,
            ui,
            cx,
            comp,
            &prm,
            lane,
            Rect::from_min_max(pos2(lane.x0, row.min.y), pos2(lane.x1, row.min.y + ROW_H)),
        );
    }
}

fn keyframe_nav(s: &mut State, ui: &mut Ui, cx: &ClipCtx, comp: &Component, prm: &Param, r: Rect) {
    let prev = prm.keys.iter().map(|k| k.time).filter(|k| *k < cx.t).max();
    let next = prm.keys.iter().map(|k| k.time).filter(|k| *k > cx.t).min();
    let at = prm.key_at(cx.t);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(r)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if icons::button(ui, Icon::KeyPrev, 18.0, false, t("Go to Previous Keyframe")).clicked()
                && let Some(k) = prev
            {
                s.ed.stop();
                s.ed.set_playhead(cx.clip.to_sequence(k));
            }
            if icons::button(
                ui,
                Icon::KeyAdd,
                18.0,
                at.is_some(),
                t("Add/Remove Keyframe"),
            )
            .clicked()
            {
                let (sid, clip, id, t0, key) = (cx.sid, cx.clip.id, comp.id, cx.t, prm.key.clone());
                s.ed.edit("Keyframe", |p| {
                    with_component(p, sid, clip, id, |c| {
                        let prm = c.param_mut(&key).ok_or(EditError::Nothing)?;
                        match prm.key_at(t0) {
                            Some(i) => {
                                prm.keys.remove(i);
                                if prm.keys.is_empty() {
                                    prm.animated = false;
                                }
                            }
                            None => {
                                let v = prm.value_at(t0);
                                prm.set_at(t0, v, Interp::Linear);
                            }
                        }
                        Ok(())
                    })
                });
            }
            if icons::button(ui, Icon::KeyNext, 18.0, false, t("Go to Next Keyframe")).clicked()
                && let Some(k) = next
            {
                s.ed.stop();
                s.ed.set_playhead(cx.clip.to_sequence(k));
            }
        },
    );
}

fn keyframe_lane(
    s: &mut State,
    ui: &mut Ui,
    cx: &ClipCtx,
    comp: &Component,
    prm: &Param,
    lane: &Lane,
    r: Rect,
) {
    let painter = ui.painter().with_clip_rect(r.expand2(vec2(6.0, 0.0)));
    for (i, k) in prm.keys.iter().enumerate() {
        let x = lane.x(cx.clip.to_sequence(k.time));
        let c = pos2(x, r.center().y);
        let hr = Rect::from_center_size(c, vec2(12.0, 14.0));
        let resp = ui.interact(
            hr,
            ui.id().with(("kf", comp.id.0, &prm.key, i)),
            Sense::click_and_drag(),
        );
        let selected =
            s.ec.kf_drag
                .as_ref()
                .is_some_and(|(cid, key, idx)| *cid == comp.id && *key == prm.key && *idx == i);
        let fill = if k.interp_out == Interp::Hold {
            theme::WARN
        } else {
            theme::KEYFRAME
        };
        let _ = &painter;
        widgets::diamond(
            ui,
            c,
            if resp.hovered() { 6.0 } else { 5.0 },
            fill,
            selected || resp.dragged(),
        );
        if resp.clicked() {
            s.ed.stop();
            s.ed.set_playhead(cx.clip.to_sequence(k.time));
        }
        if resp.drag_started() {
            s.ec.kf_drag = Some((comp.id, prm.key.clone(), i));
        }
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let rate = s.ed.active_seq().map(|q| q.rate()).unwrap_or(Rate::FPS_25);
            let seq_t = lane.t(p.x).round_frame(rate);
            let new_t = cx.clip.to_source(seq_t);
            let (sid, clip, id, key, old) = (cx.sid, cx.clip.id, comp.id, prm.key.clone(), k.time);
            let merge = Some(format!("kf-{}-{}-{}", clip.0, id.0, key));
            s.ed.edit_merge("Move Keyframe", merge, move |p| {
                with_component(p, sid, clip, id, |c| {
                    let prm = c.param_mut(&key).ok_or(EditError::Nothing)?;
                    let i = prm
                        .keys
                        .iter()
                        .position(|k| k.time == old)
                        .ok_or(EditError::Nothing)?;
                    if new_t == old || prm.keys.iter().any(|k| k.time == new_t) {
                        return Err(EditError::Nothing);
                    }
                    prm.keys[i].time = new_t;
                    prm.sort_keys();
                    Ok(())
                })
            });
        }
        if resp.drag_stopped() {
            s.ec.kf_drag = None;
            s.ed.seal();
        }
        resp.context_menu(|ui| {
            for choice in InterpChoice::ALL {
                if ui.button(t(interp_label(choice))).clicked() {
                    let (sid, clip, id, key, at) =
                        (cx.sid, cx.clip.id, comp.id, prm.key.clone(), k.time);
                    s.ed.edit("Keyframe Interpolation", |p| {
                        with_component(p, sid, clip, id, |c| {
                            let prm = c.param_mut(&key).ok_or(EditError::Nothing)?;
                            let i = prm.key_at(at).ok_or(EditError::Nothing)?;
                            prm.keys[i].set_interp(choice);
                            Ok(())
                        })
                    });
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(t("Clear")).clicked() {
                let (sid, clip, id, key, at) =
                    (cx.sid, cx.clip.id, comp.id, prm.key.clone(), k.time);
                s.ed.edit("Clear Keyframe", |p| {
                    with_component(p, sid, clip, id, |c| {
                        let prm = c.param_mut(&key).ok_or(EditError::Nothing)?;
                        let i = prm.key_at(at).ok_or(EditError::Nothing)?;
                        prm.keys.remove(i);
                        if prm.keys.is_empty() {
                            prm.animated = false;
                        }
                        Ok(())
                    })
                });
                ui.close();
            }
        });
    }
}

fn interp_label(c: InterpChoice) -> &'static str {
    match c {
        InterpChoice::Linear => "Linear",
        InterpChoice::Bezier => "Bezier",
        InterpChoice::AutoBezier => "Auto Bezier",
        InterpChoice::ContinuousBezier => "Continuous Bezier",
        InterpChoice::Hold => "Hold",
        InterpChoice::EaseIn => "Ease In",
        InterpChoice::EaseOut => "Ease Out",
    }
}

/// Editor for one value. Returns the new value when the user changed it.
fn value_editor(
    s: &mut State,
    ui: &mut Ui,
    id: egui::Id,
    cx: &ClipCtx,
    spec: &ParamSpec,
    value: &Value,
) -> Option<Value> {
    match spec.kind {
        ParamKind::Float {
            soft_min,
            soft_max,
            min,
            max,
            unit,
            decimals,
            ..
        } => {
            let mut v = value.as_f64();
            let speed = ((soft_max - soft_min) / 400.0).max(0.001);
            widgets::scrub(
                ui,
                &mut v,
                speed,
                min,
                max,
                decimals as usize,
                unit.suffix(),
            )
            .changed()
            .then_some(Value::Float(v))
        }
        ParamKind::Int { min, max, .. } => {
            let mut v = value.as_f64() as i64;
            widgets::scrub_int(ui, &mut v, min, max)
                .changed()
                .then_some(Value::Int(v))
        }
        ParamKind::Angle { .. } => {
            let mut v = value.as_f64();
            widgets::scrub(ui, &mut v, 0.5, -36_000.0, 36_000.0, 1, "\u{00B0}")
                .changed()
                .then_some(Value::Float(v))
        }
        ParamKind::Bool { .. } => {
            let mut v = value.as_bool();
            ui.checkbox(&mut v, "").changed().then_some(Value::Bool(v))
        }
        ParamKind::Choice { options, .. } => {
            let cur = value.as_choice() as usize;
            let mut out = None;
            egui::ComboBox::from_id_salt(id)
                .selected_text(options.get(cur).map(|o| tn(o)).unwrap_or_default())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (i, o) in options.iter().enumerate() {
                        if ui.selectable_label(i == cur, tn(o)).clicked() {
                            out = Some(Value::Choice(i as u32));
                        }
                    }
                });
            out
        }
        ParamKind::Color { .. } => {
            let c = value.as_color();
            let mut rgba = c.to_u8();
            ui.color_edit_button_srgba_unmultiplied(&mut rgba)
                .changed()
                .then(|| {
                    Value::Color(Rgba::new(
                        rgba[0] as f32 / 255.0,
                        rgba[1] as f32 / 255.0,
                        rgba[2] as f32 / 255.0,
                        rgba[3] as f32 / 255.0,
                    ))
                })
        }
        ParamKind::Point { space, .. } => {
            let p = value.as_point();
            match space {
                PointSpace::Wheel => {
                    let mut v = p;
                    color_wheel(ui, id, &mut v).then_some(Value::Point(v))
                }
                _ => {
                    let size = if space == PointSpace::Sequence {
                        cx.seq_size
                    } else {
                        cx.layer_size
                    };
                    let (mut x, mut y) = (p[0] * size[0], p[1] * size[1]);
                    let a = widgets::scrub(ui, &mut x, 1.0, -100_000.0, 100_000.0, 1, "").changed();
                    let b = widgets::scrub(ui, &mut y, 1.0, -100_000.0, 100_000.0, 1, "").changed();
                    (a || b).then_some(Value::Point([x / size[0], y / size[1]]))
                }
            }
        }
        ParamKind::Text { multiline, .. } => {
            if spec.key == "font" {
                if s.ec.fonts.is_empty() {
                    s.ec.fonts = op_render::text::Fonts::global().families();
                }
                let cur = value.as_text().to_string();
                let shown = if cur.is_empty() {
                    op_render::text::Fonts::default_family().to_string()
                } else {
                    cur.clone()
                };
                let mut out = None;
                egui::ComboBox::from_id_salt(id)
                    .selected_text(shown)
                    .width(170.0)
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        for f in &s.ec.fonts {
                            if ui.selectable_label(*f == cur, f).clicked() {
                                out = Some(Value::Text(f.clone()));
                            }
                        }
                    });
                return out;
            }
            let mut text = value.as_text().to_string();
            let edit = if multiline {
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(3)
                    .desired_width(ui.available_width() - 4.0)
            } else {
                egui::TextEdit::singleline(&mut text).desired_width(ui.available_width() - 4.0)
            };
            ui.add(edit).changed().then_some(Value::Text(text))
        }
        ParamKind::File => {
            let path = value.as_text().to_string();
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t("None").to_string());
            let mut out = None;
            ui.label(RichText::new(name).color(theme::VALUE))
                .on_hover_text(&path);
            if ui
                .small_button("\u{2026}")
                .on_hover_text(t("Browse"))
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("LUT", &["cube", "CUBE"])
                    .pick_file()
            {
                out = Some(Value::Text(p.display().to_string()));
            }
            if !path.is_empty()
                && ui
                    .small_button("\u{00D7}")
                    .on_hover_text(t("Clear"))
                    .clicked()
            {
                out = Some(Value::Text(String::new()));
            }
            out
        }
        ParamKind::Curve => {
            let mut pts = value.as_curve().to_vec();
            curve_editor(s, ui, id, &mut pts).then_some(Value::Curve(pts))
        }
    }
}

/// A color wheel: the point is an offset inside the unit disk.
fn color_wheel(ui: &mut Ui, id: egui::Id, v: &mut [f64; 2]) -> bool {
    let size = 88.0;
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let c = r.center();
    let rad = size / 2.0 - 2.0;
    let p = ui.painter();
    let n = 48;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let hue = egui::ecolor::Hsva::new(i as f32 / n as f32, 0.6, 0.55, 1.0);
        p.add(egui::Shape::convex_polygon(
            vec![
                c,
                c + vec2(a0.cos(), -a0.sin()) * rad,
                c + vec2(a1.cos(), -a1.sin()) * rad,
            ],
            Color32::from(hue),
            Stroke::NONE,
        ));
    }
    p.circle_filled(c, rad * 0.55, Color32::from_black_alpha(80));
    p.circle_stroke(c, rad, Stroke::new(1.0, theme::LINE));
    let puck = c + vec2(v[0] as f32, -(v[1] as f32)) * rad;
    p.circle_stroke(puck, 5.0, Stroke::new(2.0, Color32::WHITE));
    let mut changed = false;
    if (resp.dragged() || resp.clicked())
        && let Some(pp) = resp.interact_pointer_pos()
    {
        let mut d = (pp - c) / rad;
        if d.length() > 1.0 {
            d = d.normalized();
        }
        *v = [d.x as f64, -d.y as f64];
        changed = true;
    }
    if resp.double_clicked() {
        *v = [0.0, 0.0];
        changed = true;
    }
    let _ = id;
    changed
}

/// A tone curve: drag points, double-click to add, right-click to remove.
fn curve_editor(s: &mut State, ui: &mut Ui, id: egui::Id, pts: &mut Vec<[f32; 2]>) -> bool {
    let size = 140.0;
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(r, 2.0, theme::PANEL_DARK);
    for k in 1..4 {
        let f = k as f32 / 4.0;
        p.line_segment(
            [
                pos2(r.min.x + f * size, r.min.y),
                pos2(r.min.x + f * size, r.max.y),
            ],
            Stroke::new(1.0, theme::LINE),
        );
        p.line_segment(
            [
                pos2(r.min.x, r.min.y + f * size),
                pos2(r.max.x, r.min.y + f * size),
            ],
            Stroke::new(1.0, theme::LINE),
        );
    }
    let to_screen = |q: [f32; 2]| pos2(r.min.x + q[0] * size, r.max.y - q[1] * size);
    let from_screen = |q: Pos2| {
        [
            ((q.x - r.min.x) / size).clamp(0.0, 1.0),
            ((r.max.y - q.y) / size).clamp(0.0, 1.0),
        ]
    };
    let line: Vec<Pos2> = pts.iter().map(|q| to_screen(*q)).collect();
    p.add(egui::Shape::line(
        line,
        Stroke::new(1.5, theme::TEXT_BRIGHT),
    ));
    for q in pts.iter() {
        p.circle_filled(to_screen(*q), 3.5, theme::TEXT_BRIGHT);
    }
    let mut changed = false;
    let hover = resp.hover_pos();
    let nearest = |pts: &Vec<[f32; 2]>, at: Pos2| {
        pts.iter()
            .enumerate()
            .map(|(i, q)| (i, to_screen(*q).distance(at)))
            .filter(|(_, d)| *d < 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if resp.drag_started()
        && let Some(at) = resp.interact_pointer_pos()
    {
        s.ec.curve_drag = nearest(pts, at);
    }
    if resp.dragged()
        && let (Some(i), Some(at)) = (s.ec.curve_drag, resp.interact_pointer_pos())
        && i < pts.len()
    {
        let mut q = from_screen(at);
        // endpoints keep their x; inner points stay between their neighbors
        if i == 0 {
            q[0] = 0.0;
        } else if i == pts.len() - 1 {
            q[0] = 1.0;
        } else {
            q[0] = q[0].clamp(pts[i - 1][0] + 0.01, pts[i + 1][0] - 0.01);
        }
        pts[i] = q;
        changed = true;
    }
    if resp.drag_stopped() {
        s.ec.curve_drag = None;
    }
    if resp.double_clicked()
        && let Some(at) = hover
        && nearest(pts, at).is_none()
    {
        let q = from_screen(at);
        let pos = pts.partition_point(|x| x[0] < q[0]);
        if pos > 0 && pos < pts.len() {
            pts.insert(pos, q);
            changed = true;
        }
    }
    if resp.secondary_clicked()
        && let Some(at) = hover
        && let Some(i) = nearest(pts, at)
        && i > 0
        && i < pts.len() - 1
    {
        pts.remove(i);
        changed = true;
    }
    let _ = id;
    changed
}

/// A transition's timing and parameters.
fn transition_ui(s: &mut State, ui: &mut Ui, sid: SequenceId, tid: TransitionId) {
    let Some(seq) = s.ed.project.sequence(sid).cloned() else {
        return;
    };
    let Some((track, tr)) = seq
        .all_tracks()
        .find_map(|(r, t)| t.transition(tid).map(|x| (r, x.clone())))
    else {
        return;
    };
    let def = catalog::find(&tr.effect);
    let rate = seq.rate();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(def.map(|d| tn(d.name)).unwrap_or(tr.effect.clone()))
                .size(14.0)
                .color(theme::TEXT_BRIGHT),
        );
    });
    ui.add_space(6.0);
    egui::Grid::new("tr-grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t("Duration"));
            let fmt = TimecodeFormat::new(rate, seq.settings.drop_frame);
            if let Some(d) = widgets::timecode(
                ui,
                ui.id().with("tr-dur"),
                &fmt,
                tr.duration,
                13.0,
                theme::VALUE,
            ) {
                s.ed.seq_edit("Transition Duration", |p, sid, _| {
                    op_timeline::set_transition_timing(
                        p,
                        sid,
                        tid,
                        Some(d.max(rate.frame_duration())),
                        None,
                    )
                });
            }
            ui.end_row();
            ui.label(t("Alignment"));
            let current = match tr.alignment {
                Alignment::CenterAtCut => t("Center at Cut"),
                Alignment::StartAtCut => t("Start at Cut"),
                Alignment::EndAtCut => t("End at Cut"),
                Alignment::Custom(_) => t("Custom Start"),
            };
            egui::ComboBox::from_id_salt("tr-align")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (a, l) in [
                        (Alignment::CenterAtCut, "Center at Cut"),
                        (Alignment::StartAtCut, "Start at Cut"),
                        (Alignment::EndAtCut, "End at Cut"),
                    ] {
                        if ui.selectable_label(tr.alignment == a, t(l)).clicked() {
                            s.ed.seq_edit("Transition Alignment", |p, sid, _| {
                                op_timeline::set_transition_timing(p, sid, tid, None, Some(a))
                            });
                        }
                    }
                });
            ui.end_row();
            if let Some(def) = def {
                for spec in def.params {
                    let value = tr
                        .param(spec.key)
                        .map(|p| p.value.clone())
                        .unwrap_or_else(|| spec.default_value());
                    ui.label(tn(spec.label));
                    let cx = ClipCtx {
                        sid,
                        clip: seq
                            .clips()
                            .next()
                            .map(|(_, c)| c.clone())
                            .unwrap_or_else(dummy_clip),
                        t: SrcTime::ZERO,
                        seq_size: [seq.settings.width as f64, seq.settings.height as f64],
                        layer_size: [seq.settings.width as f64, seq.settings.height as f64],
                    };
                    if let Some(v) =
                        value_editor(s, ui, ui.id().with(("trp", spec.key)), &cx, spec, &value)
                    {
                        let key = spec.key.to_string();
                        let merge = Some(format!("tr-{}-{key}", tid.0));
                        s.ed.edit_merge("Change Parameter", merge, |p| {
                            let q = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                            let t = q.track_mut(track).ok_or(EditError::Nothing)?;
                            let x = t
                                .transitions
                                .iter_mut()
                                .find(|x| x.id == tid)
                                .ok_or(EditError::Nothing)?;
                            match x.params.iter_mut().find(|p| p.key == key) {
                                Some(p) => p.value = v,
                                None => x.params.push(Param::new(key, v)),
                            }
                            Ok(())
                        });
                    }
                    ui.end_row();
                }
            }
        });
}

fn dummy_clip() -> Clip {
    Clip {
        id: ClipId(0),
        name: String::new(),
        kind: TrackKind::Video,
        source: ClipSource::Graphic,
        start: SeqTime::ZERO,
        duration: Dur(1),
        source_in: SrcTime::ZERO,
        speed: Speed::NORMAL,
        reverse: false,
        hold: None,
        enabled: true,
        link: None,
        group: None,
        label: Label::None,
        components: Vec::new(),
        gain_db: 0.0,
        scale_to_frame: false,
        channels: None,
    }
}

/// Parameter editors for one component outside Effect Controls (Lumetri, Essential Graphics).
pub fn component_panel(
    s: &mut State,
    ui: &mut Ui,
    sid: SequenceId,
    clip: ClipId,
    comp: ComponentId,
) {
    let Some(cx) = ClipCtx::new(s, sid, clip) else {
        return;
    };
    let Some(c) = cx.clip.component_by_id(comp).cloned() else {
        return;
    };
    let Some(def) = c.def() else { return };
    let split = ui.max_rect().max.x - 4.0;
    params_ui(s, ui, &cx, &c, def, None, split);
}
