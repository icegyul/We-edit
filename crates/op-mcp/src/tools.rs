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
            "Opens a We edit project (.wedalur, or an older .opproj), or imports a Premiere Pro (.prproj), OTIO, FCP XML or EDL file.",
            json!({"path": s("Path of the project file")}),
            &["path"]
        ),
        tool(
            "save_project",
            "Saves the project, to `path` when given (.wedalur).",
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
            "The active sequence: tracks with their clips (id, name, start and end in seconds, effects with their parameters), transitions, markers and the playhead. A big sequence (more than `limit` clips) answers with a summary per track (clip count, first start, last end, the sources used) instead of every clip; ask for clips with `track` (like V1), `from`/`to` (seconds) and `offset` (up to `limit` clips come back, `next_offset` says where to go on).",
            json!({
                "detail": json!({"type": "boolean", "description": "Include every effect parameter, also the fixed Motion and Opacity (default false)"}),
                "track": s("Only this track, like V1 or A2"),
                "from": num("Only clips ending after this sequence time (seconds)"),
                "to": num("Only clips starting before this sequence time (seconds)"),
                "limit": int("Clips per answer, 1 to 200 (default 40)"),
                "offset": int("Clips to skip, for the next page (default 0)"),
                "summary": json!({"type": "boolean", "description": "Force the per-track summary (default: on for a big sequence when no track or time range is given)"})
            }),
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
            "detect_silence",
            "Finds silences (and, with `words`, filler words, repeated words and takes) in the sequence's audio WITHOUT changing anything: returns each stretch with its time, kind and text. Use it to preview what remove_silence would do.",
            silence_schema(),
            &[]
        ),
        tool(
            "remove_silence",
            "Cuts silences (and, with `words`, fillers, repeated words and repeated takes) out of the sequence in one undo step: removes and closes the gaps, leaves gaps, speeds the silences up, or only adds markers. Linked video and every track stay in sync.",
            {
                let mut p = silence_schema();
                if let Some(o) = p.as_object_mut() {
                    o.insert("action".into(), json!({"type": "string", "enum": ["remove", "leave_gap", "speed_up", "markers"], "description": "What to do with each stretch (default remove)"}));
                    o.insert(
                        "speed_percent".into(),
                        num("Speed for speed_up, percent (default 400)"),
                    );
                    o.insert(
                        "crossfade_ms".into(),
                        num("Audio crossfade at each join, milliseconds (default 12)"),
                    );
                }
                p
            },
            &[]
        ),
        tool(
            "set_keyframes",
            "Animates one parameter of an effect on a clip with keyframes (zoom, position, opacity, volume over time...). Times are sequence seconds inside the clip. The parameter keeps its keyframes until you pass clear=true, which stops the animation and keeps the value of the first keyframe given (or the current value).",
            json!({"clip_id": int("Clip id"), "effect": s("Effect id, or its index in the clip's effect list (Motion is 0)"), "param": s("Parameter key from get_timeline with detail, like scale or position"), "keyframes": json!({"type": "array", "description": "[{\"at\": seconds, \"value\": number | [x, y] | ...}]", "items": {"type": "object"}}), "interpolation": s("linear (default), hold, bezier or auto_bezier"), "clear": json!({"type": "boolean", "description": "Remove all keyframes of the parameter"})}),
            &["clip_id", "effect", "param"]
        ),
        tool(
            "set_track",
            "Changes a track: lock, mute, solo, eye (video visible), sync lock or name.",
            json!({"track": s("Track like V1 or A2"), "locked": json!({"type": "boolean"}), "muted": json!({"type": "boolean"}), "solo": json!({"type": "boolean"}), "visible": json!({"type": "boolean", "description": "Video track output on or off"}), "sync_lock": json!({"type": "boolean"}), "name": s("New track name")}),
            &["track"]
        ),
        tool(
            "add_tracks",
            "Adds video or audio tracks on top of the existing ones (or at `index`, counted from 0).",
            json!({"kind": s("video or audio"), "count": int("How many (default 1)"), "index": int("Position among the tracks of that kind; default: on top")}),
            &["kind"]
        ),
        tool(
            "delete_track",
            "Deletes a track and its clips (a sequence keeps at least one track of each kind).",
            json!({"track": s("Track like V3 or A2")}),
            &["track"]
        ),
        tool(
            "comfy_status",
            "ComfyUI (the image and video generator): is it running, the running generation, and the latest result with its media_id (ready for add_clip).",
            json!({}),
            &[]
        ),
        tool(
            "comfy_start",
            "Starts ComfyUI if it is not running (it needs a minute or two to load its models). With the program open this returns at once: poll comfy_status until running is true.",
            json!({}),
            &[]
        ),
        tool(
            "comfy_generate",
            "Makes an image (Z-Image Turbo or Krea 2 Turbo) or a video with sound (MiniMax H3) from a description and imports it into the project. Needs ComfyUI running. With the program open it only starts: poll comfy_status until job is null and last_result has a media_id, then use add_clip. Videos take minutes. Describe the scene in English for the best result.",
            json!({"kind": s("image (Z-Image Turbo), krea2 (Krea 2 Turbo, another image model) or video (MiniMax H3); default image"), "prompt": s("What to generate"), "width": int("Image width in pixels (default 1920)"), "height": int("Image height in pixels (default 1080)"), "seconds": num("Video length in seconds, 2 to 10 (default 5)"), "vertical": json!({"type": "boolean", "description": "Video 9:16 instead of 16:9"}), "seed": int("Fixed seed; omit for a random one"), "add_to_timeline": json!({"type": "boolean", "description": "Only when the program is closed: also put the result on the timeline"})}),
            &["prompt"]
        ),
        tool(
            "synchronize_clips",
            "Lines up clips that are already on the timeline (Clip > Synchronize): the clip on the lowest track stays and the others move so that their starts, their ends or their sound match it. Linked video and audio move together. One undo step. With mode=audio the sound of the chosen channel decides; clips whose sound does not match are left where they are and reported.",
            json!({"clip_ids": ids("Two or more clips on different tracks"), "mode": s("start, end or audio (default audio)"), "channel": s("For audio: 1, 2 or mixdown (default 1)")}),
            &["clip_ids"]
        ),
        tool(
            "search_sounds",
            "Searches the sound library: sound effects (kind=sfx: whooshes, impacts, clicks, notification dings, typing, air horn ...) and background music (kind=bgm). All are public domain (CC0): safe to use in any video without credit. Returns id, name (Korean `ko` when there is one), category and seconds; use add_sound with the id. Words match the English name, the Korean name, tags and the category.",
            json!({"query": s("Words to look for, e.g. whoosh, 휘익, upbeat, lofi; empty lists the category"), "kind": s("sfx (default) or bgm"), "category": s("Exact category name from an earlier result"), "limit": int("How many to return (default 20, at most 60)")}),
            &[]
        ),
        tool(
            "add_sound",
            "Puts a sound from the sound library on the timeline at `at` seconds (default: the playhead), on the first audio track that is free there (a new track when all are taken). Downloads the sound the first time (a few seconds).",
            json!({"sound_id": s("Id from search_sounds, like reza/whoosh_transition"), "at": num("Seconds on the timeline (default: the playhead)")}),
            &["sound_id"]
        ),
        tool(
            "remix_music",
            "Makes a music clip on an audio track longer or shorter without a visible edit (Premiere's Remix): it finds where the music repeats itself and plays that part once more (or skips it once), so the length changes in whole repeated parts: the steps differ per song (8 s, 16 s ...), not in 1 second steps. The clip becomes a group of clips joined by short crossfades. Needs a clip of an audio file (not linked to video, normal speed), at least 10 seconds. Without `seconds` it only analyses and reports the tempo and the steps. With `revert` it puts the original clip back. Use it again with another `seconds` to change the length of an already remixed clip. The first call analyses the music (a few seconds); in the open window it only starts the analysis and you call again.",
            json!({"clip_id": int("Id of the music clip (or of any piece of an already remixed one)"), "seconds": num("The length wanted, in seconds; the result is the nearest length the music's repeated parts allow"), "min_loop": num("Shortest stretch to repeat or skip, in seconds (default 4)"), "revert": json!({"type": "boolean", "description": "Put the original clip back"})}),
            &["clip_id"]
        ),
        tool(
            "list_motion_templates",
            "Lists the motion templates (animated titles and graphics): id, name, what it looks like, how many text lines it takes and its default text. Use add_motion_template to put one on the timeline.",
            json!({}),
            &[]
        ),
        tool(
            "add_motion_template",
            "Puts an animated template (pop-up title, name tag, highlight box, subscribe pop-up, flash, shake text, slide title, marker text, speech bubble, title band) on the timeline at `at` seconds (default: the playhead) as a group of graphics clips on free video tracks. Every value stays editable (get_timeline detail shows the keyframes).",
            json!({"template": s("Id from list_motion_templates"), "text": s("Main text (title, name, label); default text of the template when omitted"), "sub": s("Second line, for templates with two lines"), "color": s("Accent color as #RRGGBB"), "seconds": num("Length in seconds (default: the template's)"), "at": num("Seconds on the timeline (default: the playhead)")}),
            &["template"]
        ),
        tool(
            "generate_image_api",
            "Makes a picture with a cloud service and the user's saved API key (Preferences > 생성형 AI): provider=gemini (Nano Banana) or openai (ChatGPT image). Costs the user money at that service, so use it only when asked. The picture is saved in the generated folder and imported into the project (look for it with get_project). Fails with a clear message when no key is saved.",
            json!({"provider": s("gemini or openai"), "prompt": s("What to draw"), "aspect": s("wide (16:9, default), tall (9:16) or square")}),
            &["provider", "prompt"]
        ),
        tool(
            "copy_effects",
            "Copies the effects of a clip to paste on other clips (like copying in Effect Controls): without `effect_index` every changed effect (Motion, Opacity, volume ... with their keyframes, and added effects; when nothing was changed, all the effects with their default values, which restores a changed clip when pasted), else only that one (index in the clip's component list from get_timeline detail).",
            json!({"clip_id": int("The clip to copy from"), "effect_index": int("Only this component (its index in the clip's effects)")}),
            &["clip_id"]
        ),
        tool(
            "paste_effects",
            "Pastes the effects copied by copy_effects on all the clips in clip_ids at once (one undo step). Video effects go on video clips, audio effects on audio clips.",
            json!({"clip_ids": ids("Clips to paste on")}),
            &["clip_ids"]
        ),
        tool(
            "make_shorts",
            "Makes a vertical 1080x1920 sequence (for Shorts, Reels, TikTok) from the active horizontal one and opens it; the original stays as it is. The horizontal sequence is nested whole: layout=fill crops it to fill the frame (focus picks the stretch that stays, 0 left to 1 right), layout=blur keeps the whole picture in the middle over a blurred enlarged copy. Afterwards transcribe_sentences works on the new sequence, and captions made there get the vertical layout.",
            json!({"layout": s("fill or blur (default fill)"), "focus": num("fill only: where to look, 0 left, 0.5 middle, 1 right (default 0.5)")}),
            &[]
        ),
        tool(
            "sync_by_audio",
            "Lines up two to six recordings of the same scene by their sound (several cameras, or a camera and a separate microphone). Returns for each media the second of the sequence where it must start (`timeline_start`, use it as `at` of add_clip on its own track) so that their sound matches. The first media is the reference. A low confidence means the sound did not match: do not use that result.",
            json!({"media_ids": ids("Media ids from get_project, the reference first"), "max_seconds": num("How much of each recording's beginning to compare (default 600)")}),
            &["media_ids"]
        ),
        tool(
            "transcribe_sentences",
            "Writes down what the active sequence says, sentence by sentence, with times (Whisper large-v3 on the graphics card, much more exact than transcribe_captions). With the program open it only starts: call get_sentences, which waits for it. Sentence times are sequence seconds and match the timeline. Needed before delete_sentences.",
            json!({"language": s("Spoken language like ko or en; empty to detect")}),
            &[]
        ),
        tool(
            "get_sentences",
            "The sentences of the latest transcript: id, start-end in sequence seconds and the words, or the progress of a running transcription. While the transcription runs this call WAITS (up to wait_seconds) and answers when it is done, so call it once and again only if it still says running. A transcript is stale (and delete_sentences refuses it) once the sequence was edited by anything else.",
            json!({"wait_seconds": num("How long to wait for a running transcription, 0 to 50 (default 25)")}),
            &[]
        ),
        tool(
            "delete_sentences",
            "Cuts whole sentences out of the sequence by their ids (every track, gaps closed, audio crossfaded, one undo step). The other sentences keep their ids and move up with the timeline, so several rounds of deleting work without transcribing again.",
            json!({"sentence_ids": ids("Sentence ids from get_sentences")}),
            &["sentence_ids"]
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
            json!({"path": s("Output file (.mp4, .mov, .mxf, .png)"), "format": s("h264 (default), hevc, prores (422 HQ), prores_proxy, prores_lt, prores422, prores4444, prores4444xq, dnxhr, xdcam (XDCAM HD422 50 Mbps MXF, 1080p) or png"), "start": num("Seconds (default 0)"), "end": num("Seconds (default: the end of the sequence)"), "width": int("Default: the sequence's"), "height": int("Default: the sequence's")}),
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

/// The parameters `detect_silence` and `remove_silence` take.
fn silence_schema() -> Json {
    json!({
        "scope": {"type": "string", "enum": ["selection", "sequence", "in_out"], "description": "selection (selected clips), sequence (default) or in_out (between the In and Out points)"},
        "method": {"type": "string", "enum": ["loudness", "relative", "adaptive", "speech", "neural"], "description": "How sound is told from silence: loudness (fixed dB), relative (dB under the loudest part), adaptive (dB over the recording's own noise floor, default), speech (voice band), neural (voice detector, ignores music and noise)"},
        "threshold_db": {"type": "number", "description": "loudness: silence below this dBFS (default -40)"},
        "below_peak_db": {"type": "number", "description": "relative: silence this many dB under the loudest part (default 30)"},
        "margin_db": {"type": "number", "description": "adaptive: sound is this many dB over the noise floor (default 9)"},
        "sensitivity": {"type": "number", "description": "speech and neural: 0 to 1, higher keeps more (default 0.5)"},
        "voice_only": {"type": "boolean", "description": "Only voices count as sound (music and noise are silence)"},
        "min_silence": {"type": "number", "description": "Shortest silence to cut, seconds (default 0.4)"},
        "min_sound": {"type": "number", "description": "Shortest sound to keep, seconds (default 0.08)"},
        "pad_before": {"type": "number", "description": "Seconds of air kept after a word, before the cut (default 0.08)"},
        "pad_after": {"type": "number", "description": "Seconds of air kept before the next word, after the cut (default 0.12)"},
        "preset": {"type": "string", "enum": ["gentle", "podcast", "tutorial", "jump_cuts", "noisy_room"], "description": "Start from a preset; the other fields override it"},
        "words": {"type": "object", "description": "Also cut by what was said (transcribes on this computer; downloads the speech model once). Fields: language (code like es), model (speech model: tiny, base, small, medium, large-v3-turbo, large-v2, large-v3 or korean-turbo; large-v3 is the most exact and runs on the NVIDIA card), speech_only (cut everything that is not speech: before the first word, between sentences and after the last), speech_gap (shortest stretch cut by speech_only, seconds, default 0.25), speech_air (air left beside the words, default 0.12), fillers (default true), aggressive_fillers, custom (comma-separated words or phrases), repeated_words (default true), repeated_phrases (default true), retakes (keep the last of a repeated sentence), gap (cut dead air between words of at least this many seconds), retake_similarity (0 to 1)"}
    })
}

fn bool_arg(a: &Json, k: &str) -> Option<bool> {
    a.get(k).and_then(Json::as_bool)
}

/// The silence settings an assistant asked for.
pub fn silence_settings(a: &Json) -> Result<op_application::silence::SilenceSettings, String> {
    use op_application::silence::{Action, Scope, SilenceSettings};
    use op_silence::{Method, presets};
    let mut s = SilenceSettings {
        scope: match str_arg(a, "scope").unwrap_or("sequence") {
            "selection" => Scope::Selection,
            "in_out" => Scope::InOut,
            "sequence" => Scope::Sequence,
            other => return Err(format!("scope {other}: use selection, sequence or in_out")),
        },
        ..Default::default()
    };
    if let Some(name) = str_arg(a, "preset") {
        let wanted = name.replace('_', " ");
        let preset = presets::all()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(&wanted))
            .ok_or_else(|| format!("no preset {name}"))?;
        s.sound = preset.params;
        s.words = preset.text.clone();
        s.by_words = preset.words;
    }
    let num = |k: &str| f64_arg(a, k);
    match str_arg(a, "method") {
        None => {}
        Some("loudness") => {
            s.sound.method = Method::Loudness {
                threshold_db: num("threshold_db").unwrap_or(-40.0) as f32,
            }
        }
        Some("relative") => {
            s.sound.method = Method::Relative {
                below_peak_db: num("below_peak_db").unwrap_or(30.0) as f32,
            }
        }
        Some("adaptive") => {
            s.sound.method = Method::Adaptive {
                margin_db: num("margin_db").unwrap_or(9.0) as f32,
            }
        }
        Some("speech") => {
            s.sound.method = Method::Speech {
                sensitivity: num("sensitivity").unwrap_or(0.5) as f32,
            }
        }
        Some("neural") => {
            s.sound.method = Method::Neural {
                sensitivity: num("sensitivity").unwrap_or(0.5) as f32,
            }
        }
        Some(other) => {
            return Err(format!(
                "method {other}: use loudness, relative, adaptive, speech or neural"
            ));
        }
    }
    if let Some(v) = num("min_silence") {
        s.sound.min_silence = v.max(0.02);
    }
    if let Some(v) = num("min_sound") {
        s.sound.min_sound = v.max(0.0);
    }
    if let Some(v) = num("pad_before") {
        s.sound.pad_before = v.max(0.0);
    }
    if let Some(v) = num("pad_after") {
        s.sound.pad_after = v.max(0.0);
    }
    if let Some(v) = bool_arg(a, "voice_only") {
        s.sound.voice_only = v;
    }
    if let Some(w) = a.get("words").filter(|w| w.is_object()) {
        s.by_words = true;
        if let Some(l) = str_arg(w, "language") {
            s.language = Some(l.to_string());
        }
        if let Some(id) = str_arg(w, "model") {
            // the speech helper's models (up to large-v3); the first three are also the built-in ones
            if op_application::whisper_models::info(id).is_none() {
                let ids: Vec<&str> = op_application::whisper_models::MODELS.iter().map(|m| m.id).collect();
                return Err(format!("model {id}: use one of {ids:?}"));
            }
            s.whisper = id.to_string();
            match id {
                "tiny" => s.model = ModelSize::Tiny,
                "small" => s.model = ModelSize::Small,
                _ => {}
            }
        }
        if let Some(v) = bool_arg(w, "speech_only") {
            s.speech_only = v;
        }
        if let Some(v) = f64_arg(w, "speech_gap") {
            s.speech_gap = v.max(0.05);
        }
        if let Some(v) = f64_arg(w, "speech_air") {
            s.speech_air = v.max(0.0);
        }
        if let Some(v) = bool_arg(w, "fillers") {
            s.words.fillers = v;
        }
        if let Some(v) = bool_arg(w, "aggressive_fillers") {
            s.words.aggressive_fillers = v;
        }
        if let Some(v) = str_arg(w, "custom") {
            s.words.custom = v.to_string();
        }
        if let Some(v) = bool_arg(w, "repeated_words") {
            s.words.repeated_words = v;
        }
        if let Some(v) = bool_arg(w, "repeated_phrases") {
            s.words.repeated_phrases = v;
        }
        if let Some(v) = bool_arg(w, "retakes") {
            s.words.retakes = v;
        }
        if let Some(v) = f64_arg(w, "gap") {
            s.words.gap = v.max(0.0);
        }
        if let Some(v) = f64_arg(w, "retake_similarity") {
            s.words.retake_similarity = v.clamp(0.3, 1.0);
        }
        // speech_only alone means "only the empty space": no filler or repeat search unless asked for
        let others = ["fillers", "aggressive_fillers", "custom", "repeated_words", "repeated_phrases", "retakes", "gap", "retake_similarity"];
        if s.speech_only && !others.iter().any(|k| w.get(*k).is_some()) {
            s.by_words = false;
        }
    }
    s.action = match str_arg(a, "action").unwrap_or("remove") {
        "remove" => Action::Remove,
        "leave_gap" => Action::LeaveGap,
        "speed_up" => Action::SpeedUp(num("speed_percent").unwrap_or(400.0)),
        "markers" => Action::Markers,
        other => {
            return Err(format!(
                "action {other}: use remove, leave_gap, speed_up or markers"
            ));
        }
    };
    if let Some(v) = num("crossfade_ms") {
        s.smooth_ms = v.clamp(0.0, 100.0);
    }
    Ok(s)
}

/// Analyzes the audio and returns the finished detection (blocks until it is done).
fn analyze_blocking(
    ed: &mut Editor,
    s: &op_application::silence::SilenceSettings,
) -> Result<std::sync::Arc<op_application::silence::Detection>, String> {
    ed.silence_result = None;
    ed.analyze_silence(s)?;
    wait(ed, 3600, |e| e.silencing.is_some());
    ed.silence_result
        .clone()
        .ok_or_else(|| status_error(ed, "the audio could not be analyzed"))
}

/// The latest sentence transcript as a compact table (or its progress).
fn sentences_text(ed: &Editor) -> R {
    if let Some(job) = &ed.sentencing {
        let p = job.progress();
        return Ok(vec![Content::Text(format!(
            "state: running ({:?}, {:.0}%)
call get_sentences again (it waits for you)",
            p.stage,
            p.fraction * 100.0
        ))]);
    }
    let Some(t) = &ed.transcript else {
        return Ok(vec![Content::Text("state: none
call transcribe_sentences first".into())]);
    };
    let state = if ed.transcript_is_current() {
        "ready"
    } else {
        "stale: the sequence was edited after this transcript, transcribe again"
    };
    let mut out = format!("state: {state}
language: {}  sentences: {}
id	start-end (sequence seconds)	text
", t.language, t.sentences.len());
    for s in &t.sentences {
        out.push_str(&format!("{}	{:.2}-{:.2}	{}
", s.id, s.start, s.end, s.text));
    }
    Ok(vec![Content::Text(out)])
}

/// set_keyframes: animate one parameter of an effect on a clip.
fn set_keyframes(ed: &mut Editor, a: &Json) -> R {
    let sid = active(ed)?;
    let clip = ClipId(a.get("clip_id").and_then(Json::as_u64).ok_or("clip_id is required")?);
    let which = match a.get("effect") {
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::String(s)) => s.clone(),
        _ => return Err("effect is required".into()),
    };
    let key = str_arg(a, "param").ok_or("param is required")?.to_string();
    let clear = a.get("clear").and_then(Json::as_bool).unwrap_or(false);
    let interp = match str_arg(a, "interpolation").unwrap_or("linear") {
        "linear" => Interp::Linear,
        "hold" => Interp::Hold,
        "bezier" => Interp::Bezier,
        "auto_bezier" => Interp::AutoBezier,
        other => return Err(format!("interpolation {other} is unknown: use linear, hold, bezier or auto_bezier")),
    };
    let frames: Vec<(f64, Json)> = a
        .get("keyframes")
        .and_then(Json::as_array)
        .map(|v| {
            v.iter()
                .filter_map(|k| Some((k.get("at")?.as_f64()?, k.get("value")?.clone())))
                .collect()
        })
        .unwrap_or_default();
    if frames.is_empty() && !clear {
        return Err("keyframes (each with at and value) or clear=true is required".into());
    }
    let mut err = None;
    ed.edit("Keyframes", |p| {
        let fail = |e: String, err: &mut Option<String>| {
            *err = Some(e);
            EditError::Nothing
        };
        let c = p
            .sequence_mut(sid)
            .and_then(|s| s.clip_mut(clip))
            .ok_or(EditError::NotFound("clip".into()))?;
        // sequence time -> time inside the source, before the effect is borrowed
        let (start, end) = (c.start.seconds(), c.end().seconds());
        let mut times = Vec::new();
        for (at, v) in &frames {
            if *at < start - 1e-6 || *at > end + 1e-6 {
                return Err(fail(format!("keyframe time {at} is outside the clip ({start:.2}-{end:.2})"), &mut err));
            }
            times.push((c.to_source(SeqTime::from_seconds(*at)), v));
        }
        let comp = find_effect(c, &which).map_err(|e| fail(e, &mut err))?;
        let def = comp
            .def()
            .ok_or_else(|| fail(format!("{} is not a known effect", comp.effect), &mut err))?;
        let spec = def.params.iter().find(|p| p.key == key).ok_or_else(|| {
            let keys: Vec<&str> = def.params.iter().map(|p| p.key).collect();
            fail(format!("{} has no parameter {key}; it has {keys:?}", def.name), &mut err)
        })?;
        let values: Vec<(SrcTime, Value)> = times
            .iter()
            .map(|(t, v)| to_value(&spec.kind, v).map(|v| (*t, v)))
            .collect::<Result<_, _>>()
            .map_err(|e| fail(format!("{key}: {e}"), &mut err))?;
        let spec_key = spec.key;
        if comp.param_mut(&key).is_none() {
            let default = values.first().map(|v| v.1.clone()).unwrap_or(Value::Float(0.0));
            comp.params.push(Param::new(spec_key, default));
        }
        let param = comp
            .param_mut(&key)
            .ok_or_else(|| fail("the parameter is missing".into(), &mut err))?;
        if clear {
            if let Some((_, v)) = values.first() {
                param.value = v.clone();
            } else if let Some(first) = param.keys.first().map(|k| k.time) {
                param.value = param.value_at(first);
            }
            param.animated = false;
            param.keys.clear();
        } else {
            if !param.animated {
                param.animated = true;
                param.keys.clear();
            }
            for (t, v) in values {
                param.set_at(t, v, interp);
            }
        }
        Ok(())
    });
    match err {
        Some(e) => Err(e),
        None => text(json!({"ok": true})),
    }
}

/// The latest ComfyUI state for assistants.
fn comfy_status(ed: &Editor) -> R {
    let up = op_application::comfy::probe();
    let job = ed.comfy.job.as_ref().map(|j| {
        let p = j.progress();
        json!({"state": p.note, "seconds_running": p.started.elapsed().as_secs()})
    });
    let last = ed.comfy.last.as_ref().map(|(p, item)| json!({"path": p, "media_id": item.map(|i| i.0)}));
    text(json!({
        "running": up.is_some(),
        "starting": up.is_none() && ed.comfy.child.is_some(),
        "job": job,
        "last_result": last,
        "last_error": ed.comfy.last_error,
    }))
}

fn comfy_request(a: &Json) -> Result<op_application::comfy::Request, String> {
    use op_application::comfy::{Kind, Request};
    let kind = match str_arg(a, "kind").unwrap_or("image") {
        "image" => Kind::Image,
        "krea2" => Kind::Krea2,
        "video" => Kind::Video,
        other => return Err(format!("kind {other} is unknown: use image, krea2 or video")),
    };
    let seed = a.get("seed").and_then(Json::as_u64).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
    });
    Ok(Request {
        kind,
        prompt: str_arg(a, "prompt").ok_or("prompt is required")?.to_string(),
        width: a.get("width").and_then(Json::as_u64).unwrap_or(1920) as u32,
        height: a.get("height").and_then(Json::as_u64).unwrap_or(1080) as u32,
        seconds: f64_arg(a, "seconds").unwrap_or(5.0),
        vertical: a.get("vertical").and_then(Json::as_bool).unwrap_or(false),
        seed,
    })
}

/// sync_by_audio: where each recording must start so that their sound lines up.
fn sync_by_audio(ed: &mut Editor, a: &Json) -> R {
    let ids: Vec<ItemId> = a
        .get("media_ids")
        .and_then(Json::as_array)
        .map(|v| v.iter().filter_map(Json::as_u64).map(ItemId).collect())
        .unwrap_or_default();
    if !(2..=6).contains(&ids.len()) {
        return Err("media_ids needs two to six media ids".into());
    }
    let seconds = f64_arg(a, "max_seconds").unwrap_or(600.0).clamp(10.0, 3600.0);
    let mut media = Vec::new();
    for id in &ids {
        let item = ed.project.item(*id).ok_or(format!("no media {}", id.0))?;
        match &item.kind {
            ItemKind::Media { asset, .. } => media.push((*id, item.name.clone(), *asset)),
            _ => return Err(format!("{} is not a media file", id.0)),
        }
    }
    // the sound is decoded in the background: ask for every file, then wait for all of them
    for (_, _, asset) in &media {
        ed.media.conformed(*asset, 0);
    }
    wait(ed, 900, |e| e.media.conforming() > 0);
    let mut sound = Vec::new();
    for (_, name, asset) in &media {
        let audio = ed.media.conformed(*asset, 0).ok_or_else(|| {
            let why = ed.media.audio_error(*asset, 0).unwrap_or_default();
            format!("{name} has no usable sound {why}")
        })?;
        sound.push(op_application::audio_sync::mono(&audio, 0.0, seconds, op_application::audio_sync::Channel::MixDown));
    }
    let mut lags = vec![(0.0, f64::INFINITY)];
    for (s, (_, name, _)) in sound[1..].iter().zip(&media[1..]) {
        let l = op_application::audio_sync::lag(&sound[0], s)
            .ok_or_else(|| format!("{name} is too short to match (under a second of sound)"))?;
        lags.push((l.seconds, l.confidence));
    }
    let earliest = lags.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    let files: Vec<Json> = media
        .iter()
        .zip(&lags)
        .map(|((id, name, _), (lag, confidence))| {
            let mut f = json!({
                "media_id": id.0,
                "name": name,
                "timeline_start": round(lag - earliest),
                "offset_from_first": round(*lag),
                "confidence": if confidence.is_finite() { json!(confidence.round()) } else { json!("reference") },
            });
            if *confidence < 8.0 {
                f["warning"] = json!("low confidence: the sound did not match, do not trust this offset");
            }
            f
        })
        .collect();
    text(json!({"files": files, "note": "put each media at its timeline_start (add_clip at=...), each on its own track"}))
}

/// detect_silence and remove_silence.
fn silence_tool(ed: &mut Editor, name: &str, a: &Json, live: bool) -> R {
    active(ed)?;
    let s = silence_settings(a)?;
    if live && s.needs_words() {
        return Err("transcribing would freeze the open window: use Clip > Remove Silence there, or run the tool with We edit closed".into());
    }
    let d = analyze_blocking(ed, &s)?;
    let cuts = d.cuts(&s);
    if name == "detect_silence" {
        return text(op_application::silence::describe(&cuts));
    }
    let ranges: Vec<op_silence::Range> = cuts.iter().map(|c| c.range).collect();
    let n = ed.apply_cuts(d.origin, &ranges, s.action, s.smooth_ms);
    let mut out = op_application::silence::describe(&cuts);
    out["applied"] = json!(n);
    out["sequence_duration"] = json!(ed.active_seq().map(|q| q.duration().seconds()));
    text(out)
}

/// The clips of a remixed music from `start` on the audio track (`len` seconds long, or the first
/// clip only) and how many crossfades join them.
fn remix_pieces(ed: &Editor, track: usize, start: SeqTime, len: Option<f64>) -> (Vec<u64>, usize) {
    let Some(t) = ed.active_seq().and_then(|q| q.audio.get(track)) else { return (Vec::new(), 0) };
    let ids: Vec<u64> = match len {
        Some(l) => {
            let end = start + Dur::from_seconds(l);
            t.clips.iter().filter(|c| c.start >= start && c.start < end).map(|c| c.id.0).collect()
        }
        None => t.clips.iter().filter(|c| c.start == start).map(|c| c.id.0).collect(),
    };
    let crossfades = t.transitions.iter().filter(|x| x.from.is_some_and(|f| ids.contains(&f.0)) && x.to.is_some_and(|f| ids.contains(&f.0))).count();
    (ids, crossfades)
}

/// remix_music. In the open window the analysis only starts (waiting would freeze it).
fn remix_tool(ed: &mut Editor, a: &Json, live: bool) -> R {
    active(ed)?;
    let clip = ClipId(a.get("clip_id").and_then(Json::as_u64).ok_or("clip_id is required")?);
    if a.get("revert").and_then(Json::as_bool) == Some(true) {
        let (track, start) = ed.remix_target(clip).map(|t| (t.track, t.start))?;
        ed.remix_revert(clip)?;
        let (pieces, crossfades) = remix_pieces(ed, track, start, None);
        return text(json!({"reverted": true, "pieces": pieces, "crossfades": crossfades}));
    }
    let min_loop = f64_arg(a, "min_loop").unwrap_or(op_application::remix::DEFAULT_MIN_LOOP).clamp(2.0, 60.0);
    ed.remix_target(clip)?;
    if !ed.remix_ready_for(clip) {
        ed.remix_analyze(clip)?;
        if live {
            return text(json!({"started": true, "note": "the music is being analysed: call remix_music again with the same arguments in a few seconds"}));
        }
        wait(ed, 300, |e| e.remix.job.is_some());
        if !ed.remix_ready_for(clip) {
            return Err(status_error(ed, "the music could not be analysed"));
        }
    }
    let analysis = ed.remix_analysis().ok_or("the music could not be analysed")?;
    let (bpm, seconds) = (analysis.bpm(), analysis.seconds());
    let steps: Vec<Json> = ed
        .remix_steps(clip, min_loop)?
        .into_iter()
        .map(|(len, score)| json!({"seconds": (len * 100.0).round() / 100.0, "join_quality": (score * 100.0).round() / 100.0}))
        .collect();
    let Some(want) = f64_arg(a, "seconds") else {
        return text(json!({"analysed": true, "bpm": bpm.round(), "original_seconds": seconds, "steps": steps}));
    };
    let plan = ed.remix_plan(clip, want, min_loop)?;
    let (track, start) = ed.remix_target(clip).map(|t| (t.track, t.start))?;
    let len = ed.remix_apply(clip, &plan)?;
    let moves: Vec<Json> = plan
        .moves
        .iter()
        .map(|m| json!({"from": (m.at.start * 100.0).round() / 100.0, "to": (m.at.end * 100.0).round() / 100.0, "extra_plays": m.times, "join_quality": (m.at.score * 100.0).round() / 100.0}))
        .collect();
    let (pieces, crossfades) = remix_pieces(ed, track, start, Some(len));
    text(json!({"seconds": (len * 100.0).round() / 100.0, "requested": want, "original_seconds": seconds, "moves": moves, "pieces": pieces, "crossfades": crossfades, "steps": steps}))
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
        "comfy_start" => {
            ed.comfy_start(&ed.prefs.comfy_dir.clone())?;
            text(json!({"starting": true, "note": "ComfyUI needs a minute or two: poll comfy_status until running is true"}))
        }
        "comfy_generate" => {
            let req = comfy_request(a)?;
            ed.comfy_generate(req)?;
            text(json!({"started": true, "note": "poll comfy_status until job is null and last_result has a media_id, then add_clip it"}))
        }
        "transcribe_sentences" => {
            ed.transcribe_sentences(str_arg(a, "language").unwrap_or(""))?;
            text(json!({"started": true, "note": "this takes a while: call get_sentences, which waits until it is ready"}))
        }
        "detect_silence" | "remove_silence" => silence_tool(ed, name, a, true),
        "remix_music" => remix_tool(ed, a, true),
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
        verbatim: false,
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
            if op_project::native::is_project_file(&path) {
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
        "search_sounds" => {
            let cat = op_application::sound_library::catalog();
            let kind = str_arg(a, "kind").unwrap_or("sfx");
            if !matches!(kind, "sfx" | "bgm") {
                return Err("kind must be sfx or bgm".into());
            }
            let limit = a.get("limit").and_then(Json::as_u64).unwrap_or(20).clamp(1, 60) as usize;
            let query = str_arg(a, "query").unwrap_or("");
            let category = str_arg(a, "category").unwrap_or("");
            let found: Vec<&op_application::sound_library::Sound> = cat
                .sounds
                .iter()
                .filter(|x| x.kind == kind && (category.is_empty() || x.cat == category) && x.matches(query))
                .collect();
            let shown: Vec<Json> = found
                .iter()
                .take(limit)
                .map(|x| json!({"id": x.id, "name": x.name, "ko": x.ko, "category": x.cat, "seconds": x.sec}))
                .collect();
            let mut categories: Vec<&str> = cat.sounds.iter().filter(|x| x.kind == kind).map(|x| x.cat.as_str()).collect();
            categories.sort_unstable();
            categories.dedup();
            text(json!({"total": found.len(), "shown": shown, "categories": categories}))
        }
        "add_sound" => {
            let id = str_arg(a, "sound_id").ok_or("sound_id is required")?;
            active(ed)?;
            if let Some(t) = f64_arg(a, "at") {
                ed.set_playhead(SeqTime::from_seconds(t.max(0.0)));
            }
            let count = |e: &Editor| e.active_seq().map_or(0, |s| s.audio.iter().map(|t| t.clips.len()).sum::<usize>());
            let before = count(ed);
            ed.sound_request(id, op_application::sound_library::SoundAction::Add)?;
            wait(ed, 120, |e| e.sound.fetching().is_some());
            if count(ed) <= before {
                let why = ed.status.as_ref().filter(|s| s.error).map(|s| s.text.clone()).unwrap_or_default();
                return Err(format!("the sound could not be added {why}"));
            }
            text(json!({"added": id}))
        }
        "list_motion_templates" => {
            let list: Vec<Json> = op_application::motion_templates::TEMPLATES
                .iter()
                .map(|t| json!({"id": t.id, "name": t.name, "note": t.note, "text_lines": t.lines, "default_text": t.default_text, "default_sub": t.default_sub, "default_seconds": t.seconds}))
                .collect();
            text(json!({"templates": list}))
        }
        "add_motion_template" => {
            use op_application::motion_templates::{TemplateOptions, find as find_template};
            let id = str_arg(a, "template").ok_or("template is required")?;
            let t = find_template(id).ok_or_else(|| format!("template {id} is unknown: use list_motion_templates"))?;
            active(ed)?;
            let accent = match str_arg(a, "color") {
                Some(c) => {
                    let h = c.trim_start_matches('#');
                    let v = u32::from_str_radix(h, 16).map_err(|_| "color must look like #RRGGBB".to_string())?;
                    if h.len() != 6 {
                        return Err("color must look like #RRGGBB".into());
                    }
                    [((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0]
                }
                None => t.accent,
            };
            let o = TemplateOptions {
                text: str_arg(a, "text").unwrap_or(t.default_text).to_string(),
                sub: str_arg(a, "sub").unwrap_or(t.default_sub).to_string(),
                accent,
                seconds: f64_arg(a, "seconds").unwrap_or(t.seconds),
            };
            if let Some(at) = f64_arg(a, "at") {
                ed.set_playhead(SeqTime::from_seconds(at.max(0.0)));
            }
            let clips = ed.add_motion_template(id, &o)?;
            text(json!({"clips": clips, "clip_ids": ed.selection.clips.iter().map(|c| c.0).collect::<Vec<_>>()}))
        }
        "generate_image_api" => {
            use op_application::cloud_images::{Aspect, Provider};
            let provider = match str_arg(a, "provider").unwrap_or("") {
                "gemini" => Provider::Gemini,
                "openai" => Provider::OpenAi,
                other => return Err(format!("provider must be gemini or openai, not {other}")),
            };
            let aspect = match str_arg(a, "aspect").unwrap_or("wide") {
                "wide" => Aspect::Wide,
                "tall" => Aspect::Tall,
                "square" => Aspect::Square,
                other => return Err(format!("aspect must be wide, tall or square, not {other}")),
            };
            ed.cloud_image(provider, str_arg(a, "prompt").ok_or("prompt is required")?, aspect)?;
            wait(ed, 300, |e| e.cloud.is_some());
            wait(ed, 60, |e| e.importing > 0);
            match (&ed.cloud_last, ed.status.as_ref().filter(|s| s.error)) {
                (Some(path), _) => text(json!({"path": path, "note": "imported into the project: find it with get_project"})),
                (None, Some(err)) => Err(err.text.clone()),
                (None, None) => Err("the picture was not made".into()),
            }
        }
        "copy_effects" => {
            active(ed)?;
            let clip = ClipId(a.get("clip_id").and_then(Json::as_u64).ok_or("clip_id is required")?);
            let only = match a.get("effect_index").and_then(Json::as_u64) {
                Some(i) => Some(
                    ed.active_seq()
                        .and_then(|q| q.clip(clip))
                        .and_then(|c| c.components.get(i as usize))
                        .map(|c| c.id)
                        .ok_or("that clip has no such effect")?,
                ),
                None => None,
            };
            let names = ed.copy_effects(clip, only)?;
            text(json!({"copied": names}))
        }
        "paste_effects" => {
            active(ed)?;
            let clips = ids_arg(a, "clip_ids");
            if clips.is_empty() {
                return Err("clip_ids is required".into());
            }
            ed.selection = op_application::Selection::only(clips);
            let n = ed.paste_effects()?;
            text(json!({"pasted_on_clips": n}))
        }
        "make_shorts" => {
            let layout = match str_arg(a, "layout").unwrap_or("fill") {
                "fill" => op_application::shorts::ShortsLayout::Fill,
                "blur" => op_application::shorts::ShortsLayout::Blur,
                other => return Err(format!("layout must be fill or blur, not {other}")),
            };
            let sid = ed.make_shorts(layout, f64_arg(a, "focus").unwrap_or(0.5))?;
            let name = ed.project.sequence(sid).map(|s| s.name.clone()).unwrap_or_default();
            text(json!({"sequence_id": sid.0, "name": name, "size": "1080x1920"}))
        }
        "sync_by_audio" => sync_by_audio(ed, a),
        "comfy_status" => comfy_status(ed),
        "comfy_start" => {
            ed.comfy_start(&ed.prefs.comfy_dir.clone())?;
            // without a window nothing else runs meanwhile: wait for the models to load
            let start = Instant::now();
            while op_application::comfy::probe().is_none() && start.elapsed() < Duration::from_secs(300) {
                std::thread::sleep(Duration::from_secs(2));
            }
            comfy_status(ed)
        }
        "comfy_generate" => {
            let req = comfy_request(a)?;
            ed.comfy_generate(req)?;
            wait(ed, 3 * 3600 + 60, |e| e.comfy.job.is_some());
            if let Some(e) = ed.comfy.last_error.clone() {
                return Err(e);
            }
            // the file is imported in the next moments
            wait(ed, 120, |e| matches!(&e.comfy.last, Some((_, None))));
            let (path, item) = ed.comfy.last.clone().ok_or("nothing was generated")?;
            let media = item.ok_or("the file was made but could not be imported")?;
            if a.get("add_to_timeline").and_then(Json::as_bool).unwrap_or(false) {
                call(ed, "add_clip", &json!({"media_id": media.0}))?;
            }
            text(json!({"path": path, "media_id": media.0}))
        }
        "synchronize_clips" => {
            use op_application::audio_sync::Channel;
            use op_application::clipsync::SyncMode;
            let mode = match str_arg(a, "mode").unwrap_or("audio") {
                "start" => SyncMode::ClipStart,
                "end" => SyncMode::ClipEnd,
                "audio" => SyncMode::Audio(match str_arg(a, "channel").unwrap_or("1") {
                    "2" => Channel::Two,
                    "mixdown" => Channel::MixDown,
                    _ => Channel::One,
                }),
                other => return Err(format!("mode {other} is unknown: use start, end or audio")),
            };
            let report = ed.synchronize_clips(&ids_arg(a, "clip_ids"), mode)?;
            text(json!({"moved": report.moved, "left_in_place": report.skipped}))
        }
        "set_keyframes" => set_keyframes(ed, a),
        "set_track" => {
            let sid = active(ed)?;
            let r = parse_track(str_arg(a, "track").ok_or("track is required")?)?;
            let flag = |k: &str| a.get(k).and_then(Json::as_bool);
            let (locked, muted, solo, visible, sync) =
                (flag("locked"), flag("muted"), flag("solo"), flag("visible"), flag("sync_lock"));
            let new_name = str_arg(a, "name").map(str::to_string);
            let mut err = None;
            ed.edit("Track Settings", |p| {
                let t = p
                    .sequence_mut(sid)
                    .and_then(|s| s.track_mut(r))
                    .ok_or_else(|| {
                        err = Some(format!("there is no track {}", track_name(r)));
                        EditError::Nothing
                    })?;
                if let Some(v) = locked {
                    t.locked = v;
                }
                if let Some(v) = muted {
                    t.muted = v;
                }
                if let Some(v) = solo {
                    t.solo = v;
                }
                if let Some(v) = visible {
                    t.enabled = v;
                }
                if let Some(v) = sync {
                    t.sync_lock = v;
                }
                if let Some(n) = new_name {
                    t.name = n;
                }
                Ok(())
            });
            match err {
                Some(e) => Err(e),
                None => text(json!({"ok": true})),
            }
        }
        "add_tracks" => {
            let sid = active(ed)?;
            let kind = match str_arg(a, "kind").unwrap_or("") {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                _ => return Err("kind must be video or audio".into()),
            };
            let count = a.get("count").and_then(Json::as_u64).unwrap_or(1).clamp(1, 16) as usize;
            let existing = ed.project.sequence(sid).map(|s| s.tracks(kind).len()).unwrap_or(0);
            let index = a.get("index").and_then(Json::as_u64).map(|i| i as usize).unwrap_or(existing);
            ed.edit("Add Tracks", |p| {
                op_timeline::add_tracks(p, sid, kind, count, index, Default::default())
            });
            let now = ed.project.sequence(sid).map(|s| s.tracks(kind).len()).unwrap_or(0);
            text(json!({"added": now.saturating_sub(existing), "tracks_of_that_kind": now}))
        }
        "delete_track" => {
            let sid = active(ed)?;
            let r = parse_track(str_arg(a, "track").ok_or("track is required")?)?;
            let before = ed.history.labels().1;
            ed.edit("Delete Track", |p| op_timeline::delete_track(p, sid, r));
            if ed.history.labels().1 == before {
                return Err("that track cannot be deleted (locked, missing, or the last of its kind)".into());
            }
            text(json!({"ok": true}))
        }
        "transcribe_sentences" => {
            ed.status = None;
            ed.transcribe_sentences(str_arg(a, "language").unwrap_or(""))?;
            wait(ed, 7200, |e| e.sentencing.is_some());
            if ed.transcript.is_none() {
                return Err(status_error(ed, "the speech could not be transcribed"));
            }
            sentences_text(ed)
        }
        "get_sentences" => sentences_text(ed),
        "delete_sentences" => {
            let ids: Vec<usize> = a
                .get("sentence_ids")
                .and_then(Json::as_array)
                .map(|v| v.iter().filter_map(Json::as_u64).map(|i| i as usize).collect())
                .unwrap_or_default();
            if ids.is_empty() {
                return Err("sentence_ids is required".into());
            }
            let n = ed.delete_sentences(&ids)?;
            text(json!({"deleted": n, "sequence_duration": ed.active_seq().map(|q| q.duration().seconds())}))
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
            let only = match str_arg(a, "track") {
                Some(t) => Some(parse_track(t)?),
                None => None,
            };
            let (from, to) = (f64_arg(a, "from"), f64_arg(a, "to"));
            let limit = a.get("limit").and_then(Json::as_u64).unwrap_or(40).clamp(1, 200) as usize;
            let offset = a.get("offset").and_then(Json::as_u64).unwrap_or(0) as usize;
            let s = ed.project.sequence(sid).ok_or("no sequence")?;
            let all_clips: usize = s.video.iter().chain(s.audio.iter()).map(|t| t.clips.len()).sum();
            let big = all_clips > limit;
            let filtered = only.is_some() || from.is_some() || to.is_some();
            // a big sequence is summarised unless a track or a time range was asked for: one answer
            // with every clip of a 400-clip project is too large for an assistant to read
            let summary = a.get("summary").and_then(Json::as_bool).unwrap_or(big && !filtered);
            // ... and without the fixed Motion and Opacity of every clip, unless `detail` asks
            let compact = big && !detail;
            let in_range = |c: &Clip| {
                from.is_none_or(|f| c.end().seconds() > f) && to.is_none_or(|t| c.start.seconds() < t)
            };
            let clip_json = |c: &Clip| {
                let effects: Vec<Json> = c
                    .components
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| !(compact && x.effect.starts_with("op.fixed.")))
                    .map(|(i, x)| {
                        let mut e = json!({
                            "index": i,
                            "effect": x.effect,
                            "name": x.def().map(|d| d.name).unwrap_or(""),
                            "enabled": x.enabled,
                        });
                        let show = detail || x.effect == catalog::TEXT || x.effect == catalog::CAPTION;
                        if show {
                            e["params"] = x
                                .params
                                .iter()
                                .filter(|p| !p.key.starts_with('_'))
                                .map(|p| (p.key.clone(), value_json(&p.value)))
                                .collect::<serde_json::Map<_, _>>()
                                .into();
                            // animated parameters: their keyframes, in sequence seconds
                            let animated: serde_json::Map<String, Json> = x
                                .params
                                .iter()
                                .filter(|p| p.is_animated())
                                .map(|p| {
                                    let keys: Vec<Json> = p
                                        .keys
                                        .iter()
                                        .map(|k| {
                                            let at = c.to_sequence(k.time).seconds();
                                            json!({"at": (at * 1000.0).round() / 1000.0, "value": value_json(&k.value)})
                                        })
                                        .collect();
                                    (p.key.clone(), Json::from(keys))
                                })
                                .collect();
                            if !animated.is_empty() {
                                e["keyframes"] = animated.into();
                            }
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
            };
            let mut all_tracks: Vec<(TrackRef, &Track)> = Vec::new();
            for (i, t) in s.video.iter().enumerate().rev() {
                all_tracks.push((TrackRef::video(i), t));
            }
            for (i, t) in s.audio.iter().enumerate() {
                all_tracks.push((TrackRef::audio(i), t));
            }
            let (mut total, mut returned) = (0usize, 0usize);
            let mut tracks = Vec::new();
            for (r, t) in all_tracks.iter().filter(|(r, _)| only.is_none_or(|o| o == *r)) {
                if summary {
                    // what the track holds: how many clips, from when to when, and from which sources
                    let mut sources: Vec<(String, usize, f64, f64)> = Vec::new();
                    for c in t.clips.iter().filter(|c| in_range(c)) {
                        let (start, end) = (c.start.seconds(), c.end().seconds());
                        match sources.iter_mut().find(|x| x.0 == c.name) {
                            Some(x) => {
                                x.1 += 1;
                                x.2 = x.2.min(start);
                                x.3 = x.3.max(end);
                            }
                            None => sources.push((c.name.clone(), 1, start, end)),
                        }
                    }
                    sources.sort_by(|a, b| b.1.cmp(&a.1));
                    let count: usize = sources.iter().map(|x| x.1).sum();
                    let first = sources.iter().map(|x| x.2).fold(f64::INFINITY, f64::min);
                    let last = sources.iter().map(|x| x.3).fold(0.0, f64::max);
                    total += count;
                    tracks.push(json!({
                        "track": track_name(*r), "name": t.name, "locked": t.locked,
                        "clip_count": count,
                        "first_start": if count > 0 { (first * 1000.0).round() / 1000.0 } else { 0.0 },
                        "last_end": (last * 1000.0).round() / 1000.0,
                        "sources": sources.iter().take(8).map(|x| json!({
                            "name": x.0, "clips": x.1,
                            "first_start": (x.2 * 1000.0).round() / 1000.0,
                            "last_end": (x.3 * 1000.0).round() / 1000.0,
                        })).collect::<Vec<_>>(),
                    }));
                } else {
                    let mut clips = Vec::new();
                    for c in t.clips.iter().filter(|c| in_range(c)) {
                        total += 1;
                        if total > offset && returned + clips.len() < limit {
                            clips.push(clip_json(c));
                        }
                    }
                    returned += clips.len();
                    tracks.push(json!({"track": track_name(*r), "name": t.name, "locked": t.locked, "clips": clips}));
                }
            }
            let markers: Vec<Json> = s
                .markers
                .iter()
                .map(|m| json!({"at": secs(m.start), "name": m.name}))
                .collect();
            let mut out = json!({
                "sequence_id": sid.0,
                "name": s.name,
                "width": s.settings.width, "height": s.settings.height,
                "fps": (s.rate().as_f64() * 1000.0).round() / 1000.0,
                "duration": secs(SeqTime::ZERO + s.duration()),
                "playhead": secs(ed.playhead()),
                "selected_clips": ed.selection.clips.iter().map(|c| c.0).collect::<Vec<_>>(),
                "tracks": tracks,
                "markers": markers,
            });
            if big || filtered {
                out["clips_total"] = json!(total);
                if summary {
                    out["mode"] = json!("summary");
                    out["note"] = json!(format!(
                        "{all_clips} clips in this sequence: a summary per track. List clips with track (like V1), from/to (seconds) and offset; up to {limit} come back per call"
                    ));
                } else {
                    out["mode"] = json!("list");
                    out["offset"] = json!(offset);
                    out["returned"] = json!(returned);
                    out["next_offset"] = if offset + returned < total { json!(offset + returned) } else { Json::Null };
                }
            }
            if big {
                // compact: no indentation, the answer is for an assistant
                return Ok(vec![Content::Text(serde_json::to_string(&out).unwrap_or_default())]);
            }
            text(out)
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
        "detect_silence" | "remove_silence" => silence_tool(ed, name, a, false),
        "remix_music" => remix_tool(ed, a, false),
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
