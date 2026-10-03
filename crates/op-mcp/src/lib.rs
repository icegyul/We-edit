//! OpenPremier for AI assistants. `OpenPremier --mcp` speaks the Model Context Protocol over
//! standard input and output, so Claude Code, Codex, Cursor or any MCP client can edit with it:
//! import media, cut, add effects, titles and captions, look at frames and export.
//!
//! When OpenPremier is open and Preferences > AI Assistants allows it, the tools act on the open
//! project and the user sees every edit as it happens (and can undo it). Otherwise they act on a
//! project of their own, saved with `save_project`.

pub mod live;
pub mod render;
pub mod tools;

use std::io::{BufRead, Write};

use serde_json::{Value as Json, json};

pub use tools::Content;

/// Where tool calls go: an editor in this process, or the open program.
pub trait Target {
    fn call(&mut self, name: &str, args: &Json) -> Result<Vec<Content>, String>;
    /// Says where the edits happen, for the assistant.
    fn describe(&self) -> String;
}

/// An editor of this process (no window).
pub struct Local(pub op_application::Editor);

impl Target for Local {
    fn call(&mut self, name: &str, args: &Json) -> Result<Vec<Content>, String> {
        tools::call(&mut self.0, name, args)
    }

    fn describe(&self) -> String {
        "OpenPremier is not open (or does not allow assistants), so edits go to a project of this \
         session: save it with save_project and open the file in OpenPremier."
            .into()
    }
}

const INSTRUCTIONS: &str = "OpenPremier is a video editor. Start with get_project and \
get_timeline. Times are in seconds, tracks are named V1, V2... (video) and A1, A2... (audio). \
Use list_effects to find effect ids and parameters, render_frame to look at the result, and \
export to write a video file. Every change is one undo step.";

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// MCP content blocks.
pub fn content_json(content: &[Content]) -> Json {
    content
        .iter()
        .map(|c| match c {
            Content::Text(t) => json!({"type": "text", "text": t}),
            Content::Image(png) => {
                json!({"type": "image", "data": base64(png), "mimeType": "image/png"})
            }
            Content::Block(b) => b.clone(),
        })
        .collect()
}

/// Answers one JSON-RPC request; None for notifications.
pub fn handle(target: &mut dyn Target, msg: &Json) -> Option<Json> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Json::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Json::as_str)
                .unwrap_or("2025-06-18"),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "openpremier", "version": env!("CARGO_PKG_VERSION")},
            "instructions": format!("{INSTRUCTIONS} {}", target.describe()),
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools::list()})),
        "tools/call" => {
            let name = params.get("name").and_then(Json::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            log::info!("assistant tool {name}");
            Ok(match target.call(name, &args) {
                Ok(c) => json!({"content": content_json(&c), "isError": false}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            })
        }
        _ => Err(json!({"code": -32601, "message": format!("unknown method {method}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

/// Serves MCP over standard input and output until the client closes it.
pub fn serve_stdio(target: &mut dyn Target) {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Json>(&line) {
            Ok(msg) => handle(target, &msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": e.to_string()},
            })),
        };
        if let Some(r) = reply
            && (writeln!(stdout, "{r}").is_err() || stdout.flush().is_err())
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_tool_has_a_schema() {
        let list = tools::list();
        let names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"get_timeline") && names.contains(&"render_frame"));
        for t in list.as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }
}
