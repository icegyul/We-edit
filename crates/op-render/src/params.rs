//! Per-pass uniform data (layout of `struct U` in common.wgsl) and its upload arena.

use bytemuck::{Pod, Zeroable};

pub const SLOT: u64 = 256;
const PARAMS: usize = 56;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    pub out_size: [f32; 2],
    pub in_size: [f32; 2],
    pub scale: f32,
    pub time: f32,
    pub progress: f32,
    pub flags: u32,
    pub p: [f32; PARAMS],
}

const _: () = assert!(std::mem::size_of::<Uniforms>() == SLOT as usize);

/// Sequential parameter packer: values land in `p` in call order, matching `prm(i)` reads.
#[derive(Clone, Copy)]
pub struct P {
    pub u: Uniforms,
    n: usize,
}

impl Default for P {
    fn default() -> Self {
        P::new()
    }
}

impl P {
    pub fn new() -> P {
        P {
            u: Uniforms::zeroed(),
            n: 0,
        }
    }

    pub fn f(mut self, v: f32) -> P {
        if self.n < PARAMS {
            self.u.p[self.n] = v;
        }
        self.n += 1;
        self
    }

    pub fn d(self, v: f64) -> P {
        self.f(v as f32)
    }

    pub fn b(self, v: bool) -> P {
        self.f(if v { 1.0 } else { 0.0 })
    }

    pub fn v2(self, v: [f64; 2]) -> P {
        self.d(v[0]).d(v[1])
    }

    pub fn rgb(self, c: op_core::Rgba) -> P {
        self.f(c.r).f(c.g).f(c.b)
    }

    pub fn rgba(self, c: op_core::Rgba) -> P {
        self.f(c.r).f(c.g).f(c.b).f(c.a)
    }

    pub fn all(mut self, v: &[f32]) -> P {
        for x in v {
            self = self.f(*x);
        }
        self
    }

    /// Skips to parameter index `i` (for layouts with fixed offsets).
    pub fn at(mut self, i: usize) -> P {
        self.n = i;
        self
    }

    pub fn progress(mut self, v: f32) -> P {
        self.u.progress = v;
        self
    }

    pub fn time(mut self, v: f32) -> P {
        self.u.time = v;
        self
    }

    pub fn flags(mut self, v: u32) -> P {
        self.u.flags = v;
        self
    }
}

/// 256-byte uniform slots used with dynamic offsets, in fixed-size chunks so a frame can use
/// any number of passes. Reset every frame; slots are uploaded once before submission.
pub struct Arena {
    chunks: Vec<Chunk>,
}

struct Chunk {
    buffer: wgpu::Buffer,
    staged: Vec<u8>,
    used: u64,
}

const CHUNK_SLOTS: u64 = 1024;

impl Arena {
    pub fn new() -> Arena {
        Arena { chunks: Vec::new() }
    }

    /// Reserves a slot and returns (chunk, byte offset).
    pub fn push(&mut self, device: &wgpu::Device, u: &Uniforms) -> (usize, u32) {
        let need_new = self.chunks.last().is_none_or(|c| c.used >= CHUNK_SLOTS);
        if need_new {
            let reuse = self.chunks.iter().position(|c| c.used == 0);
            match reuse {
                Some(i) => {
                    // move the empty chunk to the end so it is the current one
                    let c = self.chunks.remove(i);
                    self.chunks.push(c);
                }
                None => self.chunks.push(Chunk {
                    buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("uniform arena"),
                        size: CHUNK_SLOTS * SLOT,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    staged: vec![0; (CHUNK_SLOTS * SLOT) as usize],
                    used: 0,
                }),
            }
        }
        let idx = self.chunks.len() - 1;
        let c = &mut self.chunks[idx];
        let off = c.used * SLOT;
        c.staged[off as usize..(off + SLOT) as usize].copy_from_slice(bytemuck::bytes_of(u));
        c.used += 1;
        (idx, off as u32)
    }

    pub fn buffer(&self, chunk: usize) -> &wgpu::Buffer {
        &self.chunks[chunk].buffer
    }

    /// Uploads every slot written since the last reset.
    pub fn flush(&mut self, queue: &wgpu::Queue) {
        for c in &self.chunks {
            if c.used > 0 {
                queue.write_buffer(&c.buffer, 0, &c.staged[..(c.used * SLOT) as usize]);
            }
        }
    }

    /// Starts a new frame. Chunks are kept for reuse; the list shrinks when mostly idle.
    pub fn reset(&mut self) {
        let used = self.chunks.iter().filter(|c| c.used > 0).count();
        for c in &mut self.chunks {
            c.used = 0;
        }
        if self.chunks.len() > used.max(1) * 2 {
            self.chunks.truncate(used.max(1));
        }
    }
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}
