//! Frame-accurate video decoding.
//!
//! A decoder serves frames by index. Forward requests close to the last decoded frame keep
//! decoding; anything else seeks to the keyframe before the target and decodes forward. Frames
//! are returned as planar YUV (8 or 16 bit, with the source's subsampling) or RGBA (8 or 16
//! bit), so the renderer can convert color on the GPU with the stream's own matrix and range.

use std::path::Path;

use ff::format::Pixel;
use ffmpeg_next as ff;
use op_core::{ColorInfo, ColorMatrix, ColorRange, Rate};

use crate::probe::{codec_params, frame_rate};
use crate::{MediaError, Result, init};

/// How the planes of a frame are laid out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelLayout {
    /// Y, U, V (and optional A) planes, one byte per sample. Chroma planes are subsampled by
    /// `1 << chroma_w` horizontally and `1 << chroma_h` vertically.
    Yuv8 {
        chroma_w: u8,
        chroma_h: u8,
        alpha: bool,
    },
    /// Same with 16-bit little-endian samples holding `bits` significant bits.
    Yuv16 {
        chroma_w: u8,
        chroma_h: u8,
        alpha: bool,
        bits: u8,
    },
    /// A Y plane and one interleaved Cb/Cr plane at half size in both directions (NV12, and
    /// P010/P016 when `wide`: 16-bit samples with the value in the high bits). Hardware
    /// decoders deliver this.
    Nv12 { wide: bool },
    /// One packed RGBA plane, 8 bits per channel.
    Rgba8,
    /// One packed RGBA plane, 16 bits per channel.
    Rgba16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plane {
    /// Tightly packed rows: `bytes_per_row * height` bytes.
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub layout: PixelLayout,
    pub planes: Vec<Plane>,
    pub color: ColorInfo,
    /// Frame index in the stream.
    pub index: i64,
}

impl VideoFrame {
    pub fn byte_size(&self) -> usize {
        self.planes.iter().map(|p| p.data.len()).sum()
    }

    /// Converts to straight RGBA8 on the CPU (tests and diagnostics).
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.to_rgba8_scaled(self.width, self.height)
    }

    /// Converts to RGBA8 at another size (thumbnails), sampling the source with a small box
    /// filter. Honors the stream's matrix and range.
    pub fn to_rgba8_scaled(&self, out_w: u32, out_h: u32) -> Vec<u8> {
        let (ow, oh) = (out_w.max(1) as usize, out_h.max(1) as usize);
        let (w, h) = (self.width.max(1) as usize, self.height.max(1) as usize);
        let mut out = vec![0u8; ow * oh * 4];
        let sx = w as f32 / ow as f32;
        let sy = h as f32 / oh as f32;
        let taps = |s: f32| if s > 1.5 { 2 } else { 1 };
        let (tx, ty) = (taps(sx), taps(sy));
        for y in 0..oh {
            for x in 0..ow {
                let mut acc = [0f32; 4];
                for j in 0..ty {
                    for i in 0..tx {
                        let px =
                            (((x as f32 + (i as f32 + 0.5) / tx as f32) * sx) as usize).min(w - 1);
                        let py =
                            (((y as f32 + (j as f32 + 0.5) / ty as f32) * sy) as usize).min(h - 1);
                        let p = self.pixel(px, py);
                        for k in 0..4 {
                            acc[k] += p[k];
                        }
                    }
                }
                let n = (tx * ty) as f32;
                let q = |c: f32| ((c / n).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[(y * ow + x) * 4..][..4].copy_from_slice(&[
                    q(acc[0]),
                    q(acc[1]),
                    q(acc[2]),
                    q(acc[3]),
                ]);
            }
        }
        out
    }

    /// Straight RGBA (0..1) of one pixel.
    pub fn pixel(&self, x: usize, y: usize) -> [f32; 4] {
        match self.layout {
            PixelLayout::Rgba8 => {
                let p = &self.planes[0];
                let i = y * p.bytes_per_row as usize + x * 4;
                [0, 1, 2, 3].map(|k| p.data[i + k] as f32 / 255.0)
            }
            PixelLayout::Rgba16 => {
                let p = &self.planes[0];
                let i = y * p.bytes_per_row as usize + x * 8;
                [0, 1, 2, 3].map(|k| {
                    u16::from_le_bytes([p.data[i + k * 2], p.data[i + k * 2 + 1]]) as f32 / 65535.0
                })
            }
            PixelLayout::Nv12 { wide } => {
                let max = if wide { 65535.0 } else { 255.0 };
                let sample = |p: &Plane, x: usize, y: usize, k: usize, n: usize| -> f32 {
                    let x = x.min(p.width as usize - 1);
                    let y = y.min(p.height as usize - 1);
                    if wide {
                        let i = y * p.bytes_per_row as usize + (x * n + k) * 2;
                        u16::from_le_bytes([p.data[i], p.data[i + 1]]) as f32 / max
                    } else {
                        p.data[y * p.bytes_per_row as usize + x * n + k] as f32 / max
                    }
                };
                let (kr, kb) = match self.color.matrix {
                    ColorMatrix::Bt601 => (0.299, 0.114),
                    ColorMatrix::Bt2020 => (0.2627, 0.0593),
                    _ => (0.2126, 0.0722),
                };
                let kg = 1.0 - kr - kb;
                let mut yv = sample(&self.planes[0], x, y, 0, 1);
                let mut u = sample(&self.planes[1], x / 2, y / 2, 0, 2) - 0.5;
                let mut v = sample(&self.planes[1], x / 2, y / 2, 1, 2) - 0.5;
                if self.color.range == ColorRange::Limited {
                    yv = (yv - 16.0 / 255.0) * 255.0 / 219.0;
                    u *= 255.0 / 224.0;
                    v *= 255.0 / 224.0;
                }
                let r = yv + 2.0 * (1.0 - kr) * v;
                let b = yv + 2.0 * (1.0 - kb) * u;
                let g = (yv - kr * r - kb * b) / kg;
                [r, g, b, 1.0]
            }
            PixelLayout::Yuv8 {
                chroma_w,
                chroma_h,
                alpha,
            }
            | PixelLayout::Yuv16 {
                chroma_w,
                chroma_h,
                alpha,
                ..
            } => {
                let (bits, wide) = match self.layout {
                    PixelLayout::Yuv16 { bits, .. } => (bits as u32, true),
                    _ => (8, false),
                };
                let max = ((1u32 << bits) - 1) as f32;
                let sample = |p: &Plane, x: usize, y: usize| -> f32 {
                    let x = x.min(p.width as usize - 1);
                    let y = y.min(p.height as usize - 1);
                    if wide {
                        let i = y * p.bytes_per_row as usize + x * 2;
                        u16::from_le_bytes([p.data[i], p.data[i + 1]]) as f32 / max
                    } else {
                        p.data[y * p.bytes_per_row as usize + x] as f32 / 255.0
                    }
                };
                let (kr, kb) = match self.color.matrix {
                    ColorMatrix::Bt601 => (0.299, 0.114),
                    ColorMatrix::Bt2020 => (0.2627, 0.0593),
                    _ => (0.2126, 0.0722),
                };
                let kg = 1.0 - kr - kb;
                let (cx, cy) = (x >> chroma_w, y >> chroma_h);
                let mut yv = sample(&self.planes[0], x, y);
                let mut u = sample(&self.planes[1], cx, cy) - 0.5;
                let mut v = sample(&self.planes[2], cx, cy) - 0.5;
                if self.color.range == ColorRange::Limited {
                    yv = (yv - 16.0 / 255.0) * 255.0 / 219.0;
                    u *= 255.0 / 224.0;
                    v *= 255.0 / 224.0;
                }
                let r = yv + 2.0 * (1.0 - kr) * v;
                let b = yv + 2.0 * (1.0 - kb) * u;
                let g = (yv - kr * r - kb * b) / kg;
                let a = if alpha {
                    sample(&self.planes[3], x, y)
                } else {
                    1.0
                };
                [r, g, b, a]
            }
        }
    }
}

/// Pixel formats uploaded as-is, and the layout they map to.
fn direct_layout(fmt: Pixel) -> Option<PixelLayout> {
    use Pixel::*;
    let yuv8 = |w, h| {
        Some(PixelLayout::Yuv8 {
            chroma_w: w,
            chroma_h: h,
            alpha: false,
        })
    };
    let yuv16 = |w, h, bits| {
        Some(PixelLayout::Yuv16 {
            chroma_w: w,
            chroma_h: h,
            alpha: false,
            bits,
        })
    };
    match fmt {
        NV12 => Some(PixelLayout::Nv12 { wide: false }),
        P010LE | P016LE => Some(PixelLayout::Nv12 { wide: true }),
        YUV420P | YUVJ420P => yuv8(1, 1),
        YUV422P | YUVJ422P => yuv8(1, 0),
        YUV444P | YUVJ444P => yuv8(0, 0),
        YUV411P => yuv8(2, 0),
        YUV440P | YUVJ440P => yuv8(0, 1),
        YUVA420P => Some(PixelLayout::Yuv8 {
            chroma_w: 1,
            chroma_h: 1,
            alpha: true,
        }),
        YUVA422P => Some(PixelLayout::Yuv8 {
            chroma_w: 1,
            chroma_h: 0,
            alpha: true,
        }),
        YUVA444P => Some(PixelLayout::Yuv8 {
            chroma_w: 0,
            chroma_h: 0,
            alpha: true,
        }),
        YUV420P10LE => yuv16(1, 1, 10),
        YUV422P10LE => yuv16(1, 0, 10),
        YUV444P10LE => yuv16(0, 0, 10),
        YUV420P12LE => yuv16(1, 1, 12),
        YUV422P12LE => yuv16(1, 0, 12),
        YUV444P12LE => yuv16(0, 0, 12),
        YUV420P16LE => yuv16(1, 1, 16),
        YUV422P16LE => yuv16(1, 0, 16),
        YUV444P16LE => yuv16(0, 0, 16),
        YUVA444P10LE => Some(PixelLayout::Yuv16 {
            chroma_w: 0,
            chroma_h: 0,
            alpha: true,
            bits: 10,
        }),
        YUVA422P10LE => Some(PixelLayout::Yuv16 {
            chroma_w: 1,
            chroma_h: 0,
            alpha: true,
            bits: 10,
        }),
        RGBA => Some(PixelLayout::Rgba8),
        RGBA64LE => Some(PixelLayout::Rgba16),
        // `Pixel::None` is in scope here, so spell out the option
        _ => Option::None,
    }
}

/// Conversion target for formats that are not uploaded directly.
fn conversion_target(fmt: Pixel) -> Pixel {
    use Pixel::*;
    match fmt {
        NV12 | NV21 => YUV420P,
        NV16 => YUV422P,
        P010LE | P010BE | P016LE | P016BE => YUV420P16LE,
        P210LE | P216LE => YUV422P16LE,
        YUV420P10BE | YUV420P12BE | YUV420P9LE | YUV420P9BE | YUV420P14LE => YUV420P16LE,
        YUV422P10BE | YUV422P12BE | YUV422P9LE | YUV422P14LE => YUV422P16LE,
        YUV444P10BE | YUV444P12BE | YUV444P9LE | YUV444P14LE => YUV444P16LE,
        YUYV422 | UYVY422 => YUV422P,
        RGB48LE | RGB48BE | RGBA64BE | BGR48LE | BGRA64LE | GBRP10LE | GBRP12LE | GBRP16LE
        | GBRAP10LE | GBRAP12LE | GBRAP16LE | GRAY16LE | GRAY10LE | GRAY12LE | GBRPF32LE
        | GBRAPF32LE => RGBA64LE,
        _ => RGBA,
    }
}

fn copy_planes(frame: &ff::frame::Video, layout: PixelLayout) -> Vec<Plane> {
    let (w, h) = (frame.width(), frame.height());
    let mut planes = Vec::new();
    let specs: Vec<(u32, u32, u32)> = match layout {
        PixelLayout::Yuv8 {
            chroma_w,
            chroma_h,
            alpha,
        }
        | PixelLayout::Yuv16 {
            chroma_w,
            chroma_h,
            alpha,
            ..
        } => {
            let bytes = if matches!(layout, PixelLayout::Yuv16 { .. }) {
                2
            } else {
                1
            };
            let cw = w.div_ceil(1 << chroma_w);
            let ch = h.div_ceil(1 << chroma_h);
            let mut v = vec![(w, h, bytes), (cw, ch, bytes), (cw, ch, bytes)];
            if alpha {
                v.push((w, h, bytes));
            }
            v
        }
        PixelLayout::Nv12 { wide } => {
            let b = if wide { 2 } else { 1 };
            vec![(w, h, b), (w.div_ceil(2), h.div_ceil(2), 2 * b)]
        }
        PixelLayout::Rgba8 => vec![(w, h, 4)],
        PixelLayout::Rgba16 => vec![(w, h, 8)],
    };
    for (i, (pw, ph, bpp)) in specs.into_iter().enumerate() {
        let stride = frame.stride(i);
        let src = frame.data(i);
        let row = (pw * bpp) as usize;
        let mut data = vec![0u8; row * ph as usize];
        for y in 0..ph as usize {
            let s = y * stride;
            if s + row <= src.len() {
                data[y * row..][..row].copy_from_slice(&src[s..s + row]);
            }
        }
        planes.push(Plane {
            data,
            width: pw,
            height: ph,
            bytes_per_row: row as u32,
        });
    }
    planes
}

/// Hardware decoding devices tried in order on this platform.
fn hw_device_types() -> &'static [ff::ffi::AVHWDeviceType] {
    use ff::ffi::AVHWDeviceType::*;
    if cfg!(windows) {
        &[AV_HWDEVICE_TYPE_D3D11VA, AV_HWDEVICE_TYPE_DXVA2]
    } else if cfg!(target_os = "macos") {
        &[AV_HWDEVICE_TYPE_VIDEOTOOLBOX]
    } else if vaapi_installed() {
        &[AV_HWDEVICE_TYPE_VAAPI]
    } else {
        &[]
    }
}

/// The packaged FFmpeg loads libva on first use and aborts the whole process when it is
/// missing, so VAAPI is only tried when its libraries can be loaded.
#[cfg(target_os = "linux")]
fn vaapi_installed() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let found = [c"libva.so.2", c"libva-drm.so.2", c"libva-x11.so.2"]
            .iter()
            .all(|name| {
                // SAFETY: dlopen with a valid C string; the handle stays loaded on purpose.
                !unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) }.is_null()
            });
        if !found {
            log::info!("VAAPI libraries not installed: hardware decoding uses the processor");
        }
        found
    })
}

#[cfg(not(target_os = "linux"))]
fn vaapi_installed() -> bool {
    false
}

/// Chooses the hardware surface format picked at open time (kept in `opaque`), else lets
/// FFmpeg fall back to a software format.
unsafe extern "C" fn pick_hw_format(
    ctx: *mut ff::ffi::AVCodecContext,
    fmts: *const ff::ffi::AVPixelFormat,
) -> ff::ffi::AVPixelFormat {
    // SAFETY: FFmpeg passes a valid context and a list terminated by AV_PIX_FMT_NONE.
    unsafe {
        let want = (*ctx).opaque as isize as i32;
        let mut p = fmts;
        while *p != ff::ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            if *p as i32 == want {
                return *p;
            }
            p = p.add(1);
        }
        ff::ffi::avcodec_default_get_format(ctx, fmts)
    }
}

/// Sets up hardware decoding on a codec context that is not open yet. Returns the device name.
fn attach_hardware(
    ctx: &mut ff::codec::context::Context,
    id: ff::codec::Id,
) -> Option<&'static str> {
    use ff::ffi;
    // SAFETY: the context is not open; FFmpeg owns the device reference we hand over.
    unsafe {
        let codec = ffi::avcodec_find_decoder(id.into());
        if codec.is_null() {
            return None;
        }
        for &kind in hw_device_types() {
            let mut fmt = None;
            for i in 0.. {
                let cfg = ffi::avcodec_get_hw_config(codec, i);
                if cfg.is_null() {
                    break;
                }
                let by_device =
                    (*cfg).methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0;
                if by_device && (*cfg).device_type == kind {
                    fmt = Some((*cfg).pix_fmt);
                    break;
                }
            }
            let Some(fmt) = fmt else { continue };
            let mut dev: *mut ffi::AVBufferRef = std::ptr::null_mut();
            if ffi::av_hwdevice_ctx_create(
                &mut dev,
                kind,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            ) < 0
            {
                continue;
            }
            let raw = ctx.as_mut_ptr();
            (*raw).hw_device_ctx = ffi::av_buffer_ref(dev);
            ffi::av_buffer_unref(&mut dev);
            (*raw).opaque = fmt as i32 as isize as *mut std::ffi::c_void;
            (*raw).get_format = Some(pick_hw_format);
            let name = ffi::av_hwdevice_get_type_name(kind);
            return Some(if name.is_null() {
                "hardware"
            } else {
                match std::ffi::CStr::from_ptr(name).to_str() {
                    Ok("d3d11va") => "d3d11va",
                    Ok("dxva2") => "dxva2",
                    Ok("vaapi") => "vaapi",
                    Ok("videotoolbox") => "videotoolbox",
                    _ => "hardware",
                }
            });
        }
        None
    }
}

pub struct VideoDecoder {
    /// The hardware device decoding this stream, if any.
    hardware: Option<&'static str>,
    input: ff::format::context::Input,
    decoder: ff::decoder::Video,
    stream: usize,
    time_base: f64,
    start_pts: i64,
    rate: Rate,
    color: ColorInfo,
    scaler: Option<(Pixel, u32, u32, ff::software::scaling::Context)>,
    /// Index of the last frame handed to the caller or held in `ahead`.
    last: Option<i64>,
    /// A decoded frame past the last request, kept for the next sequential request.
    ahead: Option<VideoFrame>,
    held: Option<VideoFrame>,
    still: Option<VideoFrame>,
    is_still: bool,
    eof: bool,
    frames: i64,
}

impl VideoDecoder {
    /// Opens the first video stream (or `stream`). `rate` overrides the file's frame rate
    /// (Interpret Footage). `color` is the stream's color description from probing.
    pub fn open(
        path: &Path,
        stream: Option<usize>,
        rate: Option<Rate>,
        color: ColorInfo,
        is_still: bool,
    ) -> Result<VideoDecoder> {
        Self::open_with(path, stream, rate, color, is_still, false)
    }

    /// Like `open`; `hardware` tries the platform's hardware decoder first (D3D11VA/DXVA2,
    /// VAAPI or VideoToolbox) and silently uses software decoding where it is not available.
    pub fn open_with(
        path: &Path,
        stream: Option<usize>,
        rate: Option<Rate>,
        color: ColorInfo,
        is_still: bool,
        hardware: bool,
    ) -> Result<VideoDecoder> {
        init();
        let input = ff::format::input(path)?;
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
        let file_rate = frame_rate(&st);
        let frames = st.frames();
        let _ = codec_params(&st);
        let mut ctx = ff::codec::context::Context::from_parameters(st.parameters())?;
        let hw = if hardware && !is_still {
            attach_hardware(&mut ctx, st.parameters().id())
        } else {
            None
        };
        ctx.set_threading(ff::threading::Config {
            kind: ff::threading::Type::Frame,
            // hardware decoders work frame by frame; more threads only add latency
            count: if hw.is_some() { 1 } else { 0 },
        });
        let decoder = ctx.decoder().video()?;
        Ok(VideoDecoder {
            hardware: hw,
            input,
            decoder,
            stream: index,
            time_base,
            start_pts,
            rate: rate.unwrap_or(file_rate),
            color,
            scaler: None,
            last: None,
            ahead: None,
            held: None,
            still: None,
            is_still,
            eof: false,
            frames,
        })
    }

    pub fn rate(&self) -> Rate {
        self.rate
    }

    /// The hardware device in use ("d3d11va", "vaapi", ...), or None for software decoding.
    pub fn hardware(&self) -> Option<&'static str> {
        self.hardware
    }

    fn index_of(&self, pts: i64) -> i64 {
        let secs = (pts - self.start_pts) as f64 * self.time_base;
        (secs * self.rate.as_f64() + 1e-3).floor() as i64
    }

    fn convert(&mut self, frame: &ff::frame::Video, index: i64) -> Result<VideoFrame> {
        let fmt = frame.format();
        let (layout, planes) = match direct_layout(fmt) {
            Some(layout) => (layout, copy_planes(frame, layout)),
            None => {
                let target = conversion_target(fmt);
                let (w, h) = (frame.width(), frame.height());
                let rebuild = !matches!(&self.scaler, Some((f, sw, sh, _)) if *f == fmt && *sw == w && *sh == h);
                if rebuild {
                    let ctx = ff::software::scaling::Context::get(
                        fmt,
                        w,
                        h,
                        target,
                        w,
                        h,
                        ff::software::scaling::Flags::POINT,
                    )?;
                    self.scaler = Some((fmt, w, h, ctx));
                }
                let mut out = ff::frame::Video::empty();
                self.scaler.as_mut().unwrap().3.run(frame, &mut out)?;
                let layout = direct_layout(target).unwrap();
                (layout, copy_planes(&out, layout))
            }
        };
        let mut color = self.color;
        if matches!(layout, PixelLayout::Rgba8 | PixelLayout::Rgba16) {
            color.matrix = ColorMatrix::Rgb;
            color.range = ColorRange::Full;
        }
        Ok(VideoFrame {
            width: frame.width(),
            height: frame.height(),
            layout,
            planes,
            color,
            index,
        })
    }

    /// Decodes the next frame in decode order, or None at the end of the stream.
    fn decode_next(&mut self) -> Result<Option<VideoFrame>> {
        let mut raw = ff::frame::Video::empty();
        loop {
            match self.decoder.receive_frame(&mut raw) {
                Ok(()) => {
                    let pts = raw.timestamp().or(raw.pts()).unwrap_or(self.start_pts);
                    let idx = self.index_of(pts);
                    // SAFETY: reading a field of a frame we own
                    let on_device = unsafe { !(*raw.as_ptr()).hw_frames_ctx.is_null() };
                    if on_device {
                        // copy the surface to memory (NV12, P010, ...)
                        let mut sw = ff::frame::Video::empty();
                        // SAFETY: both frames are valid; FFmpeg allocates the destination
                        let r = unsafe {
                            ff::ffi::av_hwframe_transfer_data(sw.as_mut_ptr(), raw.as_ptr(), 0)
                        };
                        if r < 0 {
                            return Err(MediaError::Unsupported(
                                "the hardware decoder could not hand over a frame".into(),
                            ));
                        }
                        return self.convert(&sw, idx).map(Some);
                    }
                    return self.convert(&raw, idx).map(Some);
                }
                Err(ff::Error::Eof) => return Ok(None),
                Err(ff::Error::Other { errno }) if errno == ff::util::error::EAGAIN => {}
                Err(e) => return Err(e.into()),
            }
            if self.eof {
                return Ok(None);
            }
            // feed one packet of our stream
            let mut fed = false;
            let mut packet = ff::Packet::empty();
            loop {
                match packet.read(&mut self.input) {
                    Ok(()) => {
                        if packet.stream() == self.stream {
                            self.decoder.send_packet(&packet)?;
                            fed = true;
                            break;
                        }
                    }
                    Err(ff::Error::Eof) => break,
                    Err(e) => return Err(e.into()),
                }
            }
            if !fed {
                self.eof = true;
                let _ = self.decoder.send_eof();
            }
        }
    }

    fn seek_to(&mut self, index: i64, back_off: f64) -> Result {
        let secs = (index as f64 / self.rate.as_f64() - back_off).max(0.0);
        let start_us = (self.start_pts as f64 * self.time_base * 1_000_000.0) as i64;
        let ts = (secs * 1_000_000.0) as i64 + start_us;
        self.input.seek(ts, ..ts)?;
        self.decoder.flush();
        self.eof = false;
        self.ahead = None;
        self.last = None;
        Ok(())
    }

    /// The frame shown at `index` (the latest frame whose index is not after it).
    pub fn frame(&mut self, index: i64) -> Result<VideoFrame> {
        let index = index.max(0);
        if self.is_still {
            if self.still.is_none() {
                self.seek_to(0, 0.0).ok();
                let f = self
                    .decode_next()?
                    .ok_or_else(|| MediaError::Unsupported("the image has no picture".into()))?;
                self.still = Some(f);
            }
            let mut f = self.still.clone().unwrap();
            f.index = index;
            return Ok(f);
        }
        if let Some(h) = &self.held
            && h.index == index
        {
            return Ok(h.clone());
        }
        let sequential = matches!(self.last, Some(l) if index >= l && index - l <= (self.rate.as_f64() * 2.0) as i64);
        if !sequential {
            self.seek_to(index, 0.0)?;
        }
        let mut back_off = 0.0;
        for _attempt in 0..6 {
            let mut prev: Option<VideoFrame> = None;
            if let Some(a) = self.ahead.take() {
                if a.index > index {
                    // the next frame is already past the target: the held one is still shown
                    if let Some(h) = &self.held
                        && h.index <= index
                    {
                        self.ahead = Some(a);
                        return Ok(h.clone());
                    }
                    self.ahead = Some(a);
                } else {
                    prev = Some(a);
                }
            }
            let mut first_after_seek = prev.is_none() && self.last.is_none();
            loop {
                if let Some(p) = &prev
                    && p.index == index
                {
                    break;
                }
                match self.decode_next()? {
                    Some(f) => {
                        if first_after_seek && f.index > index && index > 0 {
                            // the seek landed after the target: back off further
                            break;
                        }
                        first_after_seek = false;
                        if f.index > index {
                            self.ahead = Some(f);
                            break;
                        }
                        prev = Some(f);
                    }
                    None => break,
                }
            }
            if let Some(p) = prev {
                self.last = Some(p.index);
                self.held = Some(p.clone());
                return Ok(p);
            }
            if self.eof && self.ahead.is_none() {
                // past the end: show the last frame
                if index > 0 && self.frames > 0 && back_off < 5.0 {
                    back_off += 1.0;
                    self.seek_to(index.min(self.frames - 1), back_off)?;
                    continue;
                }
                if let Some(h) = &self.held {
                    return Ok(h.clone());
                }
                return Err(MediaError::Unsupported("no frame".into()));
            }
            back_off = if back_off == 0.0 { 1.0 } else { back_off * 2.0 };
            self.seek_to(index, back_off)?;
        }
        if let Some(a) = self.ahead.clone() {
            self.held = Some(a.clone());
            return Ok(a);
        }
        Err(MediaError::Unsupported(format!(
            "frame {index} could not be decoded"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuv_to_rgb_is_exact_for_primaries() {
        // BT.709 limited: pure red is Y=63 U=102 V=240
        let plane = |v: u8| Plane {
            data: vec![v; 4],
            width: 2,
            height: 2,
            bytes_per_row: 2,
        };
        let f = VideoFrame {
            width: 2,
            height: 2,
            layout: PixelLayout::Yuv8 {
                chroma_w: 0,
                chroma_h: 0,
                alpha: false,
            },
            planes: vec![plane(63), plane(102), plane(240)],
            color: ColorInfo::default(),
            index: 0,
        };
        let rgba = f.to_rgba8();
        assert!(
            rgba[0] >= 250 && rgba[1] <= 5 && rgba[2] <= 5,
            "{:?}",
            &rgba[..4]
        );
    }
}
