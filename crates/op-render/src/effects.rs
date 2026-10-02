//! Video effect dispatch: catalog ID -> shader passes. Pixel-sized parameters are multiplied by
//! the preview scale so a reduced-resolution preview matches full resolution.

use op_core::catalog;
use op_core::plan::{ClipLayer, EvalComponent, LayerSource};
use op_core::*;

use crate::params::P;
use crate::pool::Tex;
use crate::render::{Renderer, Request};

impl Renderer {
    fn single(&mut self, entry: &'static str, img: Tex, params: P) -> Tex {
        let out = self.work(img.width, img.height);
        self.pass(entry, &[&img.view], params, img.size(), &out);
        self.put(img);
        out
    }

    /// Applies one standard (or graphic) component to a layer image.
    pub(crate) fn effect(
        &mut self,
        fx: &EvalComponent,
        img: Tex,
        req: &Request,
        layer: &ClipLayer,
        clip_time: f32,
        seq_size: (u32, u32),
    ) -> Tex {
        let s = req.scale;
        let seed = self.frame_seed();
        match fx.effect.as_str() {
            catalog::TEXT => self.draw_text(fx, img, s, None),
            catalog::SHAPE => self.draw_shape(fx, img, s),
            catalog::CAPTION => self.draw_caption(fx, img, s, clip_time as f64, layer.duration),
            "op.video.brightness_contrast" => self.single(
                "fs_brightness_contrast",
                img,
                P::new().f(fx.f32("brightness")).f(fx.f32("contrast")),
            ),
            "op.video.procamp" => self.single(
                "fs_procamp",
                img,
                P::new()
                    .f(fx.f32("brightness"))
                    .f(fx.f32("contrast"))
                    .f(fx.f32("hue"))
                    .f(fx.f32("saturation"))
                    .b(fx.bool("split"))
                    .f(fx.f32("split_percent")),
            ),
            "op.video.levels" => self.single(
                "fs_levels",
                img,
                P::new()
                    .f(fx.f32("input_black"))
                    .f(fx.f32("input_white"))
                    .f(fx.f32("output_black"))
                    .f(fx.f32("output_white"))
                    .f(fx.f32("gamma")),
            ),
            "op.video.channel_mixer" => {
                let keys = [
                    "rr", "rg", "rb", "rc", "gr", "gg", "gb", "gc", "br", "bg", "bb", "bc",
                ];
                let mut p = P::new();
                for k in keys {
                    p = p.f(fx.f32(k));
                }
                self.single("fs_channel_mixer", img, p.b(fx.bool("monochrome")))
            }
            "op.video.extract" => self.single(
                "fs_extract",
                img,
                P::new()
                    .f(fx.f32("black"))
                    .f(fx.f32("white"))
                    .f(fx.f32("softness"))
                    .b(fx.bool("invert")),
            ),
            "op.video.gaussian_blur" => {
                let sigma = fx.f32("blurriness") / 2.0 * s;
                let dims = fx.choice("dimensions");
                self.blur(img, sigma, fx.bool("repeat_edge"), dims != 2, dims != 1)
            }
            "op.video.directional_blur" => self.single(
                "fs_directional_blur",
                img,
                P::new().f(fx.f32("direction")).f(fx.f32("length") * s),
            ),
            "op.video.sharpen" => self.single("fs_sharpen", img, P::new().f(fx.f32("amount"))),
            "op.video.unsharp_mask" => {
                let copy = self.work(img.width, img.height);
                self.pass("fs_copy", &[&img.view], P::new(), img.size(), &copy);
                let blurred = self.blur(copy, fx.f32("radius") * s, true, true, true);
                let out = self.work(img.width, img.height);
                self.pass(
                    "fs_unsharp",
                    &[&img.view, &blurred.view],
                    P::new().f(fx.f32("amount")).f(fx.f32("threshold") / 255.0),
                    img.size(),
                    &out,
                );
                self.put(img);
                self.put(blurred);
                out
            }
            "op.video.invert" => self.single(
                "fs_invert",
                img,
                P::new().f(fx.choice("channel") as f32).f(fx.f32("blend")),
            ),
            "op.video.lumetri" => self.lumetri(fx, img, s),
            "op.video.tint" => self.single(
                "fs_tint",
                img,
                P::new()
                    .rgb(fx.color("black"))
                    .rgb(fx.color("white"))
                    .f(fx.f32("amount")),
            ),
            "op.video.black_white" => self.single("fs_black_white", img, P::new()),
            "op.video.color_balance" => {
                let mut p = P::new();
                for k in [
                    "shadow_r", "shadow_g", "shadow_b", "mid_r", "mid_g", "mid_b", "high_r",
                    "high_g", "high_b",
                ] {
                    p = p.f(fx.f32(k));
                }
                self.single("fs_color_balance", img, p.b(fx.bool("preserve_luminosity")))
            }
            "op.video.leave_color" => self.single(
                "fs_leave_color",
                img,
                P::new()
                    .f(fx.f32("amount"))
                    .rgb(fx.color("color"))
                    .f(fx.f32("tolerance"))
                    .f(fx.f32("softness"))
                    .b(fx.choice("match") == 1),
            ),
            "op.video.transform" => {
                let m = transform_matrix(fx, img.width as f64, img.height as f64);
                let opacity = fx.f32("opacity") / 100.0;
                self.single("fs_affine", img, P::new().all(&m).f(opacity))
            }
            "op.video.mirror" => self.single(
                "fs_mirror",
                img,
                P::new().v2(fx.point("center")).f(fx.f32("angle")),
            ),
            "op.video.corner_pin" => {
                let (w, h) = (img.width as f64, img.height as f64);
                let px = |k: &str| {
                    let p = fx.point(k);
                    [p[0] * w, p[1] * h]
                };
                let dst = [px("ul"), px("ur"), px("lr"), px("ll")];
                let src = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
                match homography(&dst, &src) {
                    Some(hm) => self.single(
                        "fs_homography",
                        img,
                        P::new().all(&hm.map(|v| v as f32)).f(0.0).f(0.5).f(0.5),
                    ),
                    None => img,
                }
            }
            "op.video.offset" => self.single(
                "fs_offset",
                img,
                P::new().v2(fx.point("center")).f(fx.f32("blend")),
            ),
            "op.video.spherize" => self.single(
                "fs_spherize",
                img,
                P::new().f(fx.f32("radius") * s).v2(fx.point("center")),
            ),
            "op.video.twirl" => self.single(
                "fs_twirl",
                img,
                P::new()
                    .f(fx.f32("angle"))
                    .f(fx.f32("radius"))
                    .v2(fx.point("center")),
            ),
            "op.video.wave_warp" => self.single(
                "fs_wave_warp",
                img,
                P::new()
                    .f(fx.choice("wave") as f32)
                    .f(fx.f32("height") * s)
                    .f(fx.f32("width") * s)
                    .f(fx.f32("direction"))
                    .f(fx.f32("speed"))
                    .f(fx.f32("phase"))
                    .time(clip_time),
            ),
            "op.video.ramp" => self.single(
                "fs_ramp",
                img,
                P::new()
                    .v2(fx.point("start"))
                    .rgb(fx.color("start_color"))
                    .v2(fx.point("end"))
                    .rgb(fx.color("end_color"))
                    .b(fx.choice("shape") == 1)
                    .f(fx.f32("blend")),
            ),
            "op.video.gamma" => self.single("fs_gamma", img, P::new().f(fx.f32("gamma"))),
            "op.video.color_replace" => self.single(
                "fs_color_replace",
                img,
                P::new()
                    .rgb(fx.color("target"))
                    .rgb(fx.color("replace"))
                    .f(fx.f32("similarity"))
                    .b(fx.bool("solid")),
            ),
            "op.video.ultra_key" => {
                // presets change the matte generation defaults
                let (tr, tol, ped) = match fx.choice("setting") {
                    1 => (
                        fx.f32("transparency") * 0.8,
                        fx.f32("tolerance") * 0.8,
                        fx.f32("pedestal"),
                    ),
                    2 => (
                        fx.f32("transparency") * 1.2,
                        fx.f32("tolerance") * 1.2,
                        fx.f32("pedestal") * 1.5,
                    ),
                    _ => (
                        fx.f32("transparency"),
                        fx.f32("tolerance"),
                        fx.f32("pedestal"),
                    ),
                };
                let p = P::new()
                    .f(fx.choice("output") as f32)
                    .f(fx.choice("setting") as f32)
                    .rgb(fx.color("key_color"))
                    .f(tr.min(100.0))
                    .f(fx.f32("highlight"))
                    .f(fx.f32("shadow"))
                    .f(tol.min(100.0))
                    .f(ped.min(100.0))
                    .f(fx.f32("choke"))
                    .f(fx.f32("soften"))
                    .f(fx.f32("contrast"))
                    .f(fx.f32("desaturate"))
                    .f(fx.f32("range"))
                    .f(fx.f32("spill"))
                    .f(fx.f32("luma"));
                self.single("fs_ultra_key", img, p)
            }
            "op.video.color_key" => self.single(
                "fs_color_key",
                img,
                P::new()
                    .rgb(fx.color("key_color"))
                    .f(fx.f32("tolerance"))
                    .f(fx.f32("edge_thin"))
                    .f(fx.f32("edge_feather")),
            ),
            "op.video.luma_key" => self.single(
                "fs_luma_key",
                img,
                P::new().f(fx.f32("threshold")).f(fx.f32("cutoff")),
            ),
            // handled by the compositor after Motion
            "op.video.track_matte" | "op.video.posterize_time" => img,
            "op.video.noise" => self.single(
                "fs_noise",
                img,
                P::new()
                    .f(fx.f32("amount"))
                    .b(fx.bool("color"))
                    .b(fx.bool("clip"))
                    .f(seed),
            ),
            "op.video.drop_shadow" => {
                let ang = fx.f64("direction").to_radians();
                let dist = fx.f64("distance") * s as f64;
                let off = [ang.sin() * dist, -ang.cos() * dist];
                let shadow = self.work(img.width, img.height);
                self.pass(
                    "fs_shadow",
                    &[&img.view],
                    P::new()
                        .v2(off)
                        .rgb(fx.color("color"))
                        .f(fx.f32("opacity") / 100.0),
                    img.size(),
                    &shadow,
                );
                let blurred = self.blur(shadow, fx.f32("softness") * s / 3.0, false, true, true);
                let out = self.work(img.width, img.height);
                self.pass(
                    "fs_over",
                    &[&img.view, &blurred.view],
                    P::new().b(fx.bool("shadow_only")),
                    img.size(),
                    &out,
                );
                self.put(img);
                self.put(blurred);
                out
            }
            "op.video.basic_3d" => {
                let (w, h) = (img.width as f64, img.height as f64);
                let dst =
                    basic_3d_corners(fx.f64("swivel"), fx.f64("tilt"), fx.f64("distance"), w, h);
                let src = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
                let spec = if fx.bool("specular") { 0.35 } else { 0.0 };
                match homography(&dst, &src) {
                    Some(hm) => {
                        let light = [
                            0.5 + fx.f64("swivel").to_radians().sin() * 0.4,
                            0.5 - fx.f64("tilt").to_radians().sin() * 0.4,
                        ];
                        self.single(
                            "fs_homography",
                            img,
                            P::new().all(&hm.map(|v| v as f32)).f(spec).v2(light),
                        )
                    }
                    None => img,
                }
            }
            "op.video.mosaic" => self.single(
                "fs_mosaic",
                img,
                P::new()
                    .f(fx.f32("horizontal"))
                    .f(fx.f32("vertical"))
                    .b(fx.bool("sharp")),
            ),
            "op.video.posterize" => self.single("fs_posterize", img, P::new().f(fx.f32("level"))),
            "op.video.find_edges" => self.single(
                "fs_find_edges",
                img,
                P::new().b(fx.bool("invert")).f(fx.f32("blend")),
            ),
            "op.video.emboss" => self.single(
                "fs_emboss",
                img,
                P::new()
                    .f(fx.f32("direction"))
                    .f(fx.f32("relief") * s)
                    .f(fx.f32("contrast"))
                    .f(fx.f32("blend")),
            ),
            "op.video.solarize" => self.single("fs_solarize", img, P::new().f(fx.f32("threshold"))),
            "op.video.replicate" => self.single("fs_replicate", img, P::new().f(fx.f32("count"))),
            "op.video.threshold" => self.single("fs_threshold", img, P::new().f(fx.f32("level"))),
            "op.video.crop" => {
                let p = P::new()
                    .f(fx.f32("left") / 100.0)
                    .f(fx.f32("top") / 100.0)
                    .f(fx.f32("right") / 100.0)
                    .f(fx.f32("bottom") / 100.0)
                    .b(fx.bool("zoom"))
                    .f(fx.f32("feather") * s);
                self.single("fs_crop", img, p)
            }
            "op.video.horizontal_flip" => {
                let w = img.width as f32;
                self.single(
                    "fs_affine",
                    img,
                    P::new().all(&[-1.0, 0.0, w, 0.0, 1.0, 0.0]).f(1.0),
                )
            }
            "op.video.vertical_flip" => {
                let h = img.height as f32;
                self.single(
                    "fs_affine",
                    img,
                    P::new().all(&[1.0, 0.0, 0.0, 0.0, -1.0, h]).f(1.0),
                )
            }
            "op.video.edge_feather" => {
                self.single("fs_edge_feather", img, P::new().f(fx.f32("amount")))
            }
            "op.video.linear_wipe" => self.single(
                "fs_linear_wipe",
                img,
                P::new()
                    .f(fx.f32("completion") / 100.0)
                    .f(fx.f32("angle"))
                    .f(fx.f32("feather") * s),
            ),
            "op.video.radial_wipe" => self.single(
                "fs_radial_wipe",
                img,
                P::new()
                    .f(fx.f32("completion") / 100.0)
                    .f(fx.f32("start"))
                    .v2(fx.point("center"))
                    .f(fx.choice("wipe") as f32)
                    .f(fx.f32("feather") * s / 10.0),
            ),
            "op.video.timecode" => {
                let text = timecode_text(fx, req, layer);
                self.timecode_overlay(fx, img, s, text, seq_size)
            }
            "op.video.glow" | "op.video.radiant_glow" => self.glow(fx, img, s),
            "op.video.halation" => self.halation(fx, img, s),
            "op.video.halftone" => self.single(
                "fs_halftone",
                img,
                P::new()
                    .f(fx.f32("size") * s)
                    .f(fx.f32("angle"))
                    .f(fx.choice("mode") as f32)
                    .f(fx.f32("contrast") / 100.0)
                    .rgb(fx.color("ink"))
                    .rgb(fx.color("paper"))
                    .f(fx.f32("mix") / 100.0),
            ),
            "op.video.duotone" => self.single(
                "fs_duotone",
                img,
                P::new()
                    .rgb(fx.color("shadows"))
                    .rgb(fx.color("highlights"))
                    .f(fx.f32("contrast") / 100.0)
                    .f(fx.f32("mix") / 100.0),
            ),
            "op.video.oil_paint" => self.single(
                "fs_oil_paint",
                img,
                P::new()
                    .f((fx.f32("radius") * s).max(1.0))
                    .f(fx.f32("mix") / 100.0),
            ),
            "op.video.pencil_sketch" => self.single(
                "fs_sketch",
                img,
                P::new()
                    .f(fx.f32("strength") / 100.0 * 2.0)
                    .f(fx.f32("width") * s)
                    .f(fx.f32("shading") / 100.0)
                    .f(fx.f32("spacing") * s)
                    .rgb(fx.color("pencil"))
                    .rgb(fx.color("paper"))
                    .f(fx.f32("keep_color") / 100.0),
            ),
            "op.video.neon_edges" => self.single(
                "fs_neon_edges",
                img,
                P::new()
                    .f(fx.f32("thickness") * s)
                    .f(fx.f32("intensity"))
                    .f(fx.choice("colors") as f32)
                    .rgb(fx.color("color"))
                    .f(fx.f32("speed"))
                    .f(fx.f32("background") / 100.0)
                    .time(clip_time),
            ),
            "op.video.lens_flare" => self.single(
                "fs_lens_flare",
                img,
                P::new()
                    .v2(fx.point("position"))
                    .f(fx.f32("brightness"))
                    .f(0.05 + fx.f32("size") / 100.0 * 1.5)
                    .rgb(fx.color("tint"))
                    .f(fx.f32("streak") / 100.0),
            ),
            "op.video.ripple" => self.single(
                "fs_ripple",
                img,
                P::new()
                    .v2(fx.point("center"))
                    .f(fx.f32("amplitude") * s)
                    .f(fx.f32("wavelength") * s)
                    .f(fx.f32("speed"))
                    .f(fx.f32("fade") / 100.0)
                    .time(clip_time),
            ),
            "op.video.zoom_pulse" => self.single(
                "fs_zoom_pulse",
                img,
                P::new()
                    .f(fx.f32("rate"))
                    .f(fx.f32("amount") / 100.0)
                    .v2(fx.point("center"))
                    .f(fx.f32("sharpness"))
                    .f(fx.f32("offset"))
                    .time(clip_time),
            ),
            "op.video.crt" => self.single(
                "fs_crt",
                img,
                P::new()
                    .f(fx.f32("curvature") / 100.0)
                    .f(fx.f32("scanlines") / 100.0)
                    .f(fx.f32("mask") / 100.0)
                    .f(fx.f32("vignette") / 100.0)
                    .f(fx.f32("lines"))
                    .f(fx.f32("flicker") / 100.0)
                    .time(clip_time),
            ),
            "op.video.rgb_split" => self.single(
                "fs_rgb_split",
                img,
                P::new()
                    .f(fx.f32("amount") * s)
                    .f(fx.f32("angle"))
                    .f(fx.choice("mode") as f32),
            ),
            "op.video.camera_shake" => {
                let m = shake_matrix(fx, clip_time as f64, s as f64, img.width, img.height);
                self.single("fs_affine_mirror", img, P::new().all(&m))
            }
            "op.video.glitch" => self.single(
                "fs_glitch",
                img,
                P::new()
                    .f(fx.f32("intensity") / 100.0)
                    .f(fx.f32("block") * s)
                    .f(fx.f32("shift") * s)
                    .f(fx.f32("scan_lines") / 100.0)
                    .f(fx.f32("speed"))
                    .f(fx.f32("seed"))
                    .time(clip_time),
            ),
            "op.video.film_grain" => {
                let frame = if fx.bool("animated") {
                    (clip_time * 24.0).floor()
                } else {
                    0.0
                };
                self.single(
                    "fs_film_grain",
                    img,
                    P::new()
                        .f(fx.f32("amount") / 100.0)
                        .f((fx.f32("size") * s).max(0.5))
                        .b(fx.bool("color"))
                        .f(frame % 997.0),
                )
            }
            "op.video.vignette" => self.single(
                "fs_vignette",
                img,
                P::new()
                    .f(fx.f32("amount") / 100.0)
                    .f(fx.f32("midpoint") / 100.0)
                    .f(fx.f32("roundness") / 100.0)
                    .f(fx.f32("feather") / 100.0)
                    .rgb(fx.color("color")),
            ),
            "op.video.letterbox" => {
                const RATIOS: [f32; 8] = [2.39, 2.35, 2.0, 1.85, 4.0 / 3.0, 1.0, 0.8, 9.0 / 16.0];
                let ratio = RATIOS
                    .get(fx.choice("aspect") as usize)
                    .copied()
                    .unwrap_or_else(|| fx.f32("custom"));
                self.single(
                    "fs_letterbox",
                    img,
                    P::new()
                        .f(ratio)
                        .rgb(fx.color("color"))
                        .f(fx.f32("opacity") / 100.0),
                )
            }
            "op.video.lens_distortion" => self.single(
                "fs_lens",
                img,
                P::new()
                    .f(fx.f32("curvature") / 100.0)
                    .v2(fx.point("center"))
                    .f(fx.f32("zoom") / 100.0),
            ),
            "op.video.radial_blur" => self.single(
                "fs_radial_blur",
                img,
                P::new()
                    .f(fx.f32("amount"))
                    .v2(fx.point("center"))
                    .f(fx.choice("type") as f32),
            ),
            "op.video.motion_tile" => self.single(
                "fs_motion_tile",
                img,
                P::new()
                    .v2(fx.point("tile_center"))
                    .f(fx.f32("tile_width") / 100.0)
                    .f(fx.f32("tile_height") / 100.0)
                    .f(fx.f32("output_width") / 100.0)
                    .f(fx.f32("output_height") / 100.0)
                    .b(fx.bool("mirror"))
                    .f(fx.f32("phase"))
                    .b(fx.bool("horizontal_shift")),
            ),
            "op.video.light_leaks" => self.single(
                "fs_light_leaks",
                img,
                P::new()
                    .f(fx.f32("intensity") / 100.0)
                    .rgb(fx.color("color1"))
                    .rgb(fx.color("color2"))
                    .f(fx.f32("scale") / 100.0)
                    .f(fx.f32("speed"))
                    .f(fx.f32("seed"))
                    .f(fx.choice("blend") as f32)
                    .time(clip_time),
            ),
            "op.video.old_film" => self.single(
                "fs_old_film",
                img,
                P::new()
                    .f(fx.f32("sepia") / 100.0)
                    .f(fx.f32("grain") / 100.0)
                    .f(fx.f32("scratches") / 100.0)
                    .f(fx.f32("dust") / 100.0)
                    .f(fx.f32("flicker") / 100.0)
                    .f(fx.f32("vignette") / 100.0)
                    .f(fx.f32("seed"))
                    .time(clip_time),
            ),
            "op.video.vhs" => self.single(
                "fs_vhs",
                img,
                P::new()
                    .f(fx.f32("chroma") * s)
                    .f(fx.f32("noise") / 100.0)
                    .f(fx.f32("tracking") / 100.0)
                    .f(fx.f32("jitter") * s)
                    .f(fx.f32("saturation") / 100.0)
                    .f(fx.f32("seed"))
                    .time(clip_time),
            ),
            "op.video.strobe" => self.single(
                "fs_strobe",
                img,
                P::new()
                    .rgb(fx.color("color"))
                    .f(fx.f32("frequency"))
                    .f(fx.f32("duration") / 100.0)
                    .f(fx.f32("blend") / 100.0)
                    .time(clip_time),
            ),
            "op.video.kaleidoscope" => self.single(
                "fs_kaleidoscope",
                img,
                P::new()
                    .f(fx.f32("segments"))
                    .f(fx.f32("angle"))
                    .v2(fx.point("center")),
            ),
            _ => img,
        }
    }

    /// Glow: the bright part of the layer, blurred, added back as light. Radiant Glow sums
    /// blurs of six octaves (computed on a half-resolution pyramid) for a wide, natural falloff.
    fn glow(&mut self, fx: &EvalComponent, img: Tex, s: f32) -> Tex {
        let radiant = fx.effect == "op.video.radiant_glow";
        let (w, h) = (img.width, img.height);
        let bright = self.work(w, h);
        let knee = if radiant { 0.1 } else { 0.05 };
        self.pass(
            "fs_bright_pass",
            &[&img.view],
            P::new().f(fx.f32("threshold") / 100.0).f(knee),
            img.size(),
            &bright,
        );
        let radius = fx.f32("radius") * s;
        let glow = if radiant {
            self.glow_pyramid(bright, radius, fx.f32("falloff") / 100.0)
        } else {
            self.blur(bright, radius / 2.0, false, true, true)
        };
        let p = if radiant {
            P::new()
                .f(1.5 * 2f32.powf(fx.f32("exposure")))
                .rgb(fx.color("tint"))
                .f(fx.f32("tint_amount") / 100.0)
                .f(fx.f32("aberration") / 100.0 * 0.05)
                .b(fx.bool("glow_only"))
        } else {
            P::new()
                .f(fx.f32("intensity") * 1.5)
                .rgb(fx.color("color"))
                .f(if fx.choice("colors") == 1 { 1.0 } else { 0.0 })
                .f(0.0)
                .b(fx.bool("glow_only"))
        };
        let out = self.work(w, h);
        self.pass(
            "fs_glow_composite",
            &[&img.view, &glow.view],
            p,
            img.size(),
            &out,
        );
        self.put(img);
        self.put(glow);
        out
    }

    /// Halation: the red bloom film shows around highlights. A tinted glow of the bright parts.
    fn halation(&mut self, fx: &EvalComponent, img: Tex, s: f32) -> Tex {
        let (w, h) = (img.width, img.height);
        let bright = self.work(w, h);
        self.pass(
            "fs_bright_pass",
            &[&img.view],
            P::new().f(fx.f32("threshold") / 100.0).f(0.08),
            img.size(),
            &bright,
        );
        let glow = self.blur(bright, fx.f32("radius") * s / 2.0, false, true, true);
        let out = self.work(w, h);
        self.pass(
            "fs_glow_composite",
            &[&img.view, &glow.view],
            P::new()
                .f(fx.f32("intensity") * 1.5)
                .rgb(fx.color("tint"))
                .f(1.0)
                .f(0.0)
                .b(false),
            img.size(),
            &out,
        );
        self.put(glow);
        self.put(img);
        out
    }

    fn glow_pyramid(&mut self, bright: Tex, radius: f32, falloff: f32) -> Tex {
        const LEVELS: usize = 6;
        // every level is blurred by the same number of its own pixels, so level i covers a
        // radius of about radius * 2^(i - 5) in full-resolution pixels
        let sigma = (radius / 32.0).max(0.5);
        let mut blurred = Vec::with_capacity(LEVELS);
        let mut cur = bright;
        for i in 0..LEVELS {
            let next = (i + 1 < LEVELS).then(|| {
                let d = self.work((cur.width / 2).max(1), (cur.height / 2).max(1));
                self.pass("fs_downsample", &[&cur.view], P::new(), cur.size(), &d);
                d
            });
            blurred.push(self.blur(cur, sigma, false, true, true));
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        // weights: a tight falloff keeps the small octaves strong
        let raw: Vec<f32> = (0..blurred.len())
            .map(|i| 0.5f32.powf((LEVELS - 1 - i) as f32 * (1.0 - falloff)))
            .collect();
        let total: f32 = raw.iter().sum();
        let mut acc = blurred.pop().unwrap();
        let mut acc_weight = raw[blurred.len()] / total;
        while let Some(level) = blurred.pop() {
            let wi = raw[blurred.len()] / total;
            let out = self.work(level.width, level.height);
            self.pass(
                "fs_add_up",
                &[&acc.view, &level.view],
                P::new().f(acc_weight).f(wi),
                level.size(),
                &out,
            );
            self.put(acc);
            self.put(level);
            acc = out;
            acc_weight = 1.0;
        }
        acc
    }

    fn lumetri(&mut self, fx: &EvalComponent, img: Tex, s: f32) -> Tex {
        let input = self.lut(&fx.text("input_lut"));
        let look = self.lut(&fx.text("look"));
        let identity = self.identity_lut();
        let master = fx.curve("curve_master");
        let r = fx.curve("curve_red");
        let g = fx.curve("curve_green");
        let b = fx.curve("curve_blue");
        let straight = |c: &[[f32; 2]]| c.is_empty() || c == [[0.0, 0.0], [1.0, 1.0]];
        let curves_on = !(straight(&master) && straight(&r) && straight(&g) && straight(&b));
        let curves = self.curves(&master, &r, &g, &b);
        let wheel = |k: &str| fx.point(k);
        let p = P::new()
            .f(fx.f32("temperature"))
            .f(fx.f32("tint"))
            .f(fx.f32("exposure"))
            .f(fx.f32("contrast"))
            .f(fx.f32("highlights"))
            .f(fx.f32("shadows"))
            .f(fx.f32("whites"))
            .f(fx.f32("blacks"))
            .f(fx.f32("saturation"))
            .f(fx.f32("look_intensity"))
            .f(fx.f32("faded_film"))
            .f(fx.f32("vibrance"))
            .f(fx.f32("creative_saturation"))
            .v2(wheel("shadow_tint"))
            .v2(wheel("highlight_tint"))
            .f(fx.f32("tint_balance"))
            .v2(wheel("wheel_shadows"))
            .f(fx.f32("wheel_shadows_luma"))
            .v2(wheel("wheel_midtones"))
            .f(fx.f32("wheel_midtones_luma"))
            .v2(wheel("wheel_highlights"))
            .f(fx.f32("wheel_highlights_luma"))
            .f(fx.f32("vignette_amount"))
            .f(fx.f32("vignette_midpoint"))
            .f(fx.f32("vignette_roundness"))
            .f(fx.f32("vignette_feather"))
            .b(input.is_some())
            .b(look.is_some())
            .b(curves_on);
        let out = self.work(img.width, img.height);
        let lut_in = input.unwrap_or_else(|| identity.clone());
        let lut_look = look.unwrap_or(identity);
        self.pass(
            "fs_lumetri",
            &[&img.view, &curves.view, &lut_in, &lut_look],
            p,
            img.size(),
            &out,
        );
        self.put(img);
        let sharpen = fx.f32("sharpen");
        if sharpen.abs() > 0.01 {
            let _ = s;
            return self.single("fs_sharpen", out, P::new().f(sharpen));
        }
        out
    }

    fn timecode_overlay(
        &mut self,
        fx: &EvalComponent,
        img: Tex,
        s: f32,
        text: String,
        seq_size: (u32, u32),
    ) -> Tex {
        // the overlay is drawn like a text layer: white on a translucent black box
        let size = fx.f32("size") / 100.0 * seq_size.1 as f32 / s.max(1e-3);
        let box_opacity = fx.f32("opacity");
        let text_fx = EvalComponent {
            id: fx.id,
            effect: catalog::TEXT.into(),
            values: vec![
                ("font_size".into(), Value::Float(size as f64)),
                ("align".into(), Value::Choice(1)),
                ("fill".into(), Value::Color(Rgba::WHITE)),
                ("background".into(), Value::Bool(box_opacity > 0.0)),
                ("background_color".into(), Value::Color(Rgba::BLACK)),
                (
                    "background_opacity".into(),
                    Value::Float(box_opacity as f64),
                ),
                ("background_size".into(), Value::Float(size as f64 * 0.2)),
                ("position".into(), Value::Point(fx.point("position"))),
                ("scale".into(), Value::Float(100.0)),
                ("opacity".into(), Value::Float(100.0)),
                (
                    "font".into(),
                    Value::Text(if cfg!(windows) {
                        "Consolas".into()
                    } else if cfg!(target_os = "macos") {
                        "Menlo".into()
                    } else {
                        "DejaVu Sans Mono".into()
                    }),
                ),
            ],
        };
        self.draw_text(&text_fx, img, s, Some(text))
    }
}

fn timecode_text(fx: &EvalComponent, req: &Request, layer: &ClipLayer) -> String {
    let Some(seq) = req.project.sequence(req.sequence) else {
        return String::new();
    };
    let rate = seq.rate();
    let offset = rate.frames_to_dur(fx.f64("offset").round() as i64);
    let d = match fx.choice("source") {
        // media time
        0 => match layer.source {
            LayerSource::Media { time, .. } => time.since_zero(),
            _ => layer.src_time.since_zero(),
        },
        1 => layer.src_time.since_zero(),
        _ => req.time.since_zero() + seq.settings.start_timecode,
    } + offset;
    let mut f = TimecodeFormat::new(rate, seq.settings.drop_frame);
    if fx.choice("format") == 1 {
        f.display = TimeDisplay::Frames;
    }
    f.format(d)
}

/// Smooth pseudo-random motion in -1..1 (a few detuned sines), the same for every render.
fn shake_noise(t: f64, k: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    (t * tau + k).sin() * 0.5
        + (t * tau * 2.13 + k * 2.1 + 1.3).sin() * 0.3
        + (t * tau * 3.71 + k * 3.3 + 0.7).sin() * 0.2
}

/// Inverse matrix (output pixels -> input pixels) of Camera Shake at a clip time.
fn shake_matrix(fx: &EvalComponent, t: f64, scale: f64, w: u32, h: u32) -> [f32; 6] {
    let (w, h) = (w as f64, h as f64);
    let seed = fx.f64("seed") * 7.31;
    let tt = t * fx.f64("frequency");
    let amp = fx.f64("amplitude") * scale;
    let (dx, dy) = (
        shake_noise(tt, seed) * amp,
        shake_noise(tt, seed + 1.7) * amp,
    );
    let rot = shake_noise(tt, seed + 3.1) * fx.f64("rotation").to_radians();
    let zoom = (fx.f64("zoom") / 100.0).max(0.01);
    // p_in = c + R(-rot) (p_out - c - d) / zoom
    let (sn, cs) = (-rot).sin_cos();
    let (a00, a01, a10, a11) = (cs / zoom, -sn / zoom, sn / zoom, cs / zoom);
    let (cx, cy) = (w / 2.0 + dx, h / 2.0 + dy);
    let tx = w / 2.0 - (a00 * cx + a01 * cy);
    let ty = h / 2.0 - (a10 * cx + a11 * cy);
    [a00, a01, tx, a10, a11, ty].map(|v| v as f32)
}

/// Inverse matrix of the Transform effect (output pixels -> input pixels).
fn transform_matrix(fx: &EvalComponent, w: f64, h: f64) -> [f32; 6] {
    let a = fx.point("anchor");
    let p = fx.point("position");
    let (ax, ay) = (a[0] * w, a[1] * h);
    let (px, py) = (p[0] * w, p[1] * h);
    let sh = fx.f64("scale_height") / 100.0;
    let sw = if fx.bool("uniform_scale") {
        sh
    } else {
        fx.f64("scale_width") / 100.0
    };
    let rot = fx.f64("rotation").to_radians();
    let skew = fx.f64("skew").to_radians().tan();
    let skew_axis = fx.f64("skew_axis").to_radians();
    // forward: T(p) * R(rot) * Rs(axis) * K(skew) * Rs(-axis) * S * T(-a)
    let mul = |m: [[f64; 3]; 3], n: [[f64; 3]; 3]| {
        let mut o = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                o[i][j] = (0..3).map(|k| m[i][k] * n[k][j]).sum();
            }
        }
        o
    };
    let rotm = |t: f64| {
        [
            [t.cos(), -t.sin(), 0.0],
            [t.sin(), t.cos(), 0.0],
            [0.0, 0.0, 1.0],
        ]
    };
    let fwd = [
        [[1.0, 0.0, px], [0.0, 1.0, py], [0.0, 0.0, 1.0]],
        rotm(rot),
        rotm(skew_axis),
        [[1.0, skew, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        rotm(-skew_axis),
        [[sw, 0.0, 0.0], [0.0, sh, 0.0], [0.0, 0.0, 1.0]],
        [[1.0, 0.0, -ax], [0.0, 1.0, -ay], [0.0, 0.0, 1.0]],
    ]
    .into_iter()
    .reduce(mul)
    .unwrap();
    let det = fwd[0][0] * fwd[1][1] - fwd[0][1] * fwd[1][0];
    if det.abs() < 1e-12 {
        return [0.0, 0.0, -1e6, 0.0, 0.0, -1e6];
    }
    let (a00, a01, a10, a11) = (
        fwd[1][1] / det,
        -fwd[0][1] / det,
        -fwd[1][0] / det,
        fwd[0][0] / det,
    );
    let (tx, ty) = (fwd[0][2], fwd[1][2]);
    [
        a00 as f32,
        a01 as f32,
        (-(a00 * tx + a01 * ty)) as f32,
        a10 as f32,
        a11 as f32,
        (-(a10 * tx + a11 * ty)) as f32,
    ]
}

/// Homography mapping four `from` points onto four `to` points (row-major 3x3).
pub(crate) fn homography(from: &[[f64; 2]; 4], to: &[[f64; 2]; 4]) -> Option<[f64; 9]> {
    // solve the 8x8 linear system with Gaussian elimination
    let mut m = [[0f64; 9]; 8];
    for i in 0..4 {
        let (x, y) = (from[i][0], from[i][1]);
        let (u, v) = (to[i][0], to[i][1]);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    for c in 0..8 {
        let piv = (c..8).max_by(|a, b| m[*a][c].abs().total_cmp(&m[*b][c].abs()))?;
        if m[piv][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, piv);
        for r in 0..8 {
            if r != c {
                let f = m[r][c] / m[c][c];
                let pivot = m[c];
                for (x, p) in m[r].iter_mut().zip(pivot).skip(c) {
                    *x -= f * p;
                }
            }
        }
    }
    let mut h = [0f64; 9];
    for i in 0..8 {
        h[i] = m[i][8] / m[i][i];
    }
    h[8] = 1.0;
    Some(h)
}

/// Projected corners of a layer rotated by swivel (around Y) and tilt (around X), seen from a
/// camera at a distance proportional to the layer size.
fn basic_3d_corners(swivel: f64, tilt: f64, distance: f64, w: f64, h: f64) -> [[f64; 2]; 4] {
    let (sy, cy) = swivel.to_radians().sin_cos();
    let (sx, cx) = tilt.to_radians().sin_cos();
    let focal = w.max(h) * 2.0;
    let z0 = focal + distance / 100.0 * focal;
    let corners = [
        [-w / 2.0, -h / 2.0],
        [w / 2.0, -h / 2.0],
        [w / 2.0, h / 2.0],
        [-w / 2.0, h / 2.0],
    ];
    corners.map(|[x, y]| {
        // rotate around Y then X
        let (x1, z1) = (x * cy, -x * sy);
        let (y2, z2) = (y * cx - z1 * sx, y * sx + z1 * cx);
        let z = (z0 + z2).max(1.0);
        [w / 2.0 + x1 * focal / z, h / 2.0 + y2 * focal / z]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_maps_corners() {
        let from = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let to = [[1.0, 2.0], [12.0, 0.0], [11.0, 13.0], [0.0, 9.0]];
        let h = homography(&from, &to).unwrap();
        for i in 0..4 {
            let (x, y) = (from[i][0], from[i][1]);
            let w = h[6] * x + h[7] * y + h[8];
            let u = (h[0] * x + h[1] * y + h[2]) / w;
            let v = (h[3] * x + h[4] * y + h[5]) / w;
            assert!((u - to[i][0]).abs() < 1e-9 && (v - to[i][1]).abs() < 1e-9);
        }
    }

    #[test]
    fn neutral_basic_3d_is_identity() {
        let c = basic_3d_corners(0.0, 0.0, 0.0, 100.0, 50.0);
        assert!((c[0][0]).abs() < 1e-9 && (c[2][1] - 50.0).abs() < 1e-9);
    }
}
