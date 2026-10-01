//! Source and Program monitors: frames rendered by the GPU compositor on the window's device and
//! shown as egui textures, transport controls, a mini timeline and direct manipulation of Motion.

use std::time::Duration;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use op_application::Monitor;
use op_application::media::PreviewFrames;
use op_core::catalog;
use op_core::*;

use crate::app::{Drag, State};
use crate::i18n::t;
use crate::icons::{self, Icon};
use crate::{effect_controls, theme, widgets};

struct Display {
    tex: op_render::Tex,
    id: egui::TextureId,
}

#[derive(Clone, PartialEq)]
struct RenderKey {
    generation: u64,
    media: u64,
    revision: u64,
    sequence: SequenceId,
    time: SeqTime,
    scale: u32,
    alpha: bool,
    scope: Option<(op_render::ScopeKind, u32)>,
}

enum MotionDrag {
    Move {
        clip: ClipId,
        start: [f64; 2],
        from: Pos2,
    },
    Scale {
        clip: ClipId,
        start: f64,
        center: Pos2,
        dist: f32,
    },
}

pub struct MonitorView {
    which: Monitor,
    display: Option<Display>,
    scope: Option<Display>,
    key: Option<RenderKey>,
    pub safe_margins: bool,
    pub show_alpha: bool,
    /// Zoom: 0 fits the frame in the panel, otherwise a fraction of the frame size.
    pub zoom: f32,
    drag: Option<MotionDrag>,
}

impl MonitorView {
    pub fn new(which: Monitor) -> MonitorView {
        MonitorView {
            which,
            display: None,
            scope: None,
            key: None,
            safe_margins: false,
            show_alpha: false,
            zoom: 0.0,
            drag: None,
        }
    }

    pub fn reset(&mut self) {
        self.key = None;
        self.drag = None;
    }

    /// The texture of the last scope rendered with the program frame.
    pub fn scope_texture(&self) -> Option<(egui::TextureId, [u32; 2])> {
        self.scope
            .as_ref()
            .map(|d| (d.id, [d.tex.width, d.tex.height]))
    }
}

/// What a monitor shows: the project snapshot, sequence and time.
fn target(s: &mut State, which: Monitor) -> Option<(std::sync::Arc<Project>, SequenceId, SeqTime)> {
    match which {
        Monitor::Program => {
            let sid = s.ed.active?;
            Some((s.ed.snapshot(), sid, s.ed.playhead()))
        }
        Monitor::Source => {
            let (p, sid) = s.ed.source_view()?;
            Some((p, sid, s.ed.source.playhead))
        }
    }
}

fn register(s: &State, d: &mut Option<Display>, tex: op_render::Tex) -> egui::TextureId {
    let mut r = s.rs.renderer.write();
    match d {
        Some(old) if old.tex.width == tex.width && old.tex.height == tex.height => {
            r.update_egui_texture_from_wgpu_texture(
                &s.rs.device,
                &tex.view,
                wgpu::FilterMode::Linear,
                old.id,
            );
            old.tex = tex;
            old.id
        }
        Some(old) => {
            r.update_egui_texture_from_wgpu_texture(
                &s.rs.device,
                &tex.view,
                wgpu::FilterMode::Linear,
                old.id,
            );
            old.tex = tex;
            old.id
        }
        None => {
            let id = r.register_native_texture(&s.rs.device, &tex.view, wgpu::FilterMode::Linear);
            *d = Some(Display { tex, id });
            id
        }
    }
}

/// Renders the monitor's frame when anything it depends on changed.
fn render(
    s: &mut State,
    v: &mut MonitorView,
    avail: Vec2,
) -> Option<(egui::TextureId, [u32; 2], f32)> {
    let (project, sid, time) = target(s, v.which)?;
    let seq = project.sequence(sid)?;
    let w = seq.settings.width;
    let aspect = seq.settings.aspect() as f32;
    let playing = s.ed.transport.playing == Some(v.which);
    let div = if playing {
        s.ed.prefs.playback_resolution
    } else {
        s.ed.prefs.paused_resolution
    }
    .clamp(1, 8);
    let mut scale = 1.0 / div as f32;
    if v.zoom == 0.0 {
        let ppp = s.ctx.pixels_per_point();
        while w as f32 * scale > avail.x * ppp * 1.6 && scale > 0.125 {
            scale *= 0.5;
        }
    }
    let scope = (v.which == Monitor::Program
        && s.visible.contains(&crate::workspace::Panel::Scopes))
    .then_some((s.scopes.kind, (s.scopes.gain * 100.0) as u32));
    let key = RenderKey {
        generation: s.ed.frame_generation,
        media: s
            .ed
            .media
            .generation
            .load(std::sync::atomic::Ordering::Relaxed),
        revision: s.ed.history.revision(),
        sequence: sid,
        time,
        scale: (scale * 1000.0) as u32,
        alpha: v.show_alpha,
        scope,
    };
    if v.key.as_ref() == Some(&key)
        && let Some(d) = &v.display
    {
        return Some((d.id, [d.tex.width, d.tex.height], aspect));
    }
    let speed = s.ed.transport.speed;
    // never block the interface for long on a decoder: while scrubbing the nearest decoded
    // frame is shown at once and the exact one replaces it as soon as it arrives
    let scrubbing = s.ctx.input(|i| i.pointer.is_decidedly_dragging());
    let frames = PreviewFrames {
        service: s.ed.media.clone(),
        wait: if playing {
            Duration::from_millis(6)
        } else if scrubbing {
            Duration::ZERO
        } else {
            Duration::from_millis(25)
        },
        // decode about half a second ahead in the playing direction
        ahead: if playing {
            let k = op_application::media::PLAYBACK_AHEAD;
            if speed < 0.0 { -k } else { k }
        } else {
            0
        },
    };
    let frame = s.renderer.render(&op_render::Request {
        project: &project,
        sequence: sid,
        time,
        scale,
        source: &frames,
    });
    let missing = s.renderer.stats.missing_frames;
    let reuse = v
        .display
        .as_ref()
        .filter(|d| d.tex.width == frame.width && d.tex.height == frame.height)
        .map(|d| d.tex.clone());
    let display = reuse.unwrap_or_else(|| s.renderer.display_target(frame.width, frame.height));
    s.renderer.present_into(&frame, &display, v.show_alpha);
    if let Some((kind, _)) = scope {
        let st = v
            .scope
            .as_ref()
            .map(|d| d.tex.clone())
            .unwrap_or_else(|| s.renderer.display_target(512, 384));
        s.renderer.scope(&frame, kind, &st, s.scopes.gain);
        if v.scope.is_none() {
            register(s, &mut v.scope, st);
        }
    }
    s.renderer.recycle(frame);
    s.renderer.submit();
    let same = v
        .display
        .as_ref()
        .is_some_and(|d| d.tex.width == display.width && d.tex.height == display.height);
    let id = if same {
        v.display.as_ref().unwrap().id
    } else {
        register(s, &mut v.display, display.clone())
    };
    // frames still decoding: draw again soon (the media generation changes when they arrive)
    if missing > 0 {
        s.ctx.request_repaint_after(Duration::from_millis(30));
        v.key = None;
    } else {
        v.key = Some(key);
    }
    Some((id, [display.width, display.height], aspect))
}

/// Screen rect of the picture inside the monitor area.
fn picture_rect(area: Rect, aspect: f32, zoom: f32, frame_w: u32, ppp: f32) -> Rect {
    let size = if zoom > 0.0 {
        let w = frame_w as f32 * zoom / ppp;
        vec2(w, w / aspect)
    } else {
        let w = area.width().min(area.height() * aspect);
        vec2(w, w / aspect)
    };
    Rect::from_center_size(area.center(), size)
}

pub fn show(s: &mut State, ui: &mut Ui, which: Monitor) {
    let mut v = std::mem::replace(
        match which {
            Monitor::Program => &mut s.program,
            Monitor::Source => &mut s.source,
        },
        MonitorView::new(which),
    );
    body(s, &mut v, ui);
    *match which {
        Monitor::Program => &mut s.program,
        Monitor::Source => &mut s.source,
    } = v;
}

fn body(s: &mut State, v: &mut MonitorView, ui: &mut Ui) {
    let full = ui.max_rect();
    let controls_h = 74.0;
    let pic_area =
        Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - controls_h)).shrink(4.0);
    let controls = Rect::from_min_max(pos2(full.min.x, full.max.y - controls_h), full.max);
    ui.painter()
        .rect_filled(pic_area.expand(4.0), 0.0, theme::PANEL_DARK);
    if v.which == Monitor::Program && s.exporting() {
        export_view(s, ui, pic_area);
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(controls.shrink2(vec2(8.0, 2.0))),
            |ui| transport(s, v, ui),
        );
        return;
    }

    let rendered = render(s, v, pic_area.size());
    let ppp = ui.ctx().pixels_per_point();
    let resp = ui.interact(
        pic_area,
        ui.id().with(("monitor", v.which == Monitor::Program)),
        Sense::click_and_drag(),
    );
    match rendered {
        Some((id, size, aspect)) => {
            let seq_w = match v.which {
                Monitor::Program => {
                    s.ed.active_seq()
                        .map(|q| q.settings.width)
                        .unwrap_or(size[0])
                }
                Monitor::Source => size[0],
            };
            let r = picture_rect(pic_area, aspect, v.zoom, seq_w, ppp);
            let painter = ui.painter().with_clip_rect(pic_area);
            if v.show_alpha {
                painter.rect_filled(r, 0.0, Color32::BLACK);
            } else {
                checkerboard(&painter, r);
            }
            painter.image(
                id,
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            if v.safe_margins {
                for k in [0.9f32, 0.8] {
                    painter.rect_stroke(
                        Rect::from_center_size(r.center(), r.size() * k),
                        0.0,
                        Stroke::new(1.0, Color32::from_white_alpha(110)),
                        StrokeKind::Middle,
                    );
                }
            }
            if v.which == Monitor::Program {
                direct_manipulation(s, v, ui, &resp, r);
            }
        }
        None => {
            let msg = match v.which {
                Monitor::Program => t("No sequence open"),
                Monitor::Source => t("Double-click a clip in the Project panel to open it here"),
            };
            ui.painter().text(
                pic_area.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(13.0),
                theme::TEXT_DIM,
            );
        }
    }
    // the Source Monitor picture is a drag source for the timeline
    if v.which == Monitor::Source
        && resp.drag_started()
        && let Some(item) = s.ed.source.item
    {
        s.drag = Some(Drag::Items(vec![item]));
    }
    if resp.double_clicked() && v.which == Monitor::Program {
        v.zoom = 0.0;
    }
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(controls.shrink2(vec2(8.0, 2.0))),
        |ui| transport(s, v, ui),
    );
}

/// The frames an export renders, with its progress, shown in the Program Monitor.
fn export_view(s: &mut State, ui: &Ui, area: Rect) {
    let Some(p) = s.running_export().map(|j| j.progress.lock().clone()) else {
        return;
    };
    let painter = ui.painter().with_clip_rect(area);
    if let Some((id, size)) = s.export_preview() {
        let k = (area.width() / size.x).min(area.height() / size.y);
        let r = Rect::from_center_size(area.center(), size * k);
        painter.image(
            id,
            r,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let bar = Rect::from_min_max(pos2(area.min.x, area.max.y - 26.0), area.max);
    painter.rect_filled(bar, 0.0, Color32::from_black_alpha(190));
    let fill = Rect::from_min_max(
        pos2(bar.min.x, bar.max.y - 3.0),
        pos2(bar.min.x + bar.width() * p.fraction(), bar.max.y),
    );
    painter.rect_filled(fill, 0.0, theme::ACCENT);
    let text = if p.paused {
        format!("{}  {:.0} %", t("Export paused"), p.fraction() * 100.0)
    } else if p.preparing {
        t("Preparing audio...").to_string()
    } else {
        format!(
            "{}  {:.0} %",
            crate::i18n::tf("Exporting frame {} of {}", &[&p.frame, &p.total]),
            p.fraction() * 100.0
        )
    };
    painter.text(
        bar.left_center() + vec2(10.0, -1.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        theme::TEXT_BRIGHT,
    );
}

fn checkerboard(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, 0.0, Color32::from_gray(40));
    let n = 16.0;
    let mut y = r.min.y;
    let mut row = 0;
    while y < r.max.y {
        let mut x = r.min.x + if row % 2 == 0 { 0.0 } else { n };
        while x < r.max.x {
            let cell =
                Rect::from_min_max(pos2(x, y), pos2((x + n).min(r.max.x), (y + n).min(r.max.y)));
            p.rect_filled(cell, 0.0, Color32::from_gray(52));
            x += 2.0 * n;
        }
        y += n;
        row += 1;
    }
}

// ---------------------------------------------------------------------------------- transport

fn transport(s: &mut State, v: &mut MonitorView, ui: &mut Ui) {
    let which = v.which;
    let (fmt, offset) = s.timecode(which);
    let now = s.ed.monitor_time(which);
    let duration = match which {
        Monitor::Program => s.ed.active_seq().map(|q| q.duration()).unwrap_or(Dur::ZERO),
        Monitor::Source => s.ed.source_duration(),
    };
    // controls give way as the monitor narrows, instead of drawing over each other: the zoom
    // and resolution menus move into the settings menu, then the duration and the less
    // frequent buttons are left out
    let width = ui.available_width();
    let compact = width < 470.0;
    let tiny = width < 300.0;
    ui.horizontal(|ui| {
        let id = ui.id().with(("tc", which == Monitor::Program));
        if let Some(d) =
            widgets::timecode(ui, id, &fmt, now.since_zero() + offset, 15.0, theme::VALUE)
        {
            s.ed.stop();
            s.ed.set_monitor_time(which, SeqTime::ZERO + (d - offset));
        }
        let fit = if v.zoom == 0.0 {
            t("Fit").to_string()
        } else {
            format!("{:.0} %", v.zoom * 100.0)
        };
        let res = s.ed.prefs.paused_resolution;
        let res_label = |d: u32| match d {
            1 => t("Full").to_string(),
            n => format!("1/{n}"),
        };
        let zoom_choices = |ui: &mut Ui, v: &mut MonitorView| {
            ui.selectable_value(&mut v.zoom, 0.0, t("Fit"));
            for z in [0.25f32, 0.5, 0.75, 1.0, 1.5, 2.0] {
                ui.selectable_value(&mut v.zoom, z, format!("{:.0} %", z * 100.0));
            }
        };
        let res_choices = |ui: &mut Ui, s: &mut State| {
            for d in [1u32, 2, 4, 8] {
                if ui.selectable_label(res == d, res_label(d)).clicked() {
                    s.ed.prefs.paused_resolution = d;
                    s.ed.prefs.playback_resolution = s.ed.prefs.playback_resolution.max(d);
                }
            }
        };
        if !compact {
            ui.add_space(8.0);
            egui::ComboBox::from_id_salt(("zoom", which == Monitor::Program))
                .selected_text(fit.clone())
                .width(64.0)
                .show_ui(ui, |ui| zoom_choices(ui, v))
                .response
                .on_hover_text(t("Zoom"));
            egui::ComboBox::from_id_salt(("res", which == Monitor::Program))
                .selected_text(res_label(res))
                .width(56.0)
                .show_ui(ui, |ui| res_choices(ui, s))
                .response
                .on_hover_text(t("Resolution"));
        }
        let wrench = icons::button(ui, Icon::Wrench, 20.0, false, t("Settings"));
        egui::Popup::menu(&wrench).show(|ui| {
            if compact {
                ui.menu_button(format!("{}: {fit}", t("Zoom")), |ui| zoom_choices(ui, v));
                ui.menu_button(format!("{}: {}", t("Resolution"), res_label(res)), |ui| {
                    res_choices(ui, s)
                });
                ui.separator();
            }
            ui.checkbox(&mut v.safe_margins, t("Safe Margins"));
            ui.checkbox(&mut v.show_alpha, t("Show Alpha Channel"));
            let mut scrub = s.ed.prefs.audio_scrubbing;
            if ui.checkbox(&mut scrub, t("Audio Scrubbing")).changed() {
                s.ed.prefs.audio_scrubbing = scrub;
            }
            ui.checkbox(&mut s.loop_playback, t("Loop"));
        });
        if !tiny {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (i, o) = (s.ed.mark(which, true), s.ed.mark(which, false));
                let span = match (i, o) {
                    (Some(i), Some(o)) => o - i,
                    (Some(i), None) => SeqTime::ZERO + duration - i,
                    (None, Some(o)) => o.since_zero(),
                    _ => duration,
                };
                ui.label(
                    egui::RichText::new(fmt.format(span))
                        .monospace()
                        .size(13.0)
                        .color(theme::TEXT),
                )
                .on_hover_text(t("Duration"));
            });
        }
    });
    // mini timeline
    let (bar, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(bar, 2.0, theme::PANEL_DARK);
    let len = duration.seconds().max(1e-6);
    let x_of = |tt: SeqTime| bar.min.x + (tt.seconds() / len).clamp(0.0, 1.0) as f32 * bar.width();
    let (i, o) = (s.ed.mark(which, true), s.ed.mark(which, false));
    if i.is_some() || o.is_some() {
        let a = x_of(i.unwrap_or(SeqTime::ZERO));
        let b = x_of(o.unwrap_or(SeqTime::ZERO + duration));
        p.rect_filled(
            Rect::from_min_max(
                pos2(a, bar.min.y + 3.0),
                pos2(b.max(a + 1.0), bar.max.y - 3.0),
            ),
            1.0,
            theme::ACCENT_DIM,
        );
    }
    if which == Monitor::Program
        && let Some(seq) = s.ed.active_seq()
    {
        for m in &seq.markers {
            let [r, g, b] = m.color.rgb();
            p.rect_filled(
                Rect::from_center_size(pos2(x_of(m.start), bar.min.y + 3.0), vec2(4.0, 5.0)),
                0.0,
                Color32::from_rgb(r, g, b),
            );
        }
    }
    let x = x_of(now);
    p.line_segment(
        [pos2(x, bar.min.y), pos2(x, bar.max.y)],
        Stroke::new(1.5, theme::PLAYHEAD),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 5.0, bar.min.y),
            pos2(x + 5.0, bar.min.y),
            pos2(x, bar.min.y + 6.0),
        ],
        theme::PLAYHEAD,
        Stroke::NONE,
    ));
    if (resp.dragged() || resp.clicked())
        && let Some(pp) = resp.interact_pointer_pos()
    {
        let tt =
            SeqTime::from_seconds(((pp.x - bar.min.x) / bar.width()).clamp(0.0, 1.0) as f64 * len);
        s.ed.stop();
        s.ed.set_monitor_time(which, tt);
        s.ed.scrub_audio(which);
    }
    // buttons: everything when there is room, the essentials when the monitor is narrow
    let all = width >= 420.0;
    let most = width >= 300.0;
    let size = if most { 22.0 } else { 20.0 };
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let count = if all {
            14.0
        } else if most {
            10.0
        } else {
            5.0
        };
        let groups = if all {
            3.0
        } else if most {
            2.0
        } else {
            0.0
        };
        let needed = count * (size + 4.0) + groups * 8.0;
        ui.add_space(((ui.available_width() - needed) / 2.0).max(0.0));
        let playing = s.ed.transport.playing == Some(which);
        let cmd = |s: &mut State, c: &str| {
            s.focus = if which == Monitor::Program {
                op_application::Focus::Program
            } else {
                op_application::Focus::Source
            };
            s.command(c);
        };
        let b = |ui: &mut Ui, s: &State, icon: Icon, tip: &'static str, c: &str| {
            let keys =
                s.ed.keymap
                    .keys_for(c)
                    .map(|k| format!(" ({k})"))
                    .unwrap_or_default();
            icons::button(ui, icon, size, false, &format!("{}{keys}", t(tip))).clicked()
        };
        if most && b(ui, s, Icon::Marker, "Add Marker", "cmd.set.marker") {
            cmd(s, "cmd.set.marker");
        }
        if b(ui, s, Icon::MarkIn, "Mark In", "cmd.common.setin") {
            cmd(s, "cmd.common.setin");
        }
        if b(ui, s, Icon::MarkOut, "Mark Out", "cmd.common.setout") {
            cmd(s, "cmd.common.setout");
        }
        if most {
            ui.add_space(8.0);
            if b(ui, s, Icon::GoIn, "Go to In", "cmd.goto.in") {
                cmd(s, "cmd.goto.in");
            }
        }
        if b(
            ui,
            s,
            Icon::StepBack,
            "Step Back 1 Frame",
            "cmd.transport.step.back",
        ) {
            cmd(s, "cmd.transport.step.back");
        }
        let (icon, tip) = if playing {
            (Icon::Pause, "Stop")
        } else {
            (Icon::Play, "Play")
        };
        if b(ui, s, icon, tip, "cmd.transport.toggleplay") {
            cmd(s, "cmd.transport.toggleplay");
        }
        if b(
            ui,
            s,
            Icon::StepForward,
            "Step Forward 1 Frame",
            "cmd.transport.step.forward",
        ) {
            cmd(s, "cmd.transport.step.forward");
        }
        if most {
            if b(ui, s, Icon::GoOut, "Go to Out", "cmd.goto.out") {
                cmd(s, "cmd.goto.out");
            }
            ui.add_space(8.0);
            match which {
                Monitor::Source => {
                    if b(ui, s, Icon::Insert, "Insert", "cmd.clip.insert") {
                        s.command("cmd.clip.insert");
                    }
                    if b(ui, s, Icon::Overwrite, "Overwrite", "cmd.clip.overlay") {
                        s.command("cmd.clip.overlay");
                    }
                }
                Monitor::Program => {
                    if b(ui, s, Icon::Lift, "Lift", "cmd.sequence.lift") {
                        cmd(s, "cmd.sequence.lift");
                    }
                    if b(ui, s, Icon::Extract, "Extract", "cmd.sequence.extract") {
                        cmd(s, "cmd.sequence.extract");
                    }
                }
            }
        }
        if all {
            ui.add_space(8.0);
            if icons::button(ui, Icon::Loop, size, s.loop_playback, t("Loop")).clicked() {
                s.loop_playback = !s.loop_playback;
            }
            if icons::button(
                ui,
                Icon::SafeMargins,
                size,
                v.safe_margins,
                t("Safe Margins"),
            )
            .clicked()
            {
                v.safe_margins = !v.safe_margins;
            }
            if which == Monitor::Program
                && b(ui, s, Icon::Camera, "Export Frame", "cmd.export.frame")
            {
                s.command("cmd.export.frame");
            }
        }
    });
}

// ------------------------------------------------------------------------ direct manipulation

/// Corners (sequence pixels) and center of a clip's transformed layer.
fn clip_box(p: &Project, seq: &Sequence, clip: &Clip, t: SeqTime) -> Option<([Pos2; 4], Pos2)> {
    if !clip.is_video() || matches!(clip.source, ClipSource::Graphic) {
        return None;
    }
    let (sw, sh) = (seq.settings.width as f64, seq.settings.height as f64);
    let (w, h) = p.source_size(&clip.source, &seq.settings);
    let (mut lw, mut lh) = (w as f64, h as f64);
    if clip.scale_to_frame && lw > 0.0 && lh > 0.0 {
        let k = (sw / lw).min(sh / lh);
        lw *= k;
        lh *= k;
    }
    let m = clip.component(catalog::MOTION)?;
    let ts = clip.to_source(t);
    let pos = m.value_at("position", ts).as_point();
    let s = m.f64_at("scale", ts) / 100.0;
    let sx = if m.value_at("uniform_scale", ts).as_bool() {
        s
    } else {
        m.f64_at("scale_width", ts) / 100.0
    };
    let rot = m.f64_at("rotation", ts).to_radians();
    let a = m.value_at("anchor", ts).as_point();
    let (px, py) = (pos[0] * sw, pos[1] * sh);
    let (ax, ay) = (a[0] * lw, a[1] * lh);
    let (c, sn) = (rot.cos(), rot.sin());
    let map = |lx: f64, ly: f64| {
        let (vx, vy) = ((lx - ax) * sx, (ly - ay) * s);
        pos2(
            (px + c * vx - sn * vy) as f32,
            (py + sn * vx + c * vy) as f32,
        )
    };
    Some((
        [map(0.0, 0.0), map(lw, 0.0), map(lw, lh), map(0.0, lh)],
        pos2(px as f32, py as f32),
    ))
}

fn inside(poly: &[Pos2; 4], p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (poly[i], poly[(i + 1) % 4]);
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

fn write_motion(s: &mut State, clip: ClipId, key: &str, value: Value) {
    let Some(sid) = s.ed.active else { return };
    let t = s.ed.playhead();
    let Some(c) = s.ed.active_seq().and_then(|q| q.clip(clip)) else {
        return;
    };
    let Some(comp) = c.component(catalog::MOTION).map(|m| m.id) else {
        return;
    };
    let ts = c.to_source(t.clamp(c.start, c.end() - Dur(1)));
    let merge = Some(format!("motion-{}-{key}", clip.0));
    let key = key.to_string();
    s.ed.edit_merge("Motion", merge, move |p| {
        effect_controls::write_param(p, sid, clip, comp, &key, value, ts)
    });
}

/// Keyboard nudge of the selected clip's position in sequence pixels.
pub fn nudge_selected(s: &mut State, dx: f64, dy: f64) {
    let t = s.ed.playhead();
    let Some(seq) = s.ed.active_seq() else { return };
    let Some(clip) =
        s.ed.selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .find(|c| c.is_video())
    else {
        return;
    };
    let Some(m) = clip.component(catalog::MOTION) else {
        return;
    };
    let ts = clip.to_source(t.clamp(clip.start, clip.end() - Dur(1)));
    let pos = m.value_at("position", ts).as_point();
    let (w, h) = (seq.settings.width as f64, seq.settings.height as f64);
    let id = clip.id;
    write_motion(
        s,
        id,
        "position",
        Value::Point([pos[0] + dx / w, pos[1] + dy / h]),
    );
    s.ed.seal();
}

fn direct_manipulation(
    s: &mut State,
    v: &mut MonitorView,
    ui: &Ui,
    resp: &egui::Response,
    pic: Rect,
) {
    let Some(seq) = s.ed.active_seq().cloned() else {
        return;
    };
    let t = s.ed.playhead();
    let k = pic.width() / seq.settings.width.max(1) as f32;
    let to_screen = |p: Pos2| pic.min + p.to_vec2() * k;
    // the Type tool adds a text layer where the user clicks
    if s.ed.tool == op_application::Tool::Type && resp.clicked() {
        if let Some(pp) = resp.interact_pointer_pos()
            && let Some(id) = s.ed.add_graphic(catalog::TEXT, None)
        {
            let n = (pp - pic.min) / pic.size();
            let comp =
                s.ed.active_seq()
                    .and_then(|q| q.clip(id))
                    .and_then(|c| c.component(catalog::TEXT))
                    .map(|c| c.id);
            if let (Some(comp), Some(sid)) = (comp, s.ed.active) {
                let val = Value::Point([n.x.clamp(0.0, 1.0) as f64, n.y.clamp(0.0, 1.0) as f64]);
                s.ed.edit_merge("New Graphic", None, |p| {
                    effect_controls::write_param(p, sid, id, comp, "position", val, SrcTime::ZERO)
                });
            }
            s.ed.tool = op_application::Tool::Selection;
        }
        return;
    }
    // the selected video clip under the playhead gets a transform box
    let selected =
        s.ed.selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .find(|c| c.is_video() && c.range().contains(t))
            .cloned();
    let boxed = selected
        .as_ref()
        .and_then(|c| clip_box(&s.ed.project, &seq, c, t).map(|b| (c.id, b)));
    let painter = ui.painter().with_clip_rect(pic.expand(40.0));
    let mut handle_hit = None;
    if let Some((id, (corners, center))) = &boxed {
        let pts: Vec<Pos2> = corners.iter().map(|p| to_screen(*p)).collect();
        painter.add(egui::Shape::closed_line(
            pts.clone(),
            Stroke::new(1.0, theme::ACCENT),
        ));
        for p in &pts {
            painter.rect_filled(
                Rect::from_center_size(*p, vec2(7.0, 7.0)),
                0.0,
                theme::ACCENT,
            );
            if resp.hover_pos().is_some_and(|h| h.distance(*p) < 8.0) {
                handle_hit = Some((*id, to_screen(*center)));
            }
        }
        let c = to_screen(*center);
        painter.circle_stroke(c, 5.0, Stroke::new(1.0, theme::ACCENT));
        if handle_hit.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        } else if resp
            .hover_pos()
            .is_some_and(|h| inside(corners, pos2((h.x - pic.min.x) / k, (h.y - pic.min.y) / k)))
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        }
    }
    if resp.drag_started()
        && let Some(pp) = resp.interact_pointer_pos()
    {
        let seq_p = pos2((pp.x - pic.min.x) / k, (pp.y - pic.min.y) / k);
        if let Some((id, center)) = handle_hit {
            let scale = seq
                .clip(id)
                .and_then(|c| {
                    c.component(catalog::MOTION)
                        .map(|m| m.f64_at("scale", c.to_source(t)))
                })
                .unwrap_or(100.0);
            v.drag = Some(MotionDrag::Scale {
                clip: id,
                start: scale,
                center,
                dist: pp.distance(center).max(1.0),
            });
        } else {
            // hit test from the top track down
            let mut hit = boxed
                .as_ref()
                .filter(|(_, (c, _))| inside(c, seq_p))
                .map(|(id, _)| *id);
            if hit.is_none() {
                'tracks: for tr in seq.video.iter().rev() {
                    if let Some(c) = tr.clip_at(t)
                        && let Some((corners, _)) = clip_box(&s.ed.project, &seq, c, t)
                        && inside(&corners, seq_p)
                    {
                        hit = Some(c.id);
                        break 'tracks;
                    }
                }
                if let Some(id) = hit {
                    let with = if s.ed.prefs.linked_selection {
                        seq.linked(id)
                    } else {
                        vec![id]
                    };
                    s.ed.selection = op_application::Selection::only(with);
                }
            }
            if let Some(id) = hit
                && let Some(c) = seq.clip(id)
                && let Some(m) = c.component(catalog::MOTION)
            {
                let start = m.value_at("position", c.to_source(t)).as_point();
                v.drag = Some(MotionDrag::Move {
                    clip: id,
                    start,
                    from: pp,
                });
            }
        }
    }
    if resp.dragged()
        && let Some(pp) = resp.interact_pointer_pos()
    {
        match &v.drag {
            Some(MotionDrag::Move { clip, start, from }) => {
                let d = (pp - *from) / k;
                let val = Value::Point([
                    start[0] + d.x as f64 / seq.settings.width as f64,
                    start[1] + d.y as f64 / seq.settings.height as f64,
                ]);
                write_motion(s, *clip, "position", val);
            }
            Some(MotionDrag::Scale {
                clip,
                start,
                center,
                dist,
            }) => {
                let f = pp.distance(*center) / dist;
                write_motion(
                    s,
                    *clip,
                    "scale",
                    Value::Float((start * f as f64).clamp(0.0, 10000.0)),
                );
            }
            None => {}
        }
    }
    if resp.drag_stopped() {
        v.drag = None;
        s.ed.seal();
    }
    if resp.clicked() && handle_hit.is_none() {
        let pp = resp.interact_pointer_pos().unwrap_or_default();
        let seq_p = pos2((pp.x - pic.min.x) / k, (pp.y - pic.min.y) / k);
        let mut hit = None;
        for tr in seq.video.iter().rev() {
            if let Some(c) = tr.clip_at(t)
                && let Some((corners, _)) = clip_box(&s.ed.project, &seq, c, t)
                && inside(&corners, seq_p)
            {
                hit = Some(c.id);
                break;
            }
        }
        match hit {
            Some(id) => {
                let with = if s.ed.prefs.linked_selection {
                    seq.linked(id)
                } else {
                    vec![id]
                };
                s.ed.selection = op_application::Selection::only(with);
            }
            None => s.ed.selection.clear(),
        }
    }
}
