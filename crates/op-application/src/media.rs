//! Media service: probing, audio conform and peaks, decoded frame cache with one decoder thread
//! per active asset, and thumbnails. Nothing here blocks the UI thread unless a caller asks to
//! wait (export).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use op_core::*;
use op_media::thumb::Image;
use op_media::{ConformedAudio, Peaks, VideoDecoder, VideoFrame};
use parking_lot::{Condvar, Mutex};

type AssetMap = HashMap<AssetId, Arc<MediaAsset>>;

/// Frames decoded in order (and cached) to reach a target this close after the last one.
const GAP_FILL: i64 = 32;
/// Proxies decode under their own id, so their smaller frames never mix with the original's.
const PROXY_BIT: u64 = 1 << 62;
/// Requests this close to the previous one tell the playing direction (a reversed clip asks
/// for earlier source frames while the sequence plays forward).
const STEER_RANGE: i64 = 8;

/// Frames decoded ahead of the play position (about half a second at common rates).
pub const PLAYBACK_AHEAD: i64 = 12;

/// How video is decoded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HardwareDecoding {
    /// Software first; a stream that the processor cannot decode in real time moves to the
    /// graphics card's decoder.
    Auto,
    /// The graphics card's decoder whenever it supports the stream.
    Always,
    Never,
}

impl HardwareDecoding {
    pub fn from_pref(s: &str) -> HardwareDecoding {
        match s {
            "always" => HardwareDecoding::Always,
            "never" => HardwareDecoding::Never,
            _ => HardwareDecoding::Auto,
        }
    }

    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(c: u8) -> HardwareDecoding {
        match c {
            1 => HardwareDecoding::Always,
            2 => HardwareDecoding::Never,
            _ => HardwareDecoding::Auto,
        }
    }
}

/// Sequential frames timed before Auto decides whether software decoding keeps up.
const SPEED_SAMPLE: usize = 24;

/// Identity of media content for caches: path, size and modification time.
fn content_key(a: &MediaAsset, stream: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    a.path.hash(&mut h);
    a.file_size.hash(&mut h);
    a.modified_unix.hash(&mut h);
    stream.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[derive(Clone)]
enum AudioEntry {
    Pending,
    Ready(Arc<ConformedAudio>, Option<Arc<Peaks>>),
    Failed(String),
}

struct FrameCache {
    frames: HashMap<(AssetId, i64), Arc<VideoFrame>>,
    order: VecDeque<(AssetId, i64)>,
    bytes: usize,
    budget: usize,
}

impl FrameCache {
    fn get(&mut self, key: (AssetId, i64)) -> Option<Arc<VideoFrame>> {
        self.frames.get(&key).cloned()
    }

    fn insert(&mut self, key: (AssetId, i64), f: Arc<VideoFrame>) {
        if self.frames.contains_key(&key) {
            return;
        }
        self.bytes += f.byte_size();
        self.frames.insert(key, f);
        self.order.push_back(key);
        while self.bytes > self.budget && self.order.len() > 4 {
            if let Some(old) = self.order.pop_front()
                && let Some(f) = self.frames.remove(&old)
            {
                self.bytes = self.bytes.saturating_sub(f.byte_size());
            }
        }
    }

    /// Nearest cached frame of an asset (for scrubbing while the exact one decodes).
    fn nearest(&self, asset: AssetId, index: i64) -> Option<Arc<VideoFrame>> {
        self.frames
            .iter()
            .filter(|((a, _), _)| *a == asset)
            .min_by_key(|((_, i), _)| (i - index).abs())
            .map(|(_, f)| f.clone())
    }
}

#[derive(Default)]
struct WorkerState {
    want: Option<i64>,
    /// Decode ahead in this direction after the wanted frame.
    ahead: i64,
    quit: bool,
    last_use: Option<Instant>,
    /// The frame asked for last, and the direction the requests move in (0: not known yet).
    last_asked: Option<i64>,
    dir: i64,
}

impl WorkerState {
    /// The read-ahead for a request: as long as asked, in the direction the requests actually
    /// move. Reversed clips and reverse playback both read the source backwards, whatever the
    /// direction of the sequence.
    fn steer(&mut self, index: i64, ahead: i64) -> i64 {
        if let Some(prev) = self.last_asked {
            let d = index - prev;
            if d != 0 && d.abs() <= STEER_RANGE {
                self.dir = d.signum();
            }
        }
        self.last_asked = Some(index);
        if ahead == 0 {
            0
        } else if self.dir == 0 {
            ahead
        } else {
            self.dir * ahead.abs()
        }
    }
}

struct Worker {
    state: Mutex<WorkerState>,
    wake: Condvar,
}

pub struct MediaService {
    cache_dir: PathBuf,
    assets: Mutex<AssetMap>,
    audio: Mutex<HashMap<String, AudioEntry>>,
    conform_queue: Mutex<VecDeque<(String, Arc<MediaAsset>, usize)>>,
    conform_wake: Condvar,
    frames: Mutex<FrameCache>,
    frame_ready: Condvar,
    workers: Mutex<HashMap<AssetId, Arc<Worker>>>,
    hardware: std::sync::atomic::AtomicU8,
    /// Decoder seeks over all files (each one restarts decoding at a keyframe).
    seeks: AtomicU64,
    use_proxies: std::sync::atomic::AtomicBool,
    /// Frames decoded ahead while playing (from the performance profile).
    read_ahead: std::sync::atomic::AtomicI64,
    /// Assets as their proxies: the proxy file under its own id.
    proxy_assets: Mutex<HashMap<AssetId, Arc<MediaAsset>>>,
    failed: Mutex<HashMap<AssetId, String>>,
    thumbs: Mutex<HashMap<(AssetId, i64), Option<Arc<Image>>>>,
    thumb_queue: Mutex<VecDeque<(Arc<MediaAsset>, i64)>>,
    thumb_wake: Condvar,
    /// Increments whenever something new is ready (frames, audio, thumbnails), for repaints.
    pub generation: AtomicU64,
}

impl MediaService {
    pub fn new(cache_dir: PathBuf, frame_budget: usize) -> Arc<MediaService> {
        let s = Arc::new(MediaService {
            cache_dir,
            assets: Mutex::new(HashMap::new()),
            audio: Mutex::new(HashMap::new()),
            conform_queue: Mutex::new(VecDeque::new()),
            conform_wake: Condvar::new(),
            frames: Mutex::new(FrameCache {
                frames: HashMap::new(),
                order: VecDeque::new(),
                bytes: 0,
                budget: frame_budget,
            }),
            frame_ready: Condvar::new(),
            workers: Mutex::new(HashMap::new()),
            hardware: std::sync::atomic::AtomicU8::new(HardwareDecoding::Auto.code()),
            seeks: AtomicU64::new(0),
            use_proxies: std::sync::atomic::AtomicBool::new(true),
            read_ahead: std::sync::atomic::AtomicI64::new(PLAYBACK_AHEAD),
            proxy_assets: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashMap::new()),
            thumbs: Mutex::new(HashMap::new()),
            thumb_queue: Mutex::new(VecDeque::new()),
            thumb_wake: Condvar::new(),
            generation: AtomicU64::new(0),
        });
        for i in 0..2 {
            let me = s.clone();
            std::thread::Builder::new()
                .name(format!("audio-conform-{i}"))
                .spawn(move || me.conform_loop())
                .ok();
        }
        let me = s.clone();
        std::thread::Builder::new()
            .name("thumbnails".into())
            .spawn(move || me.thumb_loop())
            .ok();
        s
    }

    /// Changes how decoders opened from now on decode (running decoders keep theirs).
    pub fn set_hardware_decoding(&self, mode: HardwareDecoding) {
        self.hardware.store(mode.code(), Ordering::Relaxed);
    }

    pub fn hardware_decoding(&self) -> HardwareDecoding {
        HardwareDecoding::from_code(self.hardware.load(Ordering::Relaxed))
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Updates the assets the service can resolve by ID (call after every project change).
    pub fn set_assets<'a>(&self, assets: impl Iterator<Item = (&'a AssetId, &'a Arc<MediaAsset>)>) {
        let mut map = self.assets.lock();
        map.clear();
        for (id, a) in assets {
            map.insert(*id, a.clone());
        }
    }

    pub fn asset(&self, id: AssetId) -> Option<Arc<MediaAsset>> {
        self.assets.lock().get(&id).cloned()
    }

    pub fn set_read_ahead(&self, frames: i64) {
        self.read_ahead
            .store(frames.clamp(2, 64), Ordering::Relaxed);
    }

    /// Frames playback asks the decoder to prepare ahead of the play position.
    pub fn read_ahead(&self) -> i64 {
        self.read_ahead.load(Ordering::Relaxed)
    }

    pub fn set_use_proxies(&self, on: bool) {
        self.use_proxies.store(on, Ordering::Relaxed);
        self.bump();
    }

    pub fn use_proxies(&self) -> bool {
        self.use_proxies.load(Ordering::Relaxed)
    }

    /// What preview decodes for `a`: its proxy while proxies are enabled and the file is there,
    /// otherwise the asset itself. The proxy keeps the original's frame numbering, frame rate
    /// and color description; the renderer sizes layers from the original asset, so the smaller
    /// frames fill the same space.
    pub fn preview_asset(&self, a: &Arc<MediaAsset>) -> Arc<MediaAsset> {
        let Some(proxy) = a.proxy.as_deref() else {
            return a.clone();
        };
        if !self.use_proxies() || !Path::new(proxy).is_file() {
            return a.clone();
        }
        let mut map = self.proxy_assets.lock();
        if let Some(p) = map.get(&a.id)
            && p.path == proxy
        {
            return p.clone();
        }
        let mut d = (**a).clone();
        d.id = AssetId(a.id.0 | PROXY_BIT);
        d.path = proxy.to_string();
        d.proxy = None;
        if let Some(v) = d.video.as_mut() {
            // a proxy holds a single video stream
            v.index = 0;
        }
        let d = Arc::new(d);
        map.insert(a.id, d.clone());
        d
    }

    // --------------------------------------------------------------------------------- audio

    fn conform_loop(self: Arc<Self>) {
        loop {
            let job = {
                let mut q = self.conform_queue.lock();
                while q.is_empty() {
                    self.conform_wake.wait(&mut q);
                }
                q.pop_front()
            };
            let Some((key, asset, stream)) = job else {
                continue;
            };
            let dir = self.cache_dir.join("Audio");
            let data = dir.join(format!("{key}.opa"));
            let peaks = dir.join(format!("{key}.opk"));
            let result: Result<(), String> = if data.exists() && peaks.exists() {
                Ok(())
            } else {
                let align = asset
                    .video
                    .as_ref()
                    .map(|v| v.start)
                    .or(asset.audio.get(stream).map(|a| a.start))
                    .unwrap_or(Dur::ZERO);
                let started = Instant::now();
                let r = guarded("audio conform", || {
                    op_media::conform(
                        Path::new(&asset.path),
                        stream,
                        align,
                        &data,
                        &peaks,
                        &mut |_| true,
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
                });
                if r.is_ok() {
                    log::debug!(
                        "conformed audio stream {stream} of {} in {} ms",
                        asset.path,
                        started.elapsed().as_millis()
                    );
                }
                r
            };
            let entry =
                match result.and_then(|_| ConformedAudio::open(&data).map_err(|e| e.to_string())) {
                    Ok(a) => AudioEntry::Ready(Arc::new(a), Peaks::open(&peaks).ok().map(Arc::new)),
                    Err(e) => {
                        log::warn!("audio conform failed for {}: {e}", asset.path);
                        AudioEntry::Failed(e.to_string())
                    }
                };
            self.audio.lock().insert(key, entry);
            self.bump();
        }
    }

    fn audio_entry(&self, asset: &Arc<MediaAsset>, stream: usize) -> Option<AudioEntry> {
        if asset.audio.get(stream).is_none() || !Path::new(&asset.path).exists() {
            return None;
        }
        let key = content_key(asset, stream);
        let mut map = self.audio.lock();
        match map.get(&key) {
            Some(e) => Some(e.clone()),
            None => {
                map.insert(key.clone(), AudioEntry::Pending);
                drop(map);
                self.conform_queue
                    .lock()
                    .push_back((key, asset.clone(), stream));
                self.conform_wake.notify_one();
                Some(AudioEntry::Pending)
            }
        }
    }

    pub fn peaks(&self, asset: AssetId, stream: usize) -> Option<Arc<Peaks>> {
        let a = self.asset(asset)?;
        match self.audio_entry(&a, stream)? {
            AudioEntry::Ready(_, p) => p,
            _ => None,
        }
    }

    /// Number of audio streams still being conformed.
    pub fn conforming(&self) -> usize {
        self.audio
            .lock()
            .values()
            .filter(|e| matches!(e, AudioEntry::Pending))
            .count()
    }

    pub fn audio_error(&self, asset: AssetId, stream: usize) -> Option<String> {
        let a = self.asset(asset)?;
        match self.audio.lock().get(&content_key(&a, stream)) {
            Some(AudioEntry::Failed(e)) => Some(e.clone()),
            _ => None,
        }
    }

    /// Waits until every audio stream used by `assets` is conformed (export).
    pub fn wait_audio(&self, assets: &[(AssetId, usize)], timeout: Duration) {
        let end = Instant::now() + timeout;
        loop {
            let pending = assets.iter().any(|(a, s)| {
                self.asset(*a)
                    .and_then(|a| self.audio_entry(&a, *s))
                    .is_some_and(|e| matches!(e, AudioEntry::Pending))
            });
            if !pending || Instant::now() > end {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // -------------------------------------------------------------------------------- frames

    fn frame_index(asset: &MediaAsset, time: SrcTime) -> i64 {
        if asset.is_still() {
            return 0;
        }
        let rate = asset.frame_rate().unwrap_or(Rate::FPS_25);
        let frames = asset.video.as_ref().map(|v| v.frames).unwrap_or(1).max(1);
        rate.frame_of(time).clamp(0, frames - 1)
    }

    fn worker(self: &Arc<Self>, asset: &Arc<MediaAsset>) -> Option<Arc<Worker>> {
        if self.failed.lock().contains_key(&asset.id) {
            return None;
        }
        let mut workers = self.workers.lock();
        if let Some(w) = workers.get(&asset.id) {
            return Some(w.clone());
        }
        let w = Arc::new(Worker {
            state: Mutex::new(WorkerState::default()),
            wake: Condvar::new(),
        });
        workers.insert(asset.id, w.clone());
        let me = self.clone();
        let a = asset.clone();
        let wk = w.clone();
        std::thread::Builder::new()
            .name(format!("decode-{}", asset.id))
            .spawn(move || me.decode_loop(a, wk))
            .ok()?;
        Some(w)
    }

    fn decode_loop(self: Arc<Self>, asset: Arc<MediaAsset>, w: Arc<Worker>) {
        let v = match asset.video.as_ref() {
            Some(v) => v.clone(),
            None => return,
        };
        let mode = self.hardware_decoding();
        let open = |hw: bool| {
            guarded("decoder", || {
                VideoDecoder::open_with(
                    Path::new(&asset.path),
                    Some(v.index),
                    asset.frame_rate(),
                    v.color,
                    asset.is_still(),
                    hw,
                )
                .map_err(|e| e.to_string())
            })
        };
        let opened = open(mode == HardwareDecoding::Always);
        if let Ok(d) = &opened
            && let Some(dev) = d.hardware()
        {
            log::info!("decoding {} with {dev}", asset.path);
        }
        let mut dec = match opened {
            Ok(d) => d,
            Err(e) => {
                log::warn!("cannot decode {}: {e}", asset.path);
                self.failed.lock().insert(asset.id, e.to_string());
                self.workers.lock().remove(&asset.id);
                self.frame_ready.notify_all();
                return;
            }
        };
        let mut prefetch: Option<(i64, i64)> = None;
        // the last frame this decoder produced: requests just after it continue in order
        let mut last: Option<i64> = None;
        // reading backwards (reversed clip or reverse playback)
        let mut backwards = false;
        let mut frame_bytes = 0usize;
        // Auto: time sequential software decoding and move to hardware if it cannot keep up
        let mut timing: Vec<f64> = Vec::new();
        let mut tried_hw = mode != HardwareDecoding::Auto || dec.hardware().is_some();
        let realtime = 1.0 / asset.frame_rate().unwrap_or(v.rate).as_f64().max(1.0);
        loop {
            let target = {
                let mut st = w.state.lock();
                loop {
                    if st.quit {
                        return;
                    }
                    if let Some(t) = st.want.take() {
                        prefetch = (st.ahead != 0).then_some((t + st.ahead.signum(), st.ahead));
                        backwards = st.ahead < 0;
                        break Some(t);
                    }
                    if let Some((next, left)) = prefetch
                        && left != 0
                    {
                        prefetch = Some((next + left.signum(), left - left.signum()));
                        backwards = left < 0;
                        break Some(next);
                    }
                    let idle = st
                        .last_use
                        .map(|t| t.elapsed() > Duration::from_secs(20))
                        .unwrap_or(true);
                    if idle {
                        break None;
                    }
                    w.wake.wait_for(&mut st, Duration::from_millis(500));
                }
            };
            let Some(index) = target else {
                // idle: release the decoder
                self.workers.lock().remove(&asset.id);
                return;
            };
            if index < 0 || index >= v.frames.max(1) {
                prefetch = None;
                continue;
            }
            if self.frames.lock().get((asset.id, index)).is_some() {
                continue;
            }
            // a target a little ahead of the last decoded frame is reached in order, keeping
            // every frame on the way: skipping one would force a seek back to the previous
            // keyframe when it is asked for next (very slow with long-GOP camera footage)
            //
            // Backwards, each frame on its own would cost a seek to the previous keyframe and a
            // decode of everything up to it. Instead a block of frames ending at the target is
            // decoded in one pass and kept, so the frames before it come from the cache; the
            // block is sized to stay well inside the frame cache.
            let from = match last {
                Some(l) if index > l + 1 && index - l <= GAP_FILL => l + 1,
                _ if backwards => {
                    let budget = self.frames.lock().budget;
                    let block = (budget / 3)
                        .checked_div(frame_bytes)
                        .map_or(8, |n| n.clamp(2, GAP_FILL as usize) as i64);
                    (index - block + 1).max(0)
                }
                _ => index,
            };
            let seeks_before = dec.seeks();
            let sequential = last.is_some_and(|l| index > l);
            let t0 = Instant::now();
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut out = Vec::new();
                for i in from..=index {
                    if i < index && self.frames.lock().get((asset.id, i)).is_some() {
                        continue;
                    }
                    out.push((i, dec.frame(i)?));
                }
                Ok::<_, op_media::MediaError>(out)
            }));
            self.seeks
                .fetch_add(dec.seeks() - seeks_before, Ordering::Relaxed);
            match decoded {
                Ok(Ok(frames)) => {
                    {
                        let mut cache = self.frames.lock();
                        for (i, f) in frames {
                            frame_bytes = f.byte_size();
                            cache.insert((asset.id, i), Arc::new(f));
                        }
                    }
                    last = Some(index);
                    self.frame_ready.notify_all();
                    self.bump();
                    if !tried_hw && sequential {
                        let per_frame = t0.elapsed().as_secs_f64() / (index - from + 1) as f64;
                        timing.push(per_frame);
                        if timing.len() >= SPEED_SAMPLE {
                            tried_hw = true;
                            let mean = timing.iter().sum::<f64>() / timing.len() as f64;
                            // decoding must leave room for rendering: 80 % of a frame at most
                            if mean > realtime * 0.8 {
                                match open(true) {
                                    Ok(d) if d.hardware().is_some() => {
                                        log::info!(
                                            "{}: software decoding takes {:.1} ms per frame; using {}",
                                            asset.path,
                                            mean * 1000.0,
                                            d.hardware().unwrap_or_default()
                                        );
                                        dec = d;
                                        last = None;
                                    }
                                    _ => log::info!(
                                        "{}: software decoding is slow ({:.1} ms per frame) and no hardware decoder is available",
                                        asset.path,
                                        mean * 1000.0
                                    ),
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::debug!("frame {index} of {}: {e}", asset.path);
                    prefetch = None;
                    last = None;
                    // a hardware decoder that fails hands the file back to software decoding
                    if let Some(dev) = dec.hardware()
                        && let Ok(d) = open(false)
                    {
                        log::warn!(
                            "{dev} could not decode {}; using software decoding",
                            asset.path
                        );
                        dec = d;
                        tried_hw = true;
                    }
                }
                Err(panic) => {
                    // the decoder state is unknown after a failure: stop using this file
                    let msg = panic_message(panic.as_ref());
                    log::error!("decoder failure on frame {index} of {}: {msg}", asset.path);
                    self.failed
                        .lock()
                        .insert(asset.id, format!("internal decoder error ({msg})"));
                    self.workers.lock().remove(&asset.id);
                    self.frame_ready.notify_all();
                    return;
                }
            }
        }
    }

    /// The decoded frame at `time`. Waits up to `wait` for the decoder; when the exact frame is
    /// not ready, `nearest` returns the closest cached one instead of nothing. `ahead` asks the
    /// decoder to keep decoding frames in that direction (playback).
    pub fn frame(
        self: &Arc<Self>,
        asset: &Arc<MediaAsset>,
        time: SrcTime,
        wait: Duration,
        nearest: bool,
        ahead: i64,
    ) -> Option<Arc<VideoFrame>> {
        asset.video.as_ref()?;
        let index = Self::frame_index(asset, time);
        let key = (asset.id, index);
        // the cache lock must be released before the prefetch check below takes it again
        let cached = self.frames.lock().get(key);
        if let Some(f) = cached {
            if ahead != 0
                && let Some(w) = self.worker(asset)
            {
                let ahead = w.state.lock().steer(index, ahead);
                // keep the pipeline full while playing: the decoder continues from the first
                // frame of the read-ahead window that is not decoded yet
                let missing = {
                    let mut cache = self.frames.lock();
                    (1..=ahead.abs())
                        .map(|k| index + ahead.signum() * k)
                        .find(|i| cache.get((asset.id, *i)).is_none())
                };
                if let Some(next) = missing {
                    let mut st = w.state.lock();
                    if st.want.is_none() {
                        st.want = Some(next);
                        st.ahead = ahead;
                        st.last_use = Some(Instant::now());
                        w.wake.notify_one();
                    }
                }
            }
            return Some(f);
        }
        let w = self.worker(asset)?;
        {
            let mut st = w.state.lock();
            st.want = Some(index);
            st.ahead = st.steer(index, ahead);
            st.last_use = Some(Instant::now());
        }
        w.wake.notify_one();
        let end = Instant::now() + wait;
        let mut cache = self.frames.lock();
        loop {
            if let Some(f) = cache.get(key) {
                return Some(f);
            }
            if self.failed.lock().contains_key(&asset.id) {
                return None;
            }
            let now = Instant::now();
            if now >= end {
                return if nearest {
                    cache.nearest(asset.id, index)
                } else {
                    None
                };
            }
            self.frame_ready.wait_for(&mut cache, end - now);
        }
    }

    /// Decoder seeks so far, over all files (diagnostics and tests).
    pub fn decoder_seeks(&self) -> u64 {
        self.seeks.load(Ordering::Relaxed)
    }

    pub fn decode_error(&self, asset: AssetId) -> Option<String> {
        self.failed.lock().get(&asset).cloned()
    }

    /// Forgets failures and cached data of an asset and of its proxy (after relinking or
    /// when a proxy is attached or removed).
    pub fn forget(&self, asset: AssetId) {
        self.proxy_assets.lock().remove(&asset);
        self.forget_id(AssetId(asset.0 | PROXY_BIT));
        self.forget_id(asset);
    }

    fn forget_id(&self, asset: AssetId) {
        self.failed.lock().remove(&asset);
        self.thumbs.lock().retain(|(a, _), _| *a != asset);
        let mut c = self.frames.lock();
        let keys: Vec<(AssetId, i64)> = c
            .frames
            .keys()
            .filter(|(a, _)| *a == asset)
            .copied()
            .collect();
        for k in keys {
            if let Some(f) = c.frames.remove(&k) {
                c.bytes = c.bytes.saturating_sub(f.byte_size());
            }
        }
        c.order.retain(|(a, _)| *a != asset);
        if let Some(w) = self.workers.lock().remove(&asset) {
            w.state.lock().quit = true;
            w.wake.notify_all();
        }
    }

    pub fn set_frame_budget(&self, bytes: usize) {
        self.frames.lock().budget = bytes.max(64 << 20);
    }

    // ---------------------------------------------------------------------------- thumbnails

    fn thumb_loop(self: Arc<Self>) {
        loop {
            let job = {
                let mut q = self.thumb_queue.lock();
                while q.is_empty() {
                    self.thumb_wake.wait(&mut q);
                }
                q.pop_back()
            };
            let Some((asset, index)) = job else { continue };
            let img = guarded("thumbnail", || {
                op_media::thumb::thumbnail(&asset, index, 320, 180).map_err(|e| e.to_string())
            })
            .ok()
            .map(Arc::new);
            self.thumbs.lock().insert((asset.id, index), img);
            self.bump();
        }
    }

    /// A small picture of a frame, generated in the background.
    pub fn thumbnail(&self, asset: &Arc<MediaAsset>, index: i64) -> Option<Arc<Image>> {
        asset.video.as_ref()?;
        let key = (asset.id, index);
        let mut t = self.thumbs.lock();
        match t.get(&key) {
            Some(v) => v.clone(),
            None => {
                t.insert(key, None);
                drop(t);
                let mut q = self.thumb_queue.lock();
                if q.len() > 200 {
                    q.pop_front();
                }
                q.push_back((asset.clone(), index));
                self.thumb_wake.notify_one();
                None
            }
        }
    }
}

/// The text of a panic payload.
pub fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown failure".into())
}

/// Runs `f`, turning a panic into an error, so one damaged file cannot stop a worker thread.
pub fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = panic_message(p.as_ref());
        log::error!("{what} failed unexpectedly: {msg}");
        Err(format!("internal error ({msg})"))
    })
}

/// Probes files on a background thread.
pub fn probe_many(
    paths: Vec<PathBuf>,
) -> crossbeam_channel::Receiver<(PathBuf, Result<MediaAsset, String>)> {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::Builder::new()
        .name("probe".into())
        .spawn(move || {
            for p in paths {
                let r = guarded("probe", || op_media::probe(&p).map_err(|e| e.to_string()));
                match &r {
                    Ok(a) => log::info!(
                        "imported {} ({}, {} audio streams)",
                        p.display(),
                        a.video
                            .as_ref()
                            .map(|v| format!("{}x{} {}", v.width, v.height, v.codec))
                            .unwrap_or_else(|| "no video".into()),
                        a.audio.len()
                    ),
                    Err(e) => log::warn!("cannot import {}: {e}", p.display()),
                }
                if tx.send((p, r)).is_err() {
                    return;
                }
            }
        })
        .ok();
    rx
}

/// Frame source for interactive preview: waits briefly, falls back to the nearest frame.
pub struct PreviewFrames {
    pub service: Arc<MediaService>,
    pub wait: Duration,
    pub ahead: i64,
}

impl op_render::FrameSource for PreviewFrames {
    fn video_frame(&self, asset: &MediaAsset, time: SrcTime) -> Option<Arc<VideoFrame>> {
        let a = self
            .service
            .asset(asset.id)
            .unwrap_or_else(|| Arc::new(asset.clone()));
        let a = self.service.preview_asset(&a);
        self.service.frame(&a, time, self.wait, true, self.ahead)
    }
}

/// Frame source for export: waits for every exact frame.
pub struct ExactFrames {
    pub service: Arc<MediaService>,
}

impl op_render::FrameSource for ExactFrames {
    fn video_frame(&self, asset: &MediaAsset, time: SrcTime) -> Option<Arc<VideoFrame>> {
        let a = self
            .service
            .asset(asset.id)
            .unwrap_or_else(|| Arc::new(asset.clone()));
        self.service
            .frame(&a, time, Duration::from_secs(30), false, PLAYBACK_AHEAD)
    }
}

impl op_audio::AudioSource for MediaService {
    fn audio(&self, asset: AssetId, stream: usize) -> Option<Arc<ConformedAudio>> {
        let a = self.asset(asset)?;
        match self.audio_entry(&a, stream)? {
            AudioEntry::Ready(c, _) => Some(c),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_media::{Muxer, VideoCodec, VideoSettings};

    /// Playback asks for cached frames with a prefetch direction; that path must not wait on
    /// the cache lock it already holds.
    #[test]
    fn cached_frames_while_playing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.mp4");
        let (w, h) = (64u32, 64u32);
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 20,
            hardware: false,
        };
        let mut m = Muxer::create(&path, Some(&v), None).unwrap();
        let chroma = vec![128u8; (w / 2 * h / 2) as usize];
        for i in 0..20u32 {
            let luma = vec![(16 + 8 * i) as u8; (w * h) as usize];
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
        let mut asset = op_media::probe(&path).unwrap();
        asset.id = AssetId(1);
        let asset = Arc::new(asset);
        let service = MediaService::new(dir.path().join("cache"), 64 << 20);
        service.set_assets(std::iter::once((&asset.id, &asset)));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let s2 = service.clone();
        let a2 = asset.clone();
        std::thread::spawn(move || {
            let first = s2
                .frame(&a2, SrcTime::ZERO, Duration::from_secs(10), false, 0)
                .is_some();
            // the frame is cached now: this is the path playback takes every frame
            let again = s2
                .frame(&a2, SrcTime::ZERO, Duration::from_millis(10), true, 1)
                .is_some();
            let _ = tx.send(first && again);
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(20)), Ok(true));
        // the read-ahead window fills in order, without gaps that would force a seek back
        let start = Instant::now();
        let filled = |s: &MediaService| {
            let mut cache = s.frames.lock();
            (1..=PLAYBACK_AHEAD.min(19)).all(|i| cache.get((asset.id, i)).is_some())
        };
        while !filled(&service) && start.elapsed() < Duration::from_secs(20) {
            service.frame(&asset, SrcTime::ZERO, Duration::ZERO, true, PLAYBACK_AHEAD);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            filled(&service),
            "the frames after the play position were not decoded"
        );
    }

    /// A reversed clip plays forward in the sequence but reads its source backwards: the
    /// frames arrive in order and correct, with a seek per block of frames rather than per frame.
    #[test]
    fn backwards_reading_decodes_in_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rev.mp4");
        let (w, h) = (64u32, 64u32);
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 20,
            hardware: false,
        };
        let mut m = Muxer::create(&path, Some(&v), None).unwrap();
        let chroma = vec![128u8; (w / 2 * h / 2) as usize];
        let n = 75u32;
        for i in 0..n {
            let luma = vec![(16 + 2 * i) as u8; (w * h) as usize];
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
        let mut asset = op_media::probe(&path).unwrap();
        asset.id = AssetId(3);
        let asset = Arc::new(asset);
        let service = MediaService::new(dir.path().join("cache"), 256 << 20);
        service.set_hardware_decoding(HardwareDecoding::Never);
        service.set_assets(std::iter::once((&asset.id, &asset)));
        let rate = Rate::FPS_25.as_f64();
        // the sequence plays forward (positive read-ahead) while the source runs backwards
        for i in (0..n as i64).rev() {
            let t = SrcTime::from_seconds(i as f64 / rate);
            let f = service
                .frame(&asset, t, Duration::from_secs(20), false, PLAYBACK_AHEAD)
                .unwrap_or_else(|| panic!("frame {i}"));
            let px = f.to_rgba8_scaled(1, 1);
            let want = 16.0 + 2.0 * i as f64;
            // limited-range luma shown full range: compare loosely, but tell neighbours apart
            let got = (px[0] as f64) * 219.0 / 255.0 + 16.0;
            assert!(
                (got - want).abs() < 3.0,
                "frame {i}: luma {got:.1}, expected {want}"
            );
        }
        let seeks = service.decoder_seeks();
        assert!(
            seeks <= n as u64 / 4,
            "{seeks} seeks for {n} frames read backwards"
        );
    }

    /// Forced hardware decoding gives the same picture as the processor, or falls back to the
    /// processor on machines without a hardware decoder.
    #[test]
    fn hardware_decoding_or_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hw.mp4");
        let (w, h) = (320u32, 240u32);
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 20,
            hardware: false,
        };
        let mut m = Muxer::create(&path, Some(&v), None).unwrap();
        let luma = vec![180u8; (w * h) as usize];
        let chroma = vec![100u8; (w / 2 * h / 2) as usize];
        for _ in 0..10 {
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
        let mut asset = op_media::probe(&path).unwrap();
        asset.id = AssetId(7);
        let asset = Arc::new(asset);
        let mut pictures = Vec::new();
        for mode in [HardwareDecoding::Never, HardwareDecoding::Always] {
            let service = MediaService::new(dir.path().join(format!("cache-{mode:?}")), 64 << 20);
            service.set_hardware_decoding(mode);
            service.set_assets(std::iter::once((&asset.id, &asset)));
            let f = service
                .frame(
                    &asset,
                    SrcTime::from_seconds(0.2),
                    Duration::from_secs(20),
                    false,
                    0,
                )
                .expect("a decoded frame");
            pictures.push(f.to_rgba8_scaled(1, 1));
        }
        for k in 0..3 {
            assert!(
                (pictures[0][k] as i32 - pictures[1][k] as i32).abs() <= 3,
                "{:?} vs {:?}",
                pictures[0],
                pictures[1]
            );
        }
    }
}
