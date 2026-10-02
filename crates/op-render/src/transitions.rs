//! Transition dispatch: catalog ID -> transition shader.

use op_core::plan::EvalComponent;

use crate::params::P;
use crate::pool::Tex;
use crate::render::Renderer;

impl Renderer {
    /// Mixes the outgoing (`a`) and incoming (`b`) images at `progress`; `scale` is the preview
    /// scale (pixel-sized parameters are multiplied by it).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn transition(
        &mut self,
        effect: &str,
        progress: f32,
        fx: &EvalComponent,
        a: &Tex,
        b: &Tex,
        w: u32,
        h: u32,
        scale: f32,
    ) -> Tex {
        let reverse = fx.bool("reverse");
        let (a, b, t) = if reverse {
            (b, a, 1.0 - progress)
        } else {
            (a, b, progress)
        };
        if effect == "op.tr.blur_dissolve" {
            return self.blur_dissolve(a, b, t, fx.f32("blur") * scale, w, h);
        }
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
            "op.tr.zoom_in" | "op.tr.zoom_out" => (
                "fs_tr_zoom_motion",
                P::new()
                    .f(if effect == "op.tr.zoom_out" { 1.0 } else { 0.0 })
                    .f(fx.f32("zoom") / 100.0)
                    .f(fx.f32("blur") / 100.0)
                    .v2(fx.point("center")),
            ),
            "op.tr.spin" => (
                "fs_tr_spin",
                P::new()
                    .f(fx.f32("rotations"))
                    .f(fx.choice("direction") as f32)
                    .f(fx.f32("blur") / 100.0)
                    .f(fx.f32("zoom") / 100.0),
            ),
            "op.tr.stretch" => (
                "fs_tr_stretch",
                P::new()
                    .f(fx.choice("direction") as f32)
                    .f(fx.f32("amount") / 100.0)
                    .f(fx.f32("blur") / 100.0),
            ),
            "op.tr.smooth_slide" => (
                "fs_tr_smooth_slide",
                P::new().f(dir).f(fx.f32("blur") / 100.0),
            ),
            "op.tr.luma_fade" => (
                "fs_tr_luma_fade",
                P::new().f(fx.f32("softness") / 100.0).b(fx.bool("invert")),
            ),
            "op.tr.hexagons" => (
                "fs_tr_hexagon",
                P::new()
                    .f(fx.f32("cells"))
                    .f(fx.choice("order") as f32)
                    .rgb(fx.color("edge_color"))
                    .f(fx.f32("edge") / 100.0),
            ),
            "op.tr.shatter" => (
                "fs_tr_shatter",
                P::new()
                    .f(fx.f32("pieces"))
                    .f(fx.f32("fall") / 100.0 * 1.5)
                    .f(fx.f32("spin") / 100.0 * 1.5),
            ),
            "op.tr.ink" => (
                "fs_tr_ink",
                P::new()
                    .f(fx.f32("scale"))
                    .f(fx.f32("softness") / 100.0)
                    .rgb(fx.color("ink"))
                    .f(fx.f32("edge") / 100.0),
            ),
            "op.tr.pixelate" => ("fs_tr_pixelate", P::new().f(fx.f32("block") * scale)),
            "op.tr.kaleidoscope" => (
                "fs_tr_kaleido",
                P::new().f(fx.f32("segments")).f(fx.f32("turns")),
            ),
            "op.tr.flash" => (
                "fs_tr_flash",
                P::new()
                    .rgb(fx.color("color"))
                    .f(fx.f32("intensity") / 100.0),
            ),
            "op.tr.light_leak" | "op.tr.film_burn" => (
                "fs_tr_light",
                P::new()
                    .f(if effect == "op.tr.film_burn" {
                        1.0
                    } else {
                        0.0
                    })
                    .rgb(fx.color("color"))
                    .f(fx.f32("intensity") / 100.0),
            ),
            "op.tr.glitch" => (
                "fs_tr_glitch",
                P::new()
                    .f(fx.f32("intensity") / 100.0)
                    .f(fx.f32("block") * scale)
                    .f(fx.f32("shift") * scale),
            ),
            "op.tr.chroma_split" => ("fs_tr_chroma", P::new().f(fx.f32("amount") * scale)),
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

    /// Both images blurred (most at the cut) and cross-faded.
    fn blur_dissolve(&mut self, a: &Tex, b: &Tex, t: f32, radius: f32, w: u32, h: u32) -> Tex {
        let sigma = radius * (t.clamp(0.0, 1.0) * std::f32::consts::PI).sin() / 2.0;
        let mut blurred = Vec::with_capacity(2);
        for src in [a, b] {
            let copy = self.work(w, h);
            self.pass("fs_copy", &[&src.view], P::new(), src.size(), &copy);
            blurred.push(self.blur(copy, sigma, true, true, true));
        }
        let bb = blurred.pop().unwrap();
        let ba = blurred.pop().unwrap();
        let k = {
            let x = ((t - 0.3) / 0.4).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let out = self.work(w, h);
        self.pass(
            "fs_tr_dissolve",
            &[&ba.view, &bb.view],
            P::new().f(0.0).progress(k),
            ba.size(),
            &out,
        );
        self.put(ba);
        self.put(bb);
        out
    }
}
