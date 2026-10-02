//! GPU compositor (docs/architecture.md 4).
//!
//! Renders `op_core::plan` frame plans with wgpu: decoded planes are converted to a
//! premultiplied RGBA16F working space on the GPU, effects run in layer space, Motion and
//! Opacity composite layers with blend modes, and transitions mix two composited sides. The
//! monitor path never reads frames back to the CPU; export reads back delivery planes that are
//! already converted on the GPU.

pub mod captions;
mod effects;
pub mod gpu;
pub mod lut;
pub mod output;
pub mod params;
pub mod pipelines;
pub mod pool;
pub mod render;
pub mod text;
mod transitions;

pub use gpu::{Gpu, GpuError};
pub use output::ScopeKind;
pub use pool::Tex;
pub use render::{FrameSource, NoFrames, Renderer, Request, Stats};
