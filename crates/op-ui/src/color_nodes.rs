//! Color Nodes: a node graph for grading, in the manner of a colorist's node editor. A graph
//! runs the clip's picture through correctors (primaries, curves, qualifier, windows, key, LUT),
//! parallel mixers and layer mixers to the output. The graph is the hidden `graph` parameter of
//! the Color Nodes effect; every edit is one undo step (merged while dragging).

use egui::{
    Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use op_core::colorgraph::{
    Blend, ColorGraph, Corrector, Edge, Mixer, Node, NodeId, NodeKind, OUTPUT, SOURCE, View,
    Window, WindowKind,
};
use op_core::*;

use crate::app::State;
use crate::effect_controls;
use crate::i18n::{t, tn};
use crate::{panels, theme};

pub const EFFECT: &str = "op.video.color_nodes";

const NODE_W: f32 = 64.0;
const NODE_H: f32 = 34.0;
const SOCKET: f32 = 5.0;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Primaries,
    Curves,
    Qualifier,
    Windows,
    Key,
    Effects,
}

impl Tab {
    const ALL: [(Tab, &'static str); 6] = [
        (Tab::Primaries, "Primaries"),
        (Tab::Curves, "Curves"),
        (Tab::Qualifier, "Qualifier"),
        (Tab::Windows, "Windows"),
        (Tab::Key, "Key"),
        (Tab::Effects, "LUT & Effects"),
    ];
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum CurveKind {
    #[default]
    Master,
    Red,
    Green,
    Blue,
    HueHue,
    HueSat,
    LumSat,
    SatSat,
}

impl CurveKind {
    const ALL: [(CurveKind, &'static str); 8] = [
        (CurveKind::Master, "Master"),
        (CurveKind::Red, "Red"),
        (CurveKind::Green, "Green"),
        (CurveKind::Blue, "Blue"),
        (CurveKind::HueHue, "Hue vs Hue"),
        (CurveKind::HueSat, "Hue vs Sat"),
        (CurveKind::LumSat, "Lum vs Sat"),
        (CurveKind::SatSat, "Sat vs Sat"),
    ];
}

#[derive(Default)]
pub struct NodesView {
    sel: NodeId,
    tab: Tab,
    curve: CurveKind,
    pan: egui::Vec2,
    drag_node: Option<NodeId>,
    /// A connection being dragged from this node's output (picture or key).
    link: Option<(NodeId, bool)>,
    /// The graph being edited and the text it came from.
    cache: Option<(String, ColorGraph)>,
}

impl NodesView {
    fn selected(&self) -> NodeId {
        if self.sel == 0 { 2 } else { self.sel }
    }
}

pub fn show(s: &mut State, ui: &mut Ui) {
    let Some((sid, clip)) = panels::selected_video_clip(s) else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(t("Select a video clip to grade it with nodes"))
                    .color(theme::TEXT_DIM),
            )
        });
        return;
    };
    let comp =
        s.ed.project
            .sequence(sid)
            .and_then(|q| q.clip(clip))
            .and_then(|c| c.component(EFFECT))
            .map(|c| {
                (
                    c.id,
                    c.param("graph")
                        .map(|p| p.value.as_text().to_string())
                        .unwrap_or_default(),
                )
            });
    let Some((comp, text)) = comp else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(t(
                    "Grade this clip with a node graph: correctors, mixers, keys and windows.",
                ))
                .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            if ui.button(t("Add Color Nodes")).clicked() {
                s.ed.apply_effect(EFFECT, Some(vec![clip]));
            }
        });
        return;
    };

    let mut v = std::mem::take(&mut s.nodes);
    let mut g = match &v.cache {
        Some((cached, g)) if *cached == text => g.clone(),
        _ => ColorGraph::from_json(&text).unwrap_or_default(),
    };
    let before = g.clone();
    if g.node(v.selected()).is_none() {
        v.sel = 2;
    }

    toolbar(&mut v, &mut g, ui);
    ui.add_space(2.0);
    let height = (ui.available_height() * 0.42).clamp(150.0, 380.0);
    canvas(&mut v, &mut g, ui, height);
    ui.add_space(4.0);
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            inspector(s, &mut v, &mut g, ui);
        });

    if g != before {
        let json = g.to_json();
        v.cache = Some((json.clone(), g));
        let merge = Some(format!("nodes-{}-{}", clip.0, comp.0));
        s.ed.edit_merge("Change Color Nodes", merge, move |p| {
            effect_controls::write_param(
                p,
                sid,
                clip,
                comp,
                "graph",
                Value::Text(json),
                SrcTime::ZERO,
            )
        });
    } else {
        v.cache = Some((text, g));
    }
    s.nodes = v;
}

// ------------------------------------------------------------------------------------ toolbar

fn toolbar(v: &mut NodesView, g: &mut ColorGraph, ui: &mut Ui) {
    let sel = v.selected();
    let is_corrector = matches!(g.node(sel).map(|n| &n.kind), Some(NodeKind::Corrector(_)));
    let removable = sel != SOURCE && sel != OUTPUT;
    ui.horizontal_wrapped(|ui| {
        ui.add_space(6.0);
        if ui
            .button(t("+ Serial"))
            .on_hover_text(t("Add a corrector after the selected node"))
            .clicked()
        {
            v.sel = g.add_serial(sel);
        }
        if ui
            .add_enabled(is_corrector, egui::Button::new(t("+ Parallel")))
            .on_hover_text(t("Add a corrector beside the selected one and mix the two"))
            .clicked()
            && let Some(id) = g.add_parallel(sel)
        {
            v.sel = id;
        }
        if ui
            .button(t("+ Layer"))
            .on_hover_text(t("Add a layer above the selected node, cut by its own key"))
            .clicked()
            && let Some(id) = g.add_layer(sel)
        {
            v.sel = id;
        }
        if ui
            .add_enabled(is_corrector, egui::Button::new(t("+ Outside")))
            .on_hover_text(t(
                "Add a corrector that grades what the selected node does not select",
            ))
            .clicked()
        {
            v.sel = g.add_outside(sel);
        }
        ui.separator();
        if ui
            .add_enabled(removable, egui::Button::new(t("Delete")))
            .clicked()
        {
            g.remove(sel);
            v.sel = 2;
        }
        let enabled = g.node(sel).map(|n| n.enabled).unwrap_or(true);
        if ui
            .add_enabled(
                removable,
                egui::Button::new(if enabled { t("Disable") } else { t("Enable") }),
            )
            .clicked()
            && let Some(n) = g.node_mut(sel)
        {
            n.enabled = !n.enabled;
        }
        ui.separator();
        let view = g.view;
        let matte_on = matches!(view, Some(View::Matte(x)) if x == sel);
        let high_on = matches!(view, Some(View::Highlight(x)) if x == sel);
        if ui
            .add_enabled(
                is_corrector,
                egui::Button::new(t("Matte")).selected(matte_on),
            )
            .on_hover_text(t("Show the matte of the selected node in the viewer"))
            .clicked()
        {
            g.view = if matte_on {
                None
            } else {
                Some(View::Matte(sel))
            };
        }
        if ui
            .add_enabled(
                is_corrector,
                egui::Button::new(t("Highlight")).selected(high_on),
            )
            .on_hover_text(t("Dim what the selected node does not select"))
            .clicked()
        {
            g.view = if high_on {
                None
            } else {
                Some(View::Highlight(sel))
            };
        }
        if ui.button(t("Reset Graph")).clicked() {
            *g = ColorGraph::default();
            v.sel = 2;
        }
    });
}

// ------------------------------------------------------------------------------------- canvas

fn node_rect(n: &Node, origin: Pos2) -> Rect {
    Rect::from_min_size(origin + vec2(n.pos[0], n.pos[1]), vec2(NODE_W, NODE_H))
}

fn input_pos(g: &ColorGraph, n: &Node, r: Rect, input: u8) -> Pos2 {
    if n.kind.is_mixer() {
        let count = g.inputs_of(n.id).iter().filter(|e| !e.key).count().max(1) + 1;
        let step = r.height() / (count as f32 + 1.0);
        pos2(
            r.min.x,
            r.min.y + step * (input as f32 + 1.0).min(count as f32),
        )
    } else {
        pos2(r.min.x, r.center().y)
    }
}

fn key_in_pos(r: Rect) -> Pos2 {
    pos2(r.center().x, r.min.y)
}

fn key_out_pos(r: Rect) -> Pos2 {
    pos2(r.center().x, r.max.y)
}

fn out_pos(r: Rect) -> Pos2 {
    pos2(r.max.x, r.center().y)
}

fn curve(p: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let dx = ((b.x - a.x).abs() * 0.5).clamp(20.0, 120.0);
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b],
        false,
        Color32::TRANSPARENT,
        stroke,
    ));
}

fn canvas(v: &mut NodesView, g: &mut ColorGraph, ui: &mut Ui, height: f32) {
    let w = ui.available_width();
    let (area, resp) = ui.allocate_exact_size(vec2(w, height), Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 3.0, theme::PANEL_DARK);
    // a dotted background that follows the pan
    let step = 24.0;
    let off = vec2(v.pan.x.rem_euclid(step), v.pan.y.rem_euclid(step));
    let mut y = area.min.y + off.y;
    while y < area.max.y {
        let mut x = area.min.x + off.x;
        while x < area.max.x {
            painter.circle_filled(pos2(x, y), 0.8, theme::LINE);
            x += step;
        }
        y += step;
    }
    let origin = area.min + vec2(24.0, area.height() * 0.5 - NODE_H * 0.5) + v.pan;

    // edges
    let sel = v.selected();
    let mut edge_hit: Option<Edge> = None;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    for e in g.edges.clone() {
        let (Some(a), Some(b)) = (g.node(e.from), g.node(e.to)) else {
            continue;
        };
        let (ra, rb) = (node_rect(a, origin), node_rect(b, origin));
        let (pa, pb) = if e.key {
            (key_out_pos(ra), key_in_pos(rb))
        } else {
            (out_pos(ra), input_pos(g, b, rb, e.input))
        };
        let active = e.from == sel || e.to == sel;
        let color = if e.key {
            Color32::from_rgb(210, 190, 90)
        } else if active {
            theme::ACCENT
        } else {
            theme::TEXT_DIM
        };
        curve(
            &painter,
            pa,
            pb,
            Stroke::new(if active { 2.0 } else { 1.4 }, color),
        );
        if let Some(p) = pointer {
            let mid = pa.lerp(pb, 0.5);
            if mid.distance(p) < 7.0 {
                edge_hit = Some(e);
                painter.circle_stroke(mid, 6.0, Stroke::new(1.5, Color32::from_rgb(230, 90, 80)));
            }
        }
    }
    if let Some(e) = edge_hit
        && resp.secondary_clicked()
    {
        g.disconnect(e);
    }

    // nodes
    let mut hit_input: Option<(NodeId, u8, bool)> = None;
    let mut start_link: Option<(NodeId, bool)> = None;
    let mut clicked: Option<NodeId> = None;
    let mut menu_node: Option<NodeId> = None;
    let nodes = g.nodes.clone();
    for n in &nodes {
        let r = node_rect(n, origin);
        let selected = n.id == sel;
        let fill = match n.kind {
            NodeKind::Source | NodeKind::Output => Color32::from_rgb(56, 56, 62),
            NodeKind::Corrector(_) => Color32::from_rgb(52, 70, 92),
            NodeKind::Parallel(_) => Color32::from_rgb(74, 62, 92),
            NodeKind::Layer(_) => Color32::from_rgb(88, 66, 60),
        };
        let fill = if n.enabled {
            fill
        } else {
            fill.gamma_multiply(0.45)
        };
        painter.rect_filled(r, 4.0, fill);
        painter.rect_stroke(
            r,
            4.0,
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected { theme::ACCENT } else { theme::LINE },
            ),
            StrokeKind::Inside,
        );
        let title = g.title(n.id);
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            title,
            FontId::proportional(12.0),
            theme::TEXT_BRIGHT,
        );
        if let NodeKind::Corrector(c) = &n.kind {
            // small marks for what the node does
            let mut x = r.min.x + 4.0;
            for (on, col) in [
                (c.qualifier.enabled, Color32::from_rgb(230, 190, 80)),
                (
                    c.windows.iter().any(|w| w.enabled),
                    Color32::from_rgb(110, 200, 230),
                ),
                (c.has_matte() && n.id != 0, Color32::TRANSPARENT),
            ] {
                if on && col != Color32::TRANSPARENT {
                    painter.circle_filled(pos2(x + 2.0, r.max.y - 5.0), 2.0, col);
                    x += 7.0;
                }
            }
        } else if matches!(n.kind, NodeKind::Parallel(_) | NodeKind::Layer(_)) {
            painter.text(
                pos2(r.center().x, r.max.y - 6.0),
                Align2::CENTER_CENTER,
                if matches!(n.kind, NodeKind::Layer(_)) {
                    "L"
                } else {
                    "P"
                },
                FontId::proportional(9.0),
                theme::TEXT_DIM,
            );
        }

        // sockets
        let draw_socket = |p: Pos2, square: bool, color: Color32| {
            let rect = Rect::from_center_size(p, vec2(SOCKET * 2.0, SOCKET * 2.0));
            if square {
                painter.rect_filled(rect, 1.0, color);
            } else {
                painter.circle_filled(p, SOCKET, color);
            }
            painter.circle_stroke(
                p,
                SOCKET + 1.0,
                Stroke::new(1.0, Color32::from_black_alpha(120)),
            );
        };
        let gold = Color32::from_rgb(210, 190, 90);
        if !matches!(n.kind, NodeKind::Source) {
            if n.kind.is_mixer() {
                let used = g.inputs_of(n.id).iter().filter(|e| !e.key).count();
                for i in 0..=used {
                    let p = input_pos(g, n, r, i as u8);
                    draw_socket(p, false, theme::TEXT);
                    if pointer.is_some_and(|q| q.distance(p) < SOCKET + 4.0) {
                        hit_input = Some((n.id, i as u8, false));
                    }
                }
            } else {
                let p = input_pos(g, n, r, 0);
                draw_socket(p, false, theme::TEXT);
                if pointer.is_some_and(|q| q.distance(p) < SOCKET + 4.0) {
                    hit_input = Some((n.id, 0, false));
                }
            }
            if matches!(n.kind, NodeKind::Corrector(_)) {
                let p = key_in_pos(r);
                draw_socket(p, true, gold);
                if pointer.is_some_and(|q| q.distance(p) < SOCKET + 4.0) {
                    hit_input = Some((n.id, 0, true));
                }
            }
        }
        if !matches!(n.kind, NodeKind::Output) {
            let p = out_pos(r);
            draw_socket(p, false, theme::ACCENT);
            if resp.drag_started() && pointer.is_some_and(|q| q.distance(p) < SOCKET + 4.0) {
                start_link = Some((n.id, false));
            }
            if matches!(n.kind, NodeKind::Corrector(_)) {
                let p = key_out_pos(r);
                draw_socket(p, true, gold);
                if resp.drag_started() && pointer.is_some_and(|q| q.distance(p) < SOCKET + 4.0) {
                    start_link = Some((n.id, true));
                }
            }
        }

        // what the pointer does on the body
        if let Some(q) = pointer
            && r.contains(q)
        {
            if resp.clicked() || resp.drag_started() {
                clicked = Some(n.id);
            }
            if resp.secondary_clicked() {
                menu_node = Some(n.id);
            }
        }
    }
    if let Some(l) = start_link {
        v.link = Some(l);
        v.drag_node = None;
    } else if resp.drag_started() {
        v.drag_node = clicked.filter(|_| v.link.is_none());
        if v.drag_node.is_none() && v.link.is_none() {
            // empty space: the view moves
            v.drag_node = None;
        }
    }
    if let Some(id) = clicked {
        v.sel = id;
    }

    // moving
    if resp.dragged() && v.link.is_none() {
        let d = resp.drag_delta();
        match v.drag_node {
            Some(id) if id != SOURCE && id != OUTPUT => {
                if let Some(n) = g.node_mut(id) {
                    n.pos[0] += d.x;
                    n.pos[1] += d.y;
                }
            }
            Some(_) | None => v.pan += d,
        }
    }
    // a connection being made
    if let Some((from, key)) = v.link {
        if let (Some(n), Some(p)) = (g.node(from), pointer) {
            let r = node_rect(n, origin);
            let a = if key { key_out_pos(r) } else { out_pos(r) };
            curve(&painter, a, p, Stroke::new(1.6, theme::ACCENT));
        }
        if resp.drag_stopped() {
            if let Some((to, input, to_key)) = hit_input
                && to_key == key
                && let Err(why) = g.connect(from, to, input, key)
            {
                log::info!("color nodes: {why}");
            }
            v.link = None;
        }
    }
    if resp.drag_stopped() {
        v.drag_node = None;
    }
    resp.context_menu(|ui| {
        let target = menu_node.unwrap_or(v.selected());
        if menu_node.is_some() {
            v.sel = target;
        }
        if ui.button(t("Add Serial Node")).clicked() {
            v.sel = g.add_serial(target);
            ui.close();
        }
        if ui.button(t("Add Parallel Node")).clicked() {
            if let Some(id) = g.add_parallel(target) {
                v.sel = id;
            }
            ui.close();
        }
        if ui.button(t("Add Layer Node")).clicked() {
            if let Some(id) = g.add_layer(target) {
                v.sel = id;
            }
            ui.close();
        }
        if ui.button(t("Add Outside Node")).clicked() {
            v.sel = g.add_outside(target);
            ui.close();
        }
        if target != SOURCE && target != OUTPUT {
            ui.separator();
            let mut label = g.node(target).map(|n| n.label.clone()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(t("Label"));
                if ui
                    .add(egui::TextEdit::singleline(&mut label).desired_width(90.0))
                    .changed()
                    && let Some(n) = g.node_mut(target)
                {
                    n.label = label;
                }
            });
            if ui.button(t("Delete")).clicked() {
                g.remove(target);
                v.sel = 2;
                ui.close();
            }
        }
    });
    if ui.input(|i| i.key_pressed(egui::Key::Delete)) && resp.has_focus() {
        let id = v.selected();
        if id != SOURCE && id != OUTPUT {
            g.remove(id);
            v.sel = 2;
        }
    }
    painter.text(
        area.right_bottom() - vec2(8.0, 6.0),
        Align2::RIGHT_BOTTOM,
        t("Drag between sockets to connect; right-click a line to cut it"),
        FontId::proportional(10.0),
        theme::TEXT_DIM,
    );
}

// ---------------------------------------------------------------------------------- inspector

fn slider(
    ui: &mut Ui,
    label: &'static str,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
) -> bool {
    slider_text(ui, t(label).to_string(), v, range, default)
}

fn slider_text(
    ui: &mut Ui,
    label: String,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(vec2(96.0, 18.0), egui::Label::new(label));
        let r = ui.add(egui::Slider::new(v, range).show_value(true));
        changed = r.changed();
        if r.double_clicked() {
            *v = default;
            changed = true;
        }
    });
    changed
}

fn inspector(s: &mut State, v: &mut NodesView, g: &mut ColorGraph, ui: &mut Ui) {
    let id = v.selected();
    let Some(node) = g.node(id).cloned() else {
        return;
    };
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{}  {}", tn(node.kind.name()), g.title(id))).strong());
    });
    match node.kind {
        NodeKind::Source => {
            ui.label(RichText::new(t("The clip's picture comes in here.")).color(theme::TEXT_DIM));
        }
        NodeKind::Output => {
            ui.label(
                RichText::new(t("What leaves this node is the graded clip."))
                    .color(theme::TEXT_DIM),
            );
        }
        NodeKind::Parallel(_) | NodeKind::Layer(_) => mixer_ui(g, id, ui),
        NodeKind::Corrector(_) => {
            ui.horizontal_wrapped(|ui| {
                for (tab, name) in Tab::ALL {
                    if ui.selectable_label(v.tab == tab, t(name)).clicked() {
                        v.tab = tab;
                    }
                }
            });
            ui.add_space(4.0);
            let tab = v.tab;
            let mut view = g.view;
            if let Some(Node {
                kind: NodeKind::Corrector(c),
                ..
            }) = g.node_mut(id)
            {
                match tab {
                    Tab::Primaries => primaries(c, ui),
                    Tab::Curves => curves(s, v, c, ui),
                    Tab::Qualifier => qualifier(c, id, &mut view, ui),
                    Tab::Windows => windows(c, ui),
                    Tab::Key => key(c, ui),
                    Tab::Effects => effects(c, ui),
                }
            }
            g.view = view;
        }
    }
}

fn mixer_ui(g: &mut ColorGraph, id: NodeId, ui: &mut Ui) {
    let inputs = g.inputs_of(id).iter().filter(|e| !e.key).count();
    let layer = matches!(g.node(id).map(|n| &n.kind), Some(NodeKind::Layer(_)));
    let Some(Node {
        kind: NodeKind::Parallel(m) | NodeKind::Layer(m),
        ..
    }) = g.node_mut(id)
    else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(t("Blend"));
        egui::ComboBox::from_id_salt("mix-blend")
            .selected_text(t(m.blend.label()))
            .show_ui(ui, |ui| {
                for b in Blend::ALL {
                    ui.selectable_value(&mut m.blend, b, t(b.label()));
                }
            });
    });
    if m.weights.len() < inputs {
        m.weights.resize(inputs, 1.0);
    }
    for i in 0..inputs {
        let name = if layer {
            if i == 0 {
                t("Background").to_string()
            } else {
                format!("{} {i}", t("Layer"))
            }
        } else {
            format!("{} {}", t("Input"), i + 1)
        };
        let mut w = m.weight(i);
        if slider_text(ui, name, &mut w, 0.0..=1.0, 1.0) {
            m.weights[i] = w;
        }
    }
    let _: &Mixer = m;
}

/// A wheel for [r, g, b, master] offsets: hue and amount from where the puck is, the master
/// below it.
fn wheel(ui: &mut Ui, name: &'static str, v: &mut [f32; 4]) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.label(RichText::new(t(name)).size(11.0).color(theme::TEXT_DIM));
        let size = 92.0;
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
        p.circle_filled(c, rad * 0.5, Color32::from_black_alpha(90));
        p.circle_stroke(c, rad, Stroke::new(1.0, theme::LINE));
        // red at 0 degrees, green at 120, blue at 240
        let (px, py) = (v[0], (v[1] - v[2]) / 1.732);
        let puck = c + vec2(px, -py) * rad;
        p.circle_stroke(puck, 5.0, Stroke::new(2.0, Color32::WHITE));
        if (resp.dragged() || resp.clicked())
            && let Some(pp) = resp.interact_pointer_pos()
        {
            let mut d = (pp - c) / rad;
            if d.length() > 1.0 {
                d = d.normalized();
            }
            let (x, y) = (d.x, -d.y);
            v[0] = x;
            v[1] = -0.5 * x + 0.866 * y;
            v[2] = -0.5 * x - 0.866 * y;
            changed = true;
        }
        if resp.double_clicked() {
            *v = [0.0; 4];
            changed = true;
        }
        let mut m = v[3];
        if ui
            .add(
                egui::DragValue::new(&mut m)
                    .speed(0.005)
                    .range(-1.0..=1.0)
                    .fixed_decimals(2)
                    .prefix("Y "),
            )
            .changed()
        {
            v[3] = m;
            changed = true;
        }
    });
    changed
}

fn primaries(c: &mut Corrector, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        let g = &mut c.grade;
        wheel(ui, "Lift", &mut g.lift);
        wheel(ui, "Gamma", &mut g.gamma);
        wheel(ui, "Gain", &mut g.gain);
        wheel(ui, "Offset", &mut g.offset);
    });
    ui.add_space(4.0);
    let g = &mut c.grade;
    slider(ui, "Contrast", &mut g.contrast, 0.0..=2.0, 1.0);
    slider(ui, "Pivot", &mut g.pivot, 0.0..=1.0, 0.435);
    slider(ui, "Saturation", &mut g.saturation, 0.0..=2.0, 1.0);
    slider(ui, "Hue", &mut g.hue, -180.0..=180.0, 0.0);
    slider(ui, "Temperature", &mut g.temperature, -100.0..=100.0, 0.0);
    slider(ui, "Tint", &mut g.tint, -100.0..=100.0, 0.0);
    slider(ui, "Shadows", &mut g.shadows, -100.0..=100.0, 0.0);
    slider(ui, "Highlights", &mut g.highlights, -100.0..=100.0, 0.0);
    slider(ui, "Color Boost", &mut g.boost, 0.0..=100.0, 0.0);
    if ui.button(t("Reset Primaries")).clicked() {
        c.grade = Default::default();
    }
}

fn curves(s: &mut State, v: &mut NodesView, c: &mut Corrector, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (k, name) in CurveKind::ALL {
            if ui.selectable_label(v.curve == k, t(name)).clicked() {
                v.curve = k;
            }
        }
    });
    let reset: Vec<[f32; 2]> = match v.curve {
        CurveKind::HueHue | CurveKind::HueSat | CurveKind::LumSat | CurveKind::SatSat => {
            op_core::colorgraph::flat()
        }
        _ => op_core::colorgraph::straight(),
    };
    let cv = &mut c.curves;
    let pts = match v.curve {
        CurveKind::Master => &mut cv.master,
        CurveKind::Red => &mut cv.red,
        CurveKind::Green => &mut cv.green,
        CurveKind::Blue => &mut cv.blue,
        CurveKind::HueHue => &mut cv.hue_vs_hue,
        CurveKind::HueSat => &mut cv.hue_vs_sat,
        CurveKind::LumSat => &mut cv.lum_vs_sat,
        CurveKind::SatSat => &mut cv.sat_vs_sat,
    };
    if pts.len() < 2 {
        *pts = reset.clone();
    }
    ui.horizontal(|ui| {
        effect_controls::curve_editor(s, ui, egui::Id::new(("cg-curve", v.curve as u8)), pts);
        ui.vertical(|ui| {
            if ui.button(t("Reset Curve")).clicked() {
                *pts = reset;
            }
            ui.label(
                RichText::new(t("Double-click to add a point, right-click to remove it. On hue and saturation curves, the middle is no change."))
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        });
    });
}

fn qualifier(c: &mut Corrector, id: NodeId, view: &mut Option<View>, ui: &mut Ui) {
    let q = &mut c.qualifier;
    ui.checkbox(&mut q.enabled, t("Qualifier"));
    if !q.enabled {
        return;
    }
    ui.checkbox(&mut q.use_hue, t("Select by Hue"));
    if q.use_hue {
        hue_strip(ui, q.hue_center, q.hue_width);
        slider(ui, "Hue", &mut q.hue_center, 0.0..=1.0, 0.0);
        slider(ui, "Hue Width", &mut q.hue_width, 0.0..=1.0, 0.1);
        slider(ui, "Hue Softness", &mut q.hue_soft, 0.0..=0.5, 0.05);
    }
    ui.label(RichText::new(t("Saturation")).color(theme::TEXT_DIM));
    slider(ui, "Low", &mut q.sat.low, 0.0..=1.0, 0.0);
    slider(ui, "High", &mut q.sat.high, 0.0..=1.0, 1.0);
    slider(ui, "Low Soft", &mut q.sat.low_soft, 0.0..=1.0, 0.0);
    slider(ui, "High Soft", &mut q.sat.high_soft, 0.0..=1.0, 0.0);
    ui.label(RichText::new(t("Luminance")).color(theme::TEXT_DIM));
    slider(ui, "Low", &mut q.lum.low, 0.0..=1.0, 0.0);
    slider(ui, "High", &mut q.lum.high, 0.0..=1.0, 1.0);
    slider(ui, "Low Soft", &mut q.lum.low_soft, 0.0..=1.0, 0.0);
    slider(ui, "High Soft", &mut q.lum.high_soft, 0.0..=1.0, 0.0);
    ui.label(RichText::new(t("Matte")).color(theme::TEXT_DIM));
    slider(ui, "Blur", &mut q.blur, 0.0..=100.0, 0.0);
    slider(ui, "Clean Black", &mut q.clean_black, 0.0..=1.0, 0.0);
    slider(ui, "Clean White", &mut q.clean_white, 0.0..=1.0, 1.0);
    ui.checkbox(&mut q.invert, t("Invert"));
    let on = matches!(*view, Some(View::Matte(x)) if x == id);
    if ui.selectable_label(on, t("Show Matte")).clicked() {
        *view = if on { None } else { Some(View::Matte(id)) };
    }
}

/// The hue wheel as a strip with the selected range marked.
fn hue_strip(ui: &mut Ui, center: f32, width: f32) {
    let (r, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(320.0), 12.0), Sense::hover());
    let p = ui.painter();
    let n = 60;
    for i in 0..n {
        let x0 = r.min.x + r.width() * i as f32 / n as f32;
        let x1 = r.min.x + r.width() * (i + 1) as f32 / n as f32;
        let hue = egui::ecolor::Hsva::new(i as f32 / n as f32, 0.8, 0.8, 1.0);
        p.rect_filled(
            Rect::from_min_max(pos2(x0, r.min.y), pos2(x1 + 0.5, r.max.y)),
            0.0,
            Color32::from(hue),
        );
    }
    for k in [-1.0f32, 0.0, 1.0] {
        let lo = center - width * 0.5 + k;
        let hi = center + width * 0.5 + k;
        let x0 = r.min.x + r.width() * lo.clamp(0.0, 1.0);
        let x1 = r.min.x + r.width() * hi.clamp(0.0, 1.0);
        if x1 > x0 {
            p.rect_stroke(
                Rect::from_min_max(pos2(x0, r.min.y), pos2(x1, r.max.y)),
                0.0,
                Stroke::new(2.0, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
    }
}

fn windows(c: &mut Corrector, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (kind, name) in [
            (WindowKind::Rectangle, "Rectangle"),
            (WindowKind::Ellipse, "Ellipse"),
            (WindowKind::Linear, "Linear"),
            (WindowKind::Polygon, "Polygon"),
        ] {
            if ui.button(format!("+ {}", t(name))).clicked() {
                c.windows.push(Window::new(kind));
            }
        }
    });
    let mut remove = None;
    for (i, w) in c.windows.iter_mut().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut w.enabled,
                format!("{} {}", t(kind_name(w.kind)), i + 1),
            );
            if ui
                .small_button("\u{00D7}")
                .on_hover_text(t("Remove"))
                .clicked()
            {
                remove = Some(i);
            }
        });
        if w.kind != WindowKind::Polygon {
            slider(ui, "Center X", &mut w.center[0], 0.0..=1.0, 0.5);
            slider(ui, "Center Y", &mut w.center[1], 0.0..=1.0, 0.5);
        }
        match w.kind {
            WindowKind::Rectangle | WindowKind::Ellipse => {
                slider(ui, "Width", &mut w.size[0], 0.02..=2.0, 0.4);
                slider(ui, "Height", &mut w.size[1], 0.02..=2.0, 0.4);
                slider(ui, "Rotation", &mut w.rotation, -180.0..=180.0, 0.0);
                slider(ui, "Softness", &mut w.softness, 0.0..=1.0, 0.1);
            }
            WindowKind::Linear => {
                slider(ui, "Rotation", &mut w.rotation, -180.0..=180.0, 0.0);
                slider(ui, "Softness", &mut w.softness, 0.01..=1.0, 0.5);
            }
            WindowKind::Polygon => {
                if w.points.len() < 3 {
                    *w = Window::new(WindowKind::Polygon);
                }
                for (k, p) in w.points.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} {}", t("Point"), k + 1));
                        ui.add(
                            egui::DragValue::new(&mut p[0])
                                .speed(0.005)
                                .range(0.0..=1.0)
                                .fixed_decimals(3),
                        );
                        ui.add(
                            egui::DragValue::new(&mut p[1])
                                .speed(0.005)
                                .range(0.0..=1.0)
                                .fixed_decimals(3),
                        );
                    });
                }
                ui.horizontal(|ui| {
                    if w.points.len() < 8 && ui.small_button(t("+ Point")).clicked() {
                        let a = w.points[w.points.len() - 1];
                        let b = w.points[0];
                        w.points
                            .push([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5 + 0.1]);
                    }
                    if w.points.len() > 3 && ui.small_button(t("- Point")).clicked() {
                        w.points.pop();
                    }
                });
                slider(ui, "Softness", &mut w.softness, 0.0..=1.0, 0.1);
            }
        }
        ui.checkbox(&mut w.invert, t("Invert"));
    }
    if let Some(i) = remove {
        c.windows.remove(i);
    }
}

fn kind_name(k: WindowKind) -> &'static str {
    match k {
        WindowKind::Rectangle => "Rectangle",
        WindowKind::Ellipse => "Ellipse",
        WindowKind::Linear => "Linear",
        WindowKind::Polygon => "Polygon",
    }
}

fn key(c: &mut Corrector, ui: &mut Ui) {
    ui.label(
        RichText::new(t("The key limits where this node's grade shows. It comes from the qualifier, the windows and a key input."))
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    ui.checkbox(
        &mut c.key_in_invert,
        t("Invert the Key Input (grade what is outside)"),
    );
    slider(ui, "Key Gain", &mut c.key.gain, 0.0..=4.0, 1.0);
    slider(ui, "Key Offset", &mut c.key.offset, -1.0..=1.0, 0.0);
    slider(ui, "Key Blur", &mut c.key.blur, 0.0..=100.0, 0.0);
    ui.checkbox(&mut c.key.invert, t("Invert Key"));
}

fn effects(c: &mut Corrector, ui: &mut Ui) {
    slider(ui, "Node Amount", &mut c.amount, 0.0..=1.0, 1.0);
    ui.horizontal(|ui| {
        ui.add_sized(vec2(96.0, 18.0), egui::Label::new(t("LUT")));
        let name = std::path::Path::new(&c.lut)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| t("None").to_string());
        ui.label(RichText::new(name).color(theme::VALUE))
            .on_hover_text(&c.lut);
        if ui
            .small_button("\u{2026}")
            .on_hover_text(t("Browse"))
            .clicked()
            && let Some(p) = rfd::FileDialog::new()
                .add_filter("LUT", &["cube", "CUBE"])
                .pick_file()
        {
            c.lut = p.display().to_string();
        }
        if !c.lut.is_empty()
            && ui
                .small_button("\u{00D7}")
                .on_hover_text(t("Clear"))
                .clicked()
        {
            c.lut.clear();
        }
    });
    slider(ui, "LUT Amount", &mut c.lut_amount, 0.0..=1.0, 1.0);
    slider(ui, "Blur", &mut c.blur, 0.0..=50.0, 0.0);
    slider(ui, "Sharpen", &mut c.sharpen, -100.0..=100.0, 0.0);
}
