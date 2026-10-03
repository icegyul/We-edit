//! Encoding and muxing for export.
//!
//! The renderer hands over planar YUV frames already converted with the output matrix (BT.709
//! limited range for delivery codecs) or RGBA for codecs with alpha, so no color conversion
//! happens here beyond rearranging planes for encoders that need NV12.

use std::path::Path;

use ff::format::Pixel;
use ffmpeg_next as ff;
use op_core::Rate;

use crate::{MediaError, Result, init};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VideoCodec {
    H264,
    Hevc,
    ProRes422Hq,
    ProRes4444,
    DnxhrHq,
    Png,
}

impl VideoCodec {
    pub const ALL: [VideoCodec; 6] = [
        VideoCodec::H264,
        VideoCodec::Hevc,
        VideoCodec::ProRes422Hq,
        VideoCodec::ProRes4444,
        VideoCodec::DnxhrHq,
        VideoCodec::Png,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "HEVC (H.265)",
            VideoCodec::ProRes422Hq => "Apple ProRes 422 HQ",
            VideoCodec::ProRes4444 => "Apple ProRes 4444",
            VideoCodec::DnxhrHq => "DNxHR HQ",
            VideoCodec::Png => "PNG (QuickTime)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            VideoCodec::H264 | VideoCodec::Hevc => "mp4",
            _ => "mov",
        }
    }

    /// Pixel format the renderer must deliver.
    pub fn input(self) -> VideoInput {
        match self {
            VideoCodec::H264 | VideoCodec::Hevc => VideoInput::Yuv420p,
            VideoCodec::ProRes422Hq => VideoInput::Yuv422p10,
            VideoCodec::DnxhrHq => VideoInput::Yuv422p,
            VideoCodec::ProRes4444 => VideoInput::Yuva444p10,
            VideoCodec::Png => VideoInput::Rgba8,
        }
    }

    /// Candidate encoders, preferred first. Hardware encoders are tried only when asked.
    fn encoders(self, hardware: bool) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = Vec::new();
        match self {
            VideoCodec::H264 => {
                if hardware {
                    v.extend([
                        "h264_nvenc",
                        "h264_amf",
                        "h264_qsv",
                        "h264_mf",
                        "h264_videotoolbox",
                        "h264_vaapi",
                    ]);
                }
                v.push("libx264");
            }
            VideoCodec::Hevc => {
                if hardware {
                    v.extend([
                        "hevc_nvenc",
                        "hevc_amf",
                        "hevc_qsv",
                        "hevc_mf",
                        "hevc_videotoolbox",
                    ]);
                }
                v.push("libx265");
            }
            VideoCodec::ProRes422Hq | VideoCodec::ProRes4444 => v.push("prores_ks"),
            VideoCodec::DnxhrHq => v.push("dnxhd"),
            VideoCodec::Png => v.push("png"),
        }
        v
    }
}

/// True for encoders that run on the graphics card or a media engine (their failures are
/// worth retrying with the software encoder).
pub fn is_hardware_encoder(name: &str) -> bool {
    ["nvenc", "amf", "qsv", "_mf", "videotoolbox", "vaapi"]
        .iter()
        .any(|k| name.contains(k))
}

/// How H.264/HEVC streams are laid out: delivery files favour size, proxies favour seeking.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tuning {
    /// Keyframe interval in seconds.
    pub gop_seconds: f64,
    pub b_frames: u32,
    /// x264/x265 preset.
    pub preset: &'static str,
}

impl Tuning {
    pub const DELIVERY: Tuning = Tuning {
        gop_seconds: 2.0,
        b_frames: 2,
        preset: "medium",
    };
    /// Editing proxies: a keyframe every half second and no reordered frames, so any frame is
    /// reached by decoding a handful of others.
    pub const PROXY: Tuning = Tuning {
        gop_seconds: 0.5,
        b_frames: 0,
        preset: "veryfast",
    };
}

/// Adds which step failed to an FFmpeg error ("Invalid argument" alone says little).
fn step(what: impl std::fmt::Display) -> impl FnOnce(MediaError) -> MediaError {
    move |e| MediaError::Failed(format!("{what}: {e}"))
}

/// Layout of the frames given to `Muxer::push_video`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VideoInput {
    Yuv420p,
    Yuv422p,
    Yuv422p10,
    Yuva444p10,
    Rgba8,
}

impl VideoInput {
    fn pixel(self) -> Pixel {
        match self {
            VideoInput::Yuv420p => Pixel::YUV420P,
            VideoInput::Yuv422p => Pixel::YUV422P,
            VideoInput::Yuv422p10 => Pixel::YUV422P10LE,
            VideoInput::Yuva444p10 => Pixel::YUVA444P10LE,
            VideoInput::Rgba8 => Pixel::RGBA,
        }
    }

    /// (plane count, chroma shift x, chroma shift y, bytes per sample) for YUV inputs.
    pub fn planes(self) -> (usize, u32, u32, usize) {
        match self {
            VideoInput::Yuv420p => (3, 1, 1, 1),
            VideoInput::Yuv422p => (3, 1, 0, 1),
            VideoInput::Yuv422p10 => (3, 1, 0, 2),
            VideoInput::Yuva444p10 => (4, 0, 0, 2),
            VideoInput::Rgba8 => (1, 0, 0, 4),
        }
    }

    pub fn is_yuv(self) -> bool {
        !matches!(self, VideoInput::Rgba8)
    }

    pub fn bits(self) -> u32 {
        match self {
            VideoInput::Yuv422p10 | VideoInput::Yuva444p10 => 10,
            _ => 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoSettings {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    /// Target bitrate for H.264/HEVC; None uses constant quality.
    pub bitrate_kbps: Option<u32>,
    /// Constant-quality level (lower is better), used when no bitrate is set.
    pub quality: u8,
    pub hardware: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AudioCodec {
    Aac,
    Pcm16,
    Pcm24,
    Mp3,
}

impl AudioCodec {
    pub fn label(self) -> &'static str {
        match self {
            AudioCodec::Aac => "AAC",
            AudioCodec::Pcm16 => "PCM 16-bit",
            AudioCodec::Pcm24 => "PCM 24-bit",
            AudioCodec::Mp3 => "MP3",
        }
    }

    fn encoder(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Pcm16 => "pcm_s16le",
            AudioCodec::Pcm24 => "pcm_s24le",
            AudioCodec::Mp3 => "libmp3lame",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSettings {
    pub codec: AudioCodec,
    pub rate: u32,
    pub channels: u16,
    pub bitrate_kbps: u32,
}

struct VideoOut {
    encoder: ff::encoder::Video,
    stream: usize,
    time_base: ff::Rational,
    input: VideoInput,
    frame: ff::frame::Video,
    staging: Option<ff::frame::Video>,
    scaler: Option<ff::software::scaling::Context>,
    next_pts: i64,
    name: String,
}

struct AudioOut {
    encoder: ff::encoder::Audio,
    stream: usize,
    time_base: ff::Rational,
    frame_size: usize,
    channels: usize,
    pending: Vec<f32>,
    next_pts: i64,
    format: ff::format::Sample,
}

pub struct Muxer {
    octx: ff::format::context::Output,
    video: Option<VideoOut>,
    audio: Option<AudioOut>,
    header_written: bool,
}

fn rational(rate: Rate) -> ff::Rational {
    ff::Rational::new(rate.num as i32, rate.den as i32)
}

impl Muxer {
    /// Creates the output file with the requested streams. The container follows the file
    /// extension.
    pub fn create(
        path: &Path,
        video: Option<&VideoSettings>,
        audio: Option<&AudioSettings>,
    ) -> Result<Muxer> {
        Self::create_tuned(path, video, audio, Tuning::DELIVERY)
    }

    /// Like `create`, with the stream layout of `tuning` for H.264/HEVC.
    pub fn create_tuned(
        path: &Path,
        video: Option<&VideoSettings>,
        audio: Option<&AudioSettings>,
        tuning: Tuning,
    ) -> Result<Muxer> {
        init();
        let mut octx = ff::format::output(path)?;
        let global_header = octx
            .format()
            .flags()
            .contains(ff::format::Flags::GLOBAL_HEADER);
        let video_out = match video {
            Some(v) => Some(open_video(&mut octx, v, global_header, tuning)?),
            None => None,
        };
        let audio_out = match audio {
            Some(a) => Some(open_audio(&mut octx, a, global_header)?),
            None => None,
        };
        let mut m = Muxer {
            octx,
            video: video_out,
            audio: audio_out,
            header_written: false,
        };
        m.octx
            .write_header()
            .map_err(MediaError::from)
            .map_err(step("writing the file header"))?;
        m.header_written = true;
        // the muxer may change stream time bases while writing the header
        if let Some(v) = &mut m.video {
            v.time_base = m.octx.stream(v.stream).unwrap().time_base();
        }
        if let Some(a) = &mut m.audio {
            a.time_base = m.octx.stream(a.stream).unwrap().time_base();
        }
        Ok(m)
    }

    /// Name of the video encoder in use (e.g. `h264_nvenc`).
    pub fn video_encoder(&self) -> Option<&str> {
        self.video.as_ref().map(|v| v.name.as_str())
    }

    pub fn audio_frame_size(&self) -> usize {
        self.audio.as_ref().map(|a| a.frame_size).unwrap_or(1024)
    }

    /// Encodes one frame. `planes` are tightly packed rows in the layout of `VideoInput`.
    pub fn push_video(&mut self, planes: &[&[u8]]) -> Result {
        let Some(v) = &mut self.video else {
            return Ok(());
        };
        let (count, cx, cy, bps) = v.input.planes();
        let (w, h) = (v.frame.width() as usize, v.frame.height() as usize);
        let target = v.staging.as_mut().unwrap_or(&mut v.frame);
        for (i, plane) in planes.iter().enumerate().take(count) {
            let (pw, ph) = if i == 0 || i == 3 || !v.input.is_yuv() {
                (w, h)
            } else {
                (w.div_ceil(1 << cx), h.div_ceil(1 << cy))
            };
            let row = pw * bps;
            let stride = target.stride(i);
            let dst = target.data_mut(i);
            for y in 0..ph {
                let src = &plane[y * row..][..row];
                dst[y * stride..][..row].copy_from_slice(src);
            }
        }
        if let (Some(scaler), Some(staging)) = (&mut v.scaler, &v.staging) {
            scaler
                .run(staging, &mut v.frame)
                .map_err(MediaError::from)
                .map_err(step("converting the frame"))?;
        }
        v.frame.set_pts(Some(v.next_pts));
        v.next_pts += 1;
        v.encoder
            .send_frame(&v.frame)
            .map_err(MediaError::from)
            .map_err(step(format!("video encoder {}", v.name)))?;
        drain_video(&mut self.octx, v)?;
        Ok(())
    }

    /// Queues interleaved float samples.
    pub fn push_audio(&mut self, interleaved: &[f32]) -> Result {
        let Some(a) = &mut self.audio else {
            return Ok(());
        };
        a.pending.extend_from_slice(interleaved);
        while a.pending.len() >= a.frame_size * a.channels {
            let chunk: Vec<f32> = a.pending.drain(..a.frame_size * a.channels).collect();
            encode_audio(&mut self.octx, a, &chunk)?;
        }
        Ok(())
    }

    /// Flushes encoders and writes the trailer.
    pub fn finish(mut self) -> Result {
        if let Some(a) = &mut self.audio {
            if !a.pending.is_empty() {
                let chunk = std::mem::take(&mut a.pending);
                encode_audio(&mut self.octx, a, &chunk)?;
            }
            a.encoder
                .send_eof()
                .map_err(MediaError::from)
                .map_err(step("audio encoder"))?;
            drain_audio(&mut self.octx, a)?;
        }
        if let Some(v) = &mut self.video {
            v.encoder
                .send_eof()
                .map_err(MediaError::from)
                .map_err(step(format!("video encoder {}", v.name)))?;
            drain_video(&mut self.octx, v)?;
        }
        self.octx
            .write_trailer()
            .map_err(MediaError::from)
            .map_err(step("finishing the file"))?;
        Ok(())
    }
}

fn open_video(
    octx: &mut ff::format::context::Output,
    s: &VideoSettings,
    global_header: bool,
    tuning: Tuning,
) -> Result<VideoOut> {
    let mut last_err = None;
    for name in s.codec.encoders(s.hardware) {
        let Some(codec) = ff::encoder::find_by_name(name) else {
            continue;
        };
        match try_video(octx, s, codec, name, global_header, tuning) {
            Ok(v) => return Ok(v),
            Err(e) => {
                log::info!("video encoder {name} unavailable: {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| MediaError::Unsupported(format!("no encoder for {}", s.codec.label()))))
}

fn try_video(
    octx: &mut ff::format::context::Output,
    s: &VideoSettings,
    codec: ff::Codec,
    name: &str,
    global_header: bool,
    tuning: Tuning,
) -> Result<VideoOut> {
    let input = s.codec.input();
    let wanted = input.pixel();
    let supported: Vec<Pixel> = codec
        .video()
        .ok()
        .and_then(|v| v.formats())
        .map(|f| f.collect())
        .unwrap_or_default();
    let enc_format = if supported.is_empty() || supported.contains(&wanted) {
        wanted
    } else if supported.contains(&Pixel::NV12) && wanted == Pixel::YUV420P {
        Pixel::NV12
    } else {
        return Err(MediaError::Unsupported(format!(
            "{name} does not accept {wanted:?}"
        )));
    };
    let mut ctx = ff::codec::context::Context::new_with_codec(codec);
    // FFmpeg encodes on one core unless told otherwise; ProRes, DNxHR and PNG split each frame
    // into slices, x264 and x265 take the count for their own threads (0: one per core)
    ctx.set_threading(ff::threading::Config {
        kind: if matches!(s.codec, VideoCodec::Png) {
            ff::threading::Type::Frame
        } else {
            ff::threading::Type::Slice
        },
        count: 0,
    });
    let mut enc = ctx.encoder().video()?;
    enc.set_width(s.width);
    enc.set_height(s.height);
    enc.set_format(enc_format);
    let tb = ff::Rational::new(s.rate.den as i32, s.rate.num as i32);
    enc.set_time_base(tb);
    enc.set_frame_rate(Some(rational(s.rate)));
    enc.set_aspect_ratio(ff::Rational::new(1, 1));
    if input.is_yuv() {
        enc.set_colorspace(ff::color::Space::BT709);
        enc.set_color_range(ff::color::Range::MPEG);
        enc.set_color_primaries(ff::color::Primaries::BT709);
        enc.set_color_transfer_characteristic(ff::color::TransferCharacteristic::BT709);
    }
    let mut opts = ff::Dictionary::new();
    let delivery = matches!(s.codec, VideoCodec::H264 | VideoCodec::Hevc);
    if delivery {
        enc.set_gop(((s.rate.as_f64() * tuning.gop_seconds).round() as u32).max(1));
        // VideoToolbox can give B-frame timestamps the MP4 muxer rejects ("Invalid argument")
        let videotoolbox = name.contains("videotoolbox");
        enc.set_max_b_frames(if videotoolbox {
            0
        } else {
            tuning.b_frames as usize
        });
        if videotoolbox {
            // the software path of VideoToolbox when the media engine is busy or missing
            opts.set("allow_sw", "1");
        }
        match s.bitrate_kbps {
            Some(kbps) => {
                enc.set_bit_rate(kbps as usize * 1000);
                enc.set_max_bit_rate(kbps as usize * 1500);
            }
            None => {
                let q = s.quality.to_string();
                if name.starts_with("libx26") {
                    opts.set("crf", &q);
                } else if name.contains("nvenc") {
                    opts.set("rc", "vbr");
                    opts.set("cq", &q);
                    opts.set("b", "0");
                } else if name.contains("qsv") {
                    opts.set("global_quality", &q);
                } else {
                    // other hardware encoders take a bitrate
                    let px = (s.width * s.height) as f64 * s.rate.as_f64();
                    enc.set_bit_rate((px * 0.12) as usize);
                }
            }
        }
        if name.starts_with("libx26") {
            opts.set("preset", tuning.preset);
        } else if name.contains("nvenc") {
            opts.set("preset", "p5");
        }
        if name == "libx264" {
            opts.set("profile", "high");
        }
    }
    match s.codec {
        VideoCodec::ProRes422Hq => opts.set("profile", "3"),
        VideoCodec::ProRes4444 => opts.set("profile", "4"),
        VideoCodec::DnxhrHq => opts.set("profile", "dnxhr_hq"),
        _ => {}
    }
    if global_header {
        enc.set_flags(ff::codec::Flags::GLOBAL_HEADER);
    }
    let encoder = enc.open_as_with(codec, opts)?;
    let mut stream = octx.add_stream(codec)?;
    stream.set_parameters(&encoder);
    stream.set_time_base(tb);
    let index = stream.index();
    let frame = ff::frame::Video::new(enc_format, s.width, s.height);
    let (staging, scaler) = if enc_format != wanted {
        let staging = ff::frame::Video::new(wanted, s.width, s.height);
        let scaler = ff::software::scaling::Context::get(
            wanted,
            s.width,
            s.height,
            enc_format,
            s.width,
            s.height,
            ff::software::scaling::Flags::POINT,
        )?;
        (Some(staging), Some(scaler))
    } else {
        (None, None)
    };
    Ok(VideoOut {
        encoder,
        stream: index,
        time_base: tb,
        input,
        frame,
        staging,
        scaler,
        next_pts: 0,
        name: name.to_string(),
    })
}

fn open_audio(
    octx: &mut ff::format::context::Output,
    s: &AudioSettings,
    global_header: bool,
) -> Result<AudioOut> {
    let name = s.codec.encoder();
    let codec = ff::encoder::find_by_name(name)
        .ok_or_else(|| MediaError::Unsupported(format!("no {name} encoder")))?;
    let ctx = ff::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().audio()?;
    let formats: Vec<ff::format::Sample> = codec
        .audio()
        .ok()
        .and_then(|a| a.formats())
        .map(|f| f.collect())
        .unwrap_or_default();
    let prefer = [
        ff::format::Sample::F32(ff::format::sample::Type::Planar),
        ff::format::Sample::F32(ff::format::sample::Type::Packed),
        ff::format::Sample::I32(ff::format::sample::Type::Packed),
        ff::format::Sample::I16(ff::format::sample::Type::Packed),
        ff::format::Sample::I16(ff::format::sample::Type::Planar),
        ff::format::Sample::I32(ff::format::sample::Type::Planar),
    ];
    let format = prefer
        .iter()
        .copied()
        .find(|f| formats.is_empty() || formats.contains(f))
        .unwrap_or(prefer[0]);
    enc.set_rate(s.rate as i32);
    enc.set_channel_layout(ff::ChannelLayout::default(s.channels as i32));
    enc.set_format(format);
    enc.set_bit_rate(s.bitrate_kbps as usize * 1000);
    let tb = ff::Rational::new(1, s.rate as i32);
    enc.set_time_base(tb);
    if global_header {
        enc.set_flags(ff::codec::Flags::GLOBAL_HEADER);
    }
    let encoder = enc.open_as(codec)?;
    let frame_size = match encoder.frame_size() {
        0 => 1024,
        n => n as usize,
    };
    let mut stream = octx.add_stream(codec)?;
    stream.set_parameters(&encoder);
    stream.set_time_base(tb);
    let index = stream.index();
    Ok(AudioOut {
        encoder,
        stream: index,
        time_base: tb,
        frame_size,
        channels: s.channels as usize,
        pending: Vec::new(),
        next_pts: 0,
        format,
    })
}

fn encode_audio(
    octx: &mut ff::format::context::Output,
    a: &mut AudioOut,
    interleaved: &[f32],
) -> Result {
    let n = interleaved.len() / a.channels;
    if n == 0 {
        return Ok(());
    }
    let mut frame =
        ff::frame::Audio::new(a.format, n, ff::ChannelLayout::default(a.channels as i32));
    frame.set_rate(a.encoder.rate());
    match a.format {
        ff::format::Sample::F32(ff::format::sample::Type::Planar) => {
            for c in 0..a.channels {
                let plane = frame.plane_mut::<f32>(c);
                for (i, v) in plane.iter_mut().enumerate().take(n) {
                    *v = interleaved[i * a.channels + c];
                }
            }
        }
        ff::format::Sample::F32(ff::format::sample::Type::Packed) => {
            let data = frame.data_mut(0);
            for (i, v) in interleaved.iter().enumerate() {
                data[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        ff::format::Sample::I32(t) => {
            let q = |v: f32| (v.clamp(-1.0, 1.0) as f64 * 2147483647.0) as i32;
            for c in 0..a.channels {
                for i in 0..n {
                    let (plane, at) = if t == ff::format::sample::Type::Planar {
                        (c, i)
                    } else {
                        (0, i * a.channels + c)
                    };
                    frame.data_mut(plane)[at * 4..at * 4 + 4]
                        .copy_from_slice(&q(interleaved[i * a.channels + c]).to_le_bytes());
                }
            }
        }
        ff::format::Sample::I16(t) => {
            let q = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            for c in 0..a.channels {
                for i in 0..n {
                    let (plane, at) = if t == ff::format::sample::Type::Planar {
                        (c, i)
                    } else {
                        (0, i * a.channels + c)
                    };
                    frame.data_mut(plane)[at * 2..at * 2 + 2]
                        .copy_from_slice(&q(interleaved[i * a.channels + c]).to_le_bytes());
                }
            }
        }
        f => return Err(MediaError::Unsupported(format!("sample format {f:?}"))),
    }
    frame.set_pts(Some(a.next_pts));
    a.next_pts += n as i64;
    a.encoder
        .send_frame(&frame)
        .map_err(MediaError::from)
        .map_err(step("audio encoder"))?;
    drain_audio(octx, a)
}

fn drain_video(octx: &mut ff::format::context::Output, v: &mut VideoOut) -> Result {
    let mut packet = ff::Packet::empty();
    loop {
        match v.encoder.receive_packet(&mut packet) {
            Ok(()) => {
                packet.set_stream(v.stream);
                // without a duration the MP4 edit list would end before the last frame
                if packet.duration() <= 0 {
                    packet.set_duration(1);
                }
                packet.rescale_ts(v.encoder.time_base(), v.time_base);
                packet
                    .write_interleaved(octx)
                    .map_err(MediaError::from)
                    .map_err(step(format!("writing video from {}", v.name)))?;
            }
            Err(ff::Error::Eof) => return Ok(()),
            Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => return Ok(()),
            Err(e) => return Err(step(format!("video encoder {}", v.name))(e.into())),
        }
    }
}

fn drain_audio(octx: &mut ff::format::context::Output, a: &mut AudioOut) -> Result {
    let mut packet = ff::Packet::empty();
    loop {
        match a.encoder.receive_packet(&mut packet) {
            Ok(()) => {
                packet.set_stream(a.stream);
                packet.rescale_ts(a.encoder.time_base(), a.time_base);
                packet
                    .write_interleaved(octx)
                    .map_err(MediaError::from)
                    .map_err(step("writing audio"))?;
            }
            Err(ff::Error::Eof) => return Ok(()),
            Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => return Ok(()),
            Err(e) => return Err(step("audio encoder")(e.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio, probe, video::VideoDecoder};

    /// Writes a 2 s 320x240 H.264 + AAC file whose frame `i` has luma `16 + 4 * i` and a
    /// 1 kHz tone, then reads it back through probe, the decoder and the audio conform.
    #[test]
    fn encode_probe_decode_conform() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.mp4");
        let (w, h) = (320u32, 240u32);
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 10,
            hardware: false,
        };
        let a = AudioSettings {
            codec: AudioCodec::Aac,
            rate: 48000,
            channels: 2,
            bitrate_kbps: 192,
        };
        let mut m = Muxer::create(&path, Some(&v), Some(&a)).unwrap();
        assert_eq!(m.video_encoder(), Some("libx264"));
        let chroma = vec![128u8; (w / 2 * h / 2) as usize];
        for i in 0..50u32 {
            let luma = vec![(16 + 4 * i) as u8; (w * h) as usize];
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        let samples: Vec<f32> = (0..96000)
            .flat_map(|n| {
                let s = (n as f32 * 1000.0 * std::f32::consts::TAU / 48000.0).sin() * 0.5;
                [s, s]
            })
            .collect();
        m.push_audio(&samples).unwrap();
        m.finish().unwrap();

        let asset = probe::probe(&path).unwrap();
        let vs = asset.video.as_ref().unwrap();
        assert_eq!(
            (vs.width, vs.height, vs.rate, vs.frames),
            (320, 240, Rate::FPS_25, 50)
        );
        assert_eq!(asset.audio.len(), 1);
        assert_eq!(asset.audio[0].sample_rate, 48000);

        let mut dec = VideoDecoder::open(&path, None, None, vs.color, false).unwrap();
        for i in [0i64, 17, 18, 40, 3, 49] {
            let f = dec.frame(i).unwrap();
            assert_eq!(f.index, i);
            let y = f.planes[0].data[(120 * f.planes[0].bytes_per_row + 160) as usize] as i32;
            assert!((y - (16 + 4 * i as i32)).abs() <= 2, "frame {i}: luma {y}");
        }

        let cache = dir.path().join("t.f32");
        let peaks = dir.path().join("t.peaks");
        let info = audio::conform(&path, 0, vs.start, &cache, &peaks, &mut |_| true).unwrap();
        assert_eq!((info.rate, info.channels), (48000, 2));
        assert!((info.samples - 96000).abs() < 2048, "{}", info.samples);
        let c = audio::ConformedAudio::open(&cache).unwrap();
        let mut buf = vec![0f32; 4800 * 2];
        c.read(24000, &mut buf);
        let peak = buf.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.05, "{peak}");
        let p = audio::Peaks::open(&peaks).unwrap();
        let (lo, hi) = p.range(0, 24000, 28800);
        assert!(hi > 0.4 && lo < -0.4);
    }
}
