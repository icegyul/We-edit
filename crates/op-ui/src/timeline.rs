//! The Timeline panel: sequence tabs, track headers, ruler, clips, transitions, markers, the
//! playhead and the editing tools (docs/timeline-behavior.md). Drags are previewed by running the
//! real edit on a scratch copy of the project; releasing the mouse commits the same edit as one
//! undoable step.

use egui::{
    Align2, Color32, CursorIcon, FontId, Modifiers, PointerButton, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, pos2, vec2,
};
use op_application::{Monitor, Selection, SeqView, Tool};
use op_core::catalog;
use op_core::*;
use op_timeline::{self as tl, Edge, MoveSpec};

use crate::app::{Drag, State};
use crate::i18n::{t, tf, tn};
use crate::icons::{self, Icon};
use crate::{theme, widgets};

const HEADER_W: f32 = 178.0;
const WHEEL_ZOOM_STEP: f64 = 1.20;
const RULER_H: f32 = 30.0;
const TABS_H: f32 = 24.0;
const SCROLL_H: f32 = 14.0;
const EDGE: f32 = 7.0;
const SECTION_GAP: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TrimKind {
    Normal,
    Ripple,
    Rate,
}

#[derive(Clone, Debug, PartialEq)]
enum TlDrag {
    Scrub,
    Move {
        clips: Vec<ClipId>,
        grab: SeqTime,
        grab_track: TrackRef,
        delta: Dur,
        track_delta: i32,
        duplicate: bool,
        insert: bool,
    },
    Trim {
        clip: ClipId,
        edge: Edge,
        kind: TrimKind,
        grab: SeqTime,
        delta: Dur,
    },
    Roll {
        track: TrackRef,
        cut: SeqTime,
        grab: SeqTime,
        delta: Dur,
    },
    Slip {
        clip: ClipId,
        grab: SeqTime,
        delta: Dur,
    },
    Slide {
        clip: ClipId,
        grab: SeqTime,
        delta: Dur,
    },
    Transition {
        id: TransitionId,
        head: bool,
        grab: SeqTime,
        start: Dur,
        delta: Dur,
    },
    Marquee {
        from: Pos2,
        to: Pos2,
    },
    Hand {
        from: Pos2,
        scroll: f64,
        scroll_y: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Zone {
    Head,
    Tail,
    Body,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Clip {
        id: ClipId,
        track: TrackRef,
        zone: Zone,
    },
    Transition {
        id: TransitionId,
        track: TrackRef,
        edge: Option<bool>,
    },
    Empty {
        track: Option<TrackRef>,
        t: SeqTime,
    },
}

pub struct TimelineView {
    pub video_height: f32,
    pub audio_height: f32,
    drag: Option<TlDrag>,
    preview: Option<Project>,
    snap_at: Option<SeqTime>,
    context: Option<Hit>,
    rename_track: Option<(TrackRef, String)>,
    area_w: f32,
}

impl Default for TimelineView {
    fn default() -> Self {
        TimelineView {
            video_height: 46.0,
            audio_height: 46.0,
            drag: None,
            preview: None,
            snap_at: None,
            context: None,
            rename_track: None,
            area_w: 800.0,
        }
    }
}

impl TimelineView {
    /// A new view keeping the user's track heights.
    pub fn fresh(&self) -> TimelineView {
        TimelineView {
            video_height: self.video_height,
            audio_height: self.audio_height,
            ..Default::default()
        }
    }

    /// Cancels a drag in progress (Escape). Returns whether there was one.
    pub fn cancel(&mut self) -> bool {
        let had = self.drag.is_some() && !matches!(self.drag, Some(TlDrag::Scrub));
        self.drag = None;
        self.preview = None;
        self.snap_at = None;
        had
    }
}

struct Row {
    r: TrackRef,
    top: f32,
    h: f32,
}

struct Geo {
    body: Rect,
    zoom: f64,
    scroll: f64,
    rows: Vec<Row>,
}

impl Geo {
    fn x(&self, t: SeqTime) -> f32 {
        self.body.min.x + ((t.seconds() - self.scroll) * self.zoom) as f32
    }
    fn t(&self, x: f32) -> SeqTime {
        SeqTime::from_seconds(self.scroll + (x - self.body.min.x) as f64 / self.zoom)
    }
    fn dur(&self, px: f32) -> Dur {
        Dur::from_seconds(px as f64 / self.zoom)
    }
    fn row_at(&self, y: f32) -> Option<&Row> {
        self.rows.iter().find(|r| y >= r.top && y < r.top + r.h)
    }
    fn row_of(&self, r: TrackRef) -> Option<&Row> {
        self.rows.iter().find(|x| x.r == r)
    }
}

fn layout_rows(seq: &Sequence, v: &TimelineView, top: f32, scroll_y: f32) -> (Vec<Row>, f32) {
    let mut y = top - scroll_y;
    let mut out = Vec::new();
    for i in (0..seq.video.len()).rev() {
        out.push(Row {
            r: TrackRef::video(i),
            top: y,
            h: v.video_height,
        });
        y += v.video_height;
    }
    y += SECTION_GAP;
    for i in 0..seq.audio.len() {
        out.push(Row {
            r: TrackRef::audio(i),
            top: y,
            h: v.audio_height,
        });
        y += v.audio_height;
    }
    (out, y + scroll_y - top)
}

fn hit(seq: &Sequence, g: &Geo, p: Pos2) -> Hit {
    let t = g.t(p.x);
    let Some(row) = g.row_at(p.y) else {
        return Hit::Empty { track: None, t };
    };
    let Some(tr) = seq.track(row.r) else {
        return Hit::Empty { track: None, t };
    };
    for x in &tr.transitions {
        let rg = x.range();
        let (x0, x1) = (g.x(rg.start), g.x(rg.end));
        if p.x >= x0 - 3.0 && p.x <= x1 + 3.0 && p.y < row.top + row.h * 0.5 {
            let edge = if (p.x - x0).abs() < 5.0 {
                Some(true)
            } else if (p.x - x1).abs() < 5.0 {
                Some(false)
            } else {
                None
            };
            return Hit::Transition {
                id: x.id,
                track: row.r,
                edge,
            };
        }
    }
    for (i, c) in tr.clips.iter().enumerate() {
        let (x0, x1) = (g.x(c.start), g.x(c.end()));
        let edge = EDGE.min((x1 - x0) / 3.0).max(2.0);
        if p.x >= x0 && p.x < x1 {
            let zone = if p.x - x0 < edge {
                Zone::Head
            } else if x1 - p.x < edge {
                Zone::Tail
            } else {
                Zone::Body
            };
            return Hit::Clip {
                id: c.id,
                track: row.r,
                zone,
            };
        }
        let next_adjacent = tr.clips.get(i + 1).is_some_and(|n| n.start == c.end());
        if p.x >= x1 && p.x < x1 + EDGE && !next_adjacent {
            return Hit::Clip {
                id: c.id,
                track: row.r,
                zone: Zone::Tail,
            };
        }
        let prev_adjacent = i > 0 && tr.clips[i - 1].end() == c.start;
        if p.x < x0 && p.x > x0 - EDGE && !prev_adjacent {
            return Hit::Clip {
                id: c.id,
                track: row.r,
                zone: Zone::Head,
            };
        }
    }
    Hit::Empty {
        track: Some(row.r),
        t,
    }
}

/// Runs a drag's edit on a project.
fn apply(
    p: &mut Project,
    sid: SequenceId,
    d: &TlDrag,
    opts: tl::EditOptions,
) -> EditResult<Option<Vec<ClipId>>> {
    match d {
        TlDrag::Move {
            clips,
            delta,
            track_delta,
            duplicate,
            insert,
            ..
        } => {
            let spec = MoveSpec {
                clips: clips.clone(),
                delta: *delta,
                track_delta: *track_delta,
                insert: *insert,
                duplicate: *duplicate,
            };
            tl::move_clips(p, sid, &spec, opts).map(Some)
        }
        TlDrag::Trim {
            clip,
            edge,
            kind,
            delta,
            ..
        } => match kind {
            TrimKind::Normal => tl::trim(p, sid, *clip, *edge, *delta, opts),
            TrimKind::Ripple => tl::ripple_trim(p, sid, *clip, *edge, *delta, opts),
            TrimKind::Rate => tl::rate_stretch(p, sid, *clip, *edge, *delta, opts),
        }
        .map(|_| None),
        TlDrag::Roll {
            track, cut, delta, ..
        } => tl::roll(p, sid, *track, *cut, *delta, opts).map(|_| None),
        TlDrag::Slip { clip, delta, .. } => tl::slip(p, sid, *clip, *delta, opts).map(|_| None),
        TlDrag::Slide { clip, delta, .. } => tl::slide(p, sid, *clip, *delta, opts).map(|_| None),
        TlDrag::Transition {
            id, start, delta, ..
        } => tl::set_transition_timing(p, sid, *id, Some(*start + *delta), None).map(|_| None),
        _ => Err(EditError::Nothing),
    }
}

fn changed(d: &TlDrag) -> bool {
    match d {
        TlDrag::Move {
            delta, track_delta, ..
        } => delta.0 != 0 || *track_delta != 0,
        TlDrag::Trim { delta, .. }
        | TlDrag::Roll { delta, .. }
        | TlDrag::Slip { delta, .. }
        | TlDrag::Slide { delta, .. }
        | TlDrag::Transition { delta, .. } => delta.0 != 0,
        _ => false,
    }
}

fn label(d: &TlDrag) -> &'static str {
    match d {
        TlDrag::Move {
            duplicate: true, ..
        } => "Duplicate",
        TlDrag::Move { insert: true, .. } => "Insert",
        TlDrag::Move { .. } => "Move",
        TlDrag::Trim {
            kind: TrimKind::Normal,
            ..
        } => "Trim",
        TlDrag::Trim {
            kind: TrimKind::Ripple,
            ..
        } => "Ripple Trim",
        TlDrag::Trim {
            kind: TrimKind::Rate,
            ..
        } => "Rate Stretch",
        TlDrag::Roll { .. } => "Roll Edit",
        TlDrag::Slip { .. } => "Slip",
        TlDrag::Slide { .. } => "Slide",
        TlDrag::Transition { .. } => "Transition Duration",
        _ => "",
    }
}

// ------------------------------------------------------------------------------------ public

pub fn zoom_by(s: &mut State, f: f64) {
    let Some(sid) = s.ed.active else { return };
    let ph = s.ed.playhead().seconds();
    let v = s.ed.view(sid);
    let x = (ph - v.scroll) * v.zoom;
    v.zoom = (v.zoom * f).clamp(0.02, 20_000.0);
    v.scroll = (ph - x / v.zoom).max(0.0);
}

pub fn zoom_to_sequence(s: &mut State) {
    let Some(sid) = s.ed.active else { return };
    let len =
        s.ed.active_seq()
            .map(|q| q.duration().seconds())
            .unwrap_or(0.0)
            .max(1.0);
    let w = s.tl.area_w as f64;
    let v = s.ed.view(sid);
    v.zoom = (w * 0.97 / len).clamp(0.02, 20_000.0);
    v.scroll = 0.0;
}

pub fn page(s: &mut State, dir: f64) {
    let Some(sid) = s.ed.active else { return };
    let w = s.tl.area_w as f64;
    let v = s.ed.view(sid);
    v.scroll = (v.scroll + dir * w / v.zoom).max(0.0);
}

pub fn show(s: &mut State, ui: &mut Ui) {
    let full = ui.max_rect();
    let tabs = Rect::from_min_size(full.min, vec2(full.width(), TABS_H));
    sequence_tabs(s, ui, tabs);
    let rest = Rect::from_min_max(pos2(full.min.x, tabs.max.y), full.max);
    let Some(sid) = s.ed.active else {
        ui.painter().text(
            rest.center(),
            Align2::CENTER_CENTER,
            t("Drop media here to create a sequence"),
            FontId::proportional(13.0),
            theme::TEXT_DIM,
        );
        // dropping media on an empty timeline makes a matching sequence
        if ui.rect_contains_pointer(rest)
            && ui.input(|i| i.pointer.any_released())
            && let Some(Drag::Items(items)) = s.drag.take()
            && let Some(first) = items
                .into_iter()
                .find(|i| s.ed.project.item(*i).is_some_and(|x| !x.is_bin()))
        {
            s.ed.sequence_from_item(first);
        }
        return;
    };
    let mut view = s.ed.view(sid).clone();
    let area = Rect::from_min_max(
        pos2(rest.min.x + HEADER_W, rest.min.y + RULER_H),
        pos2(rest.max.x, rest.max.y - SCROLL_H),
    );
    s.tl.area_w = area.width();
    // what the tracks show: the drag preview when there is one
    let shown: Sequence = match s.tl.preview.as_ref().and_then(|p| p.sequence(sid)) {
        Some(q) => q.clone(),
        None => s.ed.project.sequence(sid).unwrap().clone(),
    };
    let (_, content_h) = layout_rows(&shown, &s.tl, area.min.y, view.scroll_y);
    view.scroll_y = view
        .scroll_y
        .clamp(0.0, (content_h - area.height() + 20.0).max(0.0));
    let (rows, _) = layout_rows(&shown, &s.tl, area.min.y, view.scroll_y);
    let g = Geo {
        body: area,
        zoom: view.zoom.max(0.01),
        scroll: view.scroll.max(0.0),
        rows,
    };

    corner(
        s,
        ui,
        Rect::from_min_size(rest.min, vec2(HEADER_W, RULER_H)),
    );
    ruler(
        s,
        ui,
        &g,
        Rect::from_min_max(pos2(area.min.x, rest.min.y), pos2(rest.max.x, area.min.y)),
        &shown,
    );
    headers(
        s,
        ui,
        &g,
        Rect::from_min_max(pos2(rest.min.x, area.min.y), pos2(area.min.x, area.max.y)),
        sid,
        &shown,
        &mut view,
    );
    body(s, ui, &g, sid, &shown, &mut view);
    scrollbar(
        s,
        ui,
        &g,
        Rect::from_min_max(pos2(area.min.x, area.max.y), rest.max),
        &shown,
        &mut view,
    );
    wheel(ui, &g, rest, &mut view, content_h);
    // keep the playhead visible while playing
    if s.ed.transport.playing == Some(Monitor::Program) {
        let x = g.x(s.ed.playhead());
        if x > area.max.x - 4.0 || x < area.min.x {
            view.scroll =
                (s.ed.playhead().seconds() - 0.05 * area.width() as f64 / g.zoom).max(0.0);
        }
    }
    *s.ed.view(sid) = view;
}

// ------------------------------------------------------------------------------- sections

fn sequence_tabs(s: &mut State, ui: &mut Ui, r: Rect) {
    ui.painter().rect_filled(r, 0.0, theme::PANEL_DARK);
    let mut x = r.min.x + 6.0;
    let open = s.ed.open.clone();
    for sid in open {
        let Some(seq) = s.ed.project.sequence(sid) else {
            continue;
        };
        let name = seq.name.clone();
        let active = s.ed.active == Some(sid);
        let galley = ui.painter().layout_no_wrap(
            name,
            FontId::proportional(12.0),
            if active {
                theme::TEXT_BRIGHT
            } else {
                theme::TEXT_DIM
            },
        );
        let w = galley.size().x + 34.0;
        let tab = Rect::from_min_size(pos2(x, r.min.y + 3.0), vec2(w, r.height() - 3.0));
        let resp = ui.interact(tab, ui.id().with(("seqtab", sid.0)), Sense::click());
        ui.painter().rect_filled(
            tab,
            egui::CornerRadius {
                nw: 4,
                ne: 4,
                sw: 0,
                se: 0,
            },
            if active {
                theme::PANEL
            } else if resp.hovered() {
                theme::RAISED
            } else {
                theme::PANEL_DARK
            },
        );
        ui.painter().galley(
            pos2(tab.min.x + 10.0, tab.center().y - galley.size().y / 2.0),
            galley,
            theme::TEXT,
        );
        let close =
            Rect::from_center_size(pos2(tab.max.x - 12.0, tab.center().y), vec2(14.0, 14.0));
        let cr = ui.interact(close, ui.id().with(("seqclose", sid.0)), Sense::click());
        ui.painter().text(
            close.center(),
            Align2::CENTER_CENTER,
            "\u{00D7}",
            FontId::proportional(14.0),
            if cr.hovered() {
                theme::TEXT_BRIGHT
            } else {
                theme::TEXT_DIM
            },
        );
        if cr.clicked() {
            s.ed.close_sequence(sid);
        } else if resp.clicked() {
            s.ed.open_sequence(sid);
        }
        x += w + 2.0;
    }
}

fn corner(s: &mut State, ui: &mut Ui, r: Rect) {
    ui.painter().rect_filled(r, 0.0, theme::PANEL);
    let (fmt, offset) = s.timecode(Monitor::Program);
    let ph = s.ed.playhead();
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(r.shrink2(vec2(6.0, 1.0))),
        |ui| {
            ui.horizontal(|ui| {
                if let Some(d) = widgets::timecode(
                    ui,
                    ui.id().with("tl-tc"),
                    &fmt,
                    ph.since_zero() + offset,
                    14.0,
                    theme::VALUE,
                ) {
                    s.ed.stop();
                    s.ed.set_playhead(SeqTime::ZERO + (d - offset));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    if icons::button(
                        ui,
                        Icon::Marker,
                        20.0,
                        false,
                        &format!("{} (M)", t("Add Marker")),
                    )
                    .clicked()
                    {
                        s.command("cmd.set.marker");
                    }
                    let linked = s.ed.prefs.linked_selection;
                    if icons::button(ui, Icon::Link, 20.0, linked, t("Linked Selection")).clicked()
                    {
                        s.ed.prefs.linked_selection = !linked;
                    }
                    let snap = s.ed.prefs.snapping;
                    if icons::button(
                        ui,
                        Icon::Snap,
                        20.0,
                        snap,
                        &format!("{} (S)", t("Snap in Timeline")),
                    )
                    .clicked()
                    {
                        s.ed.prefs.snapping = !snap;
                    }
                });
            });
        },
    );
}

/// Major tick interval in seconds for a zoom level.
fn tick_step(zoom: f64, rate: Rate) -> f64 {
    let f = 1.0 / rate.as_f64();
    let steps = [
        f,
        2.0 * f,
        5.0 * f,
        10.0 * f,
        0.5,
        1.0,
        2.0,
        5.0,
        10.0,
        15.0,
        30.0,
        60.0,
        120.0,
        300.0,
        600.0,
        1200.0,
        1800.0,
        3600.0,
        7200.0,
    ];
    steps
        .into_iter()
        .find(|s| s * zoom >= 90.0)
        .unwrap_or(7200.0)
}

fn ruler(s: &mut State, ui: &mut Ui, g: &Geo, r: Rect, seq: &Sequence) {
    let p = ui.painter().with_clip_rect(r);
    p.rect_filled(r, 0.0, theme::PANEL);
    let (fmt, offset) = s.timecode(Monitor::Program);
    let step = tick_step(g.zoom, seq.rate());
    let first = (g.scroll / step).floor() as i64;
    let last = ((g.scroll + r.width() as f64 / g.zoom) / step).ceil() as i64;
    for i in first..=last {
        let ts = i as f64 * step;
        let x = g.x(SeqTime::from_seconds(ts));
        p.line_segment(
            [pos2(x, r.max.y - 12.0), pos2(x, r.max.y)],
            Stroke::new(1.0, theme::TEXT_DIM),
        );
        let label = fmt.format(Dur::from_seconds(ts).round_frames(seq.rate()) + offset);
        p.text(
            pos2(x + 3.0, r.min.y + 2.0),
            Align2::LEFT_TOP,
            label,
            FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
        for k in 1..5 {
            let xm = g.x(SeqTime::from_seconds(ts + step * k as f64 / 5.0));
            p.line_segment(
                [pos2(xm, r.max.y - 5.0), pos2(xm, r.max.y)],
                Stroke::new(1.0, theme::LINE),
            );
        }
    }
    // In/Out range
    if seq.mark_in.is_some() || seq.mark_out.is_some() {
        let a = g.x(seq.mark_in.unwrap_or(SeqTime::ZERO));
        let b = g.x(seq.mark_out.unwrap_or(SeqTime::ZERO + seq.duration()));
        p.rect_filled(
            Rect::from_min_max(pos2(a, r.max.y - 8.0), pos2(b, r.max.y - 2.0)),
            0.0,
            theme::ACCENT_DIM,
        );
    }
    for m in &seq.markers {
        let x = g.x(m.start);
        let [cr, cg, cb] = m.color.rgb();
        let c = Color32::from_rgb(cr, cg, cb);
        if m.duration.0 > 0 {
            p.rect_filled(
                Rect::from_min_max(pos2(x, r.min.y + 15.0), pos2(g.x(m.end()), r.min.y + 21.0)),
                0.0,
                c.gamma_multiply(0.6),
            );
        }
        p.add(egui::Shape::convex_polygon(
            vec![
                pos2(x - 5.0, r.min.y + 13.0),
                pos2(x + 5.0, r.min.y + 13.0),
                pos2(x + 5.0, r.min.y + 20.0),
                pos2(x, r.min.y + 25.0),
                pos2(x - 5.0, r.min.y + 20.0),
            ],
            c,
            Stroke::new(1.0, Color32::from_black_alpha(120)),
        ));
    }
    let x = g.x(s.ed.playhead());
    p.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 6.0, r.min.y + 12.0),
            pos2(x + 6.0, r.min.y + 12.0),
            pos2(x + 6.0, r.max.y - 7.0),
            pos2(x, r.max.y),
            pos2(x - 6.0, r.max.y - 7.0),
        ],
        theme::PLAYHEAD,
        Stroke::NONE,
    ));
    let resp = ui.interact(r, ui.id().with("ruler"), Sense::click_and_drag());
    if resp.drag_started() {
        s.tl.drag = Some(TlDrag::Scrub);
    }
    if (resp.dragged() || resp.clicked())
        && let Some(pp) = resp.interact_pointer_pos()
    {
        let mut tt = g.t(pp.x).max(SeqTime::ZERO);
        if s.ed.prefs.snapping ^ ui.input(|i| i.modifiers.shift) {
            let targets = tl::snap_targets(seq, &[], None);
            if let Some(hit) = tl::snap_time(tt, &targets, g.dur(6.0)) {
                tt = hit.time;
            }
        }
        s.ed.stop();
        s.ed.set_playhead(tt);
        s.ed.scrub_audio(Monitor::Program);
    }
    if resp.drag_stopped() {
        s.tl.drag = None;
    }
    resp.context_menu(|ui| {
        if ui.button(t("Clear In and Out")).clicked() {
            s.focus = op_application::Focus::Timeline;
            s.command("cmd.clear.inandout");
            ui.close();
        }
        if ui.button(t("Clear All Markers")).clicked() {
            s.command("cmd.marker.clearmarker.all");
            ui.close();
        }
    });
}

fn track_label(r: TrackRef) -> String {
    match r.kind {
        TrackKind::Video => format!("V{}", r.index + 1),
        TrackKind::Audio => format!("A{}", r.index + 1),
    }
}

fn toggle_track(
    s: &mut State,
    sid: SequenceId,
    r: TrackRef,
    label: &str,
    f: impl FnOnce(&mut Track),
) {
    s.ed.edit(label, |p| {
        let tr = p
            .sequence_mut(sid)
            .and_then(|q| q.track_mut(r))
            .ok_or(EditError::Nothing)?;
        f(tr);
        Ok(())
    });
}

fn headers(
    s: &mut State,
    ui: &mut Ui,
    g: &Geo,
    area: Rect,
    sid: SequenceId,
    seq: &Sequence,
    view: &mut SeqView,
) {
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(area, 0.0, theme::PANEL);
    for row in &g.rows {
        let r = Rect::from_min_size(pos2(area.min.x, row.top), vec2(area.width(), row.h));
        if !r.intersects(area) {
            continue;
        }
        let Some(tr) = seq.track(row.r) else { continue };
        let video = row.r.kind == TrackKind::Video;
        painter.rect_filled(
            r.shrink2(vec2(0.0, 0.5)),
            0.0,
            if video {
                theme::VIDEO_TRACK
            } else {
                theme::AUDIO_TRACK
            },
        );
        painter.line_segment(
            [pos2(r.min.x, r.max.y), pos2(r.max.x, r.max.y)],
            Stroke::new(1.0, theme::BG),
        );
        let cy = r.min.y + 13.0_f32.min(row.h / 2.0);
        let id = ui.id().with(("hdr", video, row.r.index));
        // source patch
        let patch_cell = Rect::from_center_size(pos2(r.min.x + 14.0, cy), vec2(22.0, 16.0));
        let patched: Option<String> = if video {
            (view.video_patch == Some(row.r.index)).then(|| "V1".to_string())
        } else {
            view.audio_patch
                .iter()
                .position(|p| *p == Some(row.r.index))
                .map(|i| format!("A{}", i + 1))
        };
        if let Some(text) = &patched {
            painter.rect_filled(patch_cell, 3.0, theme::ACCENT_DIM);
            painter.text(
                patch_cell.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(10.5),
                theme::TEXT_BRIGHT,
            );
        } else {
            painter.rect_stroke(
                patch_cell,
                3.0,
                Stroke::new(1.0, theme::LINE),
                StrokeKind::Inside,
            );
        }
        if ui
            .interact(patch_cell, id.with("patch"), Sense::click())
            .on_hover_text(t("Source patching"))
            .clicked()
        {
            if video {
                view.video_patch = if view.video_patch == Some(row.r.index) {
                    None
                } else {
                    Some(row.r.index)
                };
            } else if let Some(i) = view
                .audio_patch
                .iter()
                .position(|p| *p == Some(row.r.index))
            {
                view.audio_patch[i] = None;
            } else {
                let i = view
                    .audio_patch
                    .iter()
                    .position(|p| p.is_none())
                    .unwrap_or(0);
                if view.audio_patch.is_empty() {
                    view.audio_patch.push(None);
                }
                view.audio_patch[i] = Some(row.r.index);
            }
        }
        // track target
        let target_cell = Rect::from_center_size(pos2(r.min.x + 42.0, cy), vec2(28.0, 18.0));
        let targeted = view.is_targeted(row.r);
        painter.rect_filled(
            target_cell,
            3.0,
            if targeted {
                theme::ACCENT
            } else {
                theme::RAISED
            },
        );
        painter.text(
            target_cell.center(),
            Align2::CENTER_CENTER,
            track_label(row.r),
            FontId::proportional(11.0),
            if targeted {
                Color32::WHITE
            } else {
                theme::TEXT
            },
        );
        if ui
            .interact(target_cell, id.with("target"), Sense::click())
            .on_hover_text(t("Target track"))
            .clicked()
        {
            view.set_targeted(row.r, !targeted);
        }
        // toggles
        let mut x = r.min.x + 62.0;
        let small = |ui: &mut Ui, x: f32, icon: Icon, on: bool, tip: &str, key: &str| -> bool {
            let rr = Rect::from_center_size(pos2(x + 9.0, cy), vec2(18.0, 18.0));
            let resp = ui.interact(rr, id.with(key), Sense::click());
            let c = if on {
                theme::TEXT_BRIGHT
            } else {
                theme::TEXT_DIM
            };
            if resp.hovered() {
                ui.painter().rect_filled(rr, 3.0, theme::RAISED);
            }
            icons::draw(ui.painter(), rr.shrink(2.0), icon, c);
            resp.on_hover_text(tip).clicked()
        };
        if small(
            ui,
            x,
            Icon::SyncLock,
            tr.sync_lock,
            t("Toggle Sync Lock"),
            "sync",
        ) {
            toggle_track(s, sid, row.r, "Toggle Sync Lock", |t| {
                t.sync_lock = !t.sync_lock
            });
        }
        x += 20.0;
        if small(
            ui,
            x,
            if tr.locked { Icon::Lock } else { Icon::Unlock },
            tr.locked,
            t("Toggle Track Lock"),
            "lock",
        ) {
            toggle_track(s, sid, row.r, "Toggle Track Lock", |t| t.locked = !t.locked);
        }
        x += 20.0;
        if video {
            if small(
                ui,
                x,
                if tr.enabled { Icon::Eye } else { Icon::EyeOff },
                tr.enabled,
                t("Toggle Track Output"),
                "eye",
            ) {
                toggle_track(s, sid, row.r, "Toggle Track Output", |t| {
                    t.enabled = !t.enabled
                });
            }
        } else {
            for (i, (label, on)) in [("M", tr.muted), ("S", tr.solo)].into_iter().enumerate() {
                let rr =
                    Rect::from_center_size(pos2(x + 9.0 + i as f32 * 21.0, cy), vec2(18.0, 16.0));
                let resp = ui.interact(rr, id.with(label), Sense::click());
                let fill = match (label, on) {
                    ("M", true) => Color32::from_rgb(0x2f, 0x9e, 0x6a),
                    ("S", true) => Color32::from_rgb(0xd4, 0xa0, 0x2a),
                    _ => theme::RAISED,
                };
                ui.painter().rect_filled(rr, 3.0, fill);
                ui.painter().text(
                    rr.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(10.5),
                    theme::TEXT_BRIGHT,
                );
                let tip = if label == "M" {
                    t("Mute Track")
                } else {
                    t("Solo Track")
                };
                if resp.on_hover_text(tip).clicked() {
                    if label == "M" {
                        toggle_track(s, sid, row.r, "Mute Track", |t| t.muted = !t.muted);
                    } else {
                        toggle_track(s, sid, row.r, "Solo Track", |t| t.solo = !t.solo);
                    }
                }
            }
            x += 21.0;
        }
        x += 22.0;
        // name (double-click renames)
        let name_rect = Rect::from_min_max(pos2(x, cy - 9.0), pos2(r.max.x - 4.0, cy + 9.0));
        if s.tl
            .rename_track
            .as_ref()
            .is_some_and(|(rr, _)| *rr == row.r)
        {
            let mut text = s.tl.rename_track.as_ref().unwrap().1.clone();
            let resp = ui.put(
                name_rect,
                egui::TextEdit::singleline(&mut text).font(FontId::proportional(11.5)),
            );
            resp.request_focus();
            if resp.lost_focus() {
                let new = text.clone();
                s.tl.rename_track = None;
                toggle_track(s, sid, row.r, "Rename Track", |t| t.name = new);
            } else {
                s.tl.rename_track = Some((row.r, text));
            }
        } else {
            let name = if tr.name.is_empty() {
                if video {
                    tf("Video {}", &[&(row.r.index + 1)])
                } else {
                    tf("Audio {}", &[&(row.r.index + 1)])
                }
            } else {
                tr.name.clone()
            };
            painter.text(
                name_rect.left_center(),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(11.5),
                theme::TEXT_DIM,
            );
        }
        let resp = ui.interact(
            Rect::from_min_max(pos2(x, r.min.y), r.max),
            id.with("name"),
            Sense::click(),
        );
        if resp.double_clicked() {
            s.tl.rename_track = Some((row.r, tr.name.clone()));
        }
        resp.context_menu(|ui| {
            if ui.button(t("Rename")).clicked() {
                s.tl.rename_track = Some((row.r, tr.name.clone()));
                ui.close();
            }
            if ui.button(t("Add Track")).clicked() {
                let kind = row.r.kind;
                let at = row.r.index + 1;
                s.ed.seq_edit("Add Tracks", |p, sid, _| {
                    tl::add_tracks(p, sid, kind, 1, at, AudioTrackLayout::Standard)
                });
                ui.close();
            }
            if ui.button(t("Delete Track")).clicked() {
                let rr = row.r;
                s.ed.seq_edit("Delete Track", |p, sid, _| tl::delete_track(p, sid, rr));
                ui.close();
            }
        });
    }
    // add a track below the last one with a double-click on empty header space
    let resp = ui.interact(area, ui.id().with("hdr-empty"), Sense::click());
    resp.context_menu(|ui| {
        if ui.button(t("Add Tracks...")).clicked() {
            s.command("op.sequence.addtracks");
            ui.close();
        }
        if ui.button(t("Delete Empty Tracks")).clicked() {
            s.command("op.sequence.deleteemptytracks");
            ui.close();
        }
    });
}

// ------------------------------------------------------------------------------------ body

fn clip_colors(c: &Clip, selected: bool) -> (Color32, Color32) {
    let base = if c.is_video() {
        theme::label_color(c.label, false)
    } else {
        theme::audio_clip_color(c.label)
    };
    let fill = if !c.enabled {
        Color32::from_gray(70)
    } else if selected {
        base.linear_multiply(1.15)
    } else {
        Color32::from_rgb(
            (base.r() as f32 * 0.72) as u8,
            (base.g() as f32 * 0.72) as u8,
            (base.b() as f32 * 0.72) as u8,
        )
    };
    let stroke = if selected {
        theme::SELECTED
    } else {
        Color32::from_black_alpha(150)
    };
    (fill, stroke)
}

fn draw_clip(
    s: &mut State,
    ui: &Ui,
    painter: &egui::Painter,
    g: &Geo,
    row: &Row,
    c: &Clip,
    selected: bool,
) {
    let (x0, x1) = (g.x(c.start), g.x(c.end()));
    if x1 < g.body.min.x || x0 > g.body.max.x {
        return;
    }
    let r = Rect::from_min_max(
        pos2(x0, row.top + 1.0),
        pos2(x1.max(x0 + 1.0), row.top + row.h - 1.0),
    );
    let (fill, stroke) = clip_colors(c, selected);
    painter.rect_filled(r, 2.0, fill);
    let inner = painter.with_clip_rect(r.intersect(g.body));
    let name_y = r.min.y + 2.0;
    if c.is_video() {
        // head thumbnail
        if row.h >= 34.0
            && let ClipSource::Asset { asset, .. } = c.source
            && let Some(a) = s.ed.project.asset(asset)
            && a.has_video()
        {
            let index = a
                .frame_rate()
                .map(|rt| rt.frame_of(c.source_in))
                .unwrap_or(0)
                .max(0);
            let th = r.height() - 16.0;
            if th > 12.0
                && let Some(tex) = s.thumb(ui.ctx(), asset, if a.is_still() { 0 } else { index })
            {
                let tw = th * 16.0 / 9.0;
                let tr = Rect::from_min_size(
                    pos2(r.min.x + 1.0, r.max.y - th - 1.0),
                    vec2(tw.min(r.width() - 2.0).max(0.0), th),
                );
                inner.image(
                    tex,
                    tr,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2((tr.width() / tw).min(1.0), 1.0)),
                    Color32::WHITE,
                );
            }
        }
    } else if row.h >= 24.0 {
        waveform(s, &inner, g, r, c);
    }
    let mut name = c.name.clone();
    if !c.speed.is_normal() || c.reverse {
        name = format!(
            "{name} [{}{:.0}%]",
            if c.reverse { "-" } else { "" },
            c.speed.percent()
        );
    }
    if c.hold.is_some() {
        name = format!("{name} [{}]", t("Frame Hold"));
    }
    let has_fx = c.components.iter().any(|x| !x.is_fixed());
    let mut tx = r.min.x + 4.0;
    if has_fx {
        let badge = Rect::from_min_size(pos2(tx, name_y + 1.0), vec2(14.0, 11.0));
        inner.rect_filled(badge, 2.0, Color32::from_rgb(0xe0, 0xc0, 0x40));
        inner.text(
            badge.center(),
            Align2::CENTER_CENTER,
            "fx",
            FontId::proportional(9.0),
            Color32::BLACK,
        );
        tx += 17.0;
    }
    inner.text(
        pos2(tx, name_y),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(11.0),
        if c.enabled {
            Color32::from_rgb(0xf4, 0xf4, 0xf8)
        } else {
            theme::TEXT_DIM
        },
    );
    if matches!(c.source, ClipSource::Sequence { .. }) {
        inner.rect_filled(
            Rect::from_min_max(pos2(r.min.x, r.max.y - 3.0), pos2(r.max.x, r.max.y)),
            0.0,
            Color32::from_white_alpha(60),
        );
    }
    painter.rect_stroke(
        r,
        2.0,
        Stroke::new(if selected { 1.5 } else { 1.0 }, stroke),
        StrokeKind::Inside,
    );
}

fn waveform(s: &State, painter: &egui::Painter, g: &Geo, r: Rect, c: &Clip) {
    let ClipSource::Asset { asset, stream, .. } = c.source else {
        return;
    };
    let Some(peaks) = s.ed.media.peaks(asset, stream) else {
        return;
    };
    let Some(audio) = op_audio::AudioSource::audio(&*s.ed.media, asset, stream) else {
        return;
    };
    let rate = audio.info.rate as f64;
    let top = r.min.y + 14.0;
    let h = (r.max.y - top - 2.0).max(4.0);
    let mid = top + h / 2.0;
    let gain = 10f32.powf(c.gain_db as f32 / 20.0);
    let x0 = r.min.x.max(g.body.min.x).floor() as i32;
    let x1 = r.max.x.min(g.body.max.x).ceil() as i32;
    let color = Color32::from_rgba_unmultiplied(230, 250, 240, 170);
    let mut shapes = Vec::with_capacity((x1 - x0).max(0) as usize);
    for x in x0..x1 {
        let (ta, tb) = (g.t(x as f32), g.t(x as f32 + 1.0));
        if tb <= c.start || ta >= c.end() {
            continue;
        }
        let (sa, sb) = (
            c.to_source(ta.max(c.start)).seconds() * rate,
            c.to_source(tb.min(c.end())).seconds() * rate,
        );
        let (a, b) = if sa <= sb { (sa, sb) } else { (sb, sa) };
        let (a, mut b) = (a as i64, b as i64 + 1);
        // very wide columns probe a part of their range (keeps zoomed-out timelines fast)
        b = b.min(a + 256 * 48);
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for ch in 0..peaks.channels.min(2) {
            let (l, h2) = peaks.range(ch, a, b);
            lo = lo.min(l);
            hi = hi.max(h2);
        }
        let (lo, hi) = ((lo * gain).max(-1.0), (hi * gain).min(1.0));
        let ya = mid - hi * h / 2.0;
        let yb = (mid - lo * h / 2.0).max(ya + 1.0);
        shapes.push(egui::Shape::line_segment(
            [pos2(x as f32 + 0.5, ya), pos2(x as f32 + 0.5, yb)],
            Stroke::new(1.0, color),
        ));
    }
    painter.extend(shapes);
}

fn select_click(s: &mut State, seq: &Sequence, id: ClipId, m: Modifiers) {
    let mut with = if s.ed.prefs.linked_selection && !m.alt {
        seq.linked(id)
    } else {
        vec![id]
    };
    if !m.alt {
        let groups: Vec<ClipId> = with.iter().flat_map(|c| seq.grouped(*c)).collect();
        for gid in groups {
            for l in if s.ed.prefs.linked_selection {
                seq.linked(gid)
            } else {
                vec![gid]
            } {
                if !with.contains(&l) {
                    with.push(l);
                }
            }
        }
    }
    if m.shift || m.command {
        let sel = &mut s.ed.selection;
        sel.transitions.clear();
        sel.gap = None;
        if sel.clips.contains(&id) {
            sel.clips.retain(|c| !with.contains(c));
        } else {
            for c in with {
                if !sel.clips.contains(&c) {
                    sel.clips.push(c);
                }
            }
        }
    } else {
        s.ed.selection = Selection::only(with);
    }
}

fn body(s: &mut State, ui: &mut Ui, g: &Geo, sid: SequenceId, seq: &Sequence, view: &mut SeqView) {
    let area = g.body;
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(area, 0.0, theme::PANEL_DARK);
    for row in &g.rows {
        let r = Rect::from_min_max(pos2(area.min.x, row.top), pos2(area.max.x, row.top + row.h));
        if r.intersects(area) {
            let video = row.r.kind == TrackKind::Video;
            painter.rect_filled(
                r.shrink2(vec2(0.0, 0.5)),
                0.0,
                if video {
                    Color32::from_rgb(0x20, 0x20, 0x24)
                } else {
                    Color32::from_rgb(0x1e, 0x20, 0x24)
                },
            );
            if seq.track(row.r).is_some_and(|t| t.locked) {
                // diagonal hatching on locked tracks
                let mut x = r.min.x - r.height();
                while x < r.max.x {
                    painter.line_segment(
                        [pos2(x, r.max.y), pos2(x + r.height(), r.min.y)],
                        Stroke::new(1.0, Color32::from_white_alpha(10)),
                    );
                    x += 10.0;
                }
            }
        }
    }
    // In/Out shading
    if seq.mark_in.is_some() || seq.mark_out.is_some() {
        let a = g.x(seq.mark_in.unwrap_or(SeqTime::ZERO));
        let b = g.x(seq.mark_out.unwrap_or(SeqTime::ZERO + seq.duration()));
        painter.rect_filled(
            Rect::from_min_max(pos2(a, area.min.y), pos2(b, area.max.y)),
            0.0,
            Color32::from_rgba_unmultiplied(80, 120, 190, 22),
        );
    }
    let selection = s.ed.selection.clone();
    for row in &g.rows {
        if row.top > area.max.y || row.top + row.h < area.min.y {
            continue;
        }
        let Some(tr) = seq.track(row.r) else { continue };
        for c in &tr.clips {
            draw_clip(s, ui, &painter, g, row, c, selection.clips.contains(&c.id));
        }
        for x in &tr.transitions {
            let rg = x.range();
            let r = Rect::from_min_max(
                pos2(g.x(rg.start), row.top + 2.0),
                pos2(g.x(rg.end), row.top + (row.h * 0.5).max(14.0)),
            );
            let sel = selection.transitions.contains(&x.id);
            painter.rect_filled(
                r,
                2.0,
                Color32::from_rgba_unmultiplied(200, 200, 215, if sel { 235 } else { 190 }),
            );
            painter.line_segment(
                [r.left_bottom(), r.right_top()],
                Stroke::new(1.0, Color32::from_black_alpha(90)),
            );
            let name = catalog::find(&x.effect)
                .map(|d| tn(d.name))
                .unwrap_or_else(|| x.effect.clone());
            painter.with_clip_rect(r).text(
                pos2(r.min.x + 3.0, r.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(10.0),
                Color32::from_gray(30),
            );
            if sel {
                painter.rect_stroke(r, 2.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
            }
        }
        if let Some((gr, gt)) = selection.gap
            && gr == row.r
            && let Some(range) = tl::gap_at(tr, gt)
        {
            let r = Rect::from_min_max(
                pos2(g.x(range.start), row.top + 1.0),
                pos2(g.x(range.end), row.top + row.h - 1.0),
            );
            painter.rect_filled(r, 0.0, Color32::from_white_alpha(28));
        }
    }
    // markers as thin lines
    for m in &seq.markers {
        let x = g.x(m.start);
        let [r, gg, b] = m.color.rgb();
        painter.line_segment(
            [pos2(x, area.min.y), pos2(x, area.max.y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(r, gg, b, 70)),
        );
    }

    let resp = ui.interact(area, ui.id().with("tl-body"), Sense::click_and_drag());
    input(s, ui, g, sid, &resp, view);
    external_drop(s, ui, g, sid, &painter);

    if let Some(ts) = s.tl.snap_at {
        let x = g.x(ts);
        painter.line_segment(
            [pos2(x, area.min.y), pos2(x, area.max.y)],
            Stroke::new(1.0, theme::SNAP),
        );
    }
    if let Some(TlDrag::Marquee { from, to }) = &s.tl.drag {
        let r = Rect::from_two_pos(*from, *to);
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(74, 156, 255, 30));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
    }
    // razor preview
    if s.ed.tool == Tool::Razor
        && let Some(p) = resp.hover_pos()
        && let Hit::Clip { track, .. } = hit(seq, g, p)
    {
        let tt = razor_time(s, seq, g, p.x);
        let x = g.x(tt);
        let all = ui.input(|i| i.modifiers.shift);
        let (y0, y1) = if all {
            (area.min.y, area.max.y)
        } else {
            g.row_of(track)
                .map(|r| (r.top, r.top + r.h))
                .unwrap_or((area.min.y, area.max.y))
        };
        painter.line_segment([pos2(x, y0), pos2(x, y1)], Stroke::new(1.0, theme::ERROR));
    }
    let x = g.x(s.ed.playhead());
    painter.line_segment(
        [pos2(x, area.min.y), pos2(x, area.max.y)],
        Stroke::new(1.0, theme::PLAYHEAD),
    );
}

fn razor_time(s: &State, seq: &Sequence, g: &Geo, x: f32) -> SeqTime {
    let mut tt = g.t(x).round_frame(seq.rate());
    if s.ed.prefs.snapping {
        let targets = tl::snap_targets(seq, &[], Some(s.ed.playhead()));
        if let Some(hit) = tl::snap_time(tt, &targets, g.dur(6.0)) {
            tt = hit.time;
        }
    }
    tt
}

fn update_preview(s: &mut State, sid: SequenceId) {
    let Some(d) = s.tl.drag.clone() else { return };
    if !changed(&d) {
        s.tl.preview = None;
        return;
    }
    let opts = s.ed.opts();
    s.tl.preview =
        s.ed.project
            .transact(|p| apply(p, sid, &d, opts))
            .ok()
            .map(|(p, _)| p);
}

fn input(
    s: &mut State,
    ui: &mut Ui,
    g: &Geo,
    sid: SequenceId,
    resp: &Response,
    view: &mut SeqView,
) {
    let seq = s.ed.project.sequence(sid).unwrap().clone();
    let m = ui.input(|i| i.modifiers);
    let rate = seq.rate();
    let tool = s.ed.tool;

    // hover feedback
    if let Some(p) = resp.hover_pos()
        && s.tl.drag.is_none()
    {
        let h = hit(&seq, g, p);
        let cursor = match (tool, h) {
            (Tool::Hand, _) => Some(CursorIcon::Grab),
            (Tool::Zoom, _) => Some(if m.alt {
                CursorIcon::ZoomOut
            } else {
                CursorIcon::ZoomIn
            }),
            (Tool::Razor, Hit::Clip { .. }) => Some(CursorIcon::Crosshair),
            (Tool::Slip | Tool::Slide, Hit::Clip { .. }) => Some(CursorIcon::ResizeColumn),
            (
                _,
                Hit::Clip {
                    zone: Zone::Head | Zone::Tail,
                    ..
                },
            ) => Some(CursorIcon::ResizeHorizontal),
            (_, Hit::Transition { edge: Some(_), .. }) => Some(CursorIcon::ResizeHorizontal),
            (Tool::TrackSelectForward | Tool::TrackSelectBackward, _) => {
                Some(CursorIcon::PointingHand)
            }
            _ => None,
        };
        if let Some(c) = cursor {
            ui.ctx().set_cursor_icon(c);
        }
    }

    // drag start
    if resp.drag_started_by(PointerButton::Primary)
        && let Some(p) = resp.interact_pointer_pos()
    {
        let h = hit(&seq, g, p);
        let tt = g.t(p.x);
        let clip_of = |id: ClipId| seq.clip(id).cloned();
        s.tl.drag = match (tool, h) {
            (Tool::Hand, _) => Some(TlDrag::Hand {
                from: p,
                scroll: view.scroll,
                scroll_y: view.scroll_y,
            }),
            (Tool::Razor | Tool::Zoom, _) => None,
            (
                _,
                Hit::Transition {
                    id,
                    edge: Some(head),
                    ..
                },
            ) => {
                let start = seq
                    .all_tracks()
                    .find_map(|(_, t)| t.transition(id))
                    .map(|x| x.duration)
                    .unwrap_or(Dur::ZERO);
                Some(TlDrag::Transition {
                    id,
                    head,
                    grab: tt,
                    start,
                    delta: Dur::ZERO,
                })
            }
            (Tool::Roll, Hit::Clip { id, track, zone }) => clip_of(id).map(|c| {
                let cut =
                    if zone == Zone::Head || (zone == Zone::Body && tt - c.start < c.end() - tt) {
                        c.start
                    } else {
                        c.end()
                    };
                TlDrag::Roll {
                    track,
                    cut,
                    grab: tt,
                    delta: Dur::ZERO,
                }
            }),
            (Tool::Slip, Hit::Clip { id, .. }) => Some(TlDrag::Slip {
                clip: id,
                grab: tt,
                delta: Dur::ZERO,
            }),
            (Tool::Slide, Hit::Clip { id, .. }) => Some(TlDrag::Slide {
                clip: id,
                grab: tt,
                delta: Dur::ZERO,
            }),
            (
                Tool::Ripple,
                Hit::Clip {
                    id,
                    zone: z @ (Zone::Head | Zone::Tail),
                    ..
                },
            ) => Some(TlDrag::Trim {
                clip: id,
                edge: if z == Zone::Head {
                    Edge::Head
                } else {
                    Edge::Tail
                },
                kind: TrimKind::Ripple,
                grab: tt,
                delta: Dur::ZERO,
            }),
            (
                Tool::RateStretch,
                Hit::Clip {
                    id,
                    zone: z @ (Zone::Head | Zone::Tail),
                    ..
                },
            ) => Some(TlDrag::Trim {
                clip: id,
                edge: if z == Zone::Head {
                    Edge::Head
                } else {
                    Edge::Tail
                },
                kind: TrimKind::Rate,
                grab: tt,
                delta: Dur::ZERO,
            }),
            (
                _,
                Hit::Clip {
                    id,
                    zone: z @ (Zone::Head | Zone::Tail),
                    ..
                },
            ) => {
                let kind = if m.command {
                    TrimKind::Ripple
                } else {
                    TrimKind::Normal
                };
                Some(TlDrag::Trim {
                    clip: id,
                    edge: if z == Zone::Head {
                        Edge::Head
                    } else {
                        Edge::Tail
                    },
                    kind,
                    grab: tt,
                    delta: Dur::ZERO,
                })
            }
            (
                _,
                Hit::Clip {
                    id,
                    track,
                    zone: Zone::Body,
                },
            ) => {
                if !s.ed.selection.clips.contains(&id) {
                    select_click(
                        s,
                        &seq,
                        id,
                        Modifiers {
                            shift: false,
                            command: false,
                            ..m
                        },
                    );
                }
                let clips = s.ed.selection.clips.clone();
                Some(TlDrag::Move {
                    clips,
                    grab: tt,
                    grab_track: track,
                    delta: Dur::ZERO,
                    track_delta: 0,
                    duplicate: m.alt,
                    insert: m.command,
                })
            }
            (_, Hit::Empty { .. }) | (_, Hit::Transition { .. }) => {
                Some(TlDrag::Marquee { from: p, to: p })
            }
        };
    }

    // drag update
    if resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(mut d) = s.tl.drag.clone()
    {
        let tt = g.t(p.x);
        let thr = g.dur(8.0);
        let snapping = s.ed.prefs.snapping;
        let playhead = s.ed.playhead();
        s.tl.snap_at = None;
        match &mut d {
            TlDrag::Hand {
                from,
                scroll,
                scroll_y,
            } => {
                view.scroll = (*scroll - (p.x - from.x) as f64 / g.zoom).max(0.0);
                view.scroll_y = (*scroll_y - (p.y - from.y)).max(0.0);
            }
            TlDrag::Marquee { to, .. } => *to = p,
            TlDrag::Move {
                clips,
                grab,
                grab_track,
                delta,
                track_delta,
                ..
            } => {
                let mut dd = (tt - *grab).round_frames(rate);
                let moving: Vec<&Clip> = clips.iter().filter_map(|c| seq.clip(*c)).collect();
                if let Some(min_start) = moving.iter().map(|c| c.start).min() {
                    // nothing moves before the sequence start
                    dd = dd.max(SeqTime::ZERO - min_start);
                }
                if snapping {
                    let anchors: Vec<SeqTime> =
                        moving.iter().flat_map(|c| [c.start, c.end()]).collect();
                    let exclude: Vec<ClipId> = clips.clone();
                    let targets = tl::snap_targets(&seq, &exclude, Some(playhead));
                    if let Some((nd, target)) = tl::snap(&anchors, dd, &targets, thr) {
                        dd = nd;
                        s.tl.snap_at = Some(target.time);
                    }
                }
                *delta = dd;
                if let Some(row) = g.row_at(p.y)
                    && row.r.kind == grab_track.kind
                {
                    *track_delta = row.r.index as i32 - grab_track.index as i32;
                }
            }
            TlDrag::Trim {
                clip,
                edge,
                grab,
                delta,
                ..
            } => {
                let mut dd = (tt - *grab).round_frames(rate);
                if snapping && let Some(c) = seq.clip(*clip) {
                    let at = if *edge == Edge::Head {
                        c.start
                    } else {
                        c.end()
                    };
                    let targets = tl::snap_targets(&seq, &[*clip], Some(playhead));
                    if let Some(target) = tl::snap_time(at + dd, &targets, thr) {
                        dd = target.time - at;
                        s.tl.snap_at = Some(target.time);
                    }
                }
                *delta = dd;
            }
            TlDrag::Roll {
                cut, grab, delta, ..
            } => {
                let mut dd = (tt - *grab).round_frames(rate);
                if snapping {
                    let targets: Vec<tl::SnapTarget> = tl::snap_targets(&seq, &[], Some(playhead))
                        .into_iter()
                        .filter(|x| x.time != *cut)
                        .collect();
                    if let Some(target) = tl::snap_time(*cut + dd, &targets, thr) {
                        dd = target.time - *cut;
                        s.tl.snap_at = Some(target.time);
                    }
                }
                *delta = dd;
            }
            TlDrag::Slip { grab, delta, .. } => *delta = (*grab - tt).round_frames(rate),
            TlDrag::Slide { grab, delta, .. } => *delta = (tt - *grab).round_frames(rate),
            TlDrag::Transition {
                head, grab, delta, ..
            } => {
                let dd = (tt - *grab).round_frames(rate);
                *delta = if *head { -dd } else { dd };
            }
            TlDrag::Scrub => {}
        }
        let moved = s.tl.drag.as_ref() != Some(&d);
        s.tl.drag = Some(d);
        if moved {
            update_preview(s, sid);
        }
    }

    // drag end
    if resp.drag_stopped() {
        let d = s.tl.drag.take();
        s.tl.preview = None;
        s.tl.snap_at = None;
        match d {
            Some(TlDrag::Marquee { from, to }) => {
                let r = Rect::from_two_pos(from, to);
                let mut ids = Vec::new();
                for row in &g.rows {
                    if row.top + row.h < r.min.y || row.top > r.max.y {
                        continue;
                    }
                    if let Some(tr) = seq.track(row.r) {
                        for c in &tr.clips {
                            if g.x(c.end()) >= r.min.x && g.x(c.start) <= r.max.x {
                                ids.extend(if s.ed.prefs.linked_selection && !m.alt {
                                    seq.linked(c.id)
                                } else {
                                    vec![c.id]
                                });
                            }
                        }
                    }
                }
                ids.sort_by_key(|c| c.0);
                ids.dedup();
                if m.shift {
                    for c in ids {
                        if !s.ed.selection.clips.contains(&c) {
                            s.ed.selection.clips.push(c);
                        }
                    }
                } else {
                    s.ed.selection = Selection::only(ids);
                }
            }
            Some(d) if changed(&d) => {
                let opts = s.ed.opts();
                let dd = d.clone();
                if let Some(Some(ids)) = s.ed.edit(label(&d), |p| apply(p, sid, &dd, opts)) {
                    s.ed.selection = Selection::only(ids);
                }
            }
            _ => {}
        }
    }

    // clicks
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let h = hit(&seq, g, p);
        let tt = g.t(p.x);
        match (tool, h) {
            (Tool::Razor, Hit::Clip { id, .. }) => {
                let at = razor_time(s, &seq, g, p.x);
                let clips: Vec<ClipId> = if m.shift {
                    seq.clips()
                        .filter(|(_, c)| c.range().contains(at) && c.start != at)
                        .map(|(_, c)| c.id)
                        .collect()
                } else if s.ed.prefs.linked_selection && !m.alt {
                    seq.linked(id)
                } else {
                    vec![id]
                };
                s.ed.seq_edit("Razor", |p, sid, opts| tl::razor(p, sid, &clips, at, opts));
            }
            (Tool::TrackSelectForward | Tool::TrackSelectBackward, _) => {
                let forward = tool == Tool::TrackSelectForward;
                let track = match h {
                    Hit::Clip { track, .. } | Hit::Transition { track, .. } => Some(track),
                    Hit::Empty { track, .. } => track,
                };
                let ids: Vec<ClipId> = seq
                    .clips()
                    .filter(|(r, c)| {
                        (m.shift || Some(*r) == track)
                            && if forward { c.end() > tt } else { c.start < tt }
                    })
                    .map(|(_, c)| c.id)
                    .collect();
                s.ed.selection = Selection::only(ids);
            }
            (Tool::Zoom, _) => {
                let f = if m.alt { 1.0 / 1.6 } else { 1.6 };
                let x = (tt.seconds() - view.scroll) * view.zoom;
                view.zoom = (view.zoom * f).clamp(0.02, 20_000.0);
                view.scroll = (tt.seconds() - x / view.zoom).max(0.0);
            }
            (_, Hit::Clip { id, .. }) => select_click(s, &seq, id, m),
            (_, Hit::Transition { id, .. }) => {
                s.ed.selection = Selection {
                    clips: vec![],
                    transitions: vec![id],
                    gap: None,
                };
            }
            (_, Hit::Empty { track, t: at }) => {
                s.ed.selection.clear();
                if let Some(r) = track
                    && let Some(tr) = seq.track(r)
                    && tl::gap_at(tr, at).is_some_and(|g| {
                        g.end < SeqTime::ZERO + seq.duration()
                            || g.start < at && tr.clips.iter().any(|c| c.start > at)
                    })
                {
                    s.ed.selection.gap = Some((r, at));
                }
            }
        }
    }
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match hit(&seq, g, p) {
            Hit::Clip { id, .. } => {
                if let Some(c) = seq.clip(id).cloned() {
                    match c.source {
                        ClipSource::Sequence { sequence, .. } => s.ed.open_sequence(sequence),
                        _ => {
                            if let Some(item) = c.source.item() {
                                s.ed.load_source(item);
                                let at = c.to_source(g.t(p.x).clamp(c.start, c.end() - Dur(1)));
                                s.ed.set_monitor_time(Monitor::Source, at.cast());
                            }
                        }
                    }
                }
            }
            Hit::Transition { .. } => s.command("uif.window.Effect Controls"),
            _ => {}
        }
    }
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let h = hit(&seq, g, p);
        if let Hit::Clip { id, .. } = h
            && !s.ed.selection.clips.contains(&id)
        {
            select_click(s, &seq, id, Modifiers::NONE);
        }
        if let Hit::Transition { id, .. } = h {
            s.ed.selection = Selection {
                clips: vec![],
                transitions: vec![id],
                gap: None,
            };
        }
        if let Hit::Empty {
            track: Some(r),
            t: at,
        } = h
        {
            s.ed.selection.clear();
            s.ed.selection.gap = Some((r, at));
        }
        s.tl.context = Some(h);
    }
    let ctx_hit = s.tl.context;
    resp.context_menu(|ui| context_menu(s, ui, ctx_hit));
}

fn context_menu(s: &mut State, ui: &mut Ui, h: Option<Hit>) {
    let run = |s: &mut State, ui: &mut Ui, label: &'static str, cmd: &str| {
        let keys = s.ed.keymap.keys_for(cmd).unwrap_or_default();
        if ui
            .add(egui::Button::new(t(label)).shortcut_text(keys))
            .clicked()
        {
            s.focus = op_application::Focus::Timeline;
            s.command(cmd);
            ui.close();
        }
    };
    match h {
        Some(Hit::Clip { id, .. }) => {
            run(s, ui, "Cut", "cmd.edit.cut");
            run(s, ui, "Copy", "cmd.edit.copy");
            run(s, ui, "Paste Attributes...", "cmd.edit.pasteattributes");
            ui.separator();
            let enabled =
                s.ed.active_seq()
                    .and_then(|q| q.clip(id))
                    .is_some_and(|c| c.enabled);
            run(
                s,
                ui,
                if enabled { "Disable" } else { "Enable" },
                "cmd.clip.enable",
            );
            run(s, ui, "Link / Unlink", "cmd.clip.linkaudioandvideo");
            run(s, ui, "Group", "cmd.clip.group");
            run(s, ui, "Ungroup", "cmd.clip.ungroup");
            ui.separator();
            run(s, ui, "Speed/Duration...", "cmd.clip.speed");
            run(s, ui, "Audio Gain...", "cmd.clip.audiooptions.gain");
            run(s, ui, "Add Frame Hold", "op.clip.framehold");
            run(s, ui, "Scale to Frame Size", "op.clip.scaletoframe");
            run(s, ui, "Nest...", "op.clip.nest");
            run(s, ui, "Rename...", "op.clip.rename");
            ui.menu_button(t("Label"), |ui| {
                for l in Label::ALL {
                    let [r, g, b] = l.rgb();
                    let text = egui::RichText::new(format!("\u{25A0} {}", t(label_name(l))))
                        .color(Color32::from_rgb(r, g, b));
                    if ui.button(text).clicked() {
                        let clips = s.ed.selection.clips.clone();
                        s.ed.seq_edit("Label", |p, sid, _| {
                            let q = p.sequence_mut(sid).unwrap();
                            for c in &clips {
                                if let Some(c) = q.clip_mut(*c) {
                                    c.label = l;
                                }
                            }
                            Ok(())
                        });
                        ui.close();
                    }
                }
            });
            if let Some(item) =
                s.ed.active_seq()
                    .and_then(|q| q.clip(id))
                    .and_then(|c| c.source.item())
                && ui.button(t("Reveal in Project")).clicked()
            {
                s.ed.items = vec![item];
                if let Some(parent) = s.ed.project.item(item).and_then(|i| i.parent) {
                    s.ed.bin = parent;
                }
                s.command("uif.window.Projects");
                ui.close();
            }
            ui.separator();
            run(s, ui, "Clear", "cmd.edit.clear");
            run(s, ui, "Ripple Delete", "cmd.edit.rippledelete");
        }
        Some(Hit::Transition { .. }) => {
            run(s, ui, "Clear", "cmd.edit.clear");
            if ui.button(t("Set Transition Duration...")).clicked() {
                s.command("uif.window.Effect Controls");
                ui.close();
            }
        }
        _ => {
            if s.ed.selection.gap.is_some() {
                run(s, ui, "Ripple Delete", "cmd.edit.rippledelete");
            }
            run(s, ui, "Paste", "cmd.edit.paste");
            run(s, ui, "Add Marker", "cmd.set.marker");
        }
    }
}

pub fn label_name(l: Label) -> &'static str {
    match l {
        Label::None => "None",
        Label::Violet => "Violet",
        Label::Iris => "Iris",
        Label::Caribbean => "Caribbean",
        Label::Lavender => "Lavender",
        Label::Cerulean => "Cerulean",
        Label::Forest => "Forest",
        Label::Rose => "Rose",
        Label::Mango => "Mango",
        Label::Purple => "Purple",
        Label::Blue => "Blue",
        Label::Teal => "Teal",
        Label::Magenta => "Magenta",
        Label::Tan => "Tan",
        Label::Green => "Green",
        Label::Brown => "Brown",
        Label::Yellow => "Yellow",
    }
}

/// Media or effects dragged from other panels.
fn external_drop(s: &mut State, ui: &Ui, g: &Geo, sid: SequenceId, painter: &egui::Painter) {
    let Some(drag) = s.drag.clone() else { return };
    let Some(p) = ui.ctx().pointer_hover_pos() else {
        return;
    };
    if !g.body.contains(p) || !ui.rect_contains_pointer(g.body) {
        return;
    }
    let seq = s.ed.project.sequence(sid).unwrap().clone();
    let released = ui.input(|i| i.pointer.any_released());
    let insert = ui.input(|i| i.modifiers.command);
    let mut tt = g.t(p.x).max(SeqTime::ZERO).round_frame(seq.rate());
    match drag {
        Drag::Items(items) => {
            let specs: Vec<tl::SourceClip> = items
                .iter()
                .filter_map(|i| tl::SourceClip::from_item(&s.ed.project, *i).ok())
                .collect();
            if specs.is_empty() {
                return;
            }
            let total: Dur = specs.iter().fold(Dur::ZERO, |a, sp| a + sp.duration());
            if s.ed.prefs.snapping {
                let targets = tl::snap_targets(&seq, &[], Some(s.ed.playhead()));
                if let Some((d, target)) =
                    tl::snap(&[tt, tt + total], Dur::ZERO, &targets, g.dur(8.0))
                {
                    tt += d;
                    s.tl.snap_at = Some(target.time);
                }
            }
            let row = g.row_at(p.y).map(|r| r.r);
            // ghost of what will land where
            let vrow = match row {
                Some(r) if r.kind == TrackKind::Video => Some(r.index),
                Some(r) => Some(r.index.min(seq.video.len().saturating_sub(1))),
                None => s.ed.view(sid).video_patch,
            };
            let arow = match row {
                Some(r) if r.kind == TrackKind::Audio => r.index,
                Some(r) => r.index,
                None => 0,
            };
            let x0 = g.x(tt);
            let x1 = g.x(tt + total);
            let has_video = specs.iter().any(|sp| sp.video.is_some());
            let has_audio = specs.iter().any(|sp| !sp.audio.is_empty());
            if has_video && let Some(r) = vrow.and_then(|i| g.row_of(TrackRef::video(i))) {
                painter.rect_stroke(
                    Rect::from_min_max(pos2(x0, r.top + 1.0), pos2(x1, r.top + r.h - 1.0)),
                    2.0,
                    Stroke::new(1.5, theme::TEXT_BRIGHT),
                    StrokeKind::Inside,
                );
            }
            if has_audio && let Some(r) = g.row_of(TrackRef::audio(arow)) {
                painter.rect_stroke(
                    Rect::from_min_max(pos2(x0, r.top + 1.0), pos2(x1, r.top + r.h - 1.0)),
                    2.0,
                    Stroke::new(1.5, theme::TEXT_BRIGHT),
                    StrokeKind::Inside,
                );
            }
            if released {
                s.drag = None;
                s.tl.snap_at = None;
                let mut at = tt;
                let mut placed = Vec::new();
                for spec in specs {
                    let base = s.ed.patch(sid);
                    let mut patch = base.clone();
                    if let Some(v) = vrow {
                        patch.video = Some(v);
                    }
                    patch.audio = (0..spec.audio.len().max(1))
                        .map(|i| Some(arow + i))
                        .collect();
                    let need_audio = arow + spec.audio.len();
                    let need_video = vrow.map(|v| v + 1).unwrap_or(0);
                    let len = spec.duration();
                    let label = if insert { "Insert" } else { "Overwrite" };
                    let r = s.ed.seq_edit(label, |p, sid, opts| {
                        let q = p.sequence(sid).unwrap();
                        let (na, nv) = (q.audio.len(), q.video.len());
                        if need_audio > na {
                            tl::add_tracks(
                                p,
                                sid,
                                TrackKind::Audio,
                                need_audio - na,
                                na,
                                AudioTrackLayout::Standard,
                            )?;
                        }
                        if spec.video.is_some() && need_video > nv {
                            tl::add_tracks(
                                p,
                                sid,
                                TrackKind::Video,
                                need_video - nv,
                                nv,
                                AudioTrackLayout::Standard,
                            )?;
                        }
                        if insert {
                            tl::insert(p, sid, &spec, at, &patch, opts)
                        } else {
                            tl::overwrite(p, sid, &spec, at, &patch, opts)
                        }
                    });
                    if let Some(ids) = r {
                        placed.extend(ids);
                        at += len;
                    }
                }
                if !placed.is_empty() {
                    s.ed.selection = Selection::only(placed);
                }
            }
        }
        Drag::Effect(effect) => {
            let Some(def) = catalog::find(effect) else {
                return;
            };
            let h = hit(&seq, g, p);
            if def.kind.is_transition() {
                let Some(row) = g.row_at(p.y) else { return };
                if (row.r.kind == TrackKind::Video) != def.kind.is_video() {
                    return;
                }
                let Some(cut) = tl::nearest_edit(&s.ed.project, sid, row.r, tt) else {
                    return;
                };
                let x = g.x(cut);
                let near = (p.x - x).abs() < 12.0;
                let alignment = if near {
                    Alignment::CenterAtCut
                } else if p.x < x {
                    Alignment::EndAtCut
                } else {
                    Alignment::StartAtCut
                };
                let dur = if def.kind.is_video() {
                    s.ed.project.settings.video_transition_duration
                } else {
                    s.ed.project.settings.audio_transition_duration
                };
                let w = (dur.seconds() * g.zoom) as f32;
                let (a, b) = match alignment {
                    Alignment::EndAtCut => (x - w, x),
                    Alignment::StartAtCut => (x, x + w),
                    _ => (x - w / 2.0, x + w / 2.0),
                };
                painter.rect_filled(
                    Rect::from_min_max(pos2(a, row.top + 2.0), pos2(b, row.top + row.h * 0.5)),
                    2.0,
                    Color32::from_white_alpha(90),
                );
                if released {
                    s.drag = None;
                    let track = row.r;
                    s.ed.seq_edit("Add Transition", |p, sid, opts| {
                        tl::add_transition(p, sid, track, cut, effect, dur, Some(alignment), opts)
                    });
                }
            } else if let Hit::Clip { id, track, .. } = h {
                if (track.kind == TrackKind::Video) != def.kind.is_video() {
                    return;
                }
                if let Some(c) = seq.clip(id)
                    && let Some(r) = g.row_of(track)
                {
                    painter.rect_stroke(
                        Rect::from_min_max(
                            pos2(g.x(c.start), r.top + 1.0),
                            pos2(g.x(c.end()), r.top + r.h - 1.0),
                        ),
                        2.0,
                        Stroke::new(2.0, theme::ACCENT),
                        StrokeKind::Inside,
                    );
                }
                if released {
                    s.drag = None;
                    // an effect dropped on a selected clip goes to the whole selection
                    let clips = if s.ed.selection.clips.contains(&id) {
                        s.ed.selection.clips.clone()
                    } else {
                        vec![id]
                    };
                    s.ed.apply_effect(effect, Some(clips));
                    if !s.ed.selection.clips.contains(&id) {
                        s.ed.selection = Selection::only(vec![id]);
                    }
                }
            }
        }
    }
    ui.ctx().request_repaint();
}

fn scrollbar(s: &mut State, ui: &mut Ui, g: &Geo, r: Rect, seq: &Sequence, view: &mut SeqView) {
    let p = ui.painter();
    p.rect_filled(r, 0.0, theme::PANEL);
    let visible = r.width() as f64 / g.zoom;
    let total = (seq.duration().seconds() + visible * 0.5)
        .max(visible)
        .max(view.scroll + visible);
    let a = r.min.x + (view.scroll / total) as f32 * r.width();
    let b = r.min.x + ((view.scroll + visible) / total) as f32 * r.width();
    let thumb = Rect::from_min_max(pos2(a, r.min.y + 3.0), pos2(b.max(a + 12.0), r.max.y - 3.0));
    let resp = ui.interact(r, ui.id().with("hscroll"), Sense::click_and_drag());
    p.rect_filled(
        thumb,
        4.0,
        if resp.hovered() || resp.dragged() {
            theme::TEXT_DIM
        } else {
            theme::LINE
        },
    );
    if resp.dragged() {
        view.scroll =
            (view.scroll + resp.drag_delta().x as f64 / r.width() as f64 * total).max(0.0);
    } else if resp.clicked()
        && let Some(pp) = resp.interact_pointer_pos()
    {
        view.scroll = (((pp.x - r.min.x) / r.width()) as f64 * total - visible / 2.0).max(0.0);
    }
    let _ = s;
}

/// Turns each physical Ctrl/Cmd+wheel event into one deterministic zoom step.
///
/// `InputState::zoom_delta` and `smooth_scroll_delta` intentionally smooth wheel input. That is
/// useful for document scrolling, but makes timeline navigation coast after the user's last
/// detent. Reading the raw events keeps the edit-time scale exact and stops immediately.
fn discrete_wheel_zoom(events: &[egui::Event]) -> Option<f64> {
    let steps = events
        .iter()
        .filter_map(|event| match event {
            egui::Event::MouseWheel {
                delta, modifiers, ..
            } if modifiers.ctrl || modifiers.command => {
                let axis = if delta.y.abs() >= delta.x.abs() {
                    delta.y
                } else {
                    delta.x
                };
                (axis != 0.0).then_some(if axis > 0.0 { 1_i32 } else { -1_i32 })
            }
            _ => None,
        })
        .sum::<i32>()
        .clamp(-8, 8);
    (steps != 0).then(|| WHEEL_ZOOM_STEP.powi(steps))
}

fn wheel(ui: &Ui, g: &Geo, panel: Rect, view: &mut SeqView, content_h: f32) {
    if !ui.rect_contains_pointer(panel) {
        return;
    }
    let (delta, m, wheel_zoom, pointer) = ui.input(|i| {
        (
            i.smooth_scroll_delta,
            i.modifiers,
            discrete_wheel_zoom(&i.events),
            i.pointer.hover_pos(),
        )
    });
    let px = pointer
        .map(|p| p.x)
        .unwrap_or(g.body.center().x)
        .clamp(g.body.min.x, g.body.max.x);
    let zoom_around = |view: &mut SeqView, f: f64| {
        let at = g.t(px).seconds();
        let x = (at - view.scroll) * view.zoom;
        view.zoom = (view.zoom * f).clamp(0.02, 20_000.0);
        view.scroll = (at - x / view.zoom).max(0.0);
    };
    if let Some(factor) = wheel_zoom {
        zoom_around(view, factor);
        return;
    }
    if delta == egui::Vec2::ZERO {
        return;
    }
    if m.shift {
        view.scroll_y = (view.scroll_y - (delta.x + delta.y))
            .clamp(0.0, (content_h - g.body.height() + 20.0).max(0.0));
    } else if pointer.is_some_and(|p| p.x < g.body.min.x) {
        view.scroll_y =
            (view.scroll_y - delta.y).clamp(0.0, (content_h - g.body.height() + 20.0).max(0.0));
    } else {
        view.scroll = (view.scroll - (delta.y + delta.x) as f64 / g.zoom).max(0.0);
    }
}

#[cfg(test)]
mod wheel_tests {
    use super::*;
    use egui::{MouseWheelUnit, TouchPhase, Vec2};

    fn wheel_event(y: f32, modified: bool) -> egui::Event {
        egui::Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, y),
            modifiers: Modifiers {
                ctrl: modified,
                command: modified,
                ..Modifiers::NONE
            },
            phase: TouchPhase::Move,
        }
    }

    #[test]
    fn ctrl_wheel_is_one_fixed_step_independent_of_delta_magnitude() {
        assert_eq!(discrete_wheel_zoom(&[wheel_event(1.0, true)]), Some(1.20));
        assert_eq!(discrete_wheel_zoom(&[wheel_event(120.0, true)]), Some(1.20));
        assert_eq!(
            discrete_wheel_zoom(&[wheel_event(-120.0, true)]),
            Some(1.0 / 1.20)
        );
    }

    #[test]
    fn wheel_steps_compose_but_unmodified_wheel_does_not_zoom() {
        assert_eq!(
            discrete_wheel_zoom(&[wheel_event(3.0, true), wheel_event(3.0, true)]),
            Some(1.20_f64.powi(2))
        );
        assert_eq!(discrete_wheel_zoom(&[wheel_event(3.0, false)]), None);
        assert_eq!(discrete_wheel_zoom(&[]), None);
    }
}
