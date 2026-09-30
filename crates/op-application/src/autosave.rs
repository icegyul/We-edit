//! Autosave and crash recovery. A lock file marks a running session; if it is still there at the
//! next start, the previous session ended unexpectedly and its newest autosave is offered.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::editor::{Editor, GENERATOR};
use crate::prefs::Dirs;

/// "YYYYMMDD-HHMMSS" in UTC.
pub fn stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

fn safe_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if n.is_empty() { "Untitled".into() } else { n }
}

/// Writes an autosave copy and prunes old ones for this project.
pub fn write(editor: &mut Editor) -> Result<PathBuf, String> {
    let project = editor.snapshot();
    write_project(
        &editor.dirs.autosave(),
        &editor.document_name(),
        editor.prefs.autosave_keep,
        &project,
    )
}

/// Writes `project` as a new autosave version in `dir` and keeps the newest `keep` versions.
pub fn write_project(
    dir: &Path,
    name: &str,
    keep: usize,
    project: &op_core::Project,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = safe_name(name);
    let path = dir.join(format!("{name}-{}.opproj", stamp(SystemTime::now())));
    op_project::native::save(&path, project, &serde_json::Value::Null, GENERATOR)
        .map_err(|e| e.to_string())?;
    let keep = keep.max(1);
    let mut mine: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{name}-")))
                && p.extension().is_some_and(|x| x == "opproj")
                && !p.to_string_lossy().ends_with("-recovery.opproj")
        })
        .collect();
    mine.sort();
    while mine.len() > keep {
        let old = mine.remove(0);
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::remove_file(old.with_file_name(format!(
            "{}.bak",
            old.file_name().unwrap().to_string_lossy()
        )));
    }
    Ok(path)
}

fn lock_path(dirs: &Dirs) -> PathBuf {
    dirs.data.join("session.lock")
}

/// Records that a session is running. Returns the newest autosave when the previous session
/// did not end cleanly.
pub fn begin_session(dirs: &Dirs) -> Option<PathBuf> {
    let lock = lock_path(dirs);
    let crashed = lock.exists();
    let _ = std::fs::create_dir_all(&dirs.data);
    let _ = std::fs::write(&lock, std::process::id().to_string());
    if !crashed {
        return None;
    }
    newest(&dirs.autosave())
}

pub fn end_session(dirs: &Dirs) {
    let _ = std::fs::remove_file(lock_path(dirs));
}

fn newest(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "opproj"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(stamp(UNIX_EPOCH), "19700101-000000");
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        assert_eq!(stamp(t), "20260921-141320");
    }

    #[test]
    fn crash_detection() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs::portable(dir.path());
        assert!(begin_session(&dirs).is_none());
        std::fs::create_dir_all(dirs.autosave()).unwrap();
        std::fs::write(dirs.autosave().join("a-1.opproj"), "{}").unwrap();
        // the lock is still there: the previous run crashed
        assert!(begin_session(&dirs).is_some());
        end_session(&dirs);
        assert!(begin_session(&dirs).is_none());
    }
}
