//! OpenPremier: command line, logging, portable mode and the self-test used by packaging.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

const USAGE: &str = "\
We edit - a video editor

Usage: We edit [options] [project or media files...]

Options:
  --portable     keep settings, caches and autosaves next to the program
  --self-test    check FFmpeg and the GPU renderer without opening a window
  --remove-silence <in> <out>
                 cut silences (and fillers, repeats) out of a file; see
                 --remove-silence --help for the options
  --mcp          serve AI assistants (Model Context Protocol over standard input and
                 output): edits the open We edit window when it allows assistants,
                 else a project of its own (the first project file given, if any)
  --version      print the version
  --help         print this help
";

/// Starts the program log with the user's preferences (`WEEDIT_LOG` overrides the level)
/// and makes every failure leave a crash report and a recovery copy of the open project.
fn init_logging(dirs: &op_application::Dirs) {
    let prefs = op_application::Preferences::load(dirs);
    let level = std::env::var("WEEDIT_LOG").unwrap_or(prefs.log_level.clone());
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

/// Prints a line where the user can read it. The program is a window application on Windows:
/// when its output is redirected (a pipe, a file) that is used; otherwise it joins the console it
/// was started from.
fn console(text: &str) {
    #[cfg(windows)]
    {
        use std::io::Write;
        unsafe extern "system" {
            fn AttachConsole(process: u32) -> i32;
            fn GetStdHandle(which: u32) -> isize;
        }
        // SAFETY: plain queries of this process's handles; AttachConsole joins the parent's console
        unsafe {
            let out = GetStdHandle(-11i32 as u32);
            if out == 0 || out == -1 {
                if AttachConsole(u32::MAX) != 0
                    && let Ok(mut con) = std::fs::OpenOptions::new().write(true).open("CONOUT$")
                {
                    let _ = writeln!(con, "{text}");
                }
                return;
            }
        }
    }
    println!("{text}");
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
    println!("We edit {}", env!("CARGO_PKG_VERSION"));
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

/// `--mcp`: hands assistants' tool calls to the open program, or to an editor of its own.
fn serve_assistants(dirs: op_application::Dirs, project: Option<&PathBuf>) {
    if let Some(mut open) = op_mcp::live::Remote::connect(&dirs.config) {
        log::info!("assistant session: editing the open program");
        op_mcp::serve_stdio(&mut open);
        return;
    }
    log::info!("assistant session: editing a project of its own");
    let prefs = op_application::Preferences::load(&dirs);
    let mut ed = op_application::Editor::new(dirs, prefs, false);
    if let Some(p) = project
        && let Err(e) = ed.open(p)
    {
        log::warn!("{}: {e}", p.display());
    }
    op_mcp::serve_stdio(&mut op_mcp::Local(ed));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("We edit {}", env!("CARGO_PKG_VERSION"));
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
        "We edit {} starting ({} {}), settings in {}{}",
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
    if let Some(at) = args.iter().position(|a| a == "--remove-silence") {
        let rest = &args[at + 1..];
        let prefs = op_application::Preferences::load(&dirs);
        let result = if rest.iter().any(|a| a == "--help" || a == "-h") {
            Ok(op_mcp::cli::USAGE.to_string())
        } else {
            op_mcp::cli::remove_silence(rest, &dirs, &prefs)
        };
        match result {
            Ok(text) => console(&text),
            Err(e) => {
                console(&format!("error: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--mcp") {
        serve_assistants(dirs, open.first());
        return;
    }
    let backend = op_application::Preferences::load(&dirs).graphics_backend;
    if let Err(e) = op_ui::run(op_ui::Options {
        open,
        portable,
        backend,
    }) {
        log::error!("{e}");
        eprintln!("We edit could not start: {e}");
        std::process::exit(1);
    }
    log::info!("We edit closed normally");
    log::logger().flush();
}
