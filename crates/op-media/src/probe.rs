//! Reads stream descriptors from a media file (DM-007: everything explicit at import).

use std::path::Path;

use ff::media::Type;
use ffmpeg_next as ff;
use op_core::*;

use crate::{MediaError, Result, init};

/// Raw codec parameters not exposed by the safe wrapper.
pub(crate) struct CodecParams {
    pub width: i32,
    pub height: i32,
    pub format: i32,
    pub color_range: u32,
    pub color_space: u32,
    pub color_primaries: u32,
    pub color_trc: u32,
    pub field_order: u32,
    pub sar: (i32, i32),
    pub sample_rate: i32,
    pub channels: i32,
    pub bits_raw: i32,
}

pub(crate) fn codec_params(stream: &ff::format::stream::Stream) -> CodecParams {
    let params = stream.parameters();
    // SAFETY: `params` owns a valid AVCodecParameters for the duration of these reads; only plain
    // integer fields are copied out.
    unsafe {
        let p = &*params.as_ptr();
        CodecParams {
            width: p.width,
            height: p.height,
            format: p.format,
            color_range: p.color_range as u32,
            color_space: p.color_space as u32,
            color_primaries: p.color_primaries as u32,
            color_trc: p.color_trc as u32,
            field_order: p.field_order as u32,
            sar: (p.sample_aspect_ratio.num, p.sample_aspect_ratio.den),
            sample_rate: p.sample_rate,
            channels: p.ch_layout.nb_channels,
            bits_raw: p.bits_per_raw_sample,
        }
    }
}

/// Clockwise display rotation from the stream's display matrix, if any.
fn rotation(stream: &ff::format::stream::Stream) -> i32 {
    for sd in stream.side_data() {
        if sd.kind() == ff::codec::packet::side_data::Type::DisplayMatrix {
            let d = sd.data();
            if d.len() >= 36 {
                let m = |i: usize| {
                    i32::from_le_bytes([d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]]) as f64
                        / 65536.0
                };
                let (a, b) = (m(0), m(1));
                if a == 0.0 && b == 0.0 {
                    return 0;
                }
                let ccw = b.atan2(a).to_degrees();
                let cw = (-ccw).rem_euclid(360.0);
                return ((cw / 90.0).round() as i32 * 90).rem_euclid(360);
            }
        }
    }
    stream
        .metadata()
        .get("rotate")
        .and_then(|r| r.parse::<i32>().ok())
        .map(|r| r.rem_euclid(360))
        .unwrap_or(0)
}

fn pixel_info(format: i32) -> (String, u8, bool) {
    // SAFETY: av_pix_fmt_desc_get accepts any value and returns null for unknown formats; the
    // descriptor it returns is static.
    unsafe {
        let desc = ff::ffi::av_pix_fmt_desc_get(
            std::mem::transmute::<i32, ff::ffi::AVPixelFormat>(format),
        );
        if desc.is_null() {
            return (String::new(), 8, false);
        }
        let d = &*desc;
        let name = std::ffi::CStr::from_ptr(d.name)
            .to_string_lossy()
            .into_owned();
        let depth = d.comp[0].depth as u8;
        let alpha = d.flags & (ff::ffi::AV_PIX_FMT_FLAG_ALPHA as u64) != 0;
        (name, depth.max(1), alpha)
    }
}

fn map_matrix(v: u32) -> ColorMatrix {
    // AVColorSpace: 1 BT709, 5 BT470BG, 6 SMPTE170M, 9 BT2020_NCL, 10 BT2020_CL, 0 RGB
    match v {
        0 => ColorMatrix::Rgb,
        5..=7 => ColorMatrix::Bt601,
        9 | 10 => ColorMatrix::Bt2020,
        _ => ColorMatrix::Bt709,
    }
}

fn map_transfer(v: u32) -> Transfer {
    // AVColorTransferCharacteristic: 1 BT709, 6 SMPTE170M, 8 LINEAR, 13 IEC61966_2_1 (sRGB), 14/15 BT2020, 16 PQ, 18 HLG
    match v {
        1 | 6 | 14 | 15 => Transfer::Bt709,
        8 => Transfer::Linear,
        13 => Transfer::Srgb,
        16 => Transfer::Pq,
        18 => Transfer::Hlg,
        2 | 0 => Transfer::Bt709,
        _ => Transfer::Unknown,
    }
}

fn map_primaries(v: u32) -> Primaries {
    // AVColorPrimaries: 1 BT709, 5 BT470BG, 6 SMPTE170M, 9 BT2020, 11/12 P3
    match v {
        0..=2 => Primaries::Bt709,
        5..=7 => Primaries::Bt601,
        9 => Primaries::Bt2020,
        11 | 12 => Primaries::P3,
        _ => Primaries::Unknown,
    }
}

const STILL_CODECS: &[&str] = &[
    "png",
    "mjpeg",
    "bmp",
    "tiff",
    "webp",
    "targa",
    "dpx",
    "exr",
    "jpeg2000",
    "sgi",
    "qoi",
    "psd",
    "ppm",
    "pgm",
    "pam",
    "pbm",
    "pgmyuv",
    "pfm",
    "phm",
    "svg",
    "pcx",
    "xbm",
    "xpm",
    "xwd",
    "dds",
    "jpegxl",
    "jpegls",
    "sunrast",
    "gem",
    "pictor",
    "photocd",
    "radiance_hdr",
    "vbn",
    "cri",
];

/// Image files whose container is shared with video (AVIF and HEIC hold one AV1 or HEVC frame
/// in an MP4-style box, JPEG XL its own): their extension says they are pictures.
pub const STILL_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "jpe", "pjpeg", "pjp", "bmp", "dib", "gif", "tif", "tiff",
    "webp", "tga", "icb", "vda", "vst", "dpx", "exr", "jp2", "j2k", "jpf", "jpx", "jpm", "sgi",
    "rgb", "rgba", "bw", "qoi", "psd", "ppm", "pgm", "pbm", "pam", "pnm", "pfm", "phm", "svg",
    "svgz", "pcx", "xbm", "xpm", "xwd", "dds", "ico", "cur", "jxl", "jls", "ras", "sun", "hdr",
    "pic", "avif", "heic", "heif", "hif", "pcd", "cri",
];

fn still_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| STILL_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Nominal frame rate. The container's base rate (r_frame_rate) is preferred when it is a
/// standard rate; the average rate is skewed by the last frame's duration in short files and by
/// dropped frames in variable-rate recordings.
pub(crate) fn frame_rate(stream: &ff::format::stream::Stream) -> Rate {
    let base = to_rate(stream.rate());
    let avg = to_rate(stream.avg_frame_rate());
    match (base, avg) {
        (Some(b), _) if Rate::SEQUENCE_RATES.contains(&b) => b,
        (_, Some(a)) if Rate::SEQUENCE_RATES.contains(&a) => a,
        (Some(b), Some(a)) if b.as_f64() > 0.0 && b.as_f64() <= 240.0 => {
            if (b.as_f64() - a.as_f64()).abs() / b.as_f64() < 0.05 {
                b
            } else {
                a
            }
        }
        (Some(b), None) => b,
        (None, Some(a)) => a,
        (_, Some(a)) => a,
        (None, None) => Rate::FPS_25,
    }
}

pub(crate) fn to_rate(r: ff::Rational) -> Option<Rate> {
    (r.numerator() > 0 && r.denominator() > 0)
        .then(|| Rate::from_f64(r.numerator() as f64 / r.denominator() as f64))
}

/// Probes a media file.
pub fn probe(path: &Path) -> Result<MediaAsset> {
    init();
    let input = ff::format::input(path)?;
    let format_name = input.format().name().to_string();
    let container_dur = if input.duration() > 0 {
        Dur::from_seconds(input.duration() as f64 / 1_000_000.0)
    } else {
        Dur::ZERO
    };
    let mut video: Option<VideoStream> = None;
    let mut audio: Vec<AudioStream> = Vec::new();
    let mut still = false;
    for stream in input.streams() {
        let tb = stream.time_base();
        let tb_f = tb.numerator() as f64 / tb.denominator().max(1) as f64;
        let stream_dur = if stream.duration() > 0 {
            Dur::from_seconds(stream.duration() as f64 * tb_f)
        } else {
            container_dur
        };
        let start = if stream.start_time() != ff::ffi::AV_NOPTS_VALUE {
            Dur::from_seconds(stream.start_time() as f64 * tb_f)
        } else {
            Dur::ZERO
        };
        let codec = stream.parameters().id().name().to_string();
        match stream.parameters().medium() {
            // an icon file holds one picture per size: the largest is used
            Type::Video if video.is_none() || format_name == "ico" => {
                // cover art in audio files is a "video" stream with the attached-picture disposition
                if stream
                    .disposition()
                    .contains(ff::format::stream::Disposition::ATTACHED_PIC)
                {
                    continue;
                }
                let cp = codec_params(&stream);
                if cp.width <= 0 || cp.height <= 0 {
                    continue;
                }
                if let Some(v) = &video
                    && (cp.width as u64 * cp.height as u64) <= (v.width as u64 * v.height as u64)
                {
                    continue;
                }
                let (pix_name, depth, alpha) = pixel_info(cp.format);
                let is_image_format =
                    format_name.contains("image2") || format_name.ends_with("_pipe");
                let frames_hint = stream.frames();
                // an animated GIF or WebP is a clip; a one-frame picture of any kind is a still
                let is_still = (is_image_format && STILL_CODECS.contains(&codec.as_str()))
                    || (STILL_CODECS.contains(&codec.as_str())
                        && frames_hint <= 1
                        && stream_dur.seconds() < 0.2)
                    || (still_extension(path) && frames_hint <= 1 && codec != "gif")
                    || (codec == "gif" && frames_hint == 1);
                let rate = frame_rate(&stream);
                let frames = if is_still {
                    1
                } else if frames_hint > 0 {
                    frames_hint
                } else {
                    rate.dur_to_frames_round(stream_dur).max(1)
                };
                still = is_still;
                let sar = if cp.sar.0 > 0 && cp.sar.1 > 0 {
                    (cp.sar.0 as u32, cp.sar.1 as u32)
                } else {
                    (1, 1)
                };
                let yuv_full = pix_name.starts_with("yuvj");
                let rgb_like = pix_name.starts_with("rgb")
                    || pix_name.starts_with("bgr")
                    || pix_name.starts_with("gbr")
                    || pix_name.starts_with("argb")
                    || pix_name.starts_with("abgr")
                    || pix_name.starts_with("pal")
                    || pix_name.starts_with("gray")
                    || pix_name.starts_with("ya");
                let mut color = ColorInfo {
                    matrix: map_matrix(cp.color_space),
                    range: if cp.color_range == 2 || yuv_full || rgb_like {
                        ColorRange::Full
                    } else {
                        ColorRange::Limited
                    },
                    transfer: map_transfer(cp.color_trc),
                    primaries: map_primaries(cp.color_primaries),
                };
                if rgb_like {
                    color.matrix = ColorMatrix::Rgb;
                }
                // unspecified matrix: SD sizes use BT.601, the rest BT.709
                if cp.color_space == 2 && !rgb_like {
                    color.matrix = if cp.height <= 576 {
                        ColorMatrix::Bt601
                    } else {
                        ColorMatrix::Bt709
                    };
                }
                if is_still && !rgb_like && cp.color_space == 2 {
                    color.matrix = ColorMatrix::Bt601; // JPEG
                }
                // AVFieldOrder by display order: TT and BT show the top field first
                let field_order = match cp.field_order {
                    2 | 5 => FieldOrder::UpperFirst,
                    3 | 4 => FieldOrder::LowerFirst,
                    _ => FieldOrder::Progressive,
                };
                let bit_depth = if cp.bits_raw > 0 {
                    cp.bits_raw as u8
                } else {
                    depth
                };
                video = Some(VideoStream {
                    index: stream.index(),
                    codec,
                    width: cp.width as u32,
                    height: cp.height as u32,
                    pixel_aspect: sar,
                    rate,
                    frames,
                    pixel_format: pix_name,
                    bit_depth,
                    alpha: if alpha {
                        AlphaMode::Straight
                    } else {
                        AlphaMode::None
                    },
                    color,
                    field_order,
                    start,
                    rotation: rotation(&stream),
                    timecode: stream.metadata().get("timecode").map(str::to_string),
                });
            }
            Type::Audio => {
                let cp = codec_params(&stream);
                if cp.sample_rate <= 0 || cp.channels <= 0 {
                    continue;
                }
                let rate = Rate::fps(cp.sample_rate as u32);
                audio.push(AudioStream {
                    index: stream.index(),
                    codec,
                    sample_rate: cp.sample_rate as u32,
                    layout: ChannelLayout::from_count(cp.channels as u16),
                    samples: rate.dur_to_frames_round(stream_dur),
                    start,
                });
            }
            _ => {}
        }
    }
    if video.is_none() && audio.is_empty() {
        return Err(MediaError::Unsupported(
            "no playable video or audio stream".into(),
        ));
    }
    if let Some(v) = &mut video
        && v.timecode.is_none()
    {
        v.timecode = input.metadata().get("timecode").map(str::to_string);
    }
    let kind = match (&video, still) {
        (Some(_), true) => MediaKind::Still,
        (Some(_), false) => MediaKind::Video,
        (None, _) => MediaKind::Audio,
    };
    let duration = match &video {
        Some(v) if !still => v.duration().max(container_dur),
        _ => audio
            .first()
            .map(|a| a.duration())
            .unwrap_or(container_dur)
            .max(container_dur),
    };
    let meta = std::fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(MediaAsset {
        id: AssetId(0),
        path: path.to_string_lossy().into_owned(),
        proxy: None,
        kind,
        video,
        audio,
        duration,
        interpretation: Interpretation::default(),
        file_size: meta.len(),
        modified_unix: modified,
    })
}
