//! The editing tools offered to assistants, and their execution on an editor. Times are seconds
//! of the sequence; tracks are named like in the timeline ("V1", "A2"); every change is one
//! undo step, so the user can take back what an assistant did with Edit > Undo.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use op_application::captions::{CaptionOptions, ModelSize, Task, TranscribeOptions};
use op_application::{Editor, Focus};
use op_core::catalog::{self, EffectKind, ParamKind};
use op_core::*;
use op_timeline::{MoveSpec, Patch, SourceClip};
use serde_json::{Value as Json, json};

/// What a tool hands back.
pub enum Content {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
    /// An MCP content block made elsewhere (the open program's answer).
    Block(Json),
}

type R = Result<Vec<Content>, String>;

fn text(v: Json) -> R {
    Ok(vec![Content::Text(
        serde_json::to_string_pretty(&v).unwrap_or_default(),
    )])
}

fn tool(name: &str, description: &str, properties: Json, required: &[&str]) -> Json {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

/// The tool definitions for `tools/list`.
pub fn list() -> Json {
    let num = |d: &str| json!({"type": "number", "description": d});
    let int = |d: &str| json!({"type": "integer", "description": d});
    let s = |d: &str| json!({"type": "string", "description": d});
    let ids = |d: &str| json!({"type": "array", "items": {"type": "integer"}, "description": d});
    json!([
        tool(
            "get_project",
            "The open project: its file, media (with ids, durations, sizes) and sequences.",
            json!({}),
            &[]
        ),
        tool(
            "new_project",
            "Starts a new empty project (unsaved changes of the current one are lost unless saved first).",
            json!({"name": s("Project name")}),
            &[]
        ),
        tool(
            "open_project",
            "Opens an OpenPremier project (.opproj), or imports a Premiere Pro (.prproj), OTIO, FCP XML or EDL file.",
            json!({"path": s("Path of the project file")}),
            &["path"]
        ),
        tool(
            "save_project",
            "Saves the project, to `path` when given (.opproj).",
            json!({"path": s("Where to save; omit to save to the current file")}),
            &[]
        ),
        tool(
            "import_media",
            "Imports video, audio and picture files into the project and waits until they are read. Returns the new media ids.",
            json!({"paths": {"type": "array", "items": {"type": "string"}, "description": "Absolute file paths"}}),
            &["paths"]
        ),
        tool(
            "create_sequence",
            "Creates a sequence and makes it the active one; with `media_id` its settings match that media.",
            json!({"name": s("Sequence name"), "width": int("Frame width (default 1920)"), "height": int("Frame height (default 1080)"), "fps": num("Frame rate (default 25)"), "media_id": int("Media whose size and rate the sequence takes")}),
            &[]
        ),
        tool(
            "open_sequence",
            "Makes a sequence the active one.",
            json!({"sequence_id": int("Sequence id")}),
            &["sequence_id"]
        ),
        tool(
            "get_timeline",
            "The active sequence: tracks with their clips (id, name, start and end in seconds, effects with their parameters), transitions, markers and the playhead.",
            json!({"detail": json!({"type": "boolean", "description": "Include every effect parameter (default false)"})}),
            &[]
        ),
        tool(
            "add_clip",
            "Puts media on the active sequence. Video goes to `track` (V1 by default) and its sound to the audio track of the same number.",
            json!({"media_id": int("Media id"), "at": num("Sequence time in seconds (default: the playhead)"), "track": s("Video or audio track, like V2 or A1"), "source_in": num("Start inside the media, seconds"), "source_out": num("End inside the media, seconds"), "insert": json!({"type": "boolean", "description": "Push later clips to the right instead of overwriting"})}),
            &["media_id"]
        ),
        tool(
            "delete_clips",
            "Removes clips (with their linked audio or video).",
            json!({"clip_ids": ids("Clip ids"), "ripple": json!({"type": "boolean", "description": "Close the gap left behind"})}),
            &["clip_ids"]
        ),
        tool(
            "move_clips",
            "Moves clips by a time offset and/or tracks up or down.",
            json!({"clip_ids": ids("Clip ids"), "by": num("Seconds, negative to the left"), "tracks": int("Tracks up (positive) or down")}),
            &["clip_ids"]
        ),
        tool(
            "split_clips",
            "Cuts clips at a time (like the Razor tool). Returns the ids of the right-hand parts.",
            json!({"at": num("Sequence time in seconds"), "clip_ids": ids("Clips to cut; omit to cut every clip at that time")}),
            &["at"]
        ),
        tool(
            "set_speed",
            "Changes clip speed (100 = normal) and direction.",
            json!({"clip_ids": ids("Clip ids"), "percent": num("Speed in percent"), "reverse": json!({"type": "boolean"}), "ripple": json!({"type": "boolean", "description": "Move later clips with the new duration"})}),
            &["clip_ids", "percent"]
        ),
        tool(
            "list_effects",
            "Searches the effects and transitions that can be added: ids, names, categories and parameters.",
            json!({"query": s("Words to look for in the name or category"), "kind": s("video, audio, transition or graphic")}),
            &[]
        ),
        tool(
            "add_effect",
            "Adds an effect to clips, optionally with parameter values.",
            json!({"clip_ids": ids("Clip ids"), "effect_id": s("Effect id from list_effects"), "params": json!({"type": "object", "description": "Parameter key -> value (number, boolean, text, [x, y] or [r, g, b(, a)] 0..1)"})}),
            &["clip_ids", "effect_id"]
        ),
        tool(
            "set_effect_params",
            "Changes parameters of an effect on a clip, including Motion, Opacity and Volume.",
            json!({"clip_id": int("Clip id"), "effect": s("Effect id, or its index in the clip's effect list"), "params": json!({"type": "object", "description": "Parameter key -> value"})}),
            &["clip_id", "effect", "params"]
        ),
        tool(
            "remove_effect",
            "Removes an effect from a clip.",
            json!({"clip_id": int("Clip id"), "effect": s("Effect id, or its index")}),
            &["clip_id", "effect"]
        ),
        tool(
            "add_transition",
            "Adds a transition at the cut where a clip ends (or starts).",
            json!({"clip_id": int("Clip id"), "effect_id": s("Transition id from list_effects (default: Cross Dissolve)"), "duration": num("Seconds (default 1)"), "at_start": json!({"type": "boolean", "description": "At the clip's start instead of its end"})}),
            &["clip_id"]
        ),
        tool(
            "add_text",
            "Adds a title (a text graphics clip) on the first free video track.",
            json!({"text": s("The words"), "at": num("Start, seconds (default: the playhead)"), "duration": num("Seconds (default 5)"), "params": json!({"type": "object", "description": "Text parameters, like font, font_size, fill, position"})}),
            &["text"]
        ),
        tool(
            "transcribe_captions",
            "Transcribes the sequence's speech on this computer and adds animated captions (downloads the speech model once). Waits until done.",
            json!({"language": s("Spoken language code like es or en (default: detect)"), "translate_to_english": json!({"type": "boolean"}), "style": int("Caption style 0-11 (2 = Karaoke)"), "model": s("tiny, base (default) or small"), "max_words": int("Words per caption (default 4)")}),
            &[]
        ),
        tool(
            "set_caption_style",
            "Changes every caption of the sequence at once (style, font, font_size, fill, highlight, position, uppercase...).",
            json!({"params": json!({"type": "object"})}),
            &["params"]
        ),
        tool(
            "add_marker",
            "Adds a sequence marker.",
            json!({"at": num("Seconds"), "name": s("Marker name")}),
            &["at"]
        ),
        tool(
            "set_playhead",
            "Moves the playhead.",
            json!({"at": num("Seconds")}),
            &["at"]
        ),
        tool(
            "render_frame",
            "Renders the active sequence at a time and returns the picture, to check how it looks.",
            json!({"at": num("Seconds (default: the playhead)"), "width": int("Picture width (default 960)")}),
            &[]
        ),
        tool(
            "export",
            "Exports the active sequence and waits until the file is written.",
            json!({"path": s("Output file (.mp4, .mov, .mxf, .png)"), "format": s("h264 (default), hevc, prores, prores4444, dnxhr or png"), "start": num("Seconds (default 0)"), "end": num("Seconds (default: the end of the sequence)"), "width": int("Default: the sequence's"), "height": int("Default: the sequence's")}),
            &["path"]
        ),
        tool(
            "run_command",
            "Runs any menu or keyboard command by id (the ids of Edit > Keyboard Shortcuts, like cmd.sequence.razorateditline), acting on the selection and playhead.",
            json!({"command": s("Command id"), "clip_ids": ids("Clips to select first")}),
            &["command"]
        ),
        tool("undo", "Undoes the last change.", json!({}), &[]),
        tool("redo", "Redoes the last undone change.", json!({}), &[]),
    ])
}

fn f64_arg(a: &Json, k: &str) -> Option<f64> {
    a.get(k).and_then(Json::as_f64)
}

fn ids_arg(a: &Json, k: &str) -> Vec<ClipId> {
    a.get(k)
        .and_then(Json::as_array)
        .map(|v| v.iter().filter_map(Json::as_u64).map(ClipId).collect())
        .unwrap_or_default()
}

fn str_arg<'a>(a: &'a Json, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Json::as_str)
}

fn secs(t: SeqTime) -> f64 {
    (t.seconds() * 1000.0).round() / 1000.0
}

fn track_name(r: TrackRef) -> String {
    format!(
        "{}{}",
        if r.kind == TrackKind::Video { "V" } else { "A" },
        r.index + 1
    )
}

fn parse_track(s: &str) -> Result<TrackRef, String> {
    let s = s.trim().to_ascii_uppercase();
    let (kind, n) = s.split_at(1.min(s.len()));
    let n: usize = n.parse().map_err(|_| format!("not a track: {s}"))?;
    if n == 0 {
        return Err(format!("not a track: {s}"));
    }
    match kind {
        "V" => Ok(TrackRef::video(n - 1)),
        "A" => Ok(TrackRef::audio(n - 1)),
        _ => Err(format!("not a track: {s} (use V1, A1...)")),
    }
}

fn active(ed: &Editor) -> Result<SequenceId, String> {
    ed.active
        .ok_or_else(|| "no sequence is open: use create_sequence or open_sequence".to_string())
}

/// A JSON value as a parameter value of the kind the effect expects.
pub fn to_value(kind: &ParamKind, v: &Json) -> Result<Value, String> {
    let nums = |v: &Json| -> Option<Vec<f64>> {
        v.as_array()
            .map(|a| a.iter().filter_map(Json::as_f64).collect())
    };
    Ok(match kind {
        ParamKind::Float { .. } | ParamKind::Angle { .. } => {
            Value::Float(v.as_f64().ok_or("a number is expected")?)
        }
        ParamKind::Int { .. } => Value::Int(v.as_f64().ok_or("a number is expected")? as i64),
        ParamKind::Bool { .. } => Value::Bool(
            v.as_bool()
                .or_else(|| v.as_f64().map(|x| x != 0.0))
                .ok_or("true or false is expected")?,
        ),
        ParamKind::Choice { options, .. } => match v {
            Json::String(s) => Value::Choice(
                options
                    .iter()
                    .position(|o| o.eq_ignore_ascii_case(s))
                    .ok_or_else(|| format!("one of {options:?} is expected"))?
                    as u32,
            ),
            _ => Value::Choice(v.as_u64().ok_or("an option number or name is expected")? as u32),
        },
        ParamKind::Color { .. } => {
            let c = match v {
                Json::String(s) => hex(s).ok_or("a color like #ff8800 is expected")?,
                _ => nums(v).ok_or("[r, g, b] from 0 to 1 is expected")?,
            };
            if c.len() < 3 {
                return Err("[r, g, b] from 0 to 1 is expected".into());
            }
            Value::Color(Rgba::new(
                c[0] as f32,
                c[1] as f32,
                c[2] as f32,
                c.get(3).copied().unwrap_or(1.0) as f32,
            ))
        }
        ParamKind::Point { .. } => {
            let p = nums(v).ok_or("[x, y] from 0 to 1 is expected")?;
            if p.len() < 2 {
                return Err("[x, y] from 0 to 1 is expected".into());
            }
            Value::Point([p[0], p[1]])
        }
        ParamKind::Text { .. } | ParamKind::File => {
            Value::Text(v.as_str().ok_or("text is expected")?.to_string())
        }
        ParamKind::Curve => return Err("curves cannot be set by assistants yet".into()),
    })
}

fn hex(s: &str) -> Option<Vec<f64>> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 && s.len() != 8 {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| {
            u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .ok()
                .map(|b| b as f64 / 255.0)
        })
        .collect()
}

fn value_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!((f * 1000.0).round() / 1000.0),
        Value::Point(p) => json!(p),
        Value::Color(c) => json!([c.r, c.g, c.b, c.a]),
        Value::Choice(c) => json!(c),
        Value::Text(t) => json!(t),
        Value::Curve(c) => json!(c),
    }
}

/// Sets parameters on a component, checking each key against the catalog.
fn set_params(comp: &mut Component, params: &Json) -> Result<usize, String> {
    let Some(obj) = params.as_object() else {
        return Ok(0);
    };
    let def = comp
        .def()
        .ok_or_else(|| format!("{} is not a known effect", comp.effect))?;
    let mut n = 0;
    for (k, v) in obj {
        let spec = def.params.iter().find(|p| p.key == k).ok_or_else(|| {
            let keys: Vec<&str> = def.params.iter().map(|p| p.key).collect();
            format!("{} has no parameter {k}; it has {keys:?}", def.name)
        })?;
        let value = to_value(&spec.kind, v).map_err(|e| format!("{k}: {e}"))?;
        match comp.param_mut(k) {
            Some(p) => {
                p.value = value;
                p.animated = false;
                p.keys.clear();
            }
            None => comp.params.push(Param::new(spec.key, value)),
        }
        n += 1;
    }
    Ok(n)
}

fn media_json(p: &Project, id: ItemId, it: &ProjectItem) -> Option<Json> {
    match &it.kind {
        ItemKind::Media { asset, .. } => {
            let a = p.asset(*asset)?;
            Some(json!({
                "media_id": id.0,
                "name": it.name,
                "path": a.path,
                "kind": format!("{:?}", a.kind),
                "duration": (a.duration.seconds() * 1000.0).round() / 1000.0,
                "width": a.video.as_ref().map(|v| v.width),
                "height": a.video.as_ref().map(|v| v.height),
                "fps": a.frame_rate().map(|r| (r.as_f64() * 1000.0).round() / 1000.0),
                "audio_streams": a.audio.len(),
                "offline": !Path::new(&a.path).exists(),
            }))
        }
        ItemKind::Synthetic { duration, .. } => Some(json!({
            "media_id": id.0, "name": it.name, "kind": "Generated", "duration": duration.seconds(),
        })),
        _ => None,
    }
}

fn find_effect<'a>(c: &'a mut Clip, which: &str) -> Result<&'a mut Component, String> {
    if let Ok(i) = which.parse::<usize>() {
        let n = c.components.len();
        return c
            .components
            .get_mut(i)
            .ok_or_else(|| format!("the clip has {n} effects (0..{})", n.saturating_sub(1)));
    }
    let effect = catalog::find(which)
        .map(|d| d.id)
        .or_else(|| {
            catalog::CATALOG
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(which))
                .map(|d| d.id)
        })
        .unwrap_or(which);
    c.components
        .iter_mut()
        .find(|x| x.effect == effect)
        .ok_or_else(|| format!("the clip has no effect {which}"))
}

/// Waits for background work the editor polls (imports, captions, exports).
fn wait(ed: &mut Editor, secs: u64, mut busy: impl FnMut(&Editor) -> bool) -> bool {
    let start = Instant::now();
    while busy(ed) {
        if start.elapsed() > Duration::from_secs(secs) {
            return false;
        }
        ed.tick();
        std::thread::sleep(Duration::from_millis(15));
    }
    true
}

/// Runs `f` as one undo step: a tool that changes the project in several steps (a title, then
/// its text, then its length) is taken back by one Undo, named after the tool.
fn one_step(ed: &mut Editor, name: &str, f: impl FnOnce(&mut Editor) -> R) -> R {
    if matches!(name, "undo" | "redo" | "open_project" | "new_project") {
        return f(ed);
    }
    let depth = ed.history.labels().1;
    let result = f(ed);
    let (labels, now) = ed.history.labels();
    let added = now.saturating_sub(depth);
    if added > 1 && labels.len() == now {
        let after = ed.project.clone();
        let label = labels.last().map(|l| l.to_string()).unwrap_or_default();
        let (active, selection, playhead) = (ed.active, ed.selection.clone(), ed.playhead());
        for _ in 0..added {
            ed.undo();
        }
        ed.edit(&label, |p| {
            *p = after;
            Ok(())
        });
        if let Some(s) = active
            && ed.active != Some(s)
            && ed.project.sequence(s).is_some()
        {
            ed.open_sequence(s);
        }
        ed.selection = selection;
        ed.set_playhead(playhead);
    }
    result
}

/// Runs a tool on the open program's editor: exports and transcriptions start and show their
/// progress in the window instead of holding it until they finish.
pub fn call_live(ed: &mut Editor, name: &str, a: &Json) -> R {
    one_step(ed, name, |ed| live(ed, name, a))
}

fn live(ed: &mut Editor, name: &str, a: &Json) -> R {
    match name {
        "open_project" | "new_project" if ed.history.is_dirty() => Err(
            "the open project has unsaved changes: ask the user to save it before opening another"
                .into(),
        ),
        "export" => crate::render::export_in_background(ed, a),
        "transcribe_captions" => {
            let options = transcribe_options(a);
            ed.status = None;
            if !ed.transcribe_captions(options) {
                return Err(status_error(ed, "a transcription is already running"));
            }
            text(
                json!({"started": true, "note": "the captions appear when the transcription ends; check get_timeline"}),
            )
        }
        _ => run(ed, name, a),
    }
}

fn transcribe_options(a: &Json) -> TranscribeOptions {
    let model = match str_arg(a, "model").unwrap_or("base") {
        "tiny" => ModelSize::Tiny,
        "small" => ModelSize::Small,
        _ => ModelSize::Base,
    };
    let mut captions = CaptionOptions::default();
    if let Some(s) = a.get("style").and_then(Json::as_u64) {
        captions.style = s as u32;
    }
    if let Some(w) = a.get("max_words").and_then(Json::as_u64) {
        captions.max_words = w.clamp(1, 12) as usize;
    }
    TranscribeOptions {
        model,
        language: str_arg(a, "language").map(str::to_string),
        task: if a.get("translate_to_english").and_then(Json::as_bool) == Some(true) {
            Task::Translate
        } else {
            Task::Transcribe
        },
        captions,
    }
}

/// Runs a tool on the editor (as one undo step).
pub fn call(ed: &mut Editor, name: &str, a: &Json) -> R {
    one_step(ed, name, |ed| run(ed, name, a))
}

fn run(ed: &mut Editor, name: &str, a: &Json) -> R {
    match name {
        "get_project" => {
            let p = &ed.project;
            let media: Vec<Json> = p
                .items
                .iter()
                .filter_map(|(id, it)| media_json(p, *id, it))
                .collect();
            let sequences: Vec<Json> = p
                .sequences
                .values()
                .map(|s| {
                    json!({
                        "sequence_id": s.id.0, "name": s.name,
                        "width": s.settings.width, "height": s.settings.height,
                        "fps": (s.rate().as_f64() * 1000.0).round() / 1000.0,
                        "duration": secs(SeqTime::ZERO + s.duration()),
                        "video_tracks": s.video.len(), "audio_tracks": s.audio.len(),
                    })
                })
                .collect();
            text(json!({
                "name": p.name,
                "file": ed.path.as_ref().map(|x| x.display().to_string()),
                "unsaved_changes": ed.history.is_dirty(),
                "active_sequence": ed.active.map(|s| s.0),
                "media": media,
                "sequences": sequences,
            }))
        }
        "new_project" => {
            ed.new_project(str_arg(a, "name").unwrap_or("Untitled"));
            text(json!({"ok": true}))
        }
        "open_project" => {
            let path = PathBuf::from(str_arg(a, "path").ok_or("path is required")?);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "opproj" {
                ed.open(&path)?;
            } else {
                ed.new_project("Untitled");
                let root = ed.project.root;
                ed.import(vec![path], root);
            }
            if ed.active.is_none()
                && let Some(s) = ed.project.sequences.keys().next().copied()
            {
                ed.open_sequence(s);
            }
            call(ed, "get_project", &json!({}))
        }
        "save_project" => {
            match str_arg(a, "path") {
                Some(p) => ed.save_as(Path::new(p))?,
                None if ed.path.is_some() => ed.save()?,
                None => return Err("the project has no file yet: give a path".into()),
            }
            text(json!({"saved": ed.path.as_ref().map(|p| p.display().to_string())}))
        }
        "import_media" => {
            let paths: Vec<PathBuf> = a
                .get("paths")
                .and_then(Json::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Json::as_str)
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default();
            if paths.is_empty() {
                return Err("paths is required".into());
            }
            for p in &paths {
                if !p.exists() {
                    return Err(format!("{} does not exist", p.display()));
                }
            }
            let before: Vec<ItemId> = ed.project.items.keys().copied().collect();
            let bin = ed.project.root;
            ed.status = None;
            ed.import(paths, bin);
            wait(ed, 600, |e| e.importing > 0 || !e.imports.is_empty());
            let p = &ed.project;
            let new: Vec<Json> = p
                .items
                .iter()
                .filter(|(id, _)| !before.contains(id))
                .filter_map(|(id, it)| media_json(p, *id, it))
                .collect();
            let errors = ed
                .status
                .as_ref()
                .filter(|s| s.error)
                .map(|s| s.text.clone());
            text(json!({"imported": new, "error": errors}))
        }
        "create_sequence" => {
            let name = str_arg(a, "name").unwrap_or("Sequence").to_string();
            let sid = if let Some(m) = a.get("media_id").and_then(Json::as_u64) {
                let sid = ed
                    .sequence_from_item(ItemId(m))
                    .ok_or("that media cannot start a sequence")?;
                // keep the media on it, renamed as asked
                ed.edit("Rename Sequence", |p| {
                    if let Some(s) = p.sequence_mut(sid) {
                        s.name = name.clone();
                    }
                    Ok(())
                });
                sid
            } else {
                let mut settings = ed.prefs.default_sequence.clone();
                if let Some(w) = a.get("width").and_then(Json::as_u64) {
                    settings.width = w as u32;
                }
                if let Some(h) = a.get("height").and_then(Json::as_u64) {
                    settings.height = h as u32;
                }
                if let Some(f) = f64_arg(a, "fps") {
                    settings.rate = Rate::from_f64(f);
                }
                ed.new_sequence(&name, settings)
                    .ok_or("the sequence could not be created")?
            };
            ed.open_sequence(sid);
            text(json!({"sequence_id": sid.0}))
        }
        "open_sequence" => {
            let sid = SequenceId(
                a.get("sequence_id")
                    .and_then(Json::as_u64)
                    .ok_or("sequence_id is required")?,
            );
            ed.project.sequence(sid).ok_or("no such sequence")?;
            ed.open_sequence(sid);
            text(json!({"ok": true}))
        }
        "get_timeline" => {
            let sid = active(ed)?;
            let detail = a.get("detail").and_then(Json::as_bool).unwrap_or(false);
            let s = ed.project.sequence(sid).ok_or("no sequence")?;
            let track = |r: TrackRef, t: &Track| {
                let clips: Vec<Json> = t
                    .clips
                    .iter()
                    .map(|c| {
                        let effects: Vec<Json> = c
                            .components
                            .iter()
                            .enumerate()
                            .map(|(i, x)| {
                                let mut e = json!({
                                    "index": i,
                                    "effect": x.effect,
                                    "name": x.def().map(|d| d.name).unwrap_or(""),
                                    "enabled": x.enabled,
                                });
                                let show = detail
                                    || x.effect == catalog::TEXT
                                    || x.effect == catalog::CAPTION;
                                if show {
                                    e["params"] = x
                                        .params
                                        .iter()
                                        .filter(|p| !p.key.starts_with('_'))
                                        .map(|p| (p.key.clone(), value_json(&p.value)))
                                        .collect::<serde_json::Map<_, _>>()
                                        .into();
                                }
                                e
                            })
                            .collect();
                        json!({
                            "clip_id": c.id.0,
                            "name": c.name,
                            "start": secs(c.start),
                            "end": secs(c.end()),
                            "source_in": (c.source_in.seconds() * 1000.0).round() / 1000.0,
                            "speed_percent": c.speed.as_f64() * 100.0,
                            "reverse": c.reverse,
                            "enabled": c.enabled,
                            "linked": c.link.is_some(),
                            "effects": effects,
                        })
                    })
                    .collect();
                json!({"track": track_name(r), "name": t.name, "locked": t.locked, "clips": clips})
            };
            let mut tracks = Vec::new();
            for (i, t) in s.video.iter().enumerate().rev() {
                tracks.push(track(TrackRef::video(i), t));
            }
            for (i, t) in s.audio.iter().enumerate() {
                tracks.push(track(TrackRef::audio(i), t));
            }
            let markers: Vec<Json> = s
                .markers
                .iter()
                .map(|m| json!({"at": secs(m.start), "name": m.name}))
                .collect();
            text(json!({
                "sequence_id": sid.0,
                "name": s.name,
                "width": s.settings.width, "height": s.settings.height,
                "fps": (s.rate().as_f64() * 1000.0).round() / 1000.0,
                "duration": secs(SeqTime::ZERO + s.duration()),
                "playhead": secs(ed.playhead()),
                "selected_clips": ed.selection.clips.iter().map(|c| c.0).collect::<Vec<_>>(),
                "tracks": tracks,
                "markers": markers,
            }))
        }
        "add_clip" => {
            let sid = active(ed)?;
            let item = ItemId(
                a.get("media_id")
                    .and_then(Json::as_u64)
                    .ok_or("media_id is required")?,
            );
            let mut spec = SourceClip::from_item(&ed.project, item).map_err(|e| e.to_string())?;
            if let Some(i) = f64_arg(a, "source_in") {
                spec.range.start = SrcTime::from_seconds(i.max(0.0));
            }
            if let Some(o) = f64_arg(a, "source_out") {
                let end = SrcTime::from_seconds(o);
                if end <= spec.range.start {
                    return Err("source_out must be after source_in".into());
                }
                spec.range.end = end.min(spec.range.end);
            }
            let at = f64_arg(a, "at")
                .map(SeqTime::from_seconds)
                .unwrap_or_else(|| ed.playhead());
            let n = match str_arg(a, "track") {
                Some(t) => parse_track(t)?.index,
                None => 0,
            };
            let patch = Patch {
                video: spec.video.as_ref().map(|_| n),
                audio: (0..spec.audio.len()).map(|i| Some(n + i)).collect(),
            };
            let insert = a.get("insert").and_then(Json::as_bool).unwrap_or(false);
            let ids = ed
                .seq_edit(
                    if insert { "Insert" } else { "Overwrite" },
                    |p, sid, opts| {
                        let s = p.sequence(sid).unwrap();
                        let (need_v, need_a) = (
                            patch.video.map(|v| v + 1).unwrap_or(0),
                            patch
                                .audio
                                .iter()
                                .flatten()
                                .max()
                                .map(|x| x + 1)
                                .unwrap_or(0),
                        );
                        let (have_v, have_a) = (s.video.len(), s.audio.len());
                        for _ in have_v..need_v {
                            let tid = p.ids.track();
                            p.sequence_mut(sid)
                                .unwrap()
                                .video
                                .push(Arc::new(Track::new(tid, TrackKind::Video)));
                        }
                        for _ in have_a..need_a {
                            let tid = p.ids.track();
                            p.sequence_mut(sid)
                                .unwrap()
                                .audio
                                .push(Arc::new(Track::new(tid, TrackKind::Audio)));
                        }
                        if insert {
                            op_timeline::insert(p, sid, &spec, at, &patch, opts)
                        } else {
                            op_timeline::overwrite(p, sid, &spec, at, &patch, opts)
                        }
                    },
                )
                .ok_or_else(|| status_error(ed, "the clip could not be placed"))?;
            let _ = sid;
            text(json!({"clip_ids": ids.iter().map(|c| c.0).collect::<Vec<_>>()}))
        }
        "delete_clips" => {
            let clips = ids_arg(a, "clip_ids");
            let ripple = a.get("ripple").and_then(Json::as_bool).unwrap_or(false);
            ed.selection = op_application::Selection::only(clips);
            ed.delete_selection(ripple);
            text(json!({"ok": true}))
        }
        "move_clips" => {
            let spec = MoveSpec {
                clips: ids_arg(a, "clip_ids"),
                delta: Dur::from_seconds(f64_arg(a, "by").unwrap_or(0.0)),
                track_delta: a.get("tracks").and_then(Json::as_i64).unwrap_or(0) as i32,
                insert: false,
                duplicate: false,
            };
            ed.seq_edit("Move", |p, sid, opts| {
                op_timeline::move_clips(p, sid, &spec, opts)
            })
            .ok_or_else(|| status_error(ed, "the clips could not be moved"))?;
            text(json!({"ok": true}))
        }
        "split_clips" => {
            let sid = active(ed)?;
            let at = SeqTime::from_seconds(f64_arg(a, "at").ok_or("at is required")?);
            let mut clips = ids_arg(a, "clip_ids");
            if clips.is_empty() {
                let s = ed.project.sequence(sid).ok_or("no sequence")?;
                clips = s
                    .video
                    .iter()
                    .chain(&s.audio)
                    .flat_map(|t| &t.clips)
                    .filter(|c| c.start < at && c.end() > at)
                    .map(|c| c.id)
                    .collect();
            }
            let right = ed
                .seq_edit("Razor", |p, sid, opts| {
                    op_timeline::razor(p, sid, &clips, at, opts)
                })
                .ok_or_else(|| status_error(ed, "nothing was cut there"))?;
            text(json!({"right_parts": right.iter().map(|c| c.0).collect::<Vec<_>>()}))
        }
        "set_speed" => {
            ed.selection = op_application::Selection::only(ids_arg(a, "clip_ids"));
            ed.set_speed(
                f64_arg(a, "percent").ok_or("percent is required")?,
                a.get("reverse").and_then(Json::as_bool).unwrap_or(false),
                a.get("ripple").and_then(Json::as_bool).unwrap_or(false),
            );
            text(json!({"ok": true}))
        }
        "list_effects" => {
            let q = str_arg(a, "query").unwrap_or("").to_lowercase();
            let kind = str_arg(a, "kind").unwrap_or("").to_lowercase();
            let found: Vec<Json> = catalog::CATALOG
                .iter()
                .filter(|d| {
                    let k = match d.kind {
                        EffectKind::VideoEffect => "video",
                        EffectKind::AudioEffect => "audio",
                        EffectKind::VideoTransition | EffectKind::AudioTransition => "transition",
                        EffectKind::Graphic => "graphic",
                        _ => "fixed",
                    };
                    (kind.is_empty() || kind == k)
                        && (q.is_empty()
                            || d.name.to_lowercase().contains(&q)
                            || d.category.to_lowercase().contains(&q)
                            || d.id.contains(&q))
                })
                .map(|d| {
                    let params: Vec<Json> = d
                        .params
                        .iter()
                        .filter(|p| !p.group.starts_with('_'))
                        .map(|p| {
                            let (kind, extra) = match p.kind {
                                ParamKind::Float { min, max, .. } => {
                                    ("number", json!({"min": min, "max": max}))
                                }
                                ParamKind::Int { min, max, .. } => {
                                    ("integer", json!({"min": min, "max": max}))
                                }
                                ParamKind::Angle { .. } => ("degrees", json!({})),
                                ParamKind::Bool { .. } => ("boolean", json!({})),
                                ParamKind::Choice { options, .. } => {
                                    ("choice", json!({"options": options}))
                                }
                                ParamKind::Color { .. } => ("color [r,g,b,a] 0..1", json!({})),
                                ParamKind::Point { .. } => ("point [x,y] 0..1", json!({})),
                                ParamKind::Text { .. } => ("text", json!({})),
                                ParamKind::File => ("file", json!({})),
                                ParamKind::Curve => ("curve", json!({})),
                            };
                            let mut j = json!({
                                "key": p.key, "label": p.label, "type": kind,
                                "default": value_json(&p.default_value()),
                            });
                            if let (Some(o), Some(e)) = (j.as_object_mut(), extra.as_object()) {
                                o.extend(e.clone());
                            }
                            j
                        })
                        .collect();
                    json!({"effect_id": d.id, "name": d.name, "category": d.category, "kind": format!("{:?}", d.kind), "params": params})
                })
                .take(60)
                .collect();
            text(json!({"count": found.len(), "effects": found}))
        }
        "add_effect" => {
            let clips = ids_arg(a, "clip_ids");
            let effect = str_arg(a, "effect_id").ok_or("effect_id is required")?;
            let def = catalog::find(effect)
                .ok_or_else(|| format!("no effect {effect}: look it up with list_effects"))?;
            let params = a.get("params").cloned().unwrap_or(Json::Null);
            // check the parameters before changing anything
            let mut probe = Component::new(def, &mut ed.project.ids.clone());
            set_params(&mut probe, &params)?;
            let sid = active(ed)?;
            ed.edit(&format!("Add {}", def.name), |p| {
                let mut ids = p.ids.clone();
                let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                let mut n = 0;
                for id in &clips {
                    if let Some(c) = s.clip_mut(*id) {
                        let mut comp = Component::new(def, &mut ids);
                        let _ = set_params(&mut comp, &params);
                        c.components.push(comp);
                        n += 1;
                    }
                }
                p.ids = ids;
                if n == 0 {
                    return Err(EditError::NotFound("clips".into()));
                }
                Ok(())
            })
            .ok_or_else(|| status_error(ed, "none of those clips exists"))?;
            text(json!({"ok": true}))
        }
        "set_effect_params" | "remove_effect" => {
            let sid = active(ed)?;
            let clip = ClipId(
                a.get("clip_id")
                    .and_then(Json::as_u64)
                    .ok_or("clip_id is required")?,
            );
            let which = match a.get("effect") {
                Some(Json::Number(n)) => n.to_string(),
                Some(Json::String(s)) => s.clone(),
                _ => return Err("effect is required".into()),
            };
            let params = a.get("params").cloned().unwrap_or(Json::Null);
            let remove = name == "remove_effect";
            let mut err = None;
            ed.edit(
                if remove {
                    "Remove Effect"
                } else {
                    "Effect Parameters"
                },
                |p| {
                    let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                    let c = s.clip_mut(clip).ok_or(EditError::NotFound("clip".into()))?;
                    if remove {
                        let comp = find_effect(c, &which).map_err(|e| {
                            err = Some(e);
                            EditError::Nothing
                        })?;
                        if comp.is_fixed() {
                            err = Some(
                                "fixed effects (Motion, Opacity, Volume...) cannot be removed"
                                    .into(),
                            );
                            return Err(EditError::Nothing);
                        }
                        let id = comp.id;
                        c.components.retain(|x| x.id != id);
                        return Ok(());
                    }
                    let comp = find_effect(c, &which).map_err(|e| {
                        err = Some(e);
                        EditError::Nothing
                    })?;
                    set_params(comp, &params).map_err(|e| {
                        err = Some(e);
                        EditError::Nothing
                    })?;
                    Ok(())
                },
            );
            match err {
                Some(e) => Err(e),
                None => text(json!({"ok": true})),
            }
        }
        "add_transition" => {
            let sid = active(ed)?;
            let clip = ClipId(
                a.get("clip_id")
                    .and_then(Json::as_u64)
                    .ok_or("clip_id is required")?,
            );
            let (track, c) = ed
                .project
                .sequence(sid)
                .and_then(|s| s.find_clip(clip).map(|(r, c)| (r, c.clone())))
                .ok_or("no such clip")?;
            let default = if track.kind == TrackKind::Video {
                catalog::CROSS_DISSOLVE
            } else {
                catalog::CONSTANT_POWER
            };
            let effect = str_arg(a, "effect_id").unwrap_or(default).to_string();
            let at_start = a.get("at_start").and_then(Json::as_bool).unwrap_or(false);
            let cut = if at_start { c.start } else { c.end() };
            let dur = Dur::from_seconds(f64_arg(a, "duration").unwrap_or(1.0));
            ed.seq_edit("Add Transition", |p, sid, opts| {
                op_timeline::add_transition(p, sid, track, cut, &effect, dur, None, opts)
            })
            .ok_or_else(|| status_error(ed, "no transition fits there"))?;
            text(json!({"ok": true}))
        }
        "add_text" => {
            active(ed)?;
            if let Some(t) = f64_arg(a, "at") {
                ed.set_playhead(SeqTime::from_seconds(t));
            }
            let id = ed
                .add_graphic(catalog::TEXT, None)
                .ok_or_else(|| status_error(ed, "the title could not be added"))?;
            let mut params = a.get("params").cloned().unwrap_or_else(|| json!({}));
            if let Some(t) = str_arg(a, "text") {
                params["text"] = json!(t);
            }
            call(
                ed,
                "set_effect_params",
                &json!({"clip_id": id.0, "effect": catalog::TEXT, "params": params}),
            )?;
            if let Some(d) = f64_arg(a, "duration") {
                ed.selection = op_application::Selection::only(vec![id]);
                let sid = active(ed)?;
                ed.edit("Duration", |p| {
                    let c = p
                        .sequence_mut(sid)
                        .and_then(|s| s.clip_mut(id))
                        .ok_or(EditError::Nothing)?;
                    c.duration = Dur::from_seconds(d.max(0.04));
                    Ok(())
                });
            }
            text(json!({"clip_id": id.0}))
        }
        "transcribe_captions" => {
            active(ed)?;
            let options = transcribe_options(a);
            ed.status = None;
            if !ed.transcribe_captions(options) {
                return Err(status_error(ed, "a transcription is already running"));
            }
            wait(ed, 3600, |e| e.captioning.is_some());
            match &ed.status {
                Some(s) if s.error => Err(s.text.clone()),
                Some(s) => text(json!({"result": s.text, "captions": ed.caption_clips().len()})),
                None => text(json!({"captions": ed.caption_clips().len()})),
            }
        }
        "set_caption_style" => {
            let clips = ed.caption_clips();
            if clips.is_empty() {
                return Err("the sequence has no captions".into());
            }
            let def = catalog::find(catalog::CAPTION).ok_or("no caption component")?;
            let mut probe = Component::new(def, &mut ed.project.ids.clone());
            let params = a.get("params").cloned().unwrap_or(Json::Null);
            set_params(&mut probe, &params)?;
            let values: Vec<(String, Value)> = params
                .as_object()
                .map(|o| {
                    o.keys()
                        .filter_map(|k| probe.param(k).map(|p| (k.clone(), p.value.clone())))
                        .collect()
                })
                .unwrap_or_default();
            ed.set_caption_values(&clips, values, None);
            text(json!({"captions": clips.len()}))
        }
        "add_marker" => {
            let sid = active(ed)?;
            let at = SeqTime::from_seconds(f64_arg(a, "at").ok_or("at is required")?);
            let label = str_arg(a, "name").unwrap_or("").to_string();
            ed.edit("Add Marker", |p| {
                let id = p.ids.marker();
                let s = p.sequence_mut(sid).ok_or(EditError::Nothing)?;
                let mut m = Marker::new(id, at);
                m.name = label.clone();
                s.markers.push(m);
                s.markers.sort_by_key(|m| m.start);
                Ok(())
            });
            text(json!({"ok": true}))
        }
        "set_playhead" => {
            ed.set_playhead(SeqTime::from_seconds(
                f64_arg(a, "at").ok_or("at is required")?.max(0.0),
            ));
            text(json!({"playhead": secs(ed.playhead())}))
        }
        "render_frame" => {
            let sid = active(ed)?;
            let at = f64_arg(a, "at")
                .map(SeqTime::from_seconds)
                .unwrap_or_else(|| ed.playhead());
            let width = a
                .get("width")
                .and_then(Json::as_u64)
                .unwrap_or(960)
                .clamp(64, 3840);
            Ok(vec![Content::Image(crate::render::frame_png(
                &ed.project,
                sid,
                at,
                width as u32,
                ed.media.clone(),
            )?)])
        }
        "export" => crate::render::export(ed, a),
        "run_command" => {
            let cmd = str_arg(a, "command").ok_or("command is required")?;
            let clips = ids_arg(a, "clip_ids");
            if !clips.is_empty() {
                ed.selection = op_application::Selection::only(clips);
            }
            ed.status = None;
            let done = ed.execute(cmd, Focus::Timeline);
            if !done {
                return Err(format!("{cmd} is not a command the editor runs"));
            }
            text(json!({"ok": true, "status": ed.status.as_ref().map(|s| s.text.clone())}))
        }
        "undo" => {
            ed.undo();
            text(json!({"ok": true}))
        }
        "redo" => {
            ed.redo();
            text(json!({"ok": true}))
        }
        _ => Err(format!("no tool {name}")),
    }
}

/// The editor's last error message, else `fallback`.
fn status_error(ed: &Editor, fallback: &str) -> String {
    ed.status
        .as_ref()
        .filter(|s| s.error)
        .map(|s| s.text.clone())
        .unwrap_or_else(|| fallback.to_string())
}
