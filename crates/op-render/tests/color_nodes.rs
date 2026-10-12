//! Pixel tests on the available GPU (a software adapter in CI).

use std::sync::Arc;

use op_core::colorgraph::*;
use op_core::*;
use op_render::*;
use op_timeline::*;

fn gpu() -> Option<Arc<Gpu>> {
    match Gpu::headless() {
        Ok(g) => Some(g),
        // CI sets this where a software adapter is installed, so effects are really tested
        Err(e) if std::env::var("WEEDIT_REQUIRE_GPU").is_ok_and(|v| v == "1") => {
            panic!("no GPU adapter: {e}")
        }
        Err(e) => {
            eprintln!("skipping GPU tests: {e}");
            None
        }
    }
}

struct Scene {
    p: Project,
    seq: SequenceId,
}

impl Scene {
    fn new() -> Scene {
        let mut p = Project::new("gpu");
        let settings = SequenceSettings {
            width: 64,
            height: 36,
            rate: Rate::FPS_25,
            ..SequenceSettings::default()
        };
        let (seq, _) = p.add_sequence(p.root, "S", settings);
        Scene { p, seq }
    }

    fn matte(&mut self, color: Rgba) -> ItemId {
        let root = self.p.root;
        self.p.add_item(
            root,
            "Matte",
            ItemKind::Synthetic {
                generator: Generator::ColorMatte { color },
                duration: Dur::from_seconds(60.0),
            },
        )
    }

    fn put(&mut self, item: ItemId, track: usize, from: f64, to: f64) -> ClipId {
        let mut spec = SourceClip::from_item(&self.p, item).unwrap();
        spec.range = SrcRange::new(SrcTime::ZERO, SrcTime::from_seconds(to - from));
        let seq = self.seq;
        let (next, ids) = self
            .p
            .transact(|p| {
                overwrite(
                    p,
                    seq,
                    &spec,
                    SeqTime::from_seconds(from),
                    &Patch {
                        video: Some(track),
                        audio: vec![],
                    },
                    EditOptions::default(),
                )
            })
            .unwrap();
        self.p = next;
        ids[0]
    }

    fn set(&mut self, clip: ClipId, effect: &str, key: &str, v: Value) {
        let c = self
            .p
            .sequence_mut(self.seq)
            .unwrap()
            .clip_mut(clip)
            .unwrap();
        let comp = match c.component_mut(effect) {
            Some(x) => x,
            None => {
                let mut ids = IdGen::default();
                ids.observe(10_000 + c.components.len() as u64);
                c.components
                    .push(Component::new(catalog::find(effect).unwrap(), &mut ids));
                c.components.last_mut().unwrap()
            }
        };
        comp.param_mut(key).unwrap().value = v;
    }

    fn render(&self, r: &mut Renderer, t: f64) -> Vec<u8> {
        let frame = r.render(&Request {
            project: &self.p,
            sequence: self.seq,
            time: SeqTime::from_seconds(t),
            scale: 1.0,
            source: &NoFrames,
        });
        let px = r.rgba8(&frame);
        r.recycle(frame);
        px
    }
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 64 + x) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol)
}

const FX: &str = "op.video.color_nodes";

fn graded(graph: &ColorGraph, color: Rgba) -> Vec<u8> {
    let Some(gpu) = gpu() else { return vec![] };
    let mut r = Renderer::new(gpu);
    let mut s = Scene::new();
    let m = s.matte(color);
    let c = s.put(m, 0, 0.0, 10.0);
    s.set(c, FX, "graph", Value::Text(graph.to_json()));
    s.render(&mut r, 1.0)
}

fn edit(g: &mut ColorGraph, id: NodeId, f: impl FnOnce(&mut Corrector)) {
    if let Some(Node {
        kind: NodeKind::Corrector(c),
        ..
    }) = g.node_mut(id)
    {
        f(c);
    }
}

const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);

#[test]
fn an_untouched_graph_changes_nothing_and_saturation_drains_color() {
    let g = ColorGraph::default();
    let px = graded(&g, RED);
    if px.is_empty() {
        return;
    }
    assert!(
        near(at(&px, 32, 18), [255, 0, 0, 255], 1),
        "{:?}",
        at(&px, 32, 18)
    );
    let mut g = ColorGraph::default();
    edit(&mut g, 2, |c| c.grade.saturation = 0.0);
    let px = graded(&g, RED);
    let p = at(&px, 32, 18);
    assert!(
        p[0].abs_diff(p[1]) <= 2 && p[1].abs_diff(p[2]) <= 2,
        "gray: {p:?}"
    );
    assert!(p[0] > 40 && p[0] < 70, "{p:?}");
}

#[test]
fn a_window_limits_the_grade_to_its_shape() {
    let mut g = ColorGraph::default();
    edit(&mut g, 2, |c| {
        c.grade.saturation = 0.0;
        let mut w = Window::new(WindowKind::Rectangle);
        w.center = [0.5, 0.5];
        w.size = [0.5, 0.5];
        w.softness = 0.0;
        c.windows.push(w);
    });
    let px = graded(&g, RED);
    if px.is_empty() {
        return;
    }
    let inside = at(&px, 32, 18);
    let outside = at(&px, 2, 2);
    assert!(
        inside[1].abs_diff(inside[0]) <= 2,
        "inside is gray: {inside:?}"
    );
    assert!(
        near(outside, [255, 0, 0, 255], 2),
        "outside keeps its color: {outside:?}"
    );
}

#[test]
fn a_qualifier_selects_by_hue() {
    for (center, selected) in [(0.0, true), (0.33, false)] {
        let mut g = ColorGraph::default();
        edit(&mut g, 2, |c| {
            c.grade.saturation = 0.0;
            c.qualifier.enabled = true;
            c.qualifier.hue_center = center;
            c.qualifier.hue_width = 0.1;
            c.qualifier.hue_soft = 0.02;
        });
        let px = graded(&g, RED);
        if px.is_empty() {
            return;
        }
        let p = at(&px, 32, 18);
        if selected {
            assert!(p[1] > 30, "red was selected and drained: {p:?}");
        } else {
            assert!(near(p, [255, 0, 0, 255], 2), "red was not selected: {p:?}");
        }
    }
}

#[test]
fn a_parallel_mixer_averages_its_branches() {
    let mut g = ColorGraph::default();
    edit(&mut g, 2, |c| c.grade.saturation = 0.0);
    let other = g.add_parallel(2).unwrap();
    let _ = other;
    let px = graded(&g, RED);
    if px.is_empty() {
        return;
    }
    // half red, half the gray of red
    let p = at(&px, 32, 18);
    assert!(p[0] > 130 && p[0] < 190 && p[1] > 20 && p[1] < 60, "{p:?}");
}

#[test]
fn the_matte_view_shows_the_selection() {
    let mut g = ColorGraph::default();
    edit(&mut g, 2, |c| {
        let mut w = Window::new(WindowKind::Rectangle);
        w.size = [0.5, 0.5];
        w.softness = 0.0;
        c.windows.push(w);
    });
    g.view = Some(View::Matte(2));
    let px = graded(&g, RED);
    if px.is_empty() {
        return;
    }
    assert!(
        near(at(&px, 32, 18), [255, 255, 255, 255], 2),
        "{:?}",
        at(&px, 32, 18)
    );
    assert!(
        near(at(&px, 2, 2), [0, 0, 0, 255], 2),
        "{:?}",
        at(&px, 2, 2)
    );
}

#[test]
fn a_layer_over_the_base_is_cut_by_its_key() {
    let mut g = ColorGraph::default();
    let layer = g.add_layer(2).unwrap();
    edit(&mut g, layer, |c| {
        c.grade.saturation = 0.0;
        c.windows.clear();
        let mut w = Window::new(WindowKind::Rectangle);
        w.size = [0.5, 0.5];
        w.softness = 0.0;
        c.windows.push(w);
    });
    let px = graded(&g, RED);
    if px.is_empty() {
        return;
    }
    let inside = at(&px, 32, 18);
    let outside = at(&px, 2, 2);
    assert!(
        inside[0].abs_diff(inside[1]) <= 3,
        "gray layer on top: {inside:?}"
    );
    assert!(
        near(outside, [255, 0, 0, 255], 2),
        "base shows around it: {outside:?}"
    );
}
