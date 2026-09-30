//! Small shared widgets.

use egui::{Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, vec2};
use op_core::{Dur, Parsed, TimecodeFormat};

use crate::theme;

/// A value that scrubs when dragged sideways and becomes editable when clicked, drawn as blue
/// text like the numeric fields editors know.
pub fn scrub(
    ui: &mut Ui,
    value: &mut f64,
    speed: f64,
    min: f64,
    max: f64,
    decimals: usize,
    suffix: &str,
) -> Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.bg_fill = Color32::TRANSPARENT;
            w.weak_bg_fill = Color32::TRANSPARENT;
            w.bg_stroke = Stroke::NONE;
            w.fg_stroke = Stroke::new(1.0, theme::VALUE);
        }
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, theme::TEXT_BRIGHT);
        ui.add(
            egui::DragValue::new(value)
                .speed(speed)
                .range(min..=max)
                .max_decimals(decimals)
                .min_decimals(decimals.min(1))
                .suffix(suffix),
        )
    })
    .inner
}

/// An integer scrubber.
pub fn scrub_int(ui: &mut Ui, value: &mut i64, min: i64, max: i64) -> Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.bg_fill = Color32::TRANSPARENT;
            w.weak_bg_fill = Color32::TRANSPARENT;
            w.bg_stroke = Stroke::NONE;
            w.fg_stroke = Stroke::new(1.0, theme::VALUE);
        }
        ui.add(egui::DragValue::new(value).speed(0.2).range(min..=max))
    })
    .inner
}

/// Timecode display that scrubs by frames when dragged and accepts typed timecode when
/// clicked. Returns the new offset when it changed.
pub fn timecode(
    ui: &mut Ui,
    id: egui::Id,
    fmt: &TimecodeFormat,
    value: Dur,
    size: f32,
    color: Color32,
) -> Option<Dur> {
    let editing_id = id.with("edit");
    let mut editing: Option<String> = ui.data(|d| d.get_temp(editing_id));
    if let Some(text) = editing.as_mut() {
        let r = ui.add(
            egui::TextEdit::singleline(text)
                .font(FontId::monospace(size))
                .desired_width(size * 7.0)
                .text_color(color),
        );
        r.request_focus();
        let done = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape)) || r.lost_focus() && !done;
        let mut out = None;
        if done {
            out = match fmt.parse(text) {
                Some(Parsed::Absolute(d)) => Some(d),
                Some(Parsed::Relative(d)) => Some(value + d),
                None => None,
            };
        }
        if done || cancel {
            ui.data_mut(|d| d.remove::<String>(editing_id));
        } else {
            ui.data_mut(|d| d.insert_temp(editing_id, text.clone()));
        }
        return out;
    }
    let text = fmt.format(value);
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), FontId::monospace(size), color);
    let (rect, resp) =
        ui.allocate_exact_size(galley.size() + vec2(4.0, 2.0), Sense::click_and_drag());
    ui.painter()
        .galley(rect.min + vec2(2.0, 1.0), galley, color);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if resp.clicked() {
        ui.data_mut(|d| d.insert_temp(editing_id, text));
        return None;
    }
    if resp.dragged() {
        let acc_id = id.with("acc");
        let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
        acc += resp.drag_delta().x / 4.0;
        let frames = acc.trunc() as i64;
        acc -= frames as f32;
        ui.data_mut(|d| d.insert_temp(acc_id, acc));
        if frames != 0 {
            return Some((value + fmt.rate.frames_to_dur(frames)).max(Dur::ZERO));
        }
    }
    None
}

/// A keyframe diamond.
pub fn diamond(ui: &Ui, center: Pos2, r: f32, fill: Color32, selected: bool) {
    let pts = vec![
        center + vec2(0.0, -r),
        center + vec2(r, 0.0),
        center + vec2(0.0, r),
        center + vec2(-r, 0.0),
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        pts,
        if selected { theme::ACCENT } else { fill },
        Stroke::new(1.0, Color32::from_black_alpha(160)),
    ));
}

/// A collapsible section title. Returns whether it is open.
pub fn section(ui: &mut Ui, id: egui::Id, title: &str, default_open: bool) -> bool {
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(default_open);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
    ui.painter().rect_filled(rect, 0.0, theme::RAISED);
    let icon = if open {
        crate::icons::Icon::ChevronDown
    } else {
        crate::icons::Icon::ChevronRight
    };
    crate::icons::draw(
        ui.painter(),
        Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2(16.0, 16.0)),
        icon,
        theme::TEXT_DIM,
    );
    // long titles are cut at the row's end (full title on hover)
    let galley = ui.painter().layout_no_wrap(
        title.to_string(),
        FontId::proportional(12.5),
        theme::TEXT_BRIGHT,
    );
    let text_rect = Rect::from_min_max(rect.min + vec2(20.0, 0.0), rect.max - vec2(4.0, 0.0));
    let cut = galley.size().x > text_rect.width();
    ui.painter()
        .with_clip_rect(text_rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(text_rect.min.x, rect.center().y - galley.size().y / 2.0),
            galley,
            theme::TEXT_BRIGHT,
        );
    let resp = if cut { resp.on_hover_text(title) } else { resp };
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    open
}

pub fn dim_label(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .color(theme::TEXT_DIM)
            .size(11.5),
    );
}

/// Formats a duration as h:mm:ss for progress displays.
pub fn clock(d: std::time::Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
