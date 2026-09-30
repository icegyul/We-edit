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
        let opened = guarded("decoder", || {
            VideoDecoder::open(
                Path::new(&asset.path),
                Some(v.index),
                asset.frame_rate(),
                v.color,
                asset.is_still(),
            )
            .map_err(|e| e.to_string())
        });
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
        loop {
            let target = {
                let mut st = w.state.lock();
                loop {
                    if st.quit {
                        return;
                    }
                    if let Some(t) = st.want.take() {
                        prefetch = (st.ahead != 0).then_some((t + st.ahead.signum(), st.ahead));
                        break Some(t);
                    }
                    if let Some((next, left)) = prefetch
                        && left != 0
                    {
                        prefetch = Some((next + left.signum(), left - left.signum()));
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
            let decoded =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dec.frame(index)));
            match decoded {
                Ok(Ok(f)) => {
                    self.frames.lock().insert((asset.id, index), Arc::new(f));
                    self.frame_ready.notify_all();
                    self.bump();
                }
                Ok(Err(e)) => {
                    log::debug!("frame {index} of {}: {e}", asset.path);
                    prefetch = None;
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
                // keep the pipeline full while playing
                let next = index + ahead.signum() * 2;
                if self.frames.lock().get((asset.id, next)).is_none() {
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
            st.ahead = ahead;
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

    pub fn decode_error(&self, asset: AssetId) -> Option<String> {
        self.failed.lock().get(&asset).cloned()
    }

    /// Forgets failures and cached data of an asset (after relinking).
    pub fn forget(&self, asset: AssetId) {
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
            .frame(&a, time, Duration::from_secs(30), false, 1)
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
    }
}
