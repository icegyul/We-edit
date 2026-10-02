//! Editing proxies: smaller H.264 copies of camera footage that decode quickly.
//!
//! A proxy keeps the original's frame numbering exactly (frame N of the proxy is frame N of the
//! original, as `VideoDecoder` numbers them, gaps and duplicates included), so the editor can
//! swap files without any time mapping. It also keeps the original's YUV values: frames are only
//! resized and reduced to 8-bit 4:2:0, never converted between color matrices, so the proxy is
//! decoded with the original's color description and looks the same. Proxies have a keyframe
//! every half second and no reordered frames, so scrubbing and reverse playback stay fast.

use std::path::Path;

use ff::format::Pixel;
use ffmpeg_next as ff;
use op_core::Rate;

use crate::encode::{Muxer, Tuning, VideoCodec, VideoSettings};
use crate::{MediaError, Result, init};

/// The proxy size for a picture: the short side at most `short_side`, never enlarged, even
/// dimensions for 4:2:0.
pub fn proxy_size(width: u32, height: u32, short_side: u32) -> (u32, u32) {
    let k = (short_side as f64 / width.min(height).max(1) as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (even(width as f64 * k), even(height as f64 * k))
}

/// Pixel formats a proxy can carry without a matrix conversion (YUV, any depth or subsampling).
fn yuv_source(fmt: Pixel) -> bool {
    let name = format!("{fmt:?}").to_ascii_lowercase();
    let rgb = [
        "rgb", "bgr", "gbr", "argb", "abgr", "gray", "pal8", "ya", "xyz",
    ];
    !rgb.iter()
        .any(|k| name.starts_with(k) || name.contains(&format!("_{k}")))
}

/// Writes a proxy of video stream `stream` (or the best one) of `src` to `dst` (an `.mp4`).
/// `rate` and `frames` are the asset's interpreted frame rate and frame count. `progress` gets
/// 0..1 and returns false to cancel; on cancel or failure no file is left behind. Returns the
/// proxy's size.
pub fn make_proxy(
    src: &Path,
    stream: Option<usize>,
    rate: Rate,
    frames: i64,
    dst: &Path,
    short_side: u32,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(u32, u32)> {
    init();
    let partial = dst.with_extension("partial.mp4");
    let result = write(src, stream, rate, frames, &partial, short_side, progress);
    match result {
        Ok(size) => {
            std::fs::rename(&partial, dst)?;
            Ok(size)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn write(
    src: &Path,
    stream: Option<usize>,
    rate: Rate,
    frames: i64,
    out: &Path,
    short_side: u32,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(u32, u32)> {
    let mut input = ff::format::input(src)?;
    let st = match stream {
        Some(i) => input
            .stream(i)
            .ok_or_else(|| MediaError::Unsupported("no such stream".into()))?,
        None => input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or_else(|| MediaError::Unsupported("no video stream".into()))?,
    };
    let index = st.index();
    let tb = st.time_base();
    let time_base = tb.numerator() as f64 / tb.denominator().max(1) as f64;
    let start_pts = if st.start_time() != ff::ffi::AV_NOPTS_VALUE {
        st.start_time()
    } else {
        0
    };
    let mut ctx = ff::codec::context::Context::from_parameters(st.parameters())?;
    ctx.set_threading(ff::threading::Config {
        kind: ff::threading::Type::Frame,
        count: 0,
    });
    let mut decoder = ctx.decoder().video()?;
    let (pw, ph) = proxy_size(decoder.width(), decoder.height(), short_side);
    let settings = VideoSettings {
        codec: VideoCodec::H264,
        width: pw,
        height: ph,
        rate,
        bitrate_kbps: None,
        quality: 23,
        hardware: false,
    };
    let mut muxer = Muxer::create_tuned(out, Some(&settings), None, Tuning::PROXY)?;
    let total = frames.max(1);
    let mut scaler: Option<(Pixel, u32, u32, ff::software::scaling::Context)> = None;
    let mut scaled = ff::frame::Video::empty();
    // the last picture written, repeated over gaps in the source's timestamps
    let mut last: Option<[Vec<u8>; 3]> = None;
    let mut written: i64 = 0;
    let index_of = |pts: i64| {
        let secs = (pts - start_pts) as f64 * time_base;
        (secs * rate.as_f64() + 1e-3).floor() as i64
    };

    let mut handle = |decoder: &mut ff::decoder::Video,
                      muxer: &mut Muxer,
                      written: &mut i64,
                      last: &mut Option<[Vec<u8>; 3]>|
     -> Result<bool> {
        let mut raw = ff::frame::Video::empty();
        while decoder.receive_frame(&mut raw).is_ok() {
            let fmt = raw.format();
            if !yuv_source(fmt) {
                return Err(MediaError::Unsupported(format!(
                    "proxies are made from YUV video; this file is {fmt:?}"
                )));
            }
            let pts = raw.timestamp().or(raw.pts()).unwrap_or(start_pts);
            let idx = index_of(pts);
            // a frame for a number already written (duplicate timestamp) adds nothing
            if idx < *written {
                continue;
            }
            let (w, h) = (raw.width(), raw.height());
            if !matches!(&scaler, Some((f, sw, sh, _)) if *f == fmt && *sw == w && *sh == h) {
                let ctx = ff::software::scaling::Context::get(
                    fmt,
                    w,
                    h,
                    Pixel::YUV420P,
                    pw,
                    ph,
                    ff::software::scaling::Flags::BILINEAR,
                )?;
                scaler = Some((fmt, w, h, ctx));
            }
            if let Some((_, _, _, s)) = &mut scaler {
                s.run(&raw, &mut scaled)?;
            }
            let planes = packed_planes(&scaled, pw as usize, ph as usize);
            // a gap in the timestamps holds the previous picture (or this one, at the start)
            while *written < idx.min(total) {
                let fill = last.as_ref().unwrap_or(&planes);
                muxer.push_video(&[&fill[0], &fill[1], &fill[2]])?;
                *written += 1;
            }
            if *written >= total {
                return Ok(false);
            }
            muxer.push_video(&[&planes[0], &planes[1], &planes[2]])?;
            *written += 1;
            *last = Some(planes);
            if !progress(*written as f32 / total as f32) {
                return Err(MediaError::Cancelled);
            }
        }
        Ok(true)
    };

    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if !handle(&mut decoder, &mut muxer, &mut written, &mut last)? {
            break;
        }
    }
    if written < total {
        let _ = decoder.send_eof();
        handle(&mut decoder, &mut muxer, &mut written, &mut last)?;
    }
    // the original's last frames may be missing: hold the last picture to the end
    if let Some(fill) = &last {
        while written < total {
            muxer.push_video(&[&fill[0], &fill[1], &fill[2]])?;
            written += 1;
        }
    }
    if written == 0 {
        return Err(MediaError::Unsupported("the video has no frames".into()));
    }
    muxer.finish()?;
    Ok((pw, ph))
}

/// The three planes of a YUV 4:2:0 frame without row padding.
fn packed_planes(f: &ff::frame::Video, w: usize, h: usize) -> [Vec<u8>; 3] {
    let plane = |i: usize, pw: usize, ph: usize| {
        let stride = f.stride(i);
        let data = f.data(i);
        let mut out = Vec::with_capacity(pw * ph);
        for y in 0..ph {
            out.extend_from_slice(&data[y * stride..y * stride + pw]);
        }
        out
    };
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    [plane(0, w, h), plane(1, cw, ch), plane(2, cw, ch)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoDecoder;

    #[test]
    fn sizes_keep_the_shape_and_never_grow() {
        assert_eq!(proxy_size(3840, 2160, 540), (960, 540));
        assert_eq!(proxy_size(1080, 1920, 540), (540, 960));
        assert_eq!(proxy_size(640, 360, 540), (640, 360));
        assert_eq!(proxy_size(956, 720, 540), (718, 540));
    }

    #[test]
    fn rgb_sources_are_refused() {
        assert!(yuv_source(Pixel::YUV420P10LE));
        assert!(yuv_source(Pixel::NV12));
        assert!(!yuv_source(Pixel::RGB24));
        assert!(!yuv_source(Pixel::GBRP));
    }

    /// The proxy has the original's frames, numbered the same way, at the smaller size.
    #[test]
    fn proxy_matches_the_original_frame_for_frame() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cam.mp4");
        let (w, h) = (1280u32, 720u32);
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 18,
            hardware: false,
        };
        let mut m = Muxer::create(&src, Some(&v), None).unwrap();
        let chroma = vec![128u8; (w / 2 * h / 2) as usize];
        for i in 0..40u32 {
            let luma = vec![(20 + 4 * i) as u8; (w * h) as usize];
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();

        let dst = dir.path().join("cam.proxy.mp4");
        let mut calls = 0;
        let size = make_proxy(&src, None, Rate::FPS_25, 40, &dst, 360, &mut |p| {
            calls += 1;
            assert!((0.0..=1.0).contains(&p));
            true
        })
        .unwrap();
        assert_eq!(size, (640, 360));
        assert!(calls >= 40);
        assert!(!dir.path().join("cam.proxy.partial.mp4").exists());

        let probed = crate::probe(&dst).unwrap();
        let pv = probed.video.unwrap();
        assert_eq!((pv.width, pv.height), (640, 360));
        assert!((pv.frames - 40).abs() <= 1, "{} frames", pv.frames);
        let mut dec = VideoDecoder::open(&dst, None, Some(Rate::FPS_25), pv.color, false).unwrap();
        // read backwards: every frame is the original one with the same number
        for i in (0..40i64).rev() {
            let f = dec.frame(i).unwrap();
            let px = f.to_rgba8_scaled(1, 1);
            let got = px[0] as f64 * 219.0 / 255.0 + 16.0;
            let want = 20.0 + 4.0 * i as f64;
            assert!((got - want).abs() < 2.0, "frame {i}: {got:.1} vs {want}");
        }
    }

    #[test]
    fn cancelling_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("c.mp4");
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: 320,
            height: 240,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 23,
            hardware: false,
        };
        let mut m = Muxer::create(&src, Some(&v), None).unwrap();
        let luma = vec![100u8; 320 * 240];
        let chroma = vec![128u8; 160 * 120];
        for _ in 0..30 {
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
        let dst = dir.path().join("c.proxy.mp4");
        let r = make_proxy(&src, None, Rate::FPS_25, 30, &dst, 120, &mut |p| p < 0.3);
        assert!(matches!(r, Err(MediaError::Cancelled)));
        assert!(!dst.exists());
        assert!(!dir.path().join("c.proxy.partial.mp4").exists());
    }
}
