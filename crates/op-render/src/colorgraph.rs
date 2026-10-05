//! Evaluates a color node graph (see `op_core::colorgraph`) into shader passes.

use std::collections::HashMap;

use op_core::colorgraph::{
    Band, ColorGraph, Corrector, Node, NodeId, NodeKind, OUTPUT, SOURCE, View, Window, WindowKind,
};

use crate::params::P;
use crate::pool::Tex;
use crate::render::Renderer;

/// Pictures made while evaluating; nodes refer to them by index so a picture passed through
/// several nodes exists once.
struct Run {
    pics: Vec<Tex>,
    /// Node -> picture index.
    out: HashMap<NodeId, usize>,
    /// Node -> matte index (its qualifier, windows and key input, adjusted).
    mattes: HashMap<NodeId, usize>,
}

impl Run {
    fn store(&mut self, t: Tex) -> usize {
        self.pics.push(t);
        self.pics.len() - 1
    }
}

impl Renderer {
    /// The "Color Nodes" effect: runs `img` through the graph in the effect's `graph` text.
    pub(crate) fn color_graph(&mut self, graph_json: &str, img: Tex, s: f32) -> Tex {
        let Some(graph) = ColorGraph::from_json(graph_json) else {
            return img;
        };
        if graph.is_identity() && graph.view.is_none() {
            return img;
        }
        let (w, h) = (img.width, img.height);
        let mut run = Run {
            pics: vec![img],
            out: HashMap::new(),
            mattes: HashMap::new(),
        };
        run.out.insert(SOURCE, 0);
        for id in graph.order() {
            if id == SOURCE || id == OUTPUT {
                continue;
            }
            let Some(node) = graph.node(id) else { continue };
            let picture_in = |g: &ColorGraph, run: &Run, input: u8| {
                g.inputs_of(id)
                    .into_iter()
                    .find(|e| !e.key && e.input == input)
                    .and_then(|e| run.out.get(&e.from).copied())
            };
            let first = picture_in(&graph, &run, 0).unwrap_or(0);
            if !node.enabled {
                run.out.insert(id, first);
                continue;
            }
            let result = match &node.kind {
                NodeKind::Corrector(c) => self.corrector(&graph, node, c, &mut run, first, s),
                NodeKind::Parallel(m) | NodeKind::Layer(m) => {
                    let layers = matches!(node.kind, NodeKind::Layer(_));
                    let inputs: Vec<(u8, usize)> = graph
                        .inputs_of(id)
                        .into_iter()
                        .filter(|e| !e.key)
                        .filter_map(|e| run.out.get(&e.from).map(|i| (e.input, *i)))
                        .collect();
                    if inputs.is_empty() {
                        first
                    } else {
                        let mut acc = inputs[0].1;
                        let mut weight = m.weight(inputs[0].0 as usize);
                        for (input, pic) in &inputs[1..] {
                            let wi = m.weight(*input as usize);
                            let out = self.work(w, h);
                            if !layers && m.blend == op_core::colorgraph::Blend::Normal {
                                self.pass(
                                    "fs_cg_wmix",
                                    &[&run.pics[acc].view, &run.pics[*pic].view],
                                    P::new().f(weight).f(wi),
                                    [w as f32, h as f32],
                                    &out,
                                );
                                weight += wi;
                            } else {
                                self.pass(
                                    "fs_cg_blend",
                                    &[&run.pics[acc].view, &run.pics[*pic].view],
                                    P::new().f(m.blend.index() as f32).f(wi),
                                    [w as f32, h as f32],
                                    &out,
                                );
                            }
                            acc = run.store(out);
                        }
                        acc
                    }
                }
                NodeKind::Source | NodeKind::Output => first,
            };
            run.out.insert(id, result);
        }

        // what the viewer shows
        let final_idx = graph
            .inputs_of(OUTPUT)
            .into_iter()
            .find(|e| !e.key && e.input == 0)
            .and_then(|e| run.out.get(&e.from).copied())
            .unwrap_or(0);
        let shown = match graph.view {
            Some(View::Matte(id)) => run.mattes.get(&id).map(|m| (true, id, *m)),
            Some(View::Highlight(id)) => run.mattes.get(&id).map(|m| (false, id, *m)),
            None => None,
        };
        let result = if let Some((matte_only, id, m)) = shown {
            let out = self.work(w, h);
            if matte_only {
                self.pass(
                    "fs_copy",
                    &[&run.pics[m].view],
                    P::new(),
                    [w as f32, h as f32],
                    &out,
                );
            } else {
                let at = run.out.get(&id).copied().unwrap_or(final_idx);
                self.pass(
                    "fs_cg_highlight",
                    &[&run.pics[at].view, &run.pics[m].view],
                    P::new(),
                    [w as f32, h as f32],
                    &out,
                );
            }
            Some(out)
        } else {
            None
        };

        let mut pics: Vec<Option<Tex>> = run.pics.into_iter().map(Some).collect();
        let picked = match result {
            Some(t) => t,
            None => pics[final_idx].take().expect("final picture"),
        };
        for t in pics.into_iter().flatten() {
            self.put(t);
        }
        picked
    }

    fn corrector(
        &mut self,
        g: &ColorGraph,
        node: &Node,
        c: &Corrector,
        run: &mut Run,
        input: usize,
        s: f32,
    ) -> usize {
        let id = node.id;
        let (w, h) = (run.pics[input].width, run.pics[input].height);
        let size = [w as f32, h as f32];

        // the matte: qualifier, windows, key input
        let mut matte: Option<usize> = None;
        let join = |this: &mut Renderer, run: &mut Run, m: usize, matte: &mut Option<usize>| {
            *matte = Some(match *matte {
                None => m,
                Some(prev) => {
                    let out = this.work(w, h);
                    this.pass(
                        "fs_cg_mul",
                        &[&run.pics[prev].view, &run.pics[m].view],
                        P::new(),
                        size,
                        &out,
                    );
                    run.store(out)
                }
            });
        };
        let q = &c.qualifier;
        if q.enabled {
            let out = self.work(w, h);
            self.pass(
                "fs_cg_qualify",
                &[&run.pics[input].view],
                P::new()
                    .b(q.use_hue)
                    .f(q.hue_center)
                    .f(q.hue_width)
                    .f(q.hue_soft)
                    .all(&band(&q.sat))
                    .all(&band(&q.lum))
                    .b(q.invert),
                size,
                &out,
            );
            let mut m = out;
            if q.blur > 0.0 {
                m = self.blur(m, q.blur * s * h as f32 / 1080.0, true, true, true);
            }
            let adjusted = self.work(w, h);
            self.pass(
                "fs_cg_adjust",
                &[&m.view],
                P::new()
                    .f(1.0)
                    .f(0.0)
                    .f(0.0)
                    .f(q.clean_black)
                    .f(q.clean_white),
                size,
                &adjusted,
            );
            self.put(m);
            let idx = run.store(adjusted);
            join(self, run, idx, &mut matte);
        }
        if c.windows.iter().any(|w| w.enabled) {
            let mut acc = self.clear_matte(w, h);
            for win in c.windows.iter().filter(|x| x.enabled) {
                let out = self.work(w, h);
                self.pass("fs_cg_window", &[&acc.view], window_params(win), size, &out);
                self.put(acc);
                acc = out;
            }
            let idx = run.store(acc);
            join(self, run, idx, &mut matte);
        }
        if let Some(key) = g.inputs_of(id).into_iter().find(|e| e.key)
            && let Some(&km) = run.mattes.get(&key.from)
        {
            let m = if c.key_in_invert {
                let out = self.work(w, h);
                self.pass(
                    "fs_cg_invert_matte",
                    &[&run.pics[km].view],
                    P::new(),
                    size,
                    &out,
                );
                run.store(out)
            } else {
                km
            };
            join(self, run, m, &mut matte);
        }
        if let Some(m) = matte
            && !c.key.is_neutral()
        {
            let mut src = None;
            if c.key.blur > 0.0 {
                // blur a copy of the matte
                let copy = self.work(w, h);
                self.pass("fs_copy", &[&run.pics[m].view], P::new(), size, &copy);
                src = Some(self.blur(copy, c.key.blur * s * h as f32 / 1080.0, true, true, true));
            }
            let out = self.work(w, h);
            let view = match &src {
                Some(t) => &t.view,
                None => &run.pics[m].view,
            };
            self.pass(
                "fs_cg_adjust",
                &[view],
                P::new()
                    .f(c.key.gain)
                    .f(c.key.offset)
                    .b(c.key.invert)
                    .f(0.0)
                    .f(1.0),
                size,
                &out,
            );
            if let Some(t) = src {
                self.put(t);
            }
            matte = Some(run.store(out));
        }
        if let Some(m) = matte {
            run.mattes.insert(id, m);
        }

        // does a layer mixer take this picture above another?
        let layered = g.outputs_of(id).iter().any(|e| {
            !e.key
                && e.input >= 1
                && matches!(g.node(e.to).map(|n| &n.kind), Some(NodeKind::Layer(_)))
        });

        // the grade
        let gr = &c.grade;
        let rgb_curves = !c.curves.rgb_is_neutral();
        let hsl_curves = !c.curves.hsl_is_neutral();
        let lut = if c.lut.is_empty() {
            None
        } else {
            self.lut(&c.lut)
        };
        let lut_on = lut.is_some();
        let grades = !gr.is_neutral() || rgb_curves || hsl_curves || lut_on || c.amount < 1.0;
        let limit_inside = matte.is_some() && !layered;
        let mut pic = input;
        if grades {
            let curves = self.grade_curves(&c.curves);
            let lut_view = lut.unwrap_or_else(|| self.identity_lut());
            let use_matte = limit_inside && !(c.blur > 0.0 || c.sharpen > 0.0);
            let out = self.work(w, h);
            let matte_view = match matte {
                Some(m) => run.pics[m].view.clone(),
                None => run.pics[input].view.clone(),
            };
            self.pass(
                "fs_cg_grade",
                &[&run.pics[input].view, &curves.view, &lut_view, &matte_view],
                P::new()
                    .all(&gr.lift)
                    .all(&gr.gamma)
                    .all(&gr.gain)
                    .all(&gr.offset)
                    .f(gr.contrast)
                    .f(gr.pivot)
                    .f(gr.saturation)
                    .f(gr.hue)
                    .f(gr.temperature)
                    .f(gr.tint)
                    .f(gr.shadows)
                    .f(gr.highlights)
                    .f(gr.boost)
                    .b(rgb_curves)
                    .b(hsl_curves)
                    .b(lut_on)
                    .f(c.lut_amount)
                    .f(c.amount)
                    .b(use_matte),
                size,
                &out,
            );
            pic = run.store(out);
        }
        // blur and sharpen the graded picture
        if c.blur > 0.0 || c.sharpen.abs() > 0.01 {
            let copy = self.work(w, h);
            self.pass("fs_copy", &[&run.pics[pic].view], P::new(), size, &copy);
            let mut t = copy;
            if c.blur > 0.0 {
                t = self.blur(t, c.blur * s, true, true, true);
            }
            if c.sharpen.abs() > 0.01 {
                let out = self.work(w, h);
                self.pass("fs_sharpen", &[&t.view], P::new().f(c.sharpen), size, &out);
                self.put(t);
                t = out;
            }
            pic = run.store(t);
            if limit_inside && let Some(m) = matte {
                // the filter shows only inside the matte
                let out = self.work(w, h);
                let ident = self.identity_lut();
                self.pass(
                    "fs_cg_limit",
                    &[
                        &run.pics[pic].view,
                        &run.pics[input].view,
                        &ident,
                        &run.pics[m].view,
                    ],
                    P::new(),
                    size,
                    &out,
                );
                pic = run.store(out);
            }
        }
        // a layer above another keeps only what its matte selects
        if layered && let Some(m) = matte {
            let out = self.work(w, h);
            self.pass(
                "fs_cg_cut",
                &[&run.pics[pic].view, &run.pics[m].view],
                P::new(),
                size,
                &out,
            );
            pic = run.store(out);
        }
        pic
    }

    /// A black matte.
    fn clear_matte(&mut self, w: u32, h: u32) -> Tex {
        let t = self.work(w, h);
        self.pass(
            "fs_fill",
            &[],
            P::new().all(&[0.0, 0.0, 0.0, 1.0]),
            [1.0, 1.0],
            &t,
        );
        t
    }
}

fn band(b: &Band) -> [f32; 4] {
    [b.low, b.high, b.low_soft, b.high_soft]
}

fn window_params(w: &Window) -> P {
    let kind = match w.kind {
        WindowKind::Rectangle => 0.0,
        WindowKind::Ellipse => 1.0,
        WindowKind::Linear => 2.0,
        WindowKind::Polygon => 3.0,
    };
    let mut p = P::new()
        .f(kind)
        .all(&w.center)
        .all(&w.size)
        .f(w.rotation.to_radians())
        .f(w.softness)
        .b(w.invert)
        .f(w.points.len().min(8) as f32);
    for pt in w.points.iter().take(8) {
        p = p.all(pt);
    }
    p
}
