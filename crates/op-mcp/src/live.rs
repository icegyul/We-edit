//! The bridge to the open program. OpenPremier, when the user allows it, listens on a local port
//! (127.0.0.1 only) and writes the port and a random key to `assistant.json` in its settings
//! folder, readable only by the user's programs; `OpenPremier --mcp` finds it there and hands
//! every tool call to the open window, which runs it between two frames.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value as Json, json};

use crate::{Content, Target};

const FILE: &str = "assistant.json";

fn key() -> String {
    // RandomState is seeded by the operating system's random source
    (0..2)
        .map(|_| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(std::process::id() as u64);
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// A tool call waiting for the window.
pub struct Request {
    pub name: String,
    pub args: Json,
    reply: mpsc::Sender<Json>,
}

impl Request {
    /// Sends the result back to the assistant.
    pub fn answer(self, result: Result<Vec<Content>, String>) {
        let msg = match result {
            Ok(c) => json!({"ok": true, "content": crate::content_json(&c)}),
            Err(e) => json!({"ok": false, "error": e}),
        };
        let _ = self.reply.send(msg);
    }
}

/// The listening side, in the program.
pub struct Server {
    file: PathBuf,
    requests: mpsc::Receiver<Request>,
}

impl Server {
    /// `wake` is called when a request arrives (the window redraws only when something happens).
    pub fn start(
        config: &Path,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Server> {
        let wake = std::sync::Arc::new(wake);
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = key();
        let file = config.join(FILE);
        std::fs::create_dir_all(config)?;
        std::fs::write(
            &file,
            json!({"port": port, "key": token, "pid": std::process::id()}).to_string(),
        )?;
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::Builder::new()
            .name("assistant-listener".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (tx, token, wake) = (tx.clone(), token.clone(), wake.clone());
                    let _ = std::thread::Builder::new()
                        .name("assistant".into())
                        .spawn(move || serve(stream, &token, &tx, &*wake));
                }
            })?;
        log::info!("assistants can connect on port {port}");
        Ok(Server { file, requests: rx })
    }

    /// Tool calls that arrived since the last frame.
    pub fn pending(&self) -> Vec<Request> {
        self.requests.try_iter().collect()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

fn serve(stream: TcpStream, token: &str, tx: &mpsc::Sender<Request>, wake: &dyn Fn()) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let Ok(msg) = serde_json::from_str::<Json>(&line) else {
            return;
        };
        if msg.get("key").and_then(Json::as_str) != Some(token) {
            let _ = writeln!(out, "{}", json!({"ok": false, "error": "wrong key"}));
            return;
        }
        let (reply, answer) = mpsc::channel();
        let req = Request {
            name: msg["name"].as_str().unwrap_or("").to_string(),
            args: msg.get("arguments").cloned().unwrap_or(json!({})),
            reply,
        };
        if tx.send(req).is_err() {
            return;
        }
        wake();
        // long tools (exports, transcriptions) may take a while
        let Ok(result) = answer.recv() else { return };
        if writeln!(out, "{result}").is_err() {
            return;
        }
    }
}

/// The calling side, in `OpenPremier --mcp`.
pub struct Remote {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    key: String,
}

impl Remote {
    /// Connects to the open program, if there is one that allows assistants.
    pub fn connect(config: &Path) -> Option<Remote> {
        let text = std::fs::read_to_string(config.join(FILE)).ok()?;
        let info: Json = serde_json::from_str(&text).ok()?;
        let port = info["port"].as_u64()? as u16;
        let stream =
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
                .ok()?;
        let reader = BufReader::new(stream.try_clone().ok()?);
        let mut r = Remote {
            stream,
            reader,
            key: info["key"].as_str()?.to_string(),
        };
        // a stale file of a program that is gone does not answer
        r.call("get_project", &json!({})).ok()?;
        Some(r)
    }
}

impl Target for Remote {
    fn call(&mut self, name: &str, args: &Json) -> Result<Vec<Content>, String> {
        let msg = json!({"key": self.key, "name": name, "arguments": args});
        writeln!(self.stream, "{msg}").map_err(|_| "OpenPremier was closed".to_string())?;
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|_| "OpenPremier was closed".to_string())?;
        let reply: Json =
            serde_json::from_str(&line).map_err(|_| "OpenPremier was closed".to_string())?;
        if reply["ok"].as_bool() == Some(true) {
            Ok(reply["content"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Content::Block)
                .collect())
        } else {
            Err(reply["error"].as_str().unwrap_or("failed").to_string())
        }
    }

    fn describe(&self) -> String {
        "OpenPremier is open: edits happen in the user's open project, which they see live.".into()
    }
}
