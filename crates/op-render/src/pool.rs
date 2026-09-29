//! Reusable textures. Intermediate images are returned to the pool as soon as a pass has
//! consumed them; command order on the queue makes reuse within one frame safe.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Tex {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

impl Tex {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        label: &str,
    ) -> Tex {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Tex {
            texture,
            view,
            width: width.max(1),
            height: height.max(1),
            format,
        }
    }

    pub fn size(&self) -> [f32; 2] {
        [self.width as f32, self.height as f32]
    }

    pub fn bytes(&self) -> u64 {
        let bpp = match self.format {
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rgba16Uint => 8,
            wgpu::TextureFormat::R8Unorm | wgpu::TextureFormat::R8Uint => 1,
            wgpu::TextureFormat::Rg8Unorm | wgpu::TextureFormat::R16Uint => 2,
            wgpu::TextureFormat::Rg16Uint => 4,
            _ => 4,
        };
        self.width as u64 * self.height as u64 * bpp
    }
}

pub const WORK_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

type Key = (u32, u32, wgpu::TextureFormat);

#[derive(Default)]
pub struct Pool {
    free: HashMap<Key, Vec<Tex>>,
    /// Frames since each size was last requested, for trimming.
    idle: HashMap<Key, u32>,
    pub allocated: u64,
}

impl Pool {
    pub fn get(
        &mut self,
        device: &wgpu::Device,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
    ) -> Tex {
        let key = (w.max(1), h.max(1), format);
        self.idle.insert(key, 0);
        if let Some(t) = self.free.get_mut(&key).and_then(|v| v.pop()) {
            return t;
        }
        let t = Tex::new(device, w, h, format, WORK_USAGE, "work");
        self.allocated += t.bytes();
        t
    }

    pub fn put(&mut self, t: Tex) {
        let key = (t.width, t.height, t.format);
        let list = self.free.entry(key).or_default();
        if list.len() < 12 {
            list.push(t);
        } else {
            self.allocated = self.allocated.saturating_sub(t.bytes());
        }
    }

    /// Drops free textures of sizes unused for a while (resolution or sequence changes).
    pub fn end_frame(&mut self) {
        let mut stale = Vec::new();
        for (k, age) in self.idle.iter_mut() {
            *age += 1;
            if *age > 90 {
                stale.push(*k);
            }
        }
        for k in stale {
            self.idle.remove(&k);
            if let Some(v) = self.free.remove(&k) {
                for t in v {
                    self.allocated = self.allocated.saturating_sub(t.bytes());
                }
            }
        }
    }
}
