//! Media identity and stream descriptors (DM-006, DM-007).
//!
//! A `MediaAsset` is identified by its ID, not by its path: relinking changes the location only.
//! Every property that affects rendering or mixing (color, alpha, pixel aspect, field order,
//! channel layout) is recorded explicitly when the file is probed and never re-guessed later.

use serde::{Deserialize, Serialize};

use crate::ids::AssetId;
use crate::time::{Dur, Rate, SrcRange, SrcTime};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum MediaKind {
    /// Moving pictures, possibly with audio.
    Video,
    /// Audio only.
    Audio,
    /// A single picture, usable for any duration.
    Still,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ColorMatrix {
    Bt601,
    #[default]
    Bt709,
    Bt2020,
    /// Already RGB, no matrix.
    Rgb,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ColorRange {
    /// 16-235 (8-bit) video levels.
    #[default]
    Limited,
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Transfer {
    #[default]
    Bt709,
    Srgb,
    Linear,
    Pq,
    Hlg,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Primaries {
    Bt601,
    #[default]
    Bt709,
    Bt2020,
    P3,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ColorInfo {
    pub matrix: ColorMatrix,
    pub range: ColorRange,
    pub transfer: Transfer,
    pub primaries: Primaries,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum FieldOrder {
    #[default]
    Progressive,
    UpperFirst,
    LowerFirst,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum AlphaMode {
    /// The file has no alpha channel.
    #[default]
    None,
    Straight,
    Premultiplied,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct VideoStream {
    /// Stream index inside the container.
    pub index: usize,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    /// Pixel aspect ratio as num/den.
    pub pixel_aspect: (u32, u32),
    pub rate: Rate,
    /// Number of frames (1 for stills).
    pub frames: i64,
    pub pixel_format: String,
    pub bit_depth: u8,
    pub alpha: AlphaMode,
    pub color: ColorInfo,
    pub field_order: FieldOrder,
    /// Presentation time of the first frame relative to the container start.
    pub start: Dur,
    /// Clockwise display rotation in degrees from container metadata (0, 90, 180, 270).
    pub rotation: i32,
    /// Source timecode of the first frame, if the file carries one.
    pub timecode: Option<String>,
}

impl VideoStream {
    /// Width and height after applying the display rotation.
    pub fn display_size(&self) -> (u32, u32) {
        if self.rotation.rem_euclid(180) == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    pub fn duration(&self) -> Dur {
        self.rate.frames_to_dur(self.frames)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    /// L R C LFE Ls Rs
    Surround51,
    /// Any other count; channels are kept discrete (DM-AUD-001).
    Discrete(u16),
}

impl ChannelLayout {
    pub fn from_count(n: u16) -> ChannelLayout {
        match n {
            1 => ChannelLayout::Mono,
            2 => ChannelLayout::Stereo,
            6 => ChannelLayout::Surround51,
            n => ChannelLayout::Discrete(n),
        }
    }

    pub fn channels(self) -> u16 {
        match self {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround51 => 6,
            ChannelLayout::Discrete(n) => n,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ChannelLayout::Mono => "audio.mono",
            ChannelLayout::Stereo => "audio.stereo",
            ChannelLayout::Surround51 => "audio.51",
            ChannelLayout::Discrete(_) => "audio.discrete",
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AudioStream {
    pub index: usize,
    pub codec: String,
    pub sample_rate: u32,
    pub layout: ChannelLayout,
    /// Length in samples at `sample_rate`.
    pub samples: i64,
    pub start: Dur,
}

impl AudioStream {
    pub fn rate(&self) -> Rate {
        Rate::fps(self.sample_rate)
    }

    pub fn duration(&self) -> Dur {
        self.rate().frames_to_dur(self.samples)
    }
}

/// User overrides from "Interpret Footage". `None` keeps the probed value.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Interpretation {
    pub frame_rate: Option<Rate>,
    pub pixel_aspect: Option<(u32, u32)>,
    pub alpha: Option<AlphaMode>,
    pub ignore_alpha: bool,
    pub invert_alpha: bool,
    pub field_order: Option<FieldOrder>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: AssetId,
    /// Original location, spelled as the user or the source project gave it.
    pub path: String,
    /// Optional proxy file; proxies share the asset identity (architecture 5).
    #[serde(default)]
    pub proxy: Option<String>,
    pub kind: MediaKind,
    #[serde(default)]
    pub video: Option<VideoStream>,
    #[serde(default)]
    pub audio: Vec<AudioStream>,
    /// Container duration.
    pub duration: Dur,
    #[serde(default)]
    pub interpretation: Interpretation,
    /// File size and modification time when probed, used to notice replaced files.
    #[serde(default)]
    pub file_size: u64,
    #[serde(default)]
    pub modified_unix: i64,
}

impl MediaAsset {
    pub fn has_video(&self) -> bool {
        self.video.is_some()
    }

    pub fn has_audio(&self) -> bool {
        !self.audio.is_empty()
    }

    pub fn is_still(&self) -> bool {
        self.kind == MediaKind::Still
    }

    /// Effective frame rate after interpretation. Audio-only media reports 0/1.
    pub fn frame_rate(&self) -> Option<Rate> {
        self.interpretation
            .frame_rate
            .or(self.video.as_ref().map(|v| v.rate))
    }

    /// Source time range that has media. Stills are unbounded.
    pub fn available(&self) -> Option<SrcRange> {
        if self.is_still() {
            return None;
        }
        let dur = match (&self.video, self.frame_rate()) {
            (Some(v), Some(rate)) => rate.frames_to_dur(v.frames),
            _ => self
                .audio
                .first()
                .map(|a| a.duration())
                .unwrap_or(self.duration),
        };
        Some(SrcRange::with_duration(SrcTime::ZERO, dur))
    }

    pub fn file_name(&self) -> &str {
        let p = self.path.trim_end_matches(['/', '\\']);
        p.rsplit(['/', '\\']).next().unwrap_or(p)
    }

    pub fn alpha(&self) -> AlphaMode {
        if self.interpretation.ignore_alpha {
            return AlphaMode::None;
        }
        self.interpretation
            .alpha
            .or(self.video.as_ref().map(|v| v.alpha))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> MediaAsset {
        MediaAsset {
            id: AssetId(1),
            path: "C:\\media\\clip.mov".into(),
            proxy: None,
            kind: MediaKind::Video,
            video: Some(VideoStream {
                index: 0,
                codec: "h264".into(),
                width: 1920,
                height: 1080,
                pixel_aspect: (1, 1),
                rate: Rate::FPS_25,
                frames: 250,
                pixel_format: "yuv420p".into(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 90,
                timecode: None,
            }),
            audio: vec![],
            duration: Dur::from_seconds(10.0),
            interpretation: Interpretation::default(),
            file_size: 0,
            modified_unix: 0,
        }
    }

    #[test]
    fn availability_follows_interpretation() {
        let mut a = asset();
        assert_eq!(a.available().unwrap().duration(), Dur::from_seconds(10.0));
        a.interpretation.frame_rate = Some(Rate::fps(50));
        assert_eq!(a.available().unwrap().duration(), Dur::from_seconds(5.0));
        assert_eq!(a.file_name(), "clip.mov");
        assert_eq!(a.video.as_ref().unwrap().display_size(), (1080, 1920));
    }
}
