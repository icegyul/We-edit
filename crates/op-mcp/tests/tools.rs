//! The assistant tools edit a project the way an assistant would use them.

use op_application::{Dirs, Editor, Preferences};
use op_core::*;
use op_mcp::{Content, Target, handle};
use serde_json::{Value as Json, json};

fn editor(dir: &std::path::Path) -> Editor {
    let mut ed = Editor::new(Dirs::portable(dir), Preferences::default(), false);
    let root = ed.project.root;
    ed.edit("Matte", |p| {
        Ok(p.add_item(
            root,
            "Red",
            ItemKind::Synthetic {
                generator: Generator::ColorMatte {
                    color: Rgba::new(1.0, 0.0, 0.0, 1.0),
                },
                duration: Dur::from_seconds(10.0),
            },
        ))
    });
    ed
}

fn call(t: &mut dyn Target, name: &str, args: Json) -> Json {
    let out = t
        .call(name, &args)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    match &out[0] {
        Content::Text(s) => serde_json::from_str(s).unwrap_or(Json::String(s.clone())),
        _ => Json::Null,
    }
}

#[test]
fn an_assistant_edits_a_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = op_mcp::Local(editor(dir.path()));
    let project = call(&mut t, "get_project", json!({}));
    let media = project["media"][0]["media_id"].as_u64().unwrap();
    call(&mut t, "create_sequence", json!({"name": "AI", "fps": 30}));
    let clip = call(
        &mut t,
        "add_clip",
        json!({"media_id": media, "at": 2, "track": "V2", "source_out": 4}),
    );
    let id = clip["clip_ids"][0].as_u64().unwrap();
    call(
        &mut t,
        "add_effect",
        json!({"clip_ids": [id], "effect_id": "op.video.gaussian_blur", "params": {"blurriness": 25}}),
    );
    call(
        &mut t,
        "set_effect_params",
        json!({"clip_id": id, "effect": "op.fixed.opacity", "params": {"opacity": 40}}),
    );
    let tl = call(&mut t, "get_timeline", json!({"detail": true}));
    let v2 = tl["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["track"] == "V2")
        .unwrap();
    let c = &v2["clips"][0];
    assert_eq!(
        (c["start"].as_f64(), c["end"].as_f64()),
        (Some(2.0), Some(6.0))
    );
    let blur = c["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["effect"] == "op.video.gaussian_blur")
        .unwrap();
    assert_eq!(blur["params"]["blurriness"], 25.0);

    // a wrong parameter is explained, and changes nothing
    let err = t
        .call("add_effect", &json!({"clip_ids": [id], "effect_id": "op.video.gaussian_blur", "params": {"radius": 3}}))
        .err()
        .unwrap();
    assert!(err.contains("blurriness"), "{err}");

    let right = call(&mut t, "split_clips", json!({"at": 3}));
    assert_eq!(right["right_parts"].as_array().unwrap().len(), 1);
    call(
        &mut t,
        "add_text",
        json!({"text": "Hola", "at": 0, "duration": 2}),
    );
    call(&mut t, "undo", json!({}));
    let tl = call(&mut t, "get_timeline", json!({}));
    let titles = tl.to_string().matches("Hola").count();
    assert_eq!(titles, 0, "undo took the title back");

    // through the protocol
    let reply = handle(&mut t, &json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "list_effects", "arguments": {"query": "dissolve", "kind": "transition"}}})).unwrap();
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["result"]["isError"], false);
    let reply = handle(
        &mut t,
        &json!({"jsonrpc": "2.0", "id": 8, "method": "nope"}),
    )
    .unwrap();
    assert_eq!(reply["error"]["code"], -32601);
    assert!(
        handle(
            &mut t,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none()
    );
}

/// A 16 kHz mono WAV: 3 s of a voice-like tone, 2 s of quiet hiss, 3 s of tone.
fn talk(path: &std::path::Path) {
    let rate = 16_000u32;
    let mut samples: Vec<i16> = Vec::new();
    let mut x = 7u32;
    for part in 0..3 {
        let secs = if part == 1 { 2 } else { 3 };
        for i in 0..rate * secs {
            let t = i as f32 / rate as f32;
            let v = if part == 1 {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.002
            } else {
                let tone: f32 = (1..=8)
                    .map(|h| (t * 180.0 * h as f32 * std::f32::consts::TAU).sin() / h as f32)
                    .sum();
                tone * 0.12 * (0.7 + 0.3 * (t * 5.0 * std::f32::consts::TAU).sin())
            };
            samples.push((v * 32767.0) as i16);
        }
    }
    let n = samples.len() as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

#[test]
fn an_assistant_previews_and_removes_silence() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("talk.wav");
    talk(&wav);
    let mut t = op_mcp::Local(Editor::new(
        Dirs::portable(dir.path()),
        Preferences::default(),
        false,
    ));
    let media = call(
        &mut t,
        "import_media",
        json!({"paths": [wav.display().to_string()]}),
    );
    let id = media["imported"][0]["media_id"].as_u64().unwrap();
    call(
        &mut t,
        "create_sequence",
        json!({"name": "Talk", "media_id": id}),
    );
    let secs = |t: &mut op_mcp::Local| {
        call(t, "get_timeline", json!({}))["duration"]
            .as_f64()
            .unwrap()
    };
    assert!((secs(&mut t) - 8.0).abs() < 0.1);

    let found = call(
        &mut t,
        "detect_silence",
        json!({"method": "adaptive", "min_silence": 0.5, "pad_before": 0.1, "pad_after": 0.1}),
    );
    assert_eq!(found["count"], 1, "{found}");
    assert!((found["seconds"].as_f64().unwrap() - 1.8).abs() < 0.15);
    // looking changes nothing
    assert!((secs(&mut t) - 8.0).abs() < 0.1);

    let done = call(
        &mut t,
        "remove_silence",
        json!({"preset": "gentle", "min_silence": 0.5, "pad_before": 0.1, "pad_after": 0.1, "crossfade_ms": 0}),
    );
    assert_eq!(done["applied"], 1, "{done}");
    assert!((secs(&mut t) - 6.2).abs() < 0.15);
    call(&mut t, "undo", json!({}));
    assert!((secs(&mut t) - 8.0).abs() < 0.1);
    // bad settings are explained
    assert!(
        t.call("remove_silence", &json!({"method": "magic"}))
            .is_err()
    );
}

/// A 400-clip project answered `get_timeline` with 585,748 characters, more than an assistant can
/// read, so it could not see its own timeline. A big sequence now answers with a summary, and clips
/// come by track and time range, a page at a time.
#[test]
fn a_big_timeline_is_summarised_and_pages_through_a_track() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = op_mcp::Local(editor(dir.path()));
    let project = call(&mut t, "get_project", json!({}));
    let media = project["media"][0]["media_id"].as_u64().unwrap();
    call(&mut t, "create_sequence", json!({"name": "Big", "fps": 30}));
    for i in 0..120 {
        let track = if i % 2 == 0 { "V1" } else { "V2" };
        call(&mut t, "add_clip", json!({"media_id": media, "at": (i / 2) as f64, "track": track, "source_out": 1}));
    }
    let raw = |t: &mut op_mcp::Local, args: Json| match &t.call("get_timeline", &args).unwrap()[0] {
        Content::Text(s) => s.clone(),
        _ => String::new(),
    };

    // no track or range asked for: a summary, small
    let text = raw(&mut t, json!({}));
    assert!(text.len() < 8_000, "{} characters", text.len());
    let tl: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(tl["mode"], "summary");
    assert_eq!(tl["clips_total"], 120);
    let v1 = tl["tracks"].as_array().unwrap().iter().find(|t| t["track"] == "V1").unwrap();
    assert_eq!(v1["clip_count"], 60);
    assert_eq!(v1["sources"][0]["clips"], 60);
    assert!(v1.get("clips").is_none(), "a summary holds no clip list");
    assert!(tl["note"].as_str().unwrap().contains("from/to"));

    // a track and a time range: its clips, without the fixed Motion and Opacity of every clip
    let tl: Json = serde_json::from_str(&raw(&mut t, json!({"track": "V1", "from": 10, "to": 20}))).unwrap();
    assert_eq!(tl["mode"], "list");
    assert_eq!((tl["clips_total"].clone(), tl["returned"].clone(), tl["next_offset"].clone()), (json!(10), json!(10), Json::Null));
    let tracks = tl["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1, "only the asked track");
    let first = &tracks[0]["clips"][0];
    assert_eq!(first["start"], 10.0);
    assert!(first["effects"].as_array().unwrap().iter().all(|e| !e["effect"].as_str().unwrap().starts_with("op.fixed.")));
    // `detail` brings them back
    let tl: Json = serde_json::from_str(&raw(&mut t, json!({"track": "V1", "limit": 1, "detail": true}))).unwrap();
    assert!(tl["tracks"][0]["clips"][0]["effects"].as_array().unwrap().iter().any(|e| e["effect"] == "op.fixed.opacity"));

    // pages
    let page = |t: &mut op_mcp::Local, offset: u64| -> Json {
        serde_json::from_str(&raw(t, json!({"track": "V1", "limit": 7, "offset": offset}))).unwrap()
    };
    let p1 = page(&mut t, 0);
    assert_eq!((p1["returned"].clone(), p1["next_offset"].clone()), (json!(7), json!(7)));
    let p2 = page(&mut t, 7);
    assert_eq!(p2["tracks"][0]["clips"][0]["start"], 7.0);
    let last = page(&mut t, 56);
    assert_eq!((last["returned"].clone(), last["next_offset"].clone()), (json!(4), Json::Null));
}
