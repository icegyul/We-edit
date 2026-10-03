//! Original vector icons, drawn with the painter so they stay sharp at any UI scale.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Play,
    Pause,
    StepBack,
    StepForward,
    GoIn,
    GoOut,
    MarkIn,
    MarkOut,
    Marker,
    Insert,
    Overwrite,
    Lift,
    Extract,
    Camera,
    Wrench,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    SyncLock,
    Snap,
    Link,
    Plus,
    Folder,
    Film,
    Sequence,
    Music,
    Image,
    Stopwatch,
    KeyPrev,
    KeyNext,
    KeyAdd,
    Reset,
    Fx,
    List,
    Grid,
    Search,
    Trash,
    Loop,
    SafeMargins,
    Proxy,
    Selection,
    TrackForward,
    TrackBackward,
    Ripple,
    Roll,
    RateStretch,
    Razor,
    Slip,
    Slide,
    Pen,
    Hand,
    Zoom,
    Type,
    ChevronRight,
    ChevronDown,
}

fn line(p: &Painter, pts: &[Pos2], s: Stroke) {
    p.add(Shape::line(pts.to_vec(), s));
}

fn fill(p: &Painter, pts: &[Pos2], c: Color32) {
    p.add(Shape::convex_polygon(pts.to_vec(), c, Stroke::NONE));
}

fn arc(
    center: (f32, f32),
    radius: (f32, f32),
    from: f32,
    to: f32,
    n: usize,
    at: &dyn Fn(f32, f32) -> Pos2,
) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            at(center.0 + radius.0 * a.cos(), center.1 + radius.1 * a.sin())
        })
        .collect()
}

/// Draws `icon` centered in `r`.
pub fn draw(p: &Painter, r: Rect, icon: Icon, c: Color32) {
    use std::f32::consts::PI;
    let s = r.width().min(r.height());
    let r = Rect::from_center_size(r.center(), vec2(s, s)).shrink(s * 0.14);
    let (x0, y0, w) = (r.left(), r.top(), r.width());
    let h = r.height();
    let at = |x: f32, y: f32| pos2(x0 + x * w, y0 + y * h);
    let st = Stroke::new((s * 0.08).max(1.2), c);
    let thin = Stroke::new((s * 0.06).max(1.0), c);
    let rect = |a: (f32, f32), b: (f32, f32)| Rect::from_min_max(at(a.0, a.1), at(b.0, b.1));
    match icon {
        Icon::Play => fill(p, &[at(0.2, 0.08), at(0.9, 0.5), at(0.2, 0.92)], c),
        Icon::Pause => {
            p.rect_filled(rect((0.18, 0.1), (0.4, 0.9)), 1.0, c);
            p.rect_filled(rect((0.6, 0.1), (0.82, 0.9)), 1.0, c);
        }
        Icon::StepBack => {
            p.rect_filled(rect((0.12, 0.15), (0.24, 0.85)), 0.0, c);
            fill(p, &[at(0.88, 0.15), at(0.88, 0.85), at(0.32, 0.5)], c);
        }
        Icon::StepForward => {
            p.rect_filled(rect((0.76, 0.15), (0.88, 0.85)), 0.0, c);
            fill(p, &[at(0.12, 0.15), at(0.12, 0.85), at(0.68, 0.5)], c);
        }
        Icon::GoIn => {
            line(
                p,
                &[at(0.35, 0.1), at(0.15, 0.1), at(0.15, 0.9), at(0.35, 0.9)],
                st,
            );
            fill(p, &[at(0.9, 0.2), at(0.9, 0.8), at(0.35, 0.5)], c);
        }
        Icon::GoOut => {
            line(
                p,
                &[at(0.65, 0.1), at(0.85, 0.1), at(0.85, 0.9), at(0.65, 0.9)],
                st,
            );
            fill(p, &[at(0.1, 0.2), at(0.1, 0.8), at(0.65, 0.5)], c);
        }
        Icon::MarkIn => line(
            p,
            &[at(0.7, 0.08), at(0.3, 0.08), at(0.3, 0.92), at(0.7, 0.92)],
            st,
        ),
        Icon::MarkOut => line(
            p,
            &[at(0.3, 0.08), at(0.7, 0.08), at(0.7, 0.92), at(0.3, 0.92)],
            st,
        ),
        Icon::Marker => fill(
            p,
            &[
                at(0.25, 0.1),
                at(0.75, 0.1),
                at(0.75, 0.6),
                at(0.5, 0.9),
                at(0.25, 0.6),
            ],
            c,
        ),
        Icon::Insert => {
            p.rect_stroke(
                rect((0.1, 0.5), (0.9, 0.9)),
                1.0,
                thin,
                egui::StrokeKind::Inside,
            );
            line(p, &[at(0.5, 0.05), at(0.5, 0.65)], st);
            line(p, &[at(0.3, 0.45), at(0.5, 0.65), at(0.7, 0.45)], st);
        }
        Icon::Overwrite => {
            p.rect_filled(rect((0.1, 0.55), (0.9, 0.9)), 1.0, c.gamma_multiply(0.6));
            line(p, &[at(0.5, 0.05), at(0.5, 0.5)], st);
            line(p, &[at(0.3, 0.3), at(0.5, 0.5), at(0.7, 0.3)], st);
        }
        Icon::Lift => {
            p.rect_stroke(
                rect((0.1, 0.6), (0.9, 0.9)),
                1.0,
                thin,
                egui::StrokeKind::Inside,
            );
            line(p, &[at(0.5, 0.55), at(0.5, 0.05)], st);
            line(p, &[at(0.3, 0.25), at(0.5, 0.05), at(0.7, 0.25)], st);
        }
        Icon::Extract => {
            line(p, &[at(0.1, 0.75), at(0.9, 0.75)], st);
            line(p, &[at(0.5, 0.6), at(0.5, 0.05)], st);
            line(p, &[at(0.3, 0.25), at(0.5, 0.05), at(0.7, 0.25)], st);
        }
        Icon::Camera => {
            p.rect_stroke(
                rect((0.08, 0.3), (0.92, 0.85)),
                2.0,
                st,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(rect((0.3, 0.18), (0.55, 0.3)), 1.0, c);
            p.circle_stroke(at(0.5, 0.58), w * 0.17, st);
        }
        Icon::Wrench => {
            line(
                p,
                &[at(0.15, 0.85), at(0.6, 0.4)],
                Stroke::new(st.width * 1.4, c),
            );
            p.circle_stroke(at(0.68, 0.32), w * 0.18, st);
        }
        Icon::Eye | Icon::EyeOff => {
            let mut pts = arc((0.5, 0.5), (0.42, 0.28), PI, 2.0 * PI, 16, &at);
            pts.extend(arc((0.5, 0.5), (0.42, 0.28), 0.0, PI, 16, &at));
            line(p, &pts, thin);
            p.circle_filled(at(0.5, 0.5), w * 0.12, c);
            if icon == Icon::EyeOff {
                line(p, &[at(0.12, 0.88), at(0.88, 0.12)], st);
            }
        }
        Icon::Lock | Icon::Unlock => {
            p.rect_filled(rect((0.2, 0.45), (0.8, 0.92)), 1.5, c);
            let top = if icon == Icon::Lock { 0.45 } else { 0.32 };
            let mut pts = vec![at(0.3, top)];
            pts.extend(arc((0.5, top), (0.2, 0.28), PI, 2.0 * PI, 12, &at));
            if icon == Icon::Lock {
                pts.push(at(0.7, 0.45));
            }
            line(p, &pts, st);
        }
        Icon::SyncLock => {
            p.circle_stroke(at(0.35, 0.5), w * 0.2, st);
            p.circle_stroke(at(0.65, 0.5), w * 0.2, st);
        }
        Icon::Snap => {
            let mut v = vec![at(0.2, 0.1), at(0.2, 0.55)];
            v.extend(
                arc((0.5, 0.55), (0.3, 0.3), PI, 0.0, 12, &at)
                    .into_iter()
                    .map(|q| pos2(q.x, 2.0 * at(0.0, 0.55).y - q.y)),
            );
            v.push(at(0.8, 0.1));
            line(p, &v, Stroke::new(st.width * 1.3, c));
        }
        Icon::Link => {
            p.rect_stroke(
                rect((0.08, 0.35), (0.55, 0.65)),
                4.0,
                st,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                rect((0.45, 0.35), (0.92, 0.65)),
                4.0,
                st,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Plus => {
            line(p, &[at(0.5, 0.12), at(0.5, 0.88)], st);
            line(p, &[at(0.12, 0.5), at(0.88, 0.5)], st);
        }
        Icon::Folder => {
            p.rect_filled(rect((0.05, 0.2), (0.45, 0.35)), 1.0, c);
            p.rect_filled(rect((0.05, 0.3), (0.95, 0.85)), 1.5, c);
        }
        Icon::Film => {
            p.rect_stroke(
                rect((0.08, 0.15), (0.92, 0.85)),
                1.0,
                thin,
                egui::StrokeKind::Inside,
            );
            for i in 0..4 {
                let x = 0.15 + i as f32 * 0.2;
                p.rect_filled(rect((x, 0.2), (x + 0.08, 0.3)), 0.0, c);
                p.rect_filled(rect((x, 0.7), (x + 0.08, 0.8)), 0.0, c);
            }
        }
        Icon::Sequence => {
            p.rect_filled(rect((0.08, 0.2), (0.6, 0.42)), 1.0, c);
            p.rect_filled(rect((0.3, 0.58), (0.92, 0.8)), 1.0, c.gamma_multiply(0.7));
        }
        Icon::Music => {
            line(
                p,
                &[at(0.4, 0.75), at(0.4, 0.15), at(0.85, 0.08), at(0.85, 0.65)],
                st,
            );
            p.circle_filled(at(0.3, 0.78), w * 0.12, c);
            p.circle_filled(at(0.75, 0.68), w * 0.12, c);
        }
        Icon::Image => {
            p.rect_stroke(
                rect((0.08, 0.15), (0.92, 0.85)),
                1.0,
                thin,
                egui::StrokeKind::Inside,
            );
            fill(p, &[at(0.15, 0.8), at(0.42, 0.45), at(0.62, 0.8)], c);
            p.circle_filled(at(0.7, 0.35), w * 0.08, c);
        }
        Icon::Stopwatch => {
            p.circle_stroke(at(0.5, 0.56), w * 0.36, thin);
            line(p, &[at(0.5, 0.56), at(0.5, 0.34)], thin);
            line(p, &[at(0.4, 0.08), at(0.6, 0.08)], thin);
        }
        Icon::KeyPrev => fill(p, &[at(0.75, 0.2), at(0.75, 0.8), at(0.25, 0.5)], c),
        Icon::KeyNext => fill(p, &[at(0.25, 0.2), at(0.25, 0.8), at(0.75, 0.5)], c),
        Icon::KeyAdd => fill(
            p,
            &[at(0.5, 0.12), at(0.88, 0.5), at(0.5, 0.88), at(0.12, 0.5)],
            c,
        ),
        Icon::Reset => {
            let pts = arc((0.5, 0.5), (0.36, 0.36), PI * 0.3, PI * 1.7, 14, &at);
            let end = pts[0];
            line(p, &pts, thin);
            fill(
                p,
                &[
                    end,
                    end + vec2(w * 0.2, -h * 0.02),
                    end + vec2(w * 0.04, -h * 0.2),
                ],
                c,
            );
        }
        Icon::Fx => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "fx",
                egui::FontId::proportional(s * 0.6),
                c,
            );
        }
        Icon::List => {
            for i in 0..3 {
                let y = 0.2 + i as f32 * 0.3;
                line(p, &[at(0.1, y), at(0.9, y)], st);
            }
        }
        Icon::Grid => {
            for (x, y) in [(0.1, 0.1), (0.55, 0.1), (0.1, 0.55), (0.55, 0.55)] {
                p.rect_filled(rect((x, y), (x + 0.35, y + 0.35)), 1.0, c);
            }
        }
        Icon::Search => {
            p.circle_stroke(at(0.42, 0.42), w * 0.28, st);
            line(p, &[at(0.62, 0.62), at(0.9, 0.9)], st);
        }
        Icon::Trash => {
            line(p, &[at(0.15, 0.22), at(0.85, 0.22)], st);
            line(p, &[at(0.4, 0.12), at(0.6, 0.12)], st);
            line(
                p,
                &[at(0.25, 0.3), at(0.3, 0.9), at(0.7, 0.9), at(0.75, 0.3)],
                st,
            );
        }
        Icon::Loop => {
            line(p, &[at(0.15, 0.6), at(0.15, 0.3), at(0.8, 0.3)], st);
            line(p, &[at(0.85, 0.4), at(0.85, 0.7), at(0.2, 0.7)], st);
            fill(p, &[at(0.72, 0.18), at(0.92, 0.3), at(0.72, 0.42)], c);
        }
        Icon::SafeMargins => {
            p.rect_stroke(
                rect((0.05, 0.15), (0.95, 0.85)),
                0.0,
                thin,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                rect((0.2, 0.3), (0.8, 0.7)),
                0.0,
                thin,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Proxy => {
            // a full frame with a smaller copy of it: the proxy
            p.rect_stroke(
                rect((0.05, 0.12), (0.72, 0.62)),
                1.0,
                thin,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(rect((0.45, 0.48), (0.95, 0.88)), 1.0, c);
        }
        Icon::Selection => {
            fill(
                p,
                &[
                    at(0.25, 0.05),
                    at(0.25, 0.78),
                    at(0.42, 0.62),
                    at(0.72, 0.58),
                ],
                c,
            );
            fill(
                p,
                &[
                    at(0.4, 0.58),
                    at(0.53, 0.55),
                    at(0.68, 0.87),
                    at(0.56, 0.92),
                ],
                c,
            );
        }
        Icon::TrackForward | Icon::TrackBackward => {
            let dir = if icon == Icon::TrackForward {
                1.0
            } else {
                -1.0
            };
            for k in 0..2 {
                let x = 0.5 + dir * (k as f32 * 0.28 - 0.14);
                let pts = [
                    at(x - dir * 0.2, 0.2),
                    at(x + dir * 0.15, 0.5),
                    at(x - dir * 0.2, 0.8),
                ];
                if dir > 0.0 {
                    fill(p, &pts, c)
                } else {
                    fill(p, &[pts[2], pts[1], pts[0]], c)
                }
            }
        }
        Icon::Ripple => {
            line(
                p,
                &[at(0.45, 0.1), at(0.25, 0.1), at(0.25, 0.9), at(0.45, 0.9)],
                st,
            );
            fill(p, &[at(0.45, 0.35), at(0.85, 0.5), at(0.45, 0.65)], c);
        }
        Icon::Roll => {
            line(p, &[at(0.35, 0.1), at(0.65, 0.1)], st);
            line(p, &[at(0.5, 0.1), at(0.5, 0.9)], st);
            line(p, &[at(0.35, 0.9), at(0.65, 0.9)], st);
            fill(p, &[at(0.3, 0.35), at(0.3, 0.65), at(0.05, 0.5)], c);
            fill(p, &[at(0.7, 0.35), at(0.95, 0.5), at(0.7, 0.65)], c);
        }
        Icon::RateStretch => {
            line(p, &[at(0.1, 0.15), at(0.1, 0.85)], st);
            line(p, &[at(0.9, 0.15), at(0.9, 0.85)], st);
            line(p, &[at(0.2, 0.5), at(0.8, 0.5)], thin);
            fill(p, &[at(0.2, 0.5), at(0.38, 0.35), at(0.38, 0.65)], c);
            fill(p, &[at(0.8, 0.5), at(0.62, 0.65), at(0.62, 0.35)], c);
        }
        Icon::Razor => {
            fill(
                p,
                &[at(0.1, 0.55), at(0.7, 0.2), at(0.9, 0.35), at(0.3, 0.7)],
                c,
            );
            line(p, &[at(0.28, 0.72), at(0.15, 0.92)], st);
        }
        Icon::Slip => {
            line(p, &[at(0.3, 0.15), at(0.3, 0.85)], st);
            line(p, &[at(0.7, 0.15), at(0.7, 0.85)], st);
            fill(p, &[at(0.05, 0.5), at(0.22, 0.35), at(0.22, 0.65)], c);
            fill(p, &[at(0.95, 0.5), at(0.78, 0.65), at(0.78, 0.35)], c);
        }
        Icon::Slide => {
            p.rect_filled(rect((0.35, 0.25), (0.65, 0.75)), 1.0, c);
            fill(p, &[at(0.05, 0.5), at(0.25, 0.32), at(0.25, 0.68)], c);
            fill(p, &[at(0.95, 0.5), at(0.75, 0.68), at(0.75, 0.32)], c);
        }
        Icon::Pen => fill(
            p,
            &[
                at(0.15, 0.85),
                at(0.25, 0.55),
                at(0.7, 0.1),
                at(0.9, 0.3),
                at(0.45, 0.75),
            ],
            c,
        ),
        Icon::Hand => {
            for (i, top) in [0.25f32, 0.12, 0.15, 0.25].iter().enumerate() {
                let x = 0.28 + i as f32 * 0.14;
                p.rect_filled(rect((x, *top), (x + 0.1, 0.6)), 3.0, c);
            }
            p.rect_filled(rect((0.26, 0.5), (0.8, 0.88)), 4.0, c);
        }
        Icon::Zoom => {
            p.circle_stroke(at(0.42, 0.42), w * 0.28, st);
            line(p, &[at(0.62, 0.62), at(0.9, 0.9)], st);
            line(p, &[at(0.3, 0.42), at(0.54, 0.42)], thin);
            line(p, &[at(0.42, 0.3), at(0.42, 0.54)], thin);
        }
        Icon::Type => {
            line(
                p,
                &[at(0.15, 0.2), at(0.85, 0.2)],
                Stroke::new(st.width * 1.2, c),
            );
            line(
                p,
                &[at(0.5, 0.2), at(0.5, 0.88)],
                Stroke::new(st.width * 1.2, c),
            );
        }
        Icon::ChevronRight => line(p, &[at(0.35, 0.2), at(0.65, 0.5), at(0.35, 0.8)], st),
        Icon::ChevronDown => line(p, &[at(0.2, 0.35), at(0.5, 0.65), at(0.8, 0.35)], st),
    }
}

/// A square icon button. `on` shows a pressed/active state.
pub fn button(ui: &mut Ui, icon: Icon, size: f32, on: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let color = if on {
        crate::theme::ACCENT
    } else if resp.hovered() {
        crate::theme::TEXT_BRIGHT
    } else {
        crate::theme::TEXT
    };
    if resp.hovered() || on {
        ui.painter().rect_filled(rect, 3.0, crate::theme::RAISED);
    }
    draw(ui.painter(), rect, icon, color);
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// The Razor tool's cursor: open scissors whose blades meet at `tip` (the cut point), drawn
/// translucent with a dark rim so the clip underneath stays visible.
pub fn scissors(p: &Painter, tip: Pos2, size: f32) {
    let k = size / 24.0;
    let at = |x: f32, y: f32| tip + vec2((x - 12.0) * k, y * k);
    let fill = Color32::from_rgba_unmultiplied(255, 255, 255, 190);
    let rim = Color32::from_rgba_unmultiplied(0, 0, 0, 150);
    let pivot = at(12.0, 11.6);
    // two slim blades open in a V around the cut line
    let blades = [
        [at(7.2, 0.0), at(8.8, 0.3), at(12.9, 11.4), at(11.5, 11.8)],
        [at(16.8, 0.0), at(15.2, 0.3), at(11.1, 11.4), at(12.5, 11.8)],
    ];
    for b in &blades {
        p.add(Shape::convex_polygon(
            b.to_vec(),
            fill,
            Stroke::new((1.2 * k).max(0.6), rim),
        ));
    }
    // arms down to the finger rings
    for x1 in [7.4, 16.6] {
        p.line_segment([at(12.0, 11.5), at(x1, 16.3)], Stroke::new(3.2 * k, rim));
        p.line_segment([at(12.0, 11.5), at(x1, 16.3)], Stroke::new(1.8 * k, fill));
    }
    for x in [6.2, 17.8] {
        let c = at(x, 19.3);
        p.circle_stroke(c, 3.4 * k, Stroke::new(3.0 * k, rim));
        p.circle_stroke(c, 3.4 * k, Stroke::new(1.7 * k, fill));
    }
    p.circle_filled(pivot, 1.3 * k, rim);
}

/// The Hand tool's pointer, drawn by the program because the system has no open-hand cursor on
/// every platform (Windows shows four arrows for it). `closed` while dragging. `at` is the
/// middle of the palm.
pub fn hand(p: &Painter, at: Pos2, size: f32, closed: bool) {
    let k = size / 24.0;
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| {
        Rect::from_min_max(
            at + vec2((x0 - 12.0) * k, (y0 - 12.0) * k),
            at + vec2((x1 - 12.0) * k, (y1 - 12.0) * k),
        )
    };
    let fill = Color32::from_rgb(250, 250, 252);
    let rim = Color32::from_rgba_unmultiplied(0, 0, 0, 200);
    // fingers: long when open, folded over the palm when grabbing
    let tops: [f32; 4] = if closed {
        [8.5, 7.5, 8.0, 9.0]
    } else {
        [3.5, 1.5, 2.5, 5.0]
    };
    let mut parts: Vec<(Rect, f32)> = tops
        .iter()
        .enumerate()
        .map(|(i, top)| {
            let x = 6.2 + i as f32 * 3.4;
            (r(x, *top, x + 3.0, 14.0), 1.5 * k)
        })
        .collect();
    // palm, and the thumb at its side
    parts.push((r(6.0, 10.0, 19.6, 21.5), 4.0 * k));
    let thumb = if closed {
        r(3.6, 12.0, 7.6, 16.0)
    } else {
        r(2.4, 11.0, 7.4, 14.2)
    };
    parts.push((thumb, 1.8 * k));
    let edge = 1.3 * k;
    for (rect, radius) in &parts {
        p.rect_filled(rect.expand(edge), radius + edge, rim);
    }
    for (rect, radius) in &parts {
        p.rect_filled(*rect, *radius, fill);
    }
    // the gaps between the fingers
    for i in 1..4 {
        let x = 6.2 + i as f32 * 3.4 - 0.2;
        let top = tops[i].max(tops[i - 1]) + 1.0;
        p.line_segment(
            [
                at + vec2((x - 12.0) * k, (top - 12.0) * k),
                at + vec2((x - 12.0) * k, (13.0 - 12.0) * k),
            ],
            Stroke::new(0.9 * k, Color32::from_rgba_unmultiplied(0, 0, 0, 120)),
        );
    }
}
