//! The export queue (like Adobe Media Encoder's): exports are added one after the other with
//! what they are made of (the project as it was when added, the settings) and run one at a time
//! when the queue is started, so the edit can go on meanwhile and the files come out together.

use std::sync::Arc;

use op_core::Project;
use op_render::Gpu;

use crate::export::{ExportJob, ExportOptions, ExportSettings, Progress};
use crate::media::MediaService;

pub enum QueueState {
    Waiting,
    Running(ExportJob),
    Done,
    Failed(String),
    Cancelled,
}

pub struct QueueItem {
    pub id: u64,
    /// The sequence's name.
    pub title: String,
    /// The preset it was made from.
    pub preset: String,
    pub settings: ExportSettings,
    pub options: ExportOptions,
    project: Arc<Project>,
    pub state: QueueState,
}

impl QueueItem {
    /// The progress of the item that is running now.
    pub fn progress(&self) -> Option<Progress> {
        match &self.state {
            QueueState::Running(job) => Some(job.progress.lock().clone()),
            _ => None,
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, QueueState::Done | QueueState::Failed(_) | QueueState::Cancelled)
    }
}

#[derive(Default)]
pub struct ExportQueue {
    pub items: Vec<QueueItem>,
    /// Whether the queue is going through its items.
    pub started: bool,
    next_id: u64,
}

impl ExportQueue {
    /// Adds an export: `project` is the project as it is now (later edits do not change it).
    pub fn add(
        &mut self,
        title: String,
        preset: String,
        project: Arc<Project>,
        settings: ExportSettings,
        options: ExportOptions,
    ) -> u64 {
        self.next_id += 1;
        self.items.push(QueueItem {
            id: self.next_id,
            title,
            preset,
            settings,
            options,
            project,
            state: QueueState::Waiting,
        });
        self.next_id
    }

    /// Takes an item out; one that is running is stopped (its unfinished file is deleted).
    pub fn remove(&mut self, id: u64) {
        if let Some(i) = self.items.iter().position(|x| x.id == id) {
            if let QueueState::Running(job) = &self.items[i].state {
                job.cancel();
            }
            self.items.remove(i);
        }
    }

    pub fn clear_finished(&mut self) {
        self.items.retain(|x| !x.is_finished());
    }

    pub fn start(&mut self) {
        self.started = true;
    }

    /// Stops after cancelling what is running (the item is marked cancelled; the others wait).
    pub fn stop(&mut self) {
        self.started = false;
        for item in &mut self.items {
            if let QueueState::Running(job) = &item.state {
                job.cancel();
            }
        }
    }

    pub fn waiting(&self) -> usize {
        self.items.iter().filter(|x| matches!(x.state, QueueState::Waiting)).count()
    }

    /// Whether an export of the queue is being made now.
    pub fn busy(&self) -> bool {
        self.items.iter().any(|x| matches!(x.state, QueueState::Running(_)))
    }

    /// Goes on with the queue: notes what finished and starts the next item when none runs.
    /// Returns messages to show (an item finished or failed, the queue ended).
    pub fn update(&mut self, gpu: &Arc<Gpu>, media: &Arc<MediaService>) -> Vec<(bool, String)> {
        let mut messages = Vec::new();
        for item in &mut self.items {
            let QueueState::Running(job) = &item.state else { continue };
            if !job.finished() {
                continue;
            }
            let pr: Progress = job.progress.lock().clone();
            let name = item.settings.path.display().to_string();
            item.state = if let Some(e) = pr.error {
                messages.push((true, format!("대기열: {name} 실패: {e}")));
                QueueState::Failed(e)
            } else if pr.cancelled {
                QueueState::Cancelled
            } else {
                messages.push((false, format!("대기열: {name} 완료")));
                QueueState::Done
            };
        }
        if self.started && !self.busy() {
            match self.items.iter_mut().find(|x| matches!(x.state, QueueState::Waiting)) {
                Some(item) => {
                    log::info!("export queue: starting {}", item.settings.path.display());
                    let job = ExportJob::start_with(item.project.clone(), item.settings.clone(), item.options, gpu.clone(), media.clone());
                    item.state = QueueState::Running(job);
                }
                None => {
                    self.started = false;
                    messages.push((false, "내보내기 대기열이 모두 끝났습니다".to_string()));
                }
            }
        }
        messages
    }
}

/// The finished and waiting counts for a short summary.
pub fn summary(q: &ExportQueue) -> String {
    let done = q.items.iter().filter(|x| matches!(x.state, QueueState::Done)).count();
    format!("{}개 중 {done}개 완료, {}개 대기", q.items.len(), q.waiting())
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_core::*;
    use std::time::{Duration, Instant};

    fn project() -> (Arc<Project>, SequenceId) {
        let mut p = Project::new("Q");
        let root = p.root;
        let (sid, _) = p.add_sequence(root, "S", SequenceSettings { width: 64, height: 36, rate: Rate::FPS_25, ..Default::default() });
        let item = p.add_item(
            root,
            "Red",
            ItemKind::Synthetic { generator: Generator::ColorMatte { color: Rgba::new(1.0, 0.0, 0.0, 1.0) }, duration: Dur::from_seconds(2.0) },
        );
        let spec = op_timeline::SourceClip::from_item(&p, item).unwrap();
        let patch = op_timeline::Patch { video: Some(0), audio: vec![] };
        let (p, _) = p
            .transact(|p| op_timeline::overwrite(p, sid, &spec, SeqTime::ZERO, &patch, op_timeline::EditOptions::default()))
            .unwrap();
        (Arc::new(p), sid)
    }

    fn settings(path: std::path::PathBuf, sid: SequenceId) -> ExportSettings {
        ExportSettings {
            path,
            sequence: sid,
            range: SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(2.0)),
            video: Some(op_media::VideoSettings {
                codec: op_media::VideoCodec::Png,
                width: 64,
                height: 36,
                rate: Rate::FPS_25,
                bitrate_kbps: None,
                quality: 0,
                hardware: false,
            }),
            audio: None,
        }
    }

    #[test]
    fn the_queue_makes_its_exports_one_after_the_other() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let media = MediaService::new(dir.path().join("cache"), 64 << 20);
        let (p, sid) = project();
        let mut q = ExportQueue::default();
        let a = q.add("S".into(), "PNG".into(), p.clone(), settings(dir.path().join("a.mov"), sid), ExportOptions { preview: false, ..Default::default() });
        let b = q.add("S".into(), "PNG".into(), p, settings(dir.path().join("b.mov"), sid), ExportOptions::default());
        assert_eq!(q.waiting(), 2);
        // nothing runs before the queue is started
        q.update(&gpu, &media);
        assert!(!q.busy());
        q.start();
        q.update(&gpu, &media);
        // one at a time: the first runs, the second waits
        assert!(matches!(q.items[0].state, QueueState::Running(_)));
        assert!(matches!(q.items[1].state, QueueState::Waiting));
        let start = Instant::now();
        let mut messages = Vec::new();
        while q.started && start.elapsed() < Duration::from_secs(120) {
            messages.extend(q.update(&gpu, &media));
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(q.items.iter().all(|x| matches!(x.state, QueueState::Done)), "{messages:?}");
        assert!(dir.path().join("a.mov").is_file() && dir.path().join("b.mov").is_file());
        assert!(messages.iter().any(|(_, m)| m.contains("모두 끝났습니다")));
        assert_eq!(q.items.iter().map(|x| x.id).collect::<Vec<_>>(), vec![a, b]);
        q.clear_finished();
        assert!(q.items.is_empty());
    }

    #[test]
    fn an_export_without_the_preview_makes_no_preview_picture() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let media = MediaService::new(dir.path().join("cache"), 64 << 20);
        let (p, sid) = project();
        let job = ExportJob::start_with(p, settings(dir.path().join("c.mov"), sid), ExportOptions { preview: false, ..Default::default() }, gpu, media);
        let start = Instant::now();
        while !job.finished() && start.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let pr = job.progress.lock().clone();
        assert!(pr.done && pr.error.is_none(), "{:?}", pr.error);
        assert!(pr.preview.is_none(), "no picture is made when the preview is off");
    }
}
