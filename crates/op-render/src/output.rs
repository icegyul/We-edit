//! Frame output: presentation for the monitors, delivery planes for export, and scopes.

use op_media::VideoInput;

use crate::params::P;
use crate::pipelines::DISPLAY;
use crate::pool::Tex;
use crate::render::Renderer;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ScopeKind {
    Waveform,
    Parade,
    Vectorscope,
    Histogram,
}

impl ScopeKind {
    pub const ALL: [ScopeKind; 4] = [
        ScopeKind::Waveform,
        ScopeKind::Parade,
        ScopeKind::Vectorscope,
        ScopeKind::Histogram,
    ];

    fn index(self) -> u32 {
        match self {
            ScopeKind::Waveform => 0,
            ScopeKind::Parade => 1,
            ScopeKind::Vectorscope => 2,
            ScopeKind::Histogram => 3,
        }
    }

    fn grid(self) -> (u32, u32) {
        match self {
            ScopeKind::Waveform => (384, 256),
            ScopeKind::Parade => (384, 256),
            ScopeKind::Vectorscope => (256, 256),
            ScopeKind::Histogram => (256, 4),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScopeUniform {
    size: [u32; 2],
    grid: [u32; 2],
    kind: u32,
    gain: f32,
    pad: [u32; 2],
}

pub(crate) struct ScopeResources {
    uniform: wgpu::Buffer,
    counts: wgpu::Buffer,
}

impl Renderer {
    /// Converts a finished frame into a display texture (Rgba8Unorm) for a monitor.
    pub fn present_into(&mut self, frame: &Tex, target: &Tex, show_alpha: bool) {
        self.pass(
            "fs_present",
            &[&frame.view],
            P::new().b(show_alpha),
            frame.size(),
            target,
        );
    }

    /// A display texture of the given size.
    pub fn display_target(&self, w: u32, h: u32) -> Tex {
        Tex::new(
            self.device(),
            w,
            h,
            DISPLAY,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            "display",
        )
    }

    /// Reads a texture back as tightly packed rows (blocking).
    pub fn read_texture(&mut self, t: &Tex, bytes_per_pixel: u32) -> Vec<u8> {
        self.read_textures(&[(t, bytes_per_pixel)])
            .pop()
            .unwrap_or_default()
    }

    /// Reads several textures back with one submission and one wait (blocking). Buffers are
    /// reused between calls.
    pub fn read_textures(&mut self, list: &[(&Tex, u32)]) -> Vec<Vec<u8>> {
        let mut enc = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        let mut jobs = Vec::with_capacity(list.len());
        for (t, bpp) in list {
            let unpadded = t.width * bpp;
            let padded = unpadded.div_ceil(256) * 256;
            let size = padded as u64 * t.height as u64;
            let buffer = self
                .readbacks
                .get_mut(&size)
                .and_then(|v| v.pop())
                .unwrap_or_else(|| {
                    self.device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("readback"),
                        size,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    })
                });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(t.height),
                    },
                },
                wgpu::Extent3d {
                    width: t.width,
                    height: t.height,
                    depth_or_array_layers: 1,
                },
            );
            jobs.push((buffer, size, unpadded, padded, t.height));
        }
        // pending passes must run first
        self.submit();
        self.gpu.queue.submit(Some(enc.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        for (i, (buffer, ..)) in jobs.iter().enumerate() {
            let tx = tx.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send((i, r.is_ok()));
            });
        }
        drop(tx);
        let _ = self.device().poll(wgpu::PollType::wait_indefinitely());
        let mut ok = vec![false; jobs.len()];
        for (i, r) in rx.iter() {
            ok[i] = r;
        }
        let mut out = Vec::with_capacity(jobs.len());
        for (i, (buffer, size, unpadded, padded, height)) in jobs.into_iter().enumerate() {
            let mut data = vec![0u8; (unpadded * height) as usize];
            if ok[i] {
                let slice = buffer.slice(..);
                if let Ok(mapped) = slice.get_mapped_range() {
                    if unpadded == padded {
                        let n = data.len();
                        data.copy_from_slice(&mapped[..n]);
                    } else {
                        for y in 0..height as usize {
                            data[y * unpadded as usize..(y + 1) * unpadded as usize]
                                .copy_from_slice(
                                    &mapped[y * padded as usize
                                        ..y * padded as usize + unpadded as usize],
                                );
                        }
                    }
                }
                buffer.unmap();
            }
            let pool = self.readbacks.entry(size).or_default();
            if pool.len() < 4 {
                pool.push(buffer);
            }
            out.push(data);
        }
        out
    }

    /// Converts a finished frame to the planes an encoder expects (tightly packed rows).
    pub fn delivery_planes(&mut self, frame: &Tex, input: VideoInput) -> Vec<Vec<u8>> {
        let (w, h) = (frame.width, frame.height);
        let (planes, cx, cy, bytes) = input.planes();
        let bits = input.bits() as f32;
        let straight = matches!(input, VideoInput::Yuva444p10);
        if !input.is_yuv() {
            let t = self.target(w, h, DISPLAY);
            self.pass(
                "fs_rgba8",
                &[&frame.view],
                P::new().b(true),
                frame.size(),
                &t,
            );
            let data = self.read_texture(&t, 4);
            self.put(t);
            return vec![data];
        }
        let (cw, ch) = (w.div_ceil(1 << cx), h.div_ceil(1 << cy));
        let wide = bytes == 2;
        let mut out = Vec::new();
        // luma
        let (luma_entry, luma_fmt, luma_bpp) = if wide {
            ("fs_luma16", wgpu::TextureFormat::R16Uint, 2)
        } else {
            ("fs_luma8", wgpu::TextureFormat::R8Unorm, 1)
        };
        let y = self.target(w, h, luma_fmt);
        self.pass(
            luma_entry,
            &[&frame.view],
            P::new().b(straight).f(bits),
            frame.size(),
            &y,
        );

        // chroma, interleaved on the GPU and split here
        let (chroma_entry, chroma_fmt, chroma_bpp) = if wide {
            ("fs_chroma16", wgpu::TextureFormat::Rg16Uint, 4)
        } else {
            ("fs_chroma8", wgpu::TextureFormat::Rg8Unorm, 2)
        };
        let c = self.target(cw, ch, chroma_fmt);
        self.pass(
            chroma_entry,
            &[&frame.view],
            P::new().b(straight).f(bits),
            frame.size(),
            &c,
        );
        // the alpha plane (if any) is drawn too, then everything is read back at once
        let a = (planes == 4).then(|| {
            let a = self.target(w, h, wgpu::TextureFormat::R16Uint);
            self.pass(
                "fs_alpha16",
                &[&frame.view],
                P::new().b(true).f(bits),
                frame.size(),
                &a,
            );
            a
        });
        let mut list: Vec<(&Tex, u32)> = vec![(&y, luma_bpp), (&c, chroma_bpp)];
        if let Some(a) = &a {
            list.push((a, 2));
        }
        let mut data = self.read_textures(&list);
        let alpha = (planes == 4).then(|| data.pop().unwrap_or_default());
        let inter = data.pop().unwrap_or_default();
        out.push(data.pop().unwrap_or_default());
        self.put(y);
        self.put(c);
        if let Some(a) = a {
            self.put(a);
        }
        let s = bytes;
        let mut u = Vec::with_capacity(inter.len() / 2);
        let mut v = Vec::with_capacity(inter.len() / 2);
        for px in inter.chunks_exact(2 * s) {
            u.extend_from_slice(&px[..s]);
            v.extend_from_slice(&px[s..]);
        }
        out.push(u);
        out.push(v);
        if let Some(alpha) = alpha {
            out.push(alpha);
        }
        out
    }

    /// Straight RGBA8 of a frame (thumbnails, still export, tests).
    pub fn rgba8(&mut self, frame: &Tex) -> Vec<u8> {
        let t = self.target(frame.width, frame.height, DISPLAY);
        self.pass("fs_straight", &[&frame.view], P::new(), frame.size(), &t);
        let data = self.read_texture(&t, 4);
        self.put(t);
        data
    }

    /// Draws a scope of `frame` into `target` (a display texture).
    pub fn scope(&mut self, frame: &Tex, kind: ScopeKind, target: &Tex, gain: f32) {
        // analyze a reduced copy: scopes need distribution, not every pixel
        let k = (480.0 / frame.width as f32)
            .min(270.0 / frame.height as f32)
            .min(1.0);
        let (aw, ah) = (
            ((frame.width as f32 * k) as u32).max(1),
            ((frame.height as f32 * k) as u32).max(1),
        );
        let small = self.work(aw, ah);
        self.pass("fs_copy", &[&frame.view], P::new(), frame.size(), &small);
        let (gw, gh) = kind.grid();
        let res = self.scope_resources(kind, gw * gh);
        let u = ScopeUniform {
            size: [aw, ah],
            grid: [gw, gh],
            kind: kind.index(),
            gain,
            pad: [0; 2],
        };
        self.gpu
            .queue
            .write_buffer(&res.uniform, 0, bytemuck::bytes_of(&u));
        let device = self.gpu.device.clone();
        let compute_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scope compute"),
            layout: &self.pipes().scope_compute_layout.clone(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: res.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&small.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: res.counts.as_entire_binding(),
                },
            ],
        });
        let draw_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scope draw"),
            layout: &self.pipes().scope_draw_layout.clone(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: res.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: res.counts.as_entire_binding(),
                },
            ],
        });
        let compute = self.pipes().scope_compute.clone();
        let draw = self.pipes().scope_draw.clone();
        let enc = self.encoder_mut();
        enc.clear_buffer(&res.counts, 0, None);
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("scope"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&compute);
            cp.set_bind_group(0, &compute_bg, &[]);
            cp.dispatch_workgroups(aw.div_ceil(16), ah.div_ceil(16), 1);
        }
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scope draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&draw);
            rp.set_bind_group(0, &draw_bg, &[]);
            rp.draw(0..3, 0..1);
        }
        self.put(small);
    }
}

impl Renderer {
    pub(crate) fn scope_resources(
        &mut self,
        kind: ScopeKind,
        cells: u32,
    ) -> std::sync::Arc<ScopeResources> {
        let key = kind.index();
        if let Some(r) = self.scopes.get(&key) {
            return r.clone();
        }
        let device = &self.gpu.device;
        let r = std::sync::Arc::new(ScopeResources {
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scope uniform"),
                size: std::mem::size_of::<ScopeUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            counts: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scope counts"),
                size: cells as u64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        });
        self.scopes.insert(key, r.clone());
        r
    }
}
