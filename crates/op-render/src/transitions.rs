//! Transition dispatch: catalog ID -> transition shader.

use op_core::plan::EvalComponent;

use crate::params::P;
use crate::pool::Tex;
use crate::render::Renderer;

impl Renderer {
    /// Mixes the outgoing (`a`) and incoming (`b`) images at `progress`.
    pub(crate) fn transition(
        &mut self,
        effect: &str,
        progress: f32,
        fx: &EvalComponent,
        a: &Tex,
        b: &Tex,
        w: u32,
        h: u32,
    ) -> Tex {
        let reverse = fx.bool("reverse");
        let (a, b, t) = if reverse {
            (b, a, 1.0 - progress)
        } else {
            (a, b, progress)
        };
        let dir = fx.choice("direction") as f32;
        let border = |p: P| p.f(fx.f32("border")).rgb(fx.color("border_color"));
        let (entry, params): (&'static str, P) = match effect {
            "op.tr.cross_dissolve" => ("fs_tr_dissolve", P::new().f(0.0)),
            "op.tr.dip_to_black" => ("fs_tr_dissolve", P::new().f(1.0)),
            "op.tr.dip_to_white" => ("fs_tr_dissolve", P::new().f(2.0)),
            "op.tr.film_dissolve" => ("fs_tr_dissolve", P::new().f(3.0)),
            "op.tr.additive_dissolve" => ("fs_tr_dissolve", P::new().f(4.0)),
            "op.tr.non_additive_dissolve" => ("fs_tr_dissolve", P::new().f(5.0)),
            "op.tr.iris_round" | "op.tr.iris_box" | "op.tr.iris_cross" | "op.tr.iris_diamond" => {
                let shape = match effect {
                    "op.tr.iris_box" => 1.0,
                    "op.tr.iris_cross" => 2.0,
                    "op.tr.iris_diamond" => 3.0,
                    _ => 0.0,
                };
                (
                    "fs_tr_iris",
                    border(P::new().f(shape).v2(fx.point("center"))).f(fx.f32("feather")),
                )
            }
            "op.tr.push" => ("fs_tr_slide", P::new().f(0.0).f(dir)),
            "op.tr.slide" => ("fs_tr_slide", border(P::new().f(1.0).f(dir))),
            "op.tr.whip" => ("fs_tr_slide", P::new().f(2.0).f(dir)),
            "op.tr.inset" => ("fs_tr_slide", border(P::new().f(3.0).f(dir))),
            "op.tr.split" => (
                "fs_tr_split",
                P::new().f(0.0).f(fx.choice("orientation") as f32),
            ),
            "op.tr.barn_doors" => (
                "fs_tr_split",
                P::new().f(1.0).f(fx.choice("orientation") as f32),
            ),
            "op.tr.center_split" => ("fs_tr_split", P::new().f(2.0).f(0.0)),
            "op.tr.wipe" => (
                "fs_tr_wipe",
                border(P::new().f(0.0).f(dir)).f(fx.f32("feather")),
            ),
            "op.tr.clock_wipe" | "op.tr.radial_wipe" => (
                "fs_tr_wipe",
                P::new()
                    .f(1.0)
                    .f(0.0)
                    .f(0.0)
                    .all(&[0.0, 0.0, 0.0])
                    .f(2.0)
                    .v2(fx.point("center"))
                    .f(fx.f32("start")),
            ),
            "op.tr.venetian_blinds" => (
                "fs_tr_wipe",
                P::new()
                    .f(2.0)
                    .f(fx.choice("direction") as f32)
                    .f(0.0)
                    .all(&[0.0, 0.0, 0.0])
                    .f(1.0)
                    .f(0.0)
                    .f(0.0)
                    .f(0.0)
                    .f(fx.f32("count")),
            ),
            "op.tr.checker_wipe" => (
                "fs_tr_wipe",
                P::new()
                    .f(3.0)
                    .f(0.0)
                    .f(0.0)
                    .all(&[0.0, 0.0, 0.0])
                    .f(1.0)
                    .f(fx.f32("columns"))
                    .f(fx.f32("rows")),
            ),
            "op.tr.random_blocks" => (
                "fs_tr_wipe",
                P::new()
                    .f(4.0)
                    .f(0.0)
                    .f(0.0)
                    .all(&[0.0, 0.0, 0.0])
                    .f(1.0)
                    .f(fx.f32("columns"))
                    .f(fx.f32("rows"))
                    .f(fx.f32("seed")),
            ),
            "op.tr.cross_zoom" => ("fs_tr_zoom", P::new().v2(fx.point("center"))),
            "op.tr.flip_over" => (
                "fs_tr_flip",
                P::new().f(fx.choice("axis") as f32).rgb(fx.color("fill")),
            ),
            _ => ("fs_tr_dissolve", P::new().f(0.0)),
        };
        let out = self.work(w, h);
        self.pass(
            entry,
            &[&a.view, &b.view],
            params.progress(t.clamp(0.0, 1.0)),
            a.size(),
            &out,
        );
        out
    }
}
