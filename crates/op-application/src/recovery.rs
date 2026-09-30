//! Crash safety. The newest project state is kept where the panic hook can reach it and written
//! to a recovery file on a background thread shortly after each change, so a crash, a frozen
//! window or a power cut loses at most a few seconds of work. The file sits in the autosave
//! folder, where the next start offers it, and is removed when the program ends normally or the
//! project is saved.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use op_core::Project;
use parking_lot::Mutex;

use crate::editor::GENERATOR;

/// How long changes wait before the recovery file is refreshed.
pub const INTERVAL: Duration = Duration::from_secs(15);

struct Latest {
    project: Arc<Project>,
    revision: u64,
    file: PathBuf,
}

static LATEST: Mutex<Option<Latest>> = Mutex::new(None);

/// Tests that use the shared recovery state run one at a time.
#[cfg(test)]
pub(crate) static TEST_GUARD: Mutex<()> = Mutex::new(());
static WRITING: AtomicBool = AtomicBool::new(false);
static LAST_EMERGENCY: Mutex<Option<Instant>> = Mutex::new(None);

/// The recovery file of a project.
pub fn file_for(autosave_dir: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() {
        "Untitled".to_string()
    } else {
        safe
    };
    autosave_dir.join(format!("{safe}-recovery.{}", op_project::native::EXTENSION))
}

/// Records the newest state (cheap: the project is shared, not copied).
pub fn remember(project: Arc<Project>, revision: u64, file: PathBuf) {
    *LATEST.lock() = Some(Latest {
        project,
        revision,
        file,
    });
}

fn write(project: &Project, file: &Path) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    op_project::native::save(file, project, &serde_json::Value::Null, GENERATOR)
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(file.with_file_name(format!(
        "{}.bak",
        file.file_name().unwrap_or_default().to_string_lossy()
    )));
    Ok(())
}

/// Writes the remembered state on a background thread unless a write is already running.
/// Returns the revision being written.
pub fn write_in_background() -> Option<u64> {
    let (project, revision, file) = {
        let l = LATEST.lock();
        let l = l.as_ref()?;
        (l.project.clone(), l.revision, l.file.clone())
    };
    if WRITING.swap(true, Ordering::AcqRel) {
        return None;
    }
    let spawned = std::thread::Builder::new()
        .name("recovery".into())
        .spawn(move || {
            let started = Instant::now();
            match write(&project, &file) {
                Ok(()) => log::debug!(
                    "recovery file updated in {} ms: {}",
                    started.elapsed().as_millis(),
                    file.display()
                ),
                Err(e) => log::warn!("recovery file not written: {e}"),
            }
            WRITING.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        WRITING.store(false, Ordering::Release);
        return None;
    }
    Some(revision)
}

/// Saves the remembered state immediately (panic hook). Never blocks on a lock held by the
/// failing thread, and runs at most every few seconds.
pub fn emergency_save() -> Option<PathBuf> {
    {
        let mut last = LAST_EMERGENCY.try_lock()?;
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return None;
        }
        *last = Some(Instant::now());
    }
    let (project, file) = {
        let l = LATEST.try_lock()?;
        let l = l.as_ref()?;
        (l.project.clone(), l.file.clone())
    };
    match write(&project, &file) {
        Ok(()) => {
            log::error!("project state saved for recovery: {}", file.display());
            Some(file)
        }
        Err(e) => {
            log::error!("emergency save failed: {e}");
            None
        }
    }
}

/// Removes the recovery file (normal exit, or the project was saved).
pub fn discard() {
    let file = LATEST.lock().as_ref().map(|l| l.file.clone());
    if let Some(f) = file {
        let _ = std::fs::remove_file(&f);
    }
}

/// Forgets the remembered state (a new or another project was opened).
pub fn forget() {
    discard();
    *LATEST.lock() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_and_emergency_writes() {
        let _guard = TEST_GUARD.lock();
        let dir = tempfile::tempdir().unwrap();
        let file = file_for(dir.path(), "My Project");
        assert!(file.ends_with("My_Project-recovery.opproj"));
        remember(Arc::new(Project::new("P")), 1, file.clone());
        assert_eq!(write_in_background(), Some(1));
        let start = Instant::now();
        while WRITING.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(file.exists());
        std::fs::remove_file(&file).unwrap();
        assert_eq!(emergency_save().as_deref(), Some(file.as_path()));
        assert!(op_project::native::load(&file).is_ok());
        forget();
        assert!(!file.exists());
    }
}
