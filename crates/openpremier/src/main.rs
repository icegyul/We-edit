//! OpenPremier: command line, logging, portable mode and the self-test used by packaging.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const USAGE: &str = "\
OpenPremier - a free and open source video editor

Usage: openpremier [options] [project or media files...]

Options:
  --portable     keep settings, caches and autosaves next to the program
  --self-test    check FFmpeg and the GPU renderer without opening a window
  --version      print the version
  --help         print this help
";

/// Logs to stderr and to a file in the data folder.
struct Logger {
    file: Mutex<Option<std::fs::File>>,
    level: log::LevelFilter,
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.level && (m.target().starts_with("op") || m.level() <= log::Level::Warn)
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("[{}] {}: {}\n", r.level(), r.target(), r.args());
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

fn init_logging(data: &Path) {
    let _ = std::fs::create_dir_all(data);
    let path = data.join("openpremier.log");
    // keep the previous session's log for bug reports
    if path.exists() {
        let _ = std::fs::rename(&path, data.join("openpremier.previous.log"));
    }
    let level = match std::env::var("OPENPREMIER_LOG").as_deref() {
        Ok("debug") => log::LevelFilter::Debug,
        Ok("trace") => log::LevelFilter::Trace,
        Ok("warn") => log::LevelFilter::Warn,
        _ => log::LevelFilter::Info,
    };
    let logger = Logger {
        file: Mutex::new(std::fs::File::create(&path).ok()),
        level,
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        prev(info);
    }));
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Renders a graphics frame on a headless GPU and checks FFmpeg (used by package checks).
fn self_test() -> Result<(), String> {
    use op_core::*;
    op_media::init();
    println!("OpenPremier {}", env!("CARGO_PKG_VERSION"));
    println!("FFmpeg {}", op_media::ffmpeg_version());
    for enc in ["libx264", "aac", "prores_ks", "png"] {
        println!(
            "encoder {enc}: {}",
            if op_media::has_encoder(enc) {
                "yes"
            } else {
                "no"
            }
        );
    }
    let gpu = op_render::Gpu::headless().map_err(|e| e.to_string())?;
    println!("GPU: {}", gpu.description());
    let mut p = Project::new("self-test");
    let root = p.root;
    let (sid, _) = p.add_sequence(
        root,
        "S",
        SequenceSettings {
            width: 320,
            height: 180,
            ..Default::default()
        },
    );
    let item = p.add_item(
        root,
        "Red",
        ItemKind::Synthetic {
            generator: Generator::ColorMatte {
                color: Rgba::new(1.0, 0.0, 0.0, 1.0),
            },
            duration: Dur::from_seconds(1.0),
        },
    );
    let spec = op_timeline::SourceClip::from_item(&p, item).map_err(|e| e.to_string())?;
    let patch = op_timeline::Patch {
        video: Some(0),
        audio: vec![Some(0), Some(1)],
    };
    let opts = op_timeline::EditOptions {
        linked_selection: true,
        ripple_markers: false,
    };
    let (p, _) = p
        .transact(|p| op_timeline::overwrite(p, sid, &spec, SeqTime::ZERO, &patch, opts))
        .map_err(|e| e.to_string())?;
    let mut r = op_render::Renderer::new(gpu);
    let frame = r.render(&op_render::Request {
        project: &p,
        sequence: sid,
        time: SeqTime::ZERO,
        scale: 1.0,
        source: &op_render::NoFrames,
    });
    let rgba = r.rgba8(&frame);
    let center = (90 * 320 + 160) * 4;
    let px = &rgba[center..center + 4];
    println!("center pixel: {px:?}");
    if px[0] < 200 || px[1] > 40 {
        return Err("the rendered frame is wrong".into());
    }
    println!("self-test passed");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("OpenPremier {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--self-test") {
        match self_test() {
            Ok(()) => return,
            Err(e) => {
                eprintln!("self-test failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let portable_marker = exe_dir()
        .map(|d| d.join("portable.txt"))
        .is_some_and(|p| p.exists());
    let portable = (args.iter().any(|a| a == "--portable") || portable_marker)
        .then(|| exe_dir().unwrap_or_default().join("UserData"));
    let dirs = match &portable {
        Some(root) => op_application::Dirs::portable(root),
        None => op_application::Dirs::system(),
    };
    init_logging(&dirs.data);
    log::info!(
        "OpenPremier {} starting ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    op_media::init();
    let open: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if let Err(e) = op_ui::run(op_ui::Options { open, portable }) {
        log::error!("{e}");
        eprintln!("OpenPremier could not start: {e}");
        std::process::exit(1);
    }
}
