//! OpenPremier: command line, logging, portable mode and the self-test used by packaging.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

const USAGE: &str = "\
OpenPremier - a free and open source video editor

Usage: openpremier [options] [project or media files...]

Options:
  --portable     keep settings, caches and autosaves next to the program
  --self-test    check FFmpeg and the GPU renderer without opening a window
  --version      print the version
  --help         print this help
";

/// Starts the program log with the user's preferences (`OPENPREMIER_LOG` overrides the level)
/// and makes every failure leave a crash report and a recovery copy of the open project.
fn init_logging(dirs: &op_application::Dirs) {
    let prefs = op_application::Preferences::load(dirs);
    let level = std::env::var("OPENPREMIER_LOG").unwrap_or(prefs.log_level.clone());
    op_application::logging::init(&dirs.logs(), prefs.logging_enabled, &level);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!("failure in thread {thread}: {info}");
        let report = format!("Thread: {thread}\n{info}\n\n{backtrace}");
        if let Some(path) = op_application::logging::crash_report(&report) {
            log::error!("crash report written to {}", path.display());
        }
        op_application::recovery::emergency_save();
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
    init_logging(&dirs);
    log::info!(
        "OpenPremier {} starting ({} {}), settings in {}{}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        dirs.config.display(),
        if portable.is_some() {
            " (portable)"
        } else {
            ""
        }
    );
    op_media::init();
    log::info!("FFmpeg {}", op_media::ffmpeg_version());
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
    log::info!("OpenPremier closed normally");
    log::logger().flush();
}
