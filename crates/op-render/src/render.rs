//! Frame rendering: plan -> sources -> effects -> Motion/Opacity -> composite.

use std::collections::HashMap;
use std::sync::Arc;

use op_core::catalog;
use op_core::plan::{self, ClipLayer, EvalComponent, LayerSource, TrackContent};
use op_core::*;
use op_media::{PixelLayout, VideoFrame};

use crate::gpu::Gpu;
use crate::params::{Arena, P};
use crate::pipelines::{Layout, Pipelines, WORK};
use crate::pool::{Pool, Tex};
use crate::text::{Fonts, TextImage, TextStyle};

/// Decoded pictures for the renderer. Implementations decide whether to wait for a decoder
/// (export) or return what is ready (interactive playback).
pub trait FrameSource {
    fn video_frame(&self, asset: &MediaAsset, time: SrcTime) -> Option<Arc<VideoFrame>>;
}

/// A source that never has frames (graphics-only rendering, tests).
pub struct NoFrames;

impl FrameSource for NoFrames {
    fn video_frame(&self, _asset: &MediaAsset, _time: SrcTime) -> Option<Arc<VideoFrame>> {
        None
    }
}

/// What to render.
pub struct Request<'a> {
    pub project: &'a Project,
    pub sequence: SequenceId,
    pub time: SeqTime,
    /// Preview resolution: 1.0 full, 0.5 half, 0.25 quarter...
    pub scale: f32,
    pub source: &'a dyn FrameSource,
}

/// Statistics of the last frame, for the playback overlay and diagnostics.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub passes: u32,
    pub missing_frames: u32,
    pub layers: u32,
}

struct UploadedPlanes {
    /// Asset, frame index, width and pixel layout (a file can switch between hardware and
    /// software decoding, which deliver different layouts).
    key: (AssetId, i64, u32, PixelLayout),
    planes: Vec<Tex>,
}

pub struct Renderer {
    pub gpu: Arc<Gpu>,
    pipes: Pipelines,
    arena: Arena,
    pool: Pool,
    encoder: Option<wgpu::CommandEncoder>,
    uploads: Vec<UploadedPlanes>,
    plane_pool: HashMap<(u32, u32, wgpu::TextureFormat), Vec<Tex>>,
    /// Readback buffers by size, reused frame after frame (exports read every frame).
    pub(crate) readbacks: HashMap<u64, Vec<wgpu::Buffer>>,
    text_cache: HashMap<u64, (Tex, [f32; 2], u32)>,
    lut_cache: HashMap<String, Option<wgpu::TextureView>>,
    curve_cache: HashMap<u64, Tex>,
    identity_lut: wgpu::TextureView,
    transparent: Tex,
    fonts: Arc<Fonts>,
    frame_counter: u32,
    pub(crate) scopes: HashMap<u32, Arc<crate::output::ScopeResources>>,
    pub stats: Stats,
}

fn hash_of<T: std::hash::Hash>(v: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

impl Renderer {
    pub fn new(gpu: Arc<Gpu>) -> Renderer {
        let device = &gpu.device;
        let pipes = Pipelines::new(device);
        let identity = crate::lut::Cube {
            size: 2,
            data: (0..8)
                .map(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32])
                .collect(),
        };
        let identity_lut = crate::lut::upload_cube(device, &gpu.queue, &identity);
        let transparent = Tex::new(device, 1, 1, WORK, crate::pool::WORK_USAGE, "transparent");
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &transparent.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8; 8],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        Renderer {
            gpu,
            pipes,
            arena: Arena::new(),
            pool: Pool::default(),
            encoder: None,
            uploads: Vec::new(),
            plane_pool: HashMap::new(),
            readbacks: HashMap::new(),
            text_cache: HashMap::new(),
            lut_cache: HashMap::new(),
            curve_cache: HashMap::new(),
            identity_lut,
            transparent,
            fonts: Fonts::global(),
            frame_counter: 0,
            scopes: HashMap::new(),
            stats: Stats::default(),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    pub(crate) fn encoder_mut(&mut self) -> &mut wgpu::CommandEncoder {
        if self.encoder.is_none() {
            self.encoder = Some(self.gpu.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                },
            ));
        }
        self.encoder.as_mut().unwrap()
    }

    /// Submits recorded work.
    pub fn submit(&mut self) -> Option<wgpu::SubmissionIndex> {
        let enc = self.encoder.take()?;
        self.arena.flush(&self.gpu.queue);
        let idx = self.gpu.queue.submit(Some(enc.finish()));
        self.arena.reset();
        Some(idx)
    }

    /// Returns a texture from `render` to the pool once it has been displayed or read.
    pub fn recycle(&mut self, t: Tex) {
        self.pool.put(t);
    }

    pub fn work(&mut self, w: u32, h: u32) -> Tex {
        self.pool.get(&self.gpu.device, w, h, WORK)
    }

    pub fn target(&mut self, w: u32, h: u32, format: wgpu::TextureFormat) -> Tex {
        self.pool.get(&self.gpu.device, w, h, format)
    }

    /// Records one full-screen pass.
    pub fn pass(
        &mut self,
        entry: &'static str,
        inputs: &[&wgpu::TextureView],
        params: P,
        in_size: [f32; 2],
        out: &Tex,
    ) {
        let mut u = params.u;
        u.out_size = out.size();
        u.in_size = in_size;
        let device = self.gpu.device.clone();
        let (chunk, offset) = self.arena.push(&device, &u);
        let layout = self.pipes.layout_of(entry);
        let bgl = self.pipes.bind_layout(layout).clone();
        let t = &self.transparent.view;
        let view = |i: usize| inputs.get(i).copied().unwrap_or(t);
        let bind = match layout {
            Layout::Main => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: self.arena.buffer(chunk),
                            offset: 0,
                            size: wgpu::BufferSize::new(crate::params::SLOT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.pipes.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(view(0)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(view(1)),
                    },
                ],
            }),
            Layout::Convert | Layout::Lumetri => {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: self.arena.buffer(chunk),
                                offset: 0,
                                size: wgpu::BufferSize::new(crate::params::SLOT),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.pipes.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(inputs[0]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(inputs[1]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(inputs[2]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(inputs[3]),
                        },
                    ],
                })
            }
        };
        let pipeline = self.pipes.get(&device, entry).clone();
        let enc = self.encoder_mut();
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(entry),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rp.set_pipeline(&pipeline);
        rp.set_bind_group(0, &bind, &[offset]);
        rp.draw(0..3, 0..1);
        drop(rp);
        self.stats.passes += 1;
    }

    fn clear(&mut self, w: u32, h: u32) -> Tex {
        let t = self.work(w, h);
        self.pass(
            "fs_fill",
            &[],
            P::new().all(&[0.0, 0.0, 0.0, 0.0]),
            [1.0, 1.0],
            &t,
        );
        t
    }

    /// Renders one sequence frame into a premultiplied working texture of the sequence size
    /// times `scale`. Call `submit` afterwards, and `recycle` the texture when done with it.
    pub fn render(&mut self, req: &Request) -> Tex {
        self.stats = Stats::default();
        self.frame_counter = self.frame_counter.wrapping_add(1);
        let t = self.render_sequence(req, req.sequence, req.time, 0);
        self.pool.end_frame();
        t
    }

    fn seq_size(seq: &Sequence, scale: f32) -> (u32, u32) {
        let w = ((seq.settings.width as f32 * scale).round() as u32).max(1);
        let h = ((seq.settings.height as f32 * scale).round() as u32).max(1);
        (w, h)
    }

    fn render_sequence(&mut self, req: &Request, sid: SequenceId, t: SeqTime, depth: usize) -> Tex {
        let Some(seq) = req.project.sequence(sid) else {
            return self.clear(16, 16);
        };
        let (w, h) = Self::seq_size(seq, req.scale);
        let mut acc = self.clear(w, h);
        if depth >= plan::MAX_NESTING {
            return acc;
        }
        let plan = plan::frame_plan(req.project, sid, t);
        let linear = plan.linear;
        for tp in &plan.tracks {
            match &tp.content {
                TrackContent::Clip(layer) => {
                    self.stats.layers += 1;
                    if layer.source == LayerSource::Adjustment {
                        acc = self.adjustment(req, layer, acc, depth);
                        continue;
                    }
                    let matte = layer
                        .effects()
                        .find(|e| e.effect == "op.video.track_matte")
                        .cloned();
                    let Some(img) = self.layer(req, seq, layer, depth) else {
                        continue;
                    };
                    let (m, opacity, mode) = self.motion(layer, &img, w, h, req.scale);
                    match matte.filter(|m| m.choice("matte") > 0) {
                        Some(me) => {
                            let canvas = self.work(w, h);
                            self.pass(
                                "fs_place",
                                &[&self.transparent.view.clone(), &img.view],
                                P::new().all(&m).f(opacity),
                                img.size(),
                                &canvas,
                            );
                            self.pool.put(img);
                            let track = me.choice("matte") as usize - 1;
                            let matte_img = self.track_alone(req, &plan, track, w, h, depth);
                            let keyed = self.work(w, h);
                            self.pass(
                                "fs_matte",
                                &[&canvas.view, &matte_img.view],
                                P::new()
                                    .b(me.choice("composite") == 1)
                                    .b(me.bool("reverse")),
                                [w as f32, h as f32],
                                &keyed,
                            );
                            self.pool.put(canvas);
                            self.pool.put(matte_img);
                            acc = self.composite(acc, &keyed, IDENTITY, 1.0, mode, linear);
                            self.pool.put(keyed);
                        }
                        None => {
                            acc = self.composite(acc, &img, m, opacity, mode, linear);
                            self.pool.put(img);
                        }
                    }
                }
                TrackContent::Transition {
                    effect,
                    progress,
                    params,
                    from,
                    to,
                } => {
                    let a = match from {
                        Some(l) => self.placed(req, seq, l, w, h, depth),
                        None => self.clear(w, h),
                    };
                    let b = match to {
                        Some(l) => self.placed(req, seq, l, w, h, depth),
                        None => self.clear(w, h),
                    };
                    let mixed =
                        self.transition(effect, *progress as f32, params, &a, &b, w, h, req.scale);
                    self.pool.put(a);
                    self.pool.put(b);
                    acc = self.composite(acc, &mixed, IDENTITY, 1.0, 0, linear);
                    self.pool.put(mixed);
                }
            }
        }
        acc
    }

    /// Content of one track alone on a transparent canvas (for track mattes).
    fn track_alone(
        &mut self,
        req: &Request,
        plan: &plan::FramePlan,
        track: usize,
        w: u32,
        h: u32,
        depth: usize,
    ) -> Tex {
        let mut acc = self.clear(w, h);
        let Some(seq) = req.project.sequence(plan.sequence) else {
            return acc;
        };
        if let Some(tp) = plan.tracks.iter().find(|t| t.track == track)
            && let TrackContent::Clip(layer) = &tp.content
            && let Some(img) = self.layer(req, seq, layer, depth)
        {
            let (m, opacity, mode) = self.motion(layer, &img, w, h, req.scale);
            acc = self.composite(acc, &img, m, opacity, mode, false);
            self.pool.put(img);
        }
        acc
    }

    /// A clip through Motion and Opacity onto a transparent canvas (transition inputs).
    fn placed(
        &mut self,
        req: &Request,
        seq: &Sequence,
        layer: &ClipLayer,
        w: u32,
        h: u32,
        depth: usize,
    ) -> Tex {
        let canvas = self.work(w, h);
        match self.layer(req, seq, layer, depth) {
            Some(img) => {
                let (m, opacity, _) = self.motion(layer, &img, w, h, req.scale);
                let t = self.transparent.view.clone();
                self.pass(
                    "fs_place",
                    &[&t, &img.view],
                    P::new().all(&m).f(opacity),
                    img.size(),
                    &canvas,
                );
                self.pool.put(img);
            }
            None => self.pass("fs_fill", &[], P::new().all(&[0.0; 4]), [1.0, 1.0], &canvas),
        }
        canvas
    }

    fn composite(
        &mut self,
        acc: Tex,
        layer: &Tex,
        m: [f32; 6],
        opacity: f32,
        mode: u32,
        linear: bool,
    ) -> Tex {
        if opacity <= 0.0 {
            return acc;
        }
        let out = self.work(acc.width, acc.height);
        let seed = (self.frame_counter % 997) as f32;
        self.pass(
            "fs_composite",
            &[&acc.view, &layer.view],
            P::new().all(&m).f(opacity).f(mode as f32).b(linear).f(seed),
            layer.size(),
            &out,
        );
        self.pool.put(acc);
        out
    }

    /// Inverse Motion matrix (sequence pixels -> layer pixels), opacity and blend mode.
    fn motion(
        &self,
        layer: &ClipLayer,
        img: &Tex,
        w: u32,
        h: u32,
        _scale: f32,
    ) -> ([f32; 6], f32, u32) {
        let (lw, lh) = (img.width as f64, img.height as f64);
        let (sw, sh) = (w as f64, h as f64);
        let (mut px, mut py, mut sx, mut sy, mut rot, mut ax, mut ay) =
            (0.5 * sw, 0.5 * sh, 1.0, 1.0, 0.0, 0.5 * lw, 0.5 * lh);
        if let Some(m) = layer.fixed(catalog::MOTION) {
            let p = m.point("position");
            px = p[0] * sw;
            py = p[1] * sh;
            let s = m.f64("scale") / 100.0;
            sy = s;
            sx = if m.bool("uniform_scale") {
                s
            } else {
                m.f64("scale_width") / 100.0
            };
            rot = m.f64("rotation").to_radians();
            let a = m.point("anchor");
            ax = a[0] * lw;
            ay = a[1] * lh;
        }
        let (opacity, mode) = match layer.fixed(catalog::OPACITY) {
            Some(o) => (
                (o.f64("opacity") / 100.0).clamp(0.0, 1.0) as f32,
                o.choice("blend_mode"),
            ),
            None => (1.0, 0),
        };
        if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
            // a layer scaled to nothing draws nothing
            return (IDENTITY, 0.0, mode);
        }
        let (c, s) = (rot.cos(), rot.sin());
        let m = [
            (c / sx) as f32,
            (s / sx) as f32,
            (ax - (c * px + s * py) / sx) as f32,
            (-s / sy) as f32,
            (c / sy) as f32,
            (ay - (-s * px + c * py) / sy) as f32,
        ];
        (m, opacity, mode)
    }

    /// Source picture of a layer with its standard effects applied (layer space).
    fn layer(
        &mut self,
        req: &Request,
        seq: &Sequence,
        layer: &ClipLayer,
        depth: usize,
    ) -> Option<Tex> {
        let (sw, sh) = Self::seq_size(seq, req.scale);
        let mut img = match &layer.source {
            LayerSource::Media { asset, time } => {
                let a = req.project.asset(*asset)?;
                let frame = match req.source.video_frame(a, *time) {
                    Some(f) => f,
                    None => {
                        self.stats.missing_frames += 1;
                        return None;
                    }
                };
                let (dw, dh) = a
                    .video
                    .as_ref()
                    .map(|v| v.display_size())
                    .unwrap_or((frame.width, frame.height));
                let par = a
                    .interpretation
                    .pixel_aspect
                    .or(a.video.as_ref().map(|v| v.pixel_aspect))
                    .unwrap_or((1, 1));
                let dw = (dw as f64 * par.0 as f64 / par.1.max(1) as f64).round() as u32;
                let (mut lw, mut lh) = (dw as f32 * req.scale, dh as f32 * req.scale);
                if layer.scale_to_frame {
                    let k = (sw as f32 / lw).min(sh as f32 / lh);
                    lw *= k;
                    lh *= k;
                }
                let rotation = a.video.as_ref().map(|v| v.rotation).unwrap_or(0);
                self.convert(
                    *asset,
                    &frame,
                    a,
                    rotation,
                    lw.round().max(1.0) as u32,
                    lh.round().max(1.0) as u32,
                )
            }
            LayerSource::Sequence { sequence, time } => {
                let nested = req.project.sequence(*sequence)?;
                let t = self.render_sequence(req, *sequence, *time, depth + 1);
                if layer.scale_to_frame
                    && (nested.settings.width, nested.settings.height)
                        != (seq.settings.width, seq.settings.height)
                {
                    let k = (sw as f32 / t.width as f32).min(sh as f32 / t.height as f32);
                    let (fw, fh) = (
                        (t.width as f32 * k).round() as u32,
                        (t.height as f32 * k).round() as u32,
                    );
                    let out = self.work(fw.max(1), fh.max(1));
                    self.pass("fs_copy", &[&t.view], P::new(), t.size(), &out);
                    self.pool.put(t);
                    out
                } else {
                    t
                }
            }
            LayerSource::Color(c) => {
                let t = self.work(sw, sh);
                self.pass("fs_fill", &[], P::new().rgba(*c), [1.0, 1.0], &t);
                t
            }
            LayerSource::Transparent | LayerSource::Adjustment => self.clear(sw, sh),
            LayerSource::Bars => {
                let t = self.work(sw, sh);
                self.pass("fs_bars", &[], P::new(), [1.0, 1.0], &t);
                t
            }
            LayerSource::Graphic => self.clear(sw, sh),
        };
        let clip_time = layer.src_time.seconds() as f32;
        let effects: Vec<EvalComponent> = layer.effects().cloned().collect();
        for fx in &effects {
            img = self.effect(fx, img, req, layer, clip_time, (sw, sh));
        }
        Some(img)
    }

    /// Adjustment layer: its effects apply to everything below, blended by its opacity.
    fn adjustment(&mut self, req: &Request, layer: &ClipLayer, acc: Tex, _depth: usize) -> Tex {
        let opacity = layer
            .fixed(catalog::OPACITY)
            .map(|o| (o.f64("opacity") / 100.0).clamp(0.0, 1.0) as f32)
            .unwrap_or(1.0);
        let copy = self.work(acc.width, acc.height);
        self.pass("fs_copy", &[&acc.view], P::new(), acc.size(), &copy);
        let mut img = copy;
        let effects: Vec<EvalComponent> = layer.effects().cloned().collect();
        let size = (acc.width, acc.height);
        for fx in &effects {
            img = self.effect(fx, img, req, layer, layer.src_time.seconds() as f32, size);
        }
        let out = self.work(acc.width, acc.height);
        self.pass(
            "fs_mix",
            &[&acc.view, &img.view],
            P::new().f(opacity),
            acc.size(),
            &out,
        );
        self.pool.put(acc);
        self.pool.put(img);
        out
    }

    /// Uploads a decoded frame (cached for repeated renders of the same frame) and converts it
    /// to a working texture of `w` x `h` in display orientation.
    fn convert(
        &mut self,
        asset: AssetId,
        frame: &Arc<VideoFrame>,
        a: &MediaAsset,
        rotation: i32,
        w: u32,
        h: u32,
    ) -> Tex {
        let key = (asset, frame.index, frame.width, frame.layout);
        let cached = self.uploads.iter().position(|u| u.key == key);
        let idx = match cached {
            Some(i) => i,
            None => {
                let planes = self.upload(frame);
                if self.uploads.len() >= 6 {
                    let old = self.uploads.remove(0);
                    for t in old.planes {
                        self.plane_pool
                            .entry((t.width, t.height, t.format))
                            .or_default()
                            .push(t);
                    }
                }
                self.uploads.push(UploadedPlanes { key, planes });
                self.uploads.len() - 1
            }
        };
        let (kind, maxv, cw, ch, alpha_plane, depth) = match frame.layout {
            PixelLayout::Yuv8 {
                chroma_w,
                chroma_h,
                alpha,
            } => (0.0, 255.0, chroma_w, chroma_h, alpha, 8.0),
            PixelLayout::Yuv16 {
                chroma_w,
                chroma_h,
                alpha,
                bits,
            } => (
                0.0,
                ((1u32 << bits) - 1) as f32,
                chroma_w,
                chroma_h,
                alpha,
                bits as f32,
            ),
            PixelLayout::Nv12 { wide } => {
                if wide {
                    (2.0, 65535.0, 1, 1, false, 16.0)
                } else {
                    (2.0, 255.0, 1, 1, false, 8.0)
                }
            }
            PixelLayout::Rgba8 => (1.0, 255.0, 0, 0, false, 8.0),
            PixelLayout::Rgba16 => (1.0, 65535.0, 0, 0, false, 16.0),
        };
        let (kr, kb) = match frame.color.matrix {
            ColorMatrix::Bt601 => (0.299, 0.114),
            ColorMatrix::Bt2020 => (0.2627, 0.0593),
            _ => (0.2126, 0.0722),
        };
        let alpha_mode = match a.alpha() {
            AlphaMode::None => 0.0,
            AlphaMode::Straight => 1.0,
            AlphaMode::Premultiplied => 2.0,
        };
        let alpha_mode =
            if alpha_plane || matches!(frame.layout, PixelLayout::Rgba8 | PixelLayout::Rgba16) {
                alpha_mode
            } else {
                0.0
            };
        let params = P::new()
            .f(kind)
            .f(maxv)
            .f(cw as f32)
            .f(ch as f32)
            .f(kr)
            .f(kb)
            .b(frame.color.range == ColorRange::Full)
            .f(alpha_mode)
            .b(alpha_plane && alpha_mode > 0.0)
            .f(rotation.rem_euclid(360) as f32)
            .b(a.interpretation.invert_alpha)
            .f(depth);
        let out = self.work(w, h);
        let planes: Vec<wgpu::TextureView> = self.uploads[idx]
            .planes
            .iter()
            .map(|t| t.view.clone())
            .collect();
        let first = planes[0].clone();
        let v: Vec<&wgpu::TextureView> = (0..4).map(|i| planes.get(i).unwrap_or(&first)).collect();
        self.pass(
            "fs_convert",
            &v,
            params,
            [frame.width as f32, frame.height as f32],
            &out,
        );
        out
    }

    fn upload(&mut self, frame: &VideoFrame) -> Vec<Tex> {
        let mut out = Vec::new();
        for (i, p) in frame.planes.iter().enumerate() {
            let (format, bpp) = match frame.layout {
                PixelLayout::Yuv8 { .. } => (wgpu::TextureFormat::R8Uint, 1),
                PixelLayout::Yuv16 { .. } => (wgpu::TextureFormat::R16Uint, 2),
                PixelLayout::Nv12 { wide: false } if i == 1 => (wgpu::TextureFormat::Rg8Uint, 2),
                PixelLayout::Nv12 { wide: false } => (wgpu::TextureFormat::R8Uint, 1),
                PixelLayout::Nv12 { wide: true } if i == 1 => (wgpu::TextureFormat::Rg16Uint, 4),
                PixelLayout::Nv12 { wide: true } => (wgpu::TextureFormat::R16Uint, 2),
                PixelLayout::Rgba8 => (wgpu::TextureFormat::Rgba8Uint, 4),
                PixelLayout::Rgba16 => (wgpu::TextureFormat::Rgba16Uint, 8),
            };
            let key = (p.width, p.height, format);
            let tex = self
                .plane_pool
                .get_mut(&key)
                .and_then(|v| v.pop())
                .unwrap_or_else(|| {
                    Tex::new(
                        &self.gpu.device,
                        p.width,
                        p.height,
                        format,
                        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        "plane",
                    )
                });
            let row = p.width * bpp;
            self.gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &p.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(p.bytes_per_row.max(row)),
                    rows_per_image: Some(p.height),
                },
                wgpu::Extent3d {
                    width: p.width,
                    height: p.height,
                    depth_or_array_layers: 1,
                },
            );
            out.push(tex);
        }
        out
    }

    // ------------------------------------------------------------------------------ graphics

    fn text_image(&mut self, style: &TextStyle) -> (Tex, [f32; 2]) {
        let key = hash_of(&format!("{style:?}"));
        let fonts = self.fonts.clone();
        self.cached_image(key, || crate::text::rasterize(style, &fonts))
    }

    /// A rasterized text image as a texture, from the cache when the same `key` was drawn
    /// recently.
    fn cached_image(&mut self, key: u64, make: impl FnOnce() -> TextImage) -> (Tex, [f32; 2]) {
        if let Some((t, anchor, used)) = self.text_cache.get_mut(&key) {
            *used = self.frame_counter;
            return (t.clone(), *anchor);
        }
        let img: TextImage = make();
        let tex = Tex::new(
            &self.gpu.device,
            img.width,
            img.height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            "text",
        );
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
        );
        if self.text_cache.len() > 256 {
            // drop entries not used for a while
            let now = self.frame_counter;
            self.text_cache
                .retain(|_, (_, _, used)| now.wrapping_sub(*used) < 120);
        }
        self.text_cache
            .insert(key, (tex.clone(), img.anchor, self.frame_counter));
        (tex, img.anchor)
    }

    /// Draws a Caption component: its words animated for `clip_time` seconds into the clip.
    pub(crate) fn draw_caption(
        &mut self,
        fx: &EvalComponent,
        canvas: Tex,
        scale: f32,
        clip_time: f64,
        duration: f64,
    ) -> Tex {
        let text = fx.text("text");
        if text.trim().is_empty() {
            return canvas;
        }
        let words = op_core::captions::words(&text, &fx.text("timing"), duration);
        let size_scale = (fx.f64("scale") / 100.0) as f32 * scale;
        let px = (fx.f32("font_size") * size_scale).clamp(1.0, 2000.0);
        let mut background = fx.color("background_color");
        background.a *= fx.f32("background_opacity") / 100.0;
        let style = crate::captions::CaptionStyle {
            style: fx.choice("style"),
            strength: fx.f32("animation") / 100.0,
            family: fx.text("font"),
            font_style: fx.choice("font_style"),
            size: px,
            uppercase: fx.bool("uppercase"),
            max_width: canvas.width as f32 * fx.f32("max_width") / 100.0,
            fill: fx.color("fill"),
            highlight: fx.color("highlight"),
            stroke: fx.bool("stroke").then(|| {
                (
                    fx.color("stroke_color"),
                    fx.f32("stroke_width") * size_scale,
                )
            }),
            background,
        };
        // still styles look the same for the whole caption; animated ones change every frame
        let moment = if style.animated() {
            (clip_time * 120.0).round() as i64
        } else {
            0
        };
        let key = hash_of(&format!("caption {style:?} {words:?} {moment}"));
        let fonts = self.fonts.clone();
        let (tex, anchor) = self.cached_image(key, || {
            crate::captions::rasterize(&style, &words, clip_time, &fonts)
        });
        self.place_text(fx, canvas, &tex, anchor, 1.0, scale)
    }

    /// Draws a Text component onto a sequence-sized canvas.
    pub(crate) fn draw_text(
        &mut self,
        fx: &EvalComponent,
        canvas: Tex,
        scale: f32,
        extra: Option<String>,
    ) -> Tex {
        let text = extra.unwrap_or_else(|| fx.text("text"));
        if text.trim().is_empty() {
            return canvas;
        }
        let size_scale = (fx.f64("scale") / 100.0) as f32 * scale;
        let px = (fx.f32("font_size") * size_scale).clamp(1.0, 2000.0);
        // rasterize at the displayed size (quantized so small changes reuse the raster)
        let raster = (px * 4.0).round() / 4.0;
        let style = TextStyle {
            text,
            family: fx.text("font"),
            style: fx.choice("font_style"),
            size: raster,
            align: fx.choice("align"),
            tracking: fx.f32("tracking"),
            leading: fx.f32("leading") * size_scale,
            fill: fx.color("fill"),
            stroke: fx.bool("stroke").then(|| {
                (
                    fx.color("stroke_color"),
                    fx.f32("stroke_width") * size_scale,
                )
            }),
            background: fx.bool("background").then(|| {
                let mut c = fx.color("background_color");
                c.a *= fx.f32("background_opacity") / 100.0;
                (c, fx.f32("background_size") * size_scale)
            }),
        };
        let (tex, anchor) = self.text_image(&style);
        self.place_text(fx, canvas, &tex, anchor, px / raster, scale)
    }

    /// Draws a rasterized text image onto the canvas at the component's position, rotation and
    /// opacity, with its shadow. `k` is the displayed size over the raster size.
    pub(crate) fn place_text(
        &mut self,
        fx: &EvalComponent,
        canvas: Tex,
        tex: &Tex,
        anchor: [f32; 2],
        k: f32,
        scale: f32,
    ) -> Tex {
        let pos = fx.point("position");
        let (cw, ch) = (canvas.width as f64, canvas.height as f64);
        let rot = fx.f64("rotation").to_radians();
        let (c, s) = (rot.cos(), rot.sin());
        let (px_, py_) = (pos[0] * cw, pos[1] * ch);
        let sx = k as f64;
        let m = [
            (c / sx) as f32,
            (s / sx) as f32,
            (anchor[0] as f64 - (c * px_ + s * py_) / sx) as f32,
            (-s / sx) as f32,
            (c / sx) as f32,
            (anchor[1] as f64 - (-s * px_ + c * py_) / sx) as f32,
        ];
        let opacity = (fx.f64("opacity") / 100.0) as f32;
        let mut canvas = canvas;
        if fx.bool("shadow") {
            // shadow: the text's alpha, moved, colored and blurred
            let placed = self.work(canvas.width, canvas.height);
            let t = self.transparent.view.clone();
            self.pass(
                "fs_place",
                &[&t, &tex.view],
                P::new().all(&m).f(opacity),
                tex.size(),
                &placed,
            );
            let ang = fx.f64("shadow_angle").to_radians();
            let dist = fx.f64("shadow_distance") * scale as f64;
            let off = [ang.sin() * dist, -ang.cos() * dist];
            let mut sc = fx.color("shadow_color");
            sc.a = 1.0;
            let shadow = self.work(canvas.width, canvas.height);
            self.pass(
                "fs_shadow",
                &[&placed.view],
                P::new().v2(off).rgb(sc).f(fx.f32("shadow_opacity") / 100.0),
                placed.size(),
                &shadow,
            );
            let blurred = self.blur(
                shadow,
                fx.f32("shadow_blur") * scale / 3.0,
                false,
                true,
                true,
            );
            let with_shadow = self.work(canvas.width, canvas.height);
            self.pass(
                "fs_over",
                &[&placed.view, &blurred.view],
                P::new().b(false),
                placed.size(),
                &with_shadow,
            );
            self.pool.put(placed);
            self.pool.put(blurred);
            let out = self.work(canvas.width, canvas.height);
            self.pass(
                "fs_composite",
                &[&canvas.view, &with_shadow.view],
                P::new().all(&IDENTITY).f(1.0).f(0.0).b(false).f(0.0),
                with_shadow.size(),
                &out,
            );
            self.pool.put(canvas);
            self.pool.put(with_shadow);
            canvas = out;
        } else {
            let out = self.work(canvas.width, canvas.height);
            self.pass(
                "fs_composite",
                &[&canvas.view, &tex.view],
                P::new().all(&m).f(opacity).f(0.0).b(false).f(0.0),
                tex.size(),
                &out,
            );
            self.pool.put(canvas);
            canvas = out;
        }
        canvas
    }

    pub(crate) fn draw_shape(&mut self, fx: &EvalComponent, canvas: Tex, scale: f32) -> Tex {
        let s = (fx.f64("scale") / 100.0) as f32 * scale;
        let pos = fx.point("position");
        let out = self.work(canvas.width, canvas.height);
        let params = P::new()
            .f(fx.choice("shape") as f32)
            .f(pos[0] as f32 * canvas.width as f32)
            .f(pos[1] as f32 * canvas.height as f32)
            .f(fx.f32("width") * s / 2.0)
            .f(fx.f32("height") * s / 2.0)
            .f(fx.f32("rotation"))
            .f(fx.f32("corner_radius") * s)
            .rgba(fx.color("fill"))
            .b(fx.bool("stroke"))
            .rgba(fx.color("stroke_color"))
            .f(fx.f32("stroke_width") * s)
            .f(fx.f32("feather") * s)
            .f(fx.f32("opacity") / 100.0);
        self.pass("fs_shape", &[&canvas.view], params, canvas.size(), &out);
        self.pool.put(canvas);
        out
    }

    // ---------------------------------------------------------------------------------- blur

    /// Two-pass Gaussian blur (sigma in pixels).
    pub(crate) fn blur(
        &mut self,
        img: Tex,
        sigma: f32,
        repeat_edge: bool,
        horizontal: bool,
        vertical: bool,
    ) -> Tex {
        if sigma < 0.05 {
            return img;
        }
        let mut cur = img;
        for (dir, on) in [([1.0, 0.0], horizontal), ([0.0, 1.0], vertical)] {
            if !on {
                continue;
            }
            let out = self.work(cur.width, cur.height);
            self.pass(
                "fs_blur",
                &[&cur.view],
                P::new().all(&dir).f(sigma).b(repeat_edge),
                cur.size(),
                &out,
            );
            self.pool.put(cur);
            cur = out;
        }
        cur
    }

    pub(crate) fn lut(&mut self, path: &str) -> Option<wgpu::TextureView> {
        if path.is_empty() {
            return None;
        }
        if let Some(v) = self.lut_cache.get(path) {
            return v.clone();
        }
        let v = match crate::lut::load_cube(std::path::Path::new(path)) {
            Ok(cube) => Some(crate::lut::upload_cube(
                &self.gpu.device,
                &self.gpu.queue,
                &cube,
            )),
            Err(e) => {
                log::warn!("LUT {path}: {e}");
                None
            }
        };
        self.lut_cache.insert(path.to_string(), v.clone());
        v
    }

    pub(crate) fn identity_lut(&self) -> wgpu::TextureView {
        self.identity_lut.clone()
    }

    /// 256x1 texture with the master and R, G, B curves in its channels.
    pub(crate) fn curves(
        &mut self,
        master: &[[f32; 2]],
        r: &[[f32; 2]],
        g: &[[f32; 2]],
        b: &[[f32; 2]],
    ) -> Tex {
        let key = hash_of(&format!("{master:?}{r:?}{g:?}{b:?}"));
        if let Some(t) = self.curve_cache.get(&key) {
            return t.clone();
        }
        let tables = [
            crate::lut::curve_table(master),
            crate::lut::curve_table(r),
            crate::lut::curve_table(g),
            crate::lut::curve_table(b),
        ];
        let mut bytes = Vec::with_capacity(256 * 8);
        for i in 0..256 {
            for t in &tables {
                bytes.extend_from_slice(&crate::lut::f16_bits(t[i]).to_le_bytes());
            }
        }
        let tex = Tex::new(
            &self.gpu.device,
            256,
            1,
            WORK,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            "curves",
        );
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        if self.curve_cache.len() > 64 {
            self.curve_cache.clear();
        }
        self.curve_cache.insert(key, tex.clone());
        tex
    }

    pub(crate) fn frame_seed(&self) -> f32 {
        (self.frame_counter % 4096) as f32
    }

    pub(crate) fn put(&mut self, t: Tex) {
        self.pool.put(t);
    }

    pub fn entry_names(&self) -> Vec<&'static str> {
        self.pipes.entry_names()
    }

    /// Creates every pipeline now (startup warm-up and shader validation in tests).
    pub fn warm_up(&mut self) {
        let device = self.gpu.device.clone();
        for e in self.pipes.entry_names() {
            self.pipes.get(&device, e);
        }
    }

    pub(crate) fn pipes(&mut self) -> &mut Pipelines {
        &mut self.pipes
    }
}

pub const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
