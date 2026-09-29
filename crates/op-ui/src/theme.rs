//! Visual design: an original dark theme tuned for long editing sessions (panel-system-spec 7).

use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x20);
pub const PANEL: Color32 = Color32::from_rgb(0x24, 0x24, 0x28);
pub const PANEL_DARK: Color32 = Color32::from_rgb(0x19, 0x19, 0x1c);
pub const RAISED: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x33);
pub const LINE: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x40);
pub const TEXT: Color32 = Color32::from_rgb(0xc9, 0xc9, 0xcf);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x92);
pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(0xee, 0xee, 0xf2);
pub const ACCENT: Color32 = Color32::from_rgb(0x4a, 0x9c, 0xff);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x2a, 0x5a, 0x96);
pub const VALUE: Color32 = Color32::from_rgb(0x5d, 0xa8, 0xff);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0x4a, 0x9c, 0xff);
pub const KEYFRAME: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xdd);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xb4, 0x40);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x5a, 0x5a);
pub const VIDEO_TRACK: Color32 = Color32::from_rgb(0x26, 0x26, 0x2b);
pub const AUDIO_TRACK: Color32 = Color32::from_rgb(0x23, 0x25, 0x2a);
pub const SELECTED: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf5);
pub const SNAP: Color32 = Color32::from_rgb(0xff, 0xe0, 0x50);

pub fn label_color(l: op_core::Label, dim: bool) -> Color32 {
    let [r, g, b] = if l == op_core::Label::None {
        [96, 116, 170]
    } else {
        l.rgb()
    };
    if dim {
        Color32::from_rgb(r / 2 + 16, g / 2 + 16, b / 2 + 16)
    } else {
        Color32::from_rgb(r, g, b)
    }
}

pub fn audio_clip_color(l: op_core::Label) -> Color32 {
    if l == op_core::Label::None {
        Color32::from_rgb(58, 122, 96)
    } else {
        label_color(l, false)
    }
}

pub fn apply(ctx: &egui::Context, scale: f32) {
    ctx.set_zoom_factor(scale.clamp(0.75, 3.0));
    let mut style = Style {
        visuals: Visuals::dark(),
        ..Style::default()
    };
    let v = &mut style.visuals;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL_DARK;
    v.faint_bg_color = Color32::from_rgb(0x28, 0x28, 0x2d);
    v.code_bg_color = PANEL_DARK;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, TEXT_BRIGHT);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x3a, 0x3a, 0x42);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3a, 0x3a, 0x42);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(3);
    }
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 2.0);
    style.spacing.interact_size.y = 20.0;
    style.spacing.slider_width = 120.0;
    style.interaction.tooltip_delay = 0.4;
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}
