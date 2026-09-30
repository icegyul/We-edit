//! Playback engine: a thread that mixes ahead of the device and provides the master clock.
//!
//! The playhead position comes from the frames the device has actually played, so video follows
//! audio (architecture 6). Edits made during playback reach the engine as new project
//! snapshots.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use op_core::*;
use parking_lot::Mutex;

use crate::device::{Output, PlayedCounter};
use crate::mixer::{AudioSource, Meters, Mixer};

const BLOCK: usize = 512;

enum Cmd {
    Play {
        project: Arc<Project>,
        sequence: SequenceId,
        from: SeqTime,
        speed: f64,
    },
    Update(Arc<Project>),
    Stop,
    Blip {
        project: Arc<Project>,
        sequence: SequenceId,
        at: SeqTime,
        len: Dur,
    },
    Quit,
}

#[derive(Clone, Copy, Debug)]
struct Clock {
    start: SeqTime,
    anchor: u64,
    speed: f64,
    rate: u32,
}

struct Shared {
    playing: AtomicBool,
    /// Set by `play` until the engine thread has started playing.
    starting: AtomicBool,
    clock: Mutex<Clock>,
    counter: Mutex<Option<PlayedCounter>>,
    info: Mutex<(String, u32, u64)>,
}

pub struct Playback {
    tx: std::sync::mpsc::Sender<Cmd>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    pub meters: Arc<Meters>,
}

impl Playback {
    /// Starts the engine on the default output device.
    pub fn start(source: Arc<dyn AudioSource>) -> Playback {
        Self::with_output(source, Output::open)
    }

    /// Starts the engine with a silent clock (no device).
    pub fn silent(source: Arc<dyn AudioSource>) -> Playback {
        Self::with_output(source, || Output::null(48000, 2))
    }

    fn with_output(
        source: Arc<dyn AudioSource>,
        open: impl FnOnce() -> Output + Send + 'static,
    ) -> Playback {
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let shared = Arc::new(Shared {
            playing: AtomicBool::new(false),
            starting: AtomicBool::new(false),
            clock: Mutex::new(Clock {
                start: SeqTime::ZERO,
                anchor: 0,
                speed: 1.0,
                rate: 48000,
            }),
            counter: Mutex::new(None),
            info: Mutex::new((String::new(), 48000, 0)),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Arc<Meters>>();
        let sh = shared.clone();
        let thread = std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || {
                let mut out = open();
                let mut mixer = Mixer::new(out.rate, 2);
                let _ = ready_tx.send(mixer.meters.clone());
                *sh.info.lock() = (out.device_name.clone(), out.rate, 0);
                *sh.counter.lock() = Some(out.counter());
                let mut project: Option<Arc<Project>> = None;
                let mut sequence = SequenceId(0);
                let mut next = SeqTime::ZERO;
                let mut speed = 1.0;
                let mut stereo = vec![0f32; BLOCK * 2];
                let mut device = vec![0f32; BLOCK * out.channels];
                loop {
                    let playing = sh.playing.load(Ordering::Acquire);
                    let wait = if playing {
                        Duration::from_millis(2)
                    } else {
                        Duration::from_millis(50)
                    };
                    match rx.recv_timeout(wait) {
                        Ok(Cmd::Play {
                            project: p,
                            sequence: s,
                            from,
                            speed: sp,
                        }) => {
                            out.flush();
                            mixer.reset();
                            project = Some(p);
                            sequence = s;
                            next = from;
                            speed = sp;
                            *sh.clock.lock() = Clock {
                                start: from,
                                anchor: out.played_frames(),
                                speed: sp,
                                rate: out.rate,
                            };
                            sh.playing.store(true, Ordering::Release);
                            sh.starting.store(false, Ordering::Release);
                        }
                        Ok(Cmd::Update(p)) => project = Some(p),
                        Ok(Cmd::Stop) => {
                            sh.playing.store(false, Ordering::Release);
                            out.flush();
                        }
                        Ok(Cmd::Blip {
                            project: p,
                            sequence: s,
                            at,
                            len,
                        }) => {
                            if !sh.playing.load(Ordering::Acquire) {
                                out.flush();
                                let frames = ((len.seconds() * out.rate as f64) as usize)
                                    .clamp(1, out.rate as usize / 4);
                                let mut buf = vec![0f32; frames * 2];
                                mixer.reset();
                                let mixed =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        mixer.render(&p, s, at, 1.0, &mut buf, &*source)
                                    }));
                                if mixed.is_err() {
                                    log::error!("audio scrubbing failed at {:.3} s", at.seconds());
                                    buf.fill(0.0);
                                    mixer.reset();
                                }
                                // short fades avoid clicks
                                let fade = frames.min(64);
                                for i in 0..fade {
                                    let g = i as f32 / fade as f32;
                                    buf[i * 2] *= g;
                                    buf[i * 2 + 1] *= g;
                                    let j = frames - 1 - i;
                                    buf[j * 2] *= g;
                                    buf[j * 2 + 1] *= g;
                                }
                                let mut dev = vec![0f32; frames * out.channels];
                                to_device(&buf, &mut dev, out.channels);
                                out.write(&dev);
                            }
                        }
                        Ok(Cmd::Quit) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if sh.playing.load(Ordering::Acquire)
                        && let Some(p) = &project
                    {
                        while out.free_frames() >= BLOCK {
                            // a failure while mixing one block plays silence, never stops the engine
                            let mixed =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    mixer.render(p, sequence, next, speed, &mut stereo, &*source)
                                }));
                            if mixed.is_err() {
                                log::error!(
                                    "audio mixing failed at {:.3} s; muted this block",
                                    next.seconds()
                                );
                                stereo.fill(0.0);
                                mixer.reset();
                            }
                            to_device(&stereo, &mut device, out.channels);
                            out.write(&device);
                            next += Dur::from_seconds(BLOCK as f64 * speed / out.rate as f64);
                        }
                    }
                    sh.info.lock().2 = out.underruns();
                }
            })
            .expect("playback thread");
        let meters = ready_rx.recv().unwrap_or_default();
        Playback {
            tx,
            shared,
            thread: Some(thread),
            meters,
        }
    }

    pub fn play(&self, project: Arc<Project>, sequence: SequenceId, from: SeqTime, speed: f64) {
        // playing from now on, even before the engine thread picks the command up
        self.shared.starting.store(true, Ordering::Release);
        let _ = self.tx.send(Cmd::Play {
            project,
            sequence,
            from,
            speed,
        });
    }

    pub fn update(&self, project: Arc<Project>) {
        let _ = self.tx.send(Cmd::Update(project));
    }

    pub fn stop(&self) {
        self.shared.starting.store(false, Ordering::Release);
        self.shared.playing.store(false, Ordering::Release);
        let _ = self.tx.send(Cmd::Stop);
    }

    /// Plays a short piece of audio at `at` (scrubbing).
    pub fn blip(&self, project: Arc<Project>, sequence: SequenceId, at: SeqTime, len: Dur) {
        let _ = self.tx.send(Cmd::Blip {
            project,
            sequence,
            at,
            len,
        });
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire) || self.shared.starting.load(Ordering::Acquire)
    }

    /// Current playhead derived from played audio, while playing.
    pub fn position(&self) -> Option<SeqTime> {
        if !self.shared.playing.load(Ordering::Acquire)
            || self.shared.starting.load(Ordering::Acquire)
        {
            return None;
        }
        let c = *self.shared.clock.lock();
        let played = self
            .shared
            .counter
            .lock()
            .as_ref()
            .map(|c| c.frames())
            .unwrap_or(0);
        let heard = played.saturating_sub(c.anchor);
        Some(c.start + Dur::from_seconds(heard as f64 * c.speed / c.rate as f64))
    }

    /// (device name, sample rate, underruns)
    pub fn device_info(&self) -> (String, u32, u64) {
        self.shared.info.lock().clone()
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn to_device(stereo: &[f32], out: &mut [f32], channels: usize) {
    let frames = stereo.len() / 2;
    match channels {
        1 => {
            for i in 0..frames {
                out[i] = (stereo[i * 2] + stereo[i * 2 + 1]) * 0.5;
            }
        }
        2 => out[..frames * 2].copy_from_slice(&stereo[..frames * 2]),
        n => {
            out.fill(0.0);
            for i in 0..frames {
                out[i * n] = stereo[i * 2];
                out[i * n + 1] = stereo[i * 2 + 1];
            }
        }
    }
}

/// Mixes a whole range offline (export): interleaved samples at `rate` with `channels`.
pub fn render_range(
    project: &Project,
    sequence: SequenceId,
    range: SeqRange,
    rate: u32,
    channels: usize,
    source: &dyn AudioSource,
    mut sink: impl FnMut(&[f32]) -> bool,
) {
    let mut mixer = Mixer::new(rate, channels);
    let total = Rate::fps(rate).dur_to_frames_round(range.duration()).max(0) as usize;
    let mut done = 0usize;
    let mut buf = vec![0f32; 4096 * channels];
    while done < total {
        let n = (total - done).min(4096);
        let t = range.start + Rate::fps(rate).frames_to_dur(done as i64);
        mixer.render(project, sequence, t, 1.0, &mut buf[..n * channels], source);
        if !sink(&buf[..n * channels]) {
            return;
        }
        done += n;
    }
}
