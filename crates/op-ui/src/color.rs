//! A color picker whose text follows the interface language (the toolkit's own picker has fixed
//! English labels): a saturation/brightness square, a hue strip, an optional opacity strip and
//! hexadecimal and RGB fields. Colors are straight (unpremultiplied) sRGB values in 0..1.

use egui::{Color32, Mesh, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::i18n::t;
use crate::theme;

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max <= 1e-6 { 0.0 } else { d / max };
    [h, s, max]
}

fn c32(rgb: [f32; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn checker(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, 0.0, Color32::from_gray(150));
    let n = 6.0;
    let mut y = r.min.y;
    let mut row = 0;
    while y < r.max.y {
        let mut x = r.min.x + if row % 2 == 0 { 0.0 } else { n };
        while x < r.max.x {
            p.rect_filled(
                Rect::from_min_max(pos2(x, y), pos2((x + n).min(r.max.x), (y + n).min(r.max.y))),
                0.0,
                Color32::from_gray(210),
            );
            x += 2.0 * n;
        }
        y += n;
        row += 1;
    }
}

/// A swatch that opens the picker when clicked. Returns true when the color changed.
pub fn button(ui: &mut Ui, rgba: &mut [f32; 4], alpha: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(44.0, 18.0), Sense::click());
    let p = ui.painter();
    if alpha && rgba[3] < 1.0 {
        checker(p, rect);
    }
    p.rect_filled(
        rect,
        2.0,
        c32(
            [rgba[0], rgba[1], rgba[2]],
            if alpha { rgba[3] } else { 1.0 },
        ),
    );
    p.rect_stroke(
        rect,
        2.0,
        Stroke::new(
            1.0,
            if resp.hovered() {
                theme::TEXT
            } else {
                theme::LINE
            },
        ),
        StrokeKind::Inside,
    );
    let resp = resp.on_hover_text(t("Click to edit color"));
    let mut changed = false;
    egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            changed = picker(ui, resp.id, rgba, alpha);
        });
    changed
}

/// The picker itself (inline). Returns true when the color changed.
pub fn picker(ui: &mut Ui, id: egui::Id, rgba: &mut [f32; 4], alpha: bool) -> bool {
    // hue and saturation are kept while the color is gray or black, where they are undefined
    let mem = id.with("hsv");
    let current = [rgba[0], rgba[1], rgba[2]];
    let mut hsv = ui
        .data(|d| d.get_temp::<([f32; 3], [f32; 3])>(mem))
        .filter(|(_, rgb)| *rgb == current)
        .map(|(h, _)| h)
        .unwrap_or_else(|| rgb_to_hsv(current));
    let mut changed = false;
    let size = 180.0;
    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

    // saturation (x) and brightness (y)
    let (sq, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let n = 16;
    let mut mesh = Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (s, v) = (i as f32 / n as f32, 1.0 - j as f32 / n as f32);
            let pos = pos2(sq.min.x + s * size, sq.min.y + (1.0 - v) * size);
            mesh.colored_vertex(pos, c32(hsv_to_rgb(hsv[0], s, v), 1.0));
        }
    }
    for j in 0..n {
        for i in 0..n {
            let a = (j * (n + 1) + i) as u32;
            let b = a + 1;
            let c = a + (n + 1) as u32;
            let d = c + 1;
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    ui.painter().add(mesh);
    let marker = pos2(sq.min.x + hsv[1] * size, sq.min.y + (1.0 - hsv[2]) * size);
    ui.painter()
        .circle_stroke(marker, 5.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter()
        .circle_stroke(marker, 6.5, Stroke::new(1.0, Color32::BLACK));
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        hsv[1] = ((p.x - sq.min.x) / size).clamp(0.0, 1.0);
        hsv[2] = (1.0 - (p.y - sq.min.y) / size).clamp(0.0, 1.0);
        changed = true;
    }
    resp.on_hover_text(t("Saturation and brightness"));

    // hue
    let (strip, resp) = ui.allocate_exact_size(vec2(size, 14.0), Sense::click_and_drag());
    let steps = 36;
    let mut mesh = Mesh::default();
    for k in 0..=steps {
        let h = k as f32 / steps as f32;
        let x = strip.min.x + h * size;
        let c = c32(hsv_to_rgb(h, 1.0, 1.0), 1.0);
        mesh.colored_vertex(pos2(x, strip.min.y), c);
        mesh.colored_vertex(pos2(x, strip.max.y), c);
    }
    for k in 0..steps as u32 {
        let a = k * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    ui.painter().add(mesh);
    let hx = strip.min.x + hsv[0] * size;
    ui.painter().rect_stroke(
        Rect::from_center_size(pos2(hx, strip.center().y), vec2(4.0, 16.0)),
        1.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Middle,
    );
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        hsv[0] = ((p.x - strip.min.x) / size).clamp(0.0, 0.9999);
        changed = true;
    }
    resp.on_hover_text(t("Hue"));

    // opacity
    if alpha {
        let (strip, resp) = ui.allocate_exact_size(vec2(size, 14.0), Sense::click_and_drag());
        checker(ui.painter(), strip);
        let rgb = hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
        let mut mesh = Mesh::default();
        mesh.colored_vertex(strip.left_top(), c32(rgb, 0.0));
        mesh.colored_vertex(strip.left_bottom(), c32(rgb, 0.0));
        mesh.colored_vertex(strip.right_top(), c32(rgb, 1.0));
        mesh.colored_vertex(strip.right_bottom(), c32(rgb, 1.0));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        ui.painter().add(mesh);
        let ax = strip.min.x + rgba[3] * size;
        ui.painter().rect_stroke(
            Rect::from_center_size(pos2(ax, strip.center().y), vec2(4.0, 16.0)),
            1.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Middle,
        );
        if let Some(p) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked())
        {
            rgba[3] = ((p.x - strip.min.x) / size).clamp(0.0, 1.0);
            changed = true;
        }
        resp.on_hover_text(t("Opacity"));
    }
    if changed {
        let rgb = hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
        rgba[..3].copy_from_slice(&rgb);
    }

    // numeric entry
    let mut bytes = [
        (rgba[0] * 255.0).round() as i32,
        (rgba[1] * 255.0).round() as i32,
        (rgba[2] * 255.0).round() as i32,
    ];
    let mut typed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (b, name) in bytes.iter_mut().zip([t("Red"), t("Green"), t("Blue")]) {
            let letter = name.chars().next().unwrap_or('?');
            if ui
                .add(
                    egui::DragValue::new(b)
                        .range(0..=255)
                        .prefix(format!("{letter} ")),
                )
                .on_hover_text(name)
                .changed()
            {
                typed = true;
            }
        }
    });
    let hex_id = id.with("hex");
    let mut hex: String = ui
        .data(|d| d.get_temp(hex_id))
        .unwrap_or_else(|| format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut hex)
            .desired_width(size)
            .font(egui::TextStyle::Monospace),
    );
    let resp = resp.on_hover_text(t("Hexadecimal color"));
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(hex_id, hex.clone()));
        if let Some(c) = parse_hex(&hex)
            && c != bytes
        {
            bytes = c;
            typed = true;
        }
    } else {
        ui.data_mut(|d| d.remove::<String>(hex_id));
    }
    if typed {
        for (k, b) in bytes.iter().enumerate() {
            rgba[k] = *b as f32 / 255.0;
        }
        hsv = rgb_to_hsv([rgba[0], rgba[1], rgba[2]]);
        changed = true;
    }
    ui.data_mut(|d| d.insert_temp(mem, (hsv, [rgba[0], rgba[1], rgba[2]])));
    changed
}

fn parse_hex(text: &str) -> Option<[i32; 3]> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| i32::from_str_radix(&h[i..i + 2], 16).ok();
    Some([v(0)?, v(2)?, v(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.2, 0.4, 0.6],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.5, 0.5, 0.1],
        ] {
            let back = hsv_to_rgb(rgb_to_hsv(rgb)[0], rgb_to_hsv(rgb)[1], rgb_to_hsv(rgb)[2]);
            for k in 0..3 {
                assert!((back[k] - rgb[k]).abs() < 1e-5, "{rgb:?} -> {back:?}");
            }
        }
        assert_eq!(parse_hex("#FF8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("12345"), None);
    }
}
