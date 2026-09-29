//! Audio output. The device callback only copies from a lock-free ring buffer and counts the
//! frames it played; it never allocates, locks, decodes or mixes (architecture 6). Without an
//! output device a clock-driven null output keeps playback running silently.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Frames played so far, readable from any thread.
#[derive(Clone)]
pub enum PlayedCounter {
    Device(Arc<AtomicU64>),
    Clock(Arc<parking_lot::Mutex<Option<Instant>>>, u32),
}

impl PlayedCounter {
    pub fn frames(&self) -> u64 {
        match self {
            PlayedCounter::Device(a) => a.load(Ordering::Acquire),
            PlayedCounter::Clock(start, rate) => start
                .lock()
                .map(|s| (s.elapsed().as_secs_f64() * *rate as f64) as u64)
                .unwrap_or(0),
        }
    }
}

pub struct Output {
    pub rate: u32,
    /// Channels written to the ring (the device's channel count).
    pub channels: usize,
    producer: Option<rtrb::Producer<f32>>,
    played: Arc<AtomicU64>,
    flush: Arc<AtomicBool>,
    underruns: Arc<AtomicU64>,
    /// Keeps the stream thread alive; dropping the sender stops it.
    _stop: Option<std::sync::mpsc::Sender<()>>,
    null_clock: Arc<parking_lot::Mutex<Option<Instant>>>,
    null_written: u64,
    pub device_name: String,
    capacity: usize,
}

impl Output {
    /// Opens the default output device, or a silent clock when there is none.
    pub fn open() -> Output {
        match Self::open_device() {
            Ok(o) => o,
            Err(e) => {
                log::warn!("no audio output ({e}); playing silently");
                Output::null(48000, 2)
            }
        }
    }

    pub fn null(rate: u32, channels: usize) -> Output {
        Output {
            rate,
            channels,
            producer: None,
            played: Arc::new(AtomicU64::new(0)),
            flush: Arc::new(AtomicBool::new(false)),
            underruns: Arc::new(AtomicU64::new(0)),
            _stop: None,
            null_clock: Arc::new(parking_lot::Mutex::new(None)),
            null_written: 0,
            device_name: String::new(),
            capacity: rate as usize / 5,
        }
    }

    fn open_device() -> Result<Output, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no default output device")?;
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_default();
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let format = config.sample_format();
        // about 150 ms of queue
        let capacity = (rate as usize * channels * 15) / 100;
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
        let played = Arc::new(AtomicU64::new(0));
        let flush = Arc::new(AtomicBool::new(false));
        let underruns = Arc::new(AtomicU64::new(0));
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let stream_config: cpal::StreamConfig = config.config();
        let (p2, f2, u2) = (played.clone(), flush.clone(), underruns.clone());
        std::thread::Builder::new()
            .name("audio-output".into())
            .spawn(move || {
                let mut consumer = consumer;
                let ch = channels;
                macro_rules! build {
                    ($t:ty, $conv:expr) => {
                        device.build_output_stream::<$t, _, _>(
                            stream_config,
                            move |data: &mut [$t], _| {
                                if f2.swap(false, Ordering::AcqRel) {
                                    let n = consumer.slots();
                                    if let Ok(chunk) = consumer.read_chunk(n) {
                                        chunk.commit_all();
                                    }
                                }
                                let want = data.len();
                                let avail = consumer.slots().min(want);
                                let avail = avail - avail % ch;
                                let conv = $conv;
                                if let Ok(chunk) = consumer.read_chunk(avail) {
                                    let (a, b) = chunk.as_slices();
                                    for (d, s) in data.iter_mut().zip(a.iter().chain(b.iter())) {
                                        *d = conv(*s);
                                    }
                                    chunk.commit_all();
                                }
                                for d in data[avail..].iter_mut() {
                                    *d = conv(0.0);
                                }
                                if avail < want {
                                    u2.fetch_add(1, Ordering::Relaxed);
                                }
                                p2.fetch_add((avail / ch) as u64, Ordering::Release);
                            },
                            |e| log::warn!("audio stream: {e}"),
                            None,
                        )
                    };
                }
                let stream = match format {
                    cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
                    cpal::SampleFormat::I16 => {
                        build!(i16, |s: f32| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    }
                    cpal::SampleFormat::U16 => build!(
                        u16,
                        |s: f32| ((s.clamp(-1.0, 1.0) * 32767.0) as i32 + 32768) as u16
                    ),
                    cpal::SampleFormat::I32 => build!(i32, |s: f32| (s.clamp(-1.0, 1.0) as f64
                        * 2147483647.0)
                        as i32),
                    f => {
                        let _ = ready_tx.send(Err(format!("unsupported sample format {f:?}")));
                        return;
                    }
                };
                let stream = match stream {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                // keep the stream alive until the output is dropped
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Output {
            rate,
            channels,
            producer: Some(producer),
            played,
            flush,
            underruns,
            _stop: Some(stop_tx),
            null_clock: Arc::new(parking_lot::Mutex::new(None)),
            null_written: 0,
            device_name: name,
            capacity: capacity / channels,
        })
    }

    pub fn is_null(&self) -> bool {
        self.producer.is_none()
    }

    /// Frames that can be queued now.
    pub fn free_frames(&mut self) -> usize {
        match &self.producer {
            Some(p) => p.slots() / self.channels,
            None => {
                let played = self.played_frames();
                (self.capacity as u64).saturating_sub(self.null_written.saturating_sub(played))
                    as usize
            }
        }
    }

    /// Queues interleaved frames in the device layout; returns frames accepted.
    pub fn write(&mut self, samples: &[f32]) -> usize {
        let frames = samples.len() / self.channels;
        match &mut self.producer {
            Some(p) => {
                let n = (p.slots() / self.channels).min(frames);
                if let Ok(mut chunk) = p.write_chunk(n * self.channels) {
                    let (a, b) = chunk.as_mut_slices();
                    let (sa, sb) = samples[..n * self.channels].split_at(a.len());
                    a.copy_from_slice(sa);
                    b.copy_from_slice(sb);
                    chunk.commit_all();
                }
                n
            }
            None => {
                {
                    let mut c = self.null_clock.lock();
                    if c.is_none() {
                        *c = Some(Instant::now());
                    }
                }
                let n = frames.min(self.free_frames());
                self.null_written += n as u64;
                n
            }
        }
    }

    /// Frames the device has played since it was opened.
    pub fn played_frames(&self) -> u64 {
        match &self.producer {
            Some(_) => self.played.load(Ordering::Acquire),
            None => self.counter().frames().min(self.null_written),
        }
    }

    pub fn counter(&self) -> PlayedCounter {
        match &self.producer {
            Some(_) => PlayedCounter::Device(self.played.clone()),
            None => PlayedCounter::Clock(self.null_clock.clone(), self.rate),
        }
    }

    /// Frames queued but not yet played.
    pub fn queued_frames(&self) -> u64 {
        match &self.producer {
            Some(p) => ((p.buffer().capacity() - p.slots()) / self.channels) as u64,
            None => self.null_written.saturating_sub(self.played_frames()),
        }
    }

    /// Drops everything queued (stop, seek).
    pub fn flush(&mut self) {
        if self.producer.is_some() {
            self.flush.store(true, Ordering::Release);
        } else {
            self.null_written = self.played_frames();
        }
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}
