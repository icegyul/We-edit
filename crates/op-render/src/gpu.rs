//! GPU device selection.

use std::sync::Arc;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("no usable graphics adapter: {0}")]
    Adapter(String),
    #[error("the graphics device could not be created: {0}")]
    Device(String),
}

/// A device and queue shared by the renderer and, in the application, the window.
#[derive(Clone)]
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Gpu({} / {:?})", self.info.name, self.info.backend)
    }
}

/// Backends in preference order: Vulkan / Direct3D 12 / Metal, never the OpenGL backend for the
/// renderer (its texture format support is too limited for the pipeline).
pub fn backends() -> wgpu::Backends {
    // WGPU_BACKEND (vulkan, dx12, metal, gl) picks one for diagnostics and tests
    wgpu::Backends::from_env()
        .unwrap_or(wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::METAL)
}

/// Features the renderer uses when the adapter offers them.
pub fn optional_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wgpu::Features::FLOAT32_FILTERABLE
}

/// Limits requested: the adapter's own, so 8K frames and large LUTs fit.
pub fn limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    wgpu::Limits::default().using_resolution(adapter.limits())
}

impl Gpu {
    /// Creates a device without a window (export, tests, command-line rendering). Falls back to
    /// a software adapter when no hardware adapter is available.
    pub fn headless() -> Result<Arc<Gpu>, GpuError> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends();
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .or_else(|_| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: true,
                compatible_surface: None,
                ..Default::default()
            }))
        })
        .map_err(|e| GpuError::Adapter(e.to_string()))?;
        Self::from_adapter(&adapter)
    }

    pub fn from_adapter(adapter: &wgpu::Adapter) -> Result<Arc<Gpu>, GpuError> {
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("OpenPremier renderer"),
            required_features: optional_features(adapter),
            required_limits: limits(adapter),
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;
        Ok(Arc::new(Gpu {
            device,
            queue,
            info: adapter.get_info(),
        }))
    }

    /// Wraps a device created elsewhere (the window's).
    pub fn shared(device: wgpu::Device, queue: wgpu::Queue, info: wgpu::AdapterInfo) -> Arc<Gpu> {
        Arc::new(Gpu {
            device,
            queue,
            info,
        })
    }

    /// Human description for diagnostics: "NVIDIA GeForce RTX 3090 Ti (Vulkan)".
    pub fn description(&self) -> String {
        format!("{} ({:?})", self.info.name, self.info.backend)
    }

    pub fn is_software(&self) -> bool {
        self.info.device_type == wgpu::DeviceType::Cpu
    }
}
