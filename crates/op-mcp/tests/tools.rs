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
