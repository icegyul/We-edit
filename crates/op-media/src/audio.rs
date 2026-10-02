//! Audio conform: every audio stream used in a project is decoded once into a cache file of
//! interleaved 32-bit float samples at the stream's own rate and channel count, with a peak file
//! for waveforms. Playback and export then read samples by index from a memory map, without
//! decoding on the real-time path (architecture 6).

use std::fs;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use ffmpeg_next as ff;
use memmap2::Mmap;
use op_core::Dur;

use crate::{MediaError, Result, init};

const AUDIO_MAGIC: &[u8; 8] = b"OPPAUD01";
const PEAK_MAGIC: &[u8; 8] = b"OPPPEAK1";
const HEADER: usize = 24;
/// Samples per peak block.
pub const PEAK_BLOCK: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioInfo {
    pub rate: u32,
    pub channels: u16,
    pub samples: i64,
}

fn header(info: AudioInfo) -> [u8; HEADER] {
    let mut h = [0u8; HEADER];
    h[..8].copy_from_slice(AUDIO_MAGIC);
    h[8..12].copy_from_slice(&info.rate.to_le_bytes());
    h[12..14].copy_from_slice(&info.channels.to_le_bytes());
    h[16..24].copy_from_slice(&info.samples.to_le_bytes());
    h
}

struct PeakBuilder {
    channels: usize,
    fill: usize,
    cur: Vec<(f32, f32)>,
    out: Vec<i16>,
}

impl PeakBuilder {
    fn new(channels: usize) -> Self {
        PeakBuilder {
            channels,
            fill: 0,
            cur: vec![(0.0, 0.0); channels],
            out: Vec::new(),
        }
    }

    fn push(&mut self, frame: &[f32]) {
        for (c, v) in frame.iter().enumerate().take(self.channels) {
            let e = &mut self.cur[c];
            if self.fill == 0 {
                *e = (*v, *v);
            } else {
                e.0 = e.0.min(*v);
                e.1 = e.1.max(*v);
            }
        }
        self.fill += 1;
        if self.fill == PEAK_BLOCK {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.fill == 0 {
            return;
        }
        let q = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        for c in 0..self.channels {
            let (lo, hi) = self.cur[c];
            self.out.push(q(lo));
            self.out.push(q(hi));
        }
        self.fill = 0;
    }
}

/// Decodes audio stream number `ordinal` (among the file's audio streams) into `out`, with
/// sample 0 at time `align_to` of the container (the first video frame, for A/V files).
/// `progress` receives 0..1 and returns false to cancel.
pub fn conform(
    path: &Path,
    ordinal: usize,
    align_to: Dur,
    out: &Path,
    peaks: &Path,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<AudioInfo> {
    init();
    let mut input = ff::format::input(path)?;
    let (index, time_base, start, duration) = {
        let st = input
            .streams()
            .filter(|s| s.parameters().medium() == ff::media::Type::Audio)
            .nth(ordinal)
            .ok_or_else(|| MediaError::Unsupported("no such audio stream".into()))?;
        let tb = st.time_base();
        let tbf = tb.numerator() as f64 / tb.denominator().max(1) as f64;
        let start = if st.start_time() != ff::ffi::AV_NOPTS_VALUE {
            st.start_time() as f64 * tbf
        } else {
            0.0
        };
        let dur = if st.duration() > 0 {
            st.duration() as f64 * tbf
        } else {
            input.duration() as f64 / 1e6
        };
        (st.index(), tbf, start, dur)
    };
    let _ = time_base;
    let params = input.stream(index).unwrap().parameters();
    let ctx = ff::codec::context::Context::from_parameters(params)?;
    let mut decoder = ctx.decoder().audio()?;
    let rate = decoder.rate();
    let channels = decoder.channel_layout().channels().max(1) as u16;
    if rate == 0 {
        return Err(MediaError::Unsupported(
            "the audio stream has no sample rate".into(),
        ));
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = out.with_extension("part");
    let mut file = BufWriter::with_capacity(1 << 20, fs::File::create(&tmp)?);
    let mut info = AudioInfo {
        rate,
        channels,
        samples: 0,
    };
    file.write_all(&header(info))?;
    let mut peaks_b = PeakBuilder::new(channels as usize);

    // leading silence or skipped samples so sample 0 lines up with `align_to`
    let offset = ((start - align_to.seconds()) * rate as f64).round() as i64;
    let mut skip = (-offset).max(0);
    let silence = offset.max(0);
    let zero = vec![0f32; channels as usize];
    for _ in 0..silence {
        for z in &zero {
            file.write_all(&z.to_le_bytes())?;
        }
        peaks_b.push(&zero);
    }
    info.samples += silence;

    let target_layout = ff::ChannelLayout::default(channels as i32);
    let mut resampler: Option<(
        ff::format::Sample,
        u64,
        u32,
        ff::software::resampling::Context,
    )> = None;
    let mut decoded = ff::frame::Audio::empty();
    let mut converted = ff::frame::Audio::empty();
    let mut packet = ff::Packet::empty();
    let mut last_report = 0.0f32;
    let mut done_reading = false;
    let expected = (duration * rate as f64).max(1.0);
    loop {
        // drain decoded frames
        loop {
            match decoder.receive_frame(&mut decoded) {
                Ok(()) => {}
                Err(ff::Error::Eof) => {
                    done_reading = true;
                    break;
                }
                Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => break,
                Err(e) => return Err(e.into()),
            }
            let fmt = decoded.format();
            // files that do not name their channel layout (many WAV writers) decode with an
            // unspecified one; the frames must carry the layout the resampler is made for, or
            // every conversion fails with "Input changed"
            let mut src_layout = decoded.channel_layout();
            if src_layout.channels() != decoded.channels() as i32 || src_layout.is_empty() {
                src_layout = ff::ChannelLayout::default(decoded.channels() as i32);
                decoded.set_channel_layout(src_layout);
            }
            let layout_bits = src_layout.bits();
            let frate = decoded.rate();
            let rebuild = !matches!(&resampler, Some((f, l, r, _)) if *f == fmt && *l == layout_bits && *r == frate);
            if rebuild {
                let ctx = ff::software::resampling::Context::get(
                    fmt,
                    src_layout,
                    frate.max(1),
                    ff::format::Sample::F32(ff::format::sample::Type::Packed),
                    target_layout,
                    rate,
                )?;
                resampler = Some((fmt, layout_bits, frate, ctx));
            }
            resampler
                .as_mut()
                .unwrap()
                .3
                .run(&decoded, &mut converted)?;
            let n = converted.samples();
            let data = converted.data(0);
            let bytes_per_frame = channels as usize * 4;
            let mut frame = vec![0f32; channels as usize];
            for raw in data[..(n * bytes_per_frame).min(data.len())].chunks_exact(bytes_per_frame) {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                file.write_all(raw)?;
                for (v, b) in frame.iter_mut().zip(raw.as_chunks::<4>().0) {
                    *v = f32::from_le_bytes(*b);
                }
                peaks_b.push(&frame);
                info.samples += 1;
            }
            let p = (info.samples as f64 / expected).min(1.0) as f32;
            if p - last_report > 0.01 {
                last_report = p;
                if !progress(p) {
                    drop(file);
                    let _ = fs::remove_file(&tmp);
                    return Err(MediaError::Cancelled);
                }
            }
        }
        if done_reading {
            break;
        }
        // feed
        let mut fed = false;
        loop {
            match packet.read(&mut input) {
                Ok(()) => {
                    if packet.stream() == index {
                        // corrupt packets are skipped rather than failing the whole conform
                        if decoder.send_packet(&packet).is_ok() {
                            fed = true;
                        }
                        break;
                    }
                }
                Err(ff::Error::Eof) => break,
                Err(e) => return Err(e.into()),
            }
        }
        if !fed {
            let _ = decoder.send_eof();
        }
    }
    peaks_b.flush();
    let mut f = file.into_inner().map_err(|e| e.into_error())?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&header(info))?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, out)?;

    let mut pf = BufWriter::new(fs::File::create(peaks.with_extension("part"))?);
    pf.write_all(PEAK_MAGIC)?;
    pf.write_all(&(PEAK_BLOCK as u32).to_le_bytes())?;
    pf.write_all(&channels.to_le_bytes())?;
    pf.write_all(&[0, 0])?;
    let blocks = (peaks_b.out.len() / (2 * channels as usize)) as u64;
    pf.write_all(&blocks.to_le_bytes())?;
    for v in &peaks_b.out {
        pf.write_all(&v.to_le_bytes())?;
    }
    pf.flush()?;
    drop(pf);
    fs::rename(peaks.with_extension("part"), peaks)?;
    progress(1.0);
    Ok(info)
}

/// Writes interleaved samples as a cache file (generated audio, tests).
pub fn write_cache(path: &Path, rate: u32, channels: u16, samples: &[f32]) -> Result<AudioInfo> {
    let info = AudioInfo {
        rate,
        channels,
        samples: (samples.len() / channels.max(1) as usize) as i64,
    };
    let mut f = BufWriter::new(fs::File::create(path)?);
    f.write_all(&header(info))?;
    for v in samples {
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()?;
    Ok(info)
}

/// A conformed audio cache file.
pub struct ConformedAudio {
    map: Mmap,
    pub info: AudioInfo,
}

impl ConformedAudio {
    pub fn open(path: &Path) -> Result<ConformedAudio> {
        let file = fs::File::open(path)?;
        // SAFETY: cache files are written once, renamed into place and never modified afterwards.
        let map = unsafe { Mmap::map(&file)? };
        if map.len() < HEADER || &map[..8] != AUDIO_MAGIC {
            return Err(MediaError::Unsupported("not an audio cache file".into()));
        }
        let rate = u32::from_le_bytes(map[8..12].try_into().unwrap());
        let channels = u16::from_le_bytes(map[12..14].try_into().unwrap());
        let samples = i64::from_le_bytes(map[16..24].try_into().unwrap());
        let available = ((map.len() - HEADER) / (4 * channels.max(1) as usize)) as i64;
        Ok(ConformedAudio {
            map,
            info: AudioInfo {
                rate,
                channels,
                samples: samples.min(available),
            },
        })
    }

    /// Copies interleaved samples `[start, start + frames)` into `out`; outside the file reads
    /// silence.
    pub fn read(&self, start: i64, out: &mut [f32]) {
        let ch = self.info.channels.max(1) as usize;
        let frames = out.len() / ch;
        out.fill(0.0);
        let s0 = start.max(0);
        let s1 = (start + frames as i64).min(self.info.samples);
        if s1 <= s0 {
            return;
        }
        let bytes = &self.map[HEADER + s0 as usize * ch * 4..HEADER + s1 as usize * ch * 4];
        let dst = &mut out[(s0 - start) as usize * ch..][..(s1 - s0) as usize * ch];
        for (d, b) in dst.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *d = f32::from_le_bytes(*b);
        }
    }

    /// One sample of one channel.
    pub fn sample(&self, index: i64, channel: usize) -> f32 {
        let ch = self.info.channels.max(1) as usize;
        if index < 0 || index >= self.info.samples || channel >= ch {
            return 0.0;
        }
        let at = HEADER + (index as usize * ch + channel) * 4;
        f32::from_le_bytes(self.map[at..at + 4].try_into().unwrap())
    }
}

/// Waveform peaks: min/max per channel per `PEAK_BLOCK` samples.
#[derive(Clone, Debug, Default)]
pub struct Peaks {
    pub channels: usize,
    pub blocks: usize,
    data: Vec<i16>,
}

impl Peaks {
    pub fn open(path: &Path) -> Result<Peaks> {
        let bytes = fs::read(path)?;
        if bytes.len() < 24 || &bytes[..8] != PEAK_MAGIC {
            return Err(MediaError::Unsupported("not a peak file".into()));
        }
        let channels = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
        let blocks = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        let data: Vec<i16> = bytes[24..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        let blocks = blocks.min(data.len() / (2 * channels.max(1)));
        Ok(Peaks {
            channels,
            blocks,
            data,
        })
    }

    /// (min, max) of a channel over samples `[s0, s1)`.
    pub fn range(&self, channel: usize, s0: i64, s1: i64) -> (f32, f32) {
        if self.channels == 0 || channel >= self.channels {
            return (0.0, 0.0);
        }
        let b0 = (s0.max(0) as usize) / PEAK_BLOCK;
        let b1 = ((s1.max(0) as usize).div_ceil(PEAK_BLOCK))
            .min(self.blocks)
            .max(b0 + 1)
            .min(self.blocks);
        if b0 >= self.blocks {
            return (0.0, 0.0);
        }
        let (mut lo, mut hi) = (i16::MAX, i16::MIN);
        for b in b0..b1 {
            let i = (b * self.channels + channel) * 2;
            lo = lo.min(self.data[i]);
            hi = hi.max(self.data[i + 1]);
        }
        (lo as f32 / 32767.0, hi as f32 / 32767.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain PCM WAV (mono, 16-bit) has no channel layout; it must conform like any file.
    #[test]
    fn wav_without_a_channel_layout_conforms() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("speech.wav");
        let rate = 16_000u32;
        let samples: Vec<i16> = (0..rate)
            .map(|i| ((i as f32 * 0.07).sin() * 12_000.0) as i16)
            .collect();
        let data_len = samples.len() as u32 * 2;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for s in &samples {
            b.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(&wav, b).unwrap();
        let info = conform(
            &wav,
            0,
            Dur::ZERO,
            &dir.path().join("a.pcm"),
            &dir.path().join("a.peaks"),
            &mut |_| true,
        )
        .expect("conformed");
        assert!(info.samples >= 15_000, "{info:?}");
        let audio = ConformedAudio::open(&dir.path().join("a.pcm")).unwrap();
        assert!(audio.info.samples >= 15_000);
    }
}
