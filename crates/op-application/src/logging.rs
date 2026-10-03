//! Program log. Every message goes to a ring buffer shown by the in-app log viewer and, when
//! logging is enabled, to a log file in the data folder; the previous session's file is kept for
//! bug reports. The detail level and the file output can be changed while the program runs
//! (Preferences). Crash reports are written separately and always.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

/// Detail levels offered to the user, least to most verbose.
pub const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

const RING: usize = 4000;

/// The log file stops growing here (a session normally writes well under a megabyte).
const MAX_FILE: u64 = 100 << 20;

/// OPENPREMIER_LOG_LIBRARIES=1 records every library at the chosen level.
static ALL_LIBRARIES: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("OPENPREMIER_LOG_LIBRARIES").is_ok_and(|v| v == "1"));

/// One recorded message.
#[derive(Clone, Debug)]
pub struct Line {
    /// Seconds since the program started.
    pub at: f64,
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

impl Line {
    pub fn format(&self) -> String {
        format!(
            "{:>9.3} {:<5} {}: {}",
            self.at, self.level, self.target, self.message
        )
    }
}

struct Logger {
    enabled: AtomicBool,
    level: AtomicUsize,
    dir: Mutex<Option<PathBuf>>,
    file: Mutex<Option<std::fs::File>>,
    ring: Mutex<VecDeque<Line>>,
    /// Bumped on every recorded line (the viewer redraws when it changes).
    serial: AtomicUsize,
    start: Instant,
    /// Bytes written to the file this session.
    written: std::sync::atomic::AtomicU64,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

fn logger() -> &'static Logger {
    LOGGER.get_or_init(|| Logger {
        enabled: AtomicBool::new(true),
        level: AtomicUsize::new(log::LevelFilter::Info as usize),
        dir: Mutex::new(None),
        file: Mutex::new(None),
        ring: Mutex::new(VecDeque::with_capacity(RING)),
        serial: AtomicUsize::new(0),
        start: Instant::now(),
        written: std::sync::atomic::AtomicU64::new(0),
    })
}

fn filter(level: &str) -> log::LevelFilter {
    match level.to_ascii_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" | "warning" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

fn level_index(f: log::LevelFilter) -> usize {
    f as usize
}

/// "YYYY-MM-DD HH:MM:SS" in UTC.
pub fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let stamp = crate::autosave::stamp(UNIX_EPOCH + std::time::Duration::from_secs(secs as u64));
    // "YYYYMMDD-HHMMSS" -> "YYYY-MM-DD HH:MM:SS"
    format!(
        "{}-{}-{} {}:{}:{}",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13],
        &stamp[13..15]
    )
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        let max = if self.enabled.load(Ordering::Relaxed) {
            self.level.load(Ordering::Relaxed)
        } else {
            // with logging off only errors are kept, for the viewer and crash analysis
            level_index(log::LevelFilter::Error)
        };
        if (m.level() as usize) > max {
            return false;
        }
        // other libraries only report problems: at the most detailed levels the windowing and
        // graphics libraries write thousands of lines a second (gigabytes in minutes);
        // OPENPREMIER_LOG_LIBRARIES=1 lets them through for debugging those libraries
        let ours = m.target().starts_with("op_") || m.target().starts_with("openpremier");
        if ours || *ALL_LIBRARIES {
            return true;
        }
        // the Vulkan loader reports missing third-party layers as errors; that is noise
        if m.target().starts_with("wgpu_hal::vulkan::instance") {
            return max >= level_index(log::LevelFilter::Debug);
        }
        m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = Line {
            at: self.start.elapsed().as_secs_f64(),
            level: r.level(),
            target: r.target().to_string(),
            message: r.args().to_string(),
        };
        let text = line.format();
        if cfg!(debug_assertions) {
            eprintln!("{text}");
        }
        if self.enabled.load(Ordering::Relaxed) {
            let mut file = self.file.lock();
            if let Some(f) = file.as_mut() {
                let n = self
                    .written
                    .fetch_add(text.len() as u64 + 1, Ordering::Relaxed);
                if n < MAX_FILE {
                    let _ = writeln!(f, "{text}");
                } else {
                    // a runaway log must not fill the disk
                    let _ = writeln!(
                        f,
                        "the log reached {} MB; later lines are only kept in the log viewer",
                        MAX_FILE >> 20
                    );
                    *file = None;
                }
            }
        }
        let mut ring = self.ring.lock();
        if ring.len() >= RING {
            ring.pop_front();
        }
        ring.push_back(line);
        self.serial.fetch_add(1, Ordering::Relaxed);
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.flush();
        }
    }
}

fn open_file(dir: &Path) -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("openpremier.log");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

/// Installs the logger. `dir` receives `openpremier.log`; the previous session's log becomes
/// `openpremier.previous.log`.
pub fn init(dir: &Path, enabled: bool, level: &str) {
    let l = logger();
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("openpremier.log");
    let previous = dir.join("openpremier.previous.log");
    let too_big = |p: &Path| std::fs::metadata(p).is_ok_and(|m| m.len() > MAX_FILE);
    if too_big(&previous) {
        let _ = std::fs::remove_file(&previous);
    }
    if path.exists() {
        // a log that ran away (older versions logged libraries without limit) is not kept
        if too_big(&path) {
            let _ = std::fs::remove_file(&path);
        } else {
            let _ = std::fs::rename(&path, &previous);
        }
    }
    *l.dir.lock() = Some(dir.to_path_buf());
    if log::set_logger(l).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
    configure(enabled, level);
    log::info!("log started {} UTC", utc_now());
}

/// Changes the file output and the detail level while running.
pub fn configure(enabled: bool, level: &str) {
    let l = logger();
    let was = l.enabled.swap(enabled, Ordering::Relaxed);
    l.level.store(level_index(filter(level)), Ordering::Relaxed);
    let mut file = l.file.lock();
    if enabled && file.is_none() {
        if let Some(dir) = l.dir.lock().clone() {
            *file = open_file(&dir);
        }
    } else if !enabled && file.is_some() {
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "logging switched off");
        }
        *file = None;
    }
    drop(file);
    if enabled && !was {
        log::info!("logging switched on (level {level})");
    }
}

/// The recorded lines at or above `level`, oldest first.
pub fn recent(level: log::LevelFilter) -> Vec<Line> {
    logger()
        .ring
        .lock()
        .iter()
        .filter(|l| l.level <= level)
        .cloned()
        .collect()
}

/// Changes whenever a line is recorded.
pub fn serial() -> usize {
    logger().serial.load(Ordering::Relaxed)
}

pub fn clear() {
    logger().ring.lock().clear();
    logger().serial.fetch_add(1, Ordering::Relaxed);
}

/// The folder holding the log files and crash reports.
pub fn folder() -> Option<PathBuf> {
    logger().dir.lock().clone()
}

pub fn file_path() -> Option<PathBuf> {
    folder().map(|d| d.join("openpremier.log"))
}

/// Writes a crash report (always, even with logging off). Returns its path.
pub fn crash_report(text: &str) -> Option<PathBuf> {
    let dir = folder()?.join("CrashReports");
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("crash-{}.txt", crate::autosave::stamp(SystemTime::now()));
    let path = dir.join(name);
    let mut body = format!(
        "OpenPremier {} ({} {})\n{} UTC\n\n{text}\n\nRecent log:\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        utc_now()
    );
    for l in recent(log::LevelFilter::Trace).iter().rev().take(200).rev() {
        body.push_str(&l.format());
        body.push('\n');
    }
    std::fs::write(&path, body).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_ring() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), true, "debug");
        log::debug!(target: "op_test", "hello {}", 1);
        log::trace!(target: "op_test", "not recorded");
        let lines = recent(log::LevelFilter::Trace);
        assert!(lines.iter().any(|l| l.message == "hello 1"));
        assert!(!lines.iter().any(|l| l.message == "not recorded"));
        configure(false, "debug");
        log::info!(target: "op_test", "off");
        assert!(
            !recent(log::LevelFilter::Trace)
                .iter()
                .any(|l| l.message == "off")
        );
        configure(true, "info");
        let text = std::fs::read_to_string(dir.path().join("openpremier.log")).unwrap();
        assert!(text.contains("hello 1"));
        assert!(crash_report("test panic").is_some_and(|p| p.exists()));
        assert!(utc_now().len() == 19);
    }
}
