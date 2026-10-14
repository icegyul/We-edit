//! `We edit --remove-silence input output [options]`: cuts the silences (and optionally
//! fillers, repeated words and repeated takes) out of a video or audio file and writes the
//! result, without opening a window. It runs the same tools assistants use.

use op_application::Editor;
use serde_json::json;

use crate::tools;

pub const USAGE: &str = "\
Usage: We edit --remove-silence <input> <output> [options]
       weedit --remove-silence <input>... --out-dir <folder> [options]

  --out-dir <folder>     process every input file (video or audio) into this folder
  --suffix <text>        added to each output's name with --out-dir (default _cut)

  --preset <name>        gentle, podcast, tutorial, jump_cuts or noisy_room
  --method <name>        loudness, relative, adaptive (default), speech or neural
  --level <number>       the method's setting: dBFS for loudness (-40), dB under the loudest
                         for relative (30), dB over the noise for adaptive (9), 0-1 for
                         speech and neural (0.5)
  --min-silence <s>      shortest silence to cut (0.4)
  --pad <s>              air kept on both sides of every cut (0.1)
  --voice-only           music and noise count as silence (voice detector)
  --words                also cut filler words and repeated words (transcribes; the speech
                         model is downloaded once)
  --retakes              with --words: a sentence said again keeps only its last take
  --language <code>      spoken language for --words (default: detect)
  --speed <percent>      speed the silences up instead of cutting them
  --format <name>        output container: mp4 (default), mov (ProRes) or mxf (DNxHR) for video;
                         wav or m4a for audio. Without --out-dir the output's extension decides
";

fn value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn number(args: &[String], flag: &str) -> Result<Option<f64>, String> {
    value(args, flag)
        .map(|v| {
            v.parse::<f64>()
                .map_err(|_| format!("{flag} needs a number"))
        })
        .transpose()
}

/// The two paths: arguments that are neither flags nor the value of a flag.
fn paths(args: &[String]) -> Vec<&String> {
    let with_value = [
        "--preset",
        "--method",
        "--level",
        "--min-silence",
        "--pad",
        "--language",
        "--speed",
        "--format",
        "--out-dir",
        "--suffix",
    ];
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if with_value.contains(&a.as_str()) {
            skip = true;
        } else if !a.starts_with("--") {
            out.push(a);
        }
    }
    out
}

/// Runs the command; `args` follow `--remove-silence`. Returns what to print. Each file is cut by
/// an editor of its own (no window), one after the other.
pub fn remove_silence(
    args: &[String],
    dirs: &op_application::Dirs,
    prefs: &op_application::Preferences,
) -> Result<String, String> {
    use std::sync::atomic::AtomicBool;
    let files = paths(args);
    let out_dir = value(args, "--out-dir").map(std::path::PathBuf::from);
    let jobs: Vec<(std::path::PathBuf, std::path::PathBuf)> = match (&out_dir, &files[..]) {
        (None, [input, output]) => vec![(
            std::path::absolute(input).map_err(|e| e.to_string())?,
            std::path::absolute(output).map_err(|e| e.to_string())?,
        )],
        (Some(dir), inputs) if !inputs.is_empty() => {
            let suffix = value(args, "--suffix").unwrap_or_else(|| "_cut".into());
            let dir = std::path::absolute(dir).map_err(|e| e.to_string())?;
            let mut jobs = Vec::new();
            for i in inputs {
                let input = std::path::absolute(i).map_err(|e| e.to_string())?;
                let out = op_application::silence::output_name(&dir, &input, &suffix);
                jobs.push((input, out));
            }
            jobs
        }
        _ => return Err(USAGE.to_string()),
    };
    let mut cut = json!({"scope": "sequence"});
    if let Some(v) = value(args, "--preset") {
        cut["preset"] = json!(v);
    }
    let method = value(args, "--method");
    if let Some(m) = &method {
        cut["method"] = json!(m);
    }
    if let Some(level) = number(args, "--level")? {
        let key = match method.as_deref().unwrap_or("adaptive") {
            "loudness" => "threshold_db",
            "relative" => "below_peak_db",
            "adaptive" => "margin_db",
            _ => "sensitivity",
        };
        cut[key] = json!(level);
    }
    if let Some(v) = number(args, "--min-silence")? {
        cut["min_silence"] = json!(v);
    }
    if let Some(v) = number(args, "--pad")? {
        cut["pad_before"] = json!(v);
        cut["pad_after"] = json!(v);
    }
    if args.iter().any(|a| a == "--voice-only") {
        cut["voice_only"] = json!(true);
    }
    if args.iter().any(|a| a == "--words" || a == "--retakes") {
        let mut w = json!({"retakes": args.iter().any(|a| a == "--retakes")});
        if let Some(l) = value(args, "--language") {
            w["language"] = json!(l);
        }
        cut["words"] = w;
    }
    if let Some(v) = number(args, "--speed")? {
        cut["action"] = json!("speed_up");
        cut["speed_percent"] = json!(v);
    }
    let settings = tools::silence_settings(&cut)?;
    let gpu = op_render::Gpu::headless().map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    let mut report = Vec::new();
    let mut failed = 0;
    for (input, output) in &jobs {
        if !input.exists() {
            report.push(format!("{}: does not exist", input.display()));
            failed += 1;
            continue;
        }
        let mut ed = Editor::new(dirs.clone(), prefs.clone(), false);
        ed.background = true;
        match op_application::silence::cut_file(&mut ed, &gpu, input, output, &settings, &cancel) {
            Ok(d) => report.push(format!(
                "{}: {} cuts, {:.1} s of {:.1} s removed; {:.1} s written to {}",
                input
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                d.count,
                d.removed,
                d.before,
                d.after,
                output.display()
            )),
            Err(e) => {
                report.push(format!("{}: {e}", input.display()));
                failed += 1;
            }
        }
    }
    let text = report.join("\n");
    if failed == jobs.len() {
        Err(text)
    } else {
        Ok(text)
    }
}
