//! Proxy creation in the background (Project panel > Proxy > Create Proxies).
//!
//! Proxies are written one at a time on a worker thread into the data folder (`Proxies`), named
//! after the source and a hash of its path, size and modification time, so a changed file gets a
//! new proxy and an unchanged one is not made twice. The editor attaches each finished proxy to
//! its asset; playback uses it while proxies are enabled, export always uses the original.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use op_core::{AssetId, MediaAsset};
use parking_lot::Mutex;

/// Short side of a proxy in pixels (a 4K clip becomes 960x540).
pub const SHORT_SIDE: u32 = 540;

#[derive(Clone, Debug, Default)]
pub struct ProxyProgress {
    /// The file being converted.
    pub current: Option<String>,
    pub fraction: f32,
    /// Finished and still waiting, in this batch.
    pub done: usize,
    pub total: usize,
}

#[derive(Default)]
struct Shared {
    queue: VecDeque<(Arc<MediaAsset>, PathBuf)>,
    progress: ProxyProgress,
    results: Vec<(AssetId, Result<PathBuf, String>)>,
    running: bool,
}

#[derive(Default)]
pub struct ProxyQueue {
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
}

/// Where the proxy of `asset` goes.
pub fn proxy_path(folder: &Path, asset: &MediaAsset) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    asset.path.hash(&mut h);
    SHORT_SIDE.hash(&mut h);
    if let Ok(m) = std::fs::metadata(&asset.path) {
        m.len().hash(&mut h);
        if let Ok(t) = m.modified() {
            t.hash(&mut h);
        }
    }
    let stem = Path::new(&asset.path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_ ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    folder.join(format!("{stem}-{:016x}.mp4", h.finish()))
}

/// Why an asset cannot have a proxy, if it cannot.
pub fn unsuitable(asset: &MediaAsset) -> Option<&'static str> {
    let v = asset.video.as_ref()?;
    if asset.is_still() {
        Some("Still images do not need proxies")
    } else if v.alpha != op_core::AlphaMode::None && !asset.interpretation.ignore_alpha {
        Some("Clips with transparency keep their original media")
    } else {
        None
    }
}

impl ProxyQueue {
    /// Queues assets; the worker starts if it is not running.
    pub fn add(&self, jobs: Vec<(Arc<MediaAsset>, PathBuf)>) {
        if jobs.is_empty() {
            return;
        }
        let start = {
            let mut s = self.shared.lock();
            for j in jobs {
                if !s.queue.iter().any(|(a, _)| a.id == j.0.id) {
                    s.queue.push_back(j);
                    s.progress.total += 1;
                }
            }
            let start = !s.running;
            s.running = true;
            start
        };
        if start {
            self.cancel.store(false, Ordering::Relaxed);
            let shared = self.shared.clone();
            let cancel = self.cancel.clone();
            let spawned = std::thread::Builder::new()
                .name("proxies".into())
                .spawn(move || work(&shared, &cancel));
            if spawned.is_err() {
                self.shared.lock().running = false;
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.shared.lock().running
    }

    pub fn progress(&self) -> ProxyProgress {
        self.shared.lock().progress.clone()
    }

    /// Stops the current conversion and forgets the waiting ones.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let mut s = self.shared.lock();
        s.queue.clear();
    }

    /// Finished proxies (or failures) since the last call.
    pub fn take_results(&self) -> Vec<(AssetId, Result<PathBuf, String>)> {
        std::mem::take(&mut self.shared.lock().results)
    }
}

fn work(shared: &Mutex<Shared>, cancel: &AtomicBool) {
    loop {
        let next = {
            let mut s = shared.lock();
            match s.queue.pop_front() {
                Some(j) => {
                    s.progress.current = Some(j.0.file_name().to_string());
                    s.progress.fraction = 0.0;
                    Some(j)
                }
                None => {
                    s.running = false;
                    s.progress = ProxyProgress::default();
                    None
                }
            }
        };
        let Some((asset, dst)) = next else { return };
        let result = make(&asset, &dst, shared, cancel);
        match &result {
            Ok(p) => log::info!("proxy for {}: {}", asset.path, p.display()),
            Err(e) => log::warn!("no proxy for {}: {e}", asset.path),
        }
        let mut s = shared.lock();
        s.progress.done += 1;
        s.results.push((asset.id, result));
        if cancel.load(Ordering::Relaxed) {
            s.queue.clear();
        }
    }
}

fn make(
    asset: &MediaAsset,
    dst: &Path,
    shared: &Mutex<Shared>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if dst.is_file() {
        return Ok(dst.to_path_buf());
    }
    let v = asset.video.as_ref().ok_or("no video")?;
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let rate = asset.frame_rate().unwrap_or(v.rate);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        op_media::proxy::make_proxy(
            Path::new(&asset.path),
            Some(v.index),
            rate,
            v.frames,
            dst,
            SHORT_SIDE,
            &mut |f| {
                shared.lock().progress.fraction = f;
                !cancel.load(Ordering::Relaxed)
            },
        )
    }))
    .unwrap_or_else(|_| {
        Err(op_media::MediaError::Unsupported(
            "the converter stopped".into(),
        ))
    });
    match result {
        Ok(_) => Ok(dst.to_path_buf()),
        Err(op_media::MediaError::Cancelled) => Err("cancelled".into()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_core::Rate;
    use op_media::{Muxer, VideoCodec, VideoSettings};
    use std::time::{Duration, Instant};

    #[test]
    fn queue_makes_proxies_and_reports_them() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cam one.mp4");
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: 1280,
            height: 720,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 23,
            hardware: false,
        };
        let mut m = Muxer::create(&src, Some(&v), None).unwrap();
        let luma = vec![120u8; 1280 * 720];
        let chroma = vec![128u8; 640 * 360];
        for _ in 0..20 {
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
        let mut asset = op_media::probe(&src).unwrap();
        asset.id = AssetId(9);
        assert!(unsuitable(&asset).is_none());
        let asset = Arc::new(asset);
        let folder = dir.path().join("Proxies");
        let dst = proxy_path(&folder, &asset);
        assert!(
            dst.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cam one-")
        );
        assert_eq!(dst, proxy_path(&folder, &asset), "the name is stable");

        let q = ProxyQueue::default();
        q.add(vec![(asset.clone(), dst.clone())]);
        let start = Instant::now();
        let mut results = Vec::new();
        while results.is_empty() && start.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(20));
            results = q.take_results();
        }
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, AssetId(9));
        assert_eq!(results[0].1.as_ref().unwrap(), &dst);
        let p = op_media::probe(&dst).unwrap().video.unwrap();
        assert_eq!((p.width, p.height), (960, 540));
        while q.busy() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(q.progress().total, 0, "the batch is over");
    }
}
