//! Media I/O through FFmpeg (docs/architecture.md 5).
//!
//! This crate is the only place that talks to FFmpeg. It translates files into project-neutral
//! descriptors (`op_core::MediaAsset`) and plain frame/sample buffers, and owns every FFmpeg
//! object; nothing FFmpeg-specific crosses its public API.

pub mod audio;
pub mod encode;
pub mod probe;
pub mod thumb;
pub mod video;

use std::sync::Once;

use thiserror::Error;

pub use audio::{AudioInfo, ConformedAudio, Peaks, conform};
pub use encode::{AudioCodec, AudioSettings, Muxer, VideoCodec, VideoInput, VideoSettings};
pub use probe::probe;
pub use video::{PixelLayout, Plane, VideoDecoder, VideoFrame};

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("{0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Unsupported(String),
    /// A step of encoding or writing failed; the text says which.
    #[error("{0}")]
    Failed(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T = ()> = std::result::Result<T, MediaError>;

static INIT: Once = Once::new();

/// Initializes FFmpeg once and keeps its logging quiet.
pub fn init() {
    INIT.call_once(|| {
        let _ = ffmpeg_next::init();
        ffmpeg_next::util::log::set_level(ffmpeg_next::util::log::Level::Error);
    });
}

/// FFmpeg version string for diagnostics.
pub fn ffmpeg_version() -> String {
    init();
    let v = ffmpeg_next::format::version();
    format!("libavformat {}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff)
}

/// Whether an encoder is available in the linked FFmpeg.
pub fn has_encoder(name: &str) -> bool {
    init();
    ffmpeg_next::encoder::find_by_name(name).is_some()
}
