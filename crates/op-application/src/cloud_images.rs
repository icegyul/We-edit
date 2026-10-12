//! Pictures from cloud image services with the user's own API key: Nano Banana (Google Gemini)
//! and ChatGPT images (OpenAI). The key comes from the Credential Manager (`secrets`) and goes
//! only to that service's address; it is never logged.
//!
//! The requests follow the services' public documentation. Because the answer's exact layout
//! differs between services and versions, the picture is looked for in the whole reply.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::editor::Editor;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    Gemini,
    OpenAi,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Gemini, Provider::OpenAi];

    pub fn label(self) -> &'static str {
        match self {
            Provider::Gemini => "Nano Banana (Google Gemini)",
            Provider::OpenAi => "ChatGPT 이미지 (OpenAI)",
        }
    }

    /// Name of the key in the Credential Manager.
    pub fn secret(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini-api-key",
            Provider::OpenAi => "openai-api-key",
        }
    }

    /// Models to choose from (the first is the default).
    pub fn models(self) -> &'static [&'static str] {
        match self {
            Provider::Gemini => &["gemini-nano-banana-2.1", "gemini-3.1-flash-lite-image", "gemini-3-pro-image"],
            Provider::OpenAi => &["gpt-image-2.5-flare", "gpt-image-2.5-sunburst"],
        }
    }

    /// Where the user makes a key.
    pub fn key_page(self) -> &'static str {
        match self {
            Provider::Gemini => "https://aistudio.google.com/apikey",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
        }
    }

    fn base(self) -> &'static str {
        match self {
            Provider::Gemini => "https://generativelanguage.googleapis.com",
            Provider::OpenAi => "https://api.openai.com",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aspect {
    Wide,
    Tall,
    Square,
}

impl Aspect {
    pub const ALL: [Aspect; 3] = [Aspect::Wide, Aspect::Tall, Aspect::Square];

    pub fn label(self) -> &'static str {
        match self {
            Aspect::Wide => "가로 16:9",
            Aspect::Tall => "세로 9:16 (쇼츠)",
            Aspect::Square => "정사각 1:1",
        }
    }
}

pub struct Image {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// The message of an error reply, without anything that could be the key.
fn problem(p: Provider, status: u16, body: &str, key: &str) -> String {
    let message = serde_json::from_str::<Json>(body)
        .ok()
        .and_then(|j| j.pointer("/error/message").and_then(Json::as_str).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect());
    let message = if key.is_empty() { message } else { message.replace(key, "***") };
    format!("{} (HTTP {status}): {message}", p.label())
}

/// Finds the first base64 string in `v` that decodes to a PNG, JPEG or WebP picture.
pub fn find_image(v: &Json) -> Option<Image> {
    match v {
        Json::String(s) if s.len() > 64 => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(s.trim()).ok()?;
            let ext = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                "png"
            } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "jpg"
            } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
                "webp"
            } else {
                return None;
            };
            Some(Image { bytes, ext })
        }
        Json::Array(a) => a.iter().find_map(find_image),
        Json::Object(o) => o.values().find_map(find_image),
        _ => None,
    }
}

fn authorized(p: Provider, req: ureq::RequestBuilder<ureq::typestate::WithBody>, key: &str) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
    match p {
        Provider::Gemini => req.header("x-goog-api-key", key),
        Provider::OpenAi => req.header("Authorization", &format!("Bearer {key}")),
    }
}

/// Makes one picture. `base` is the service's address (a test server in the tests).
pub fn generate_at(base: &str, p: Provider, key: &str, model: &str, prompt: &str, aspect: Aspect) -> Result<Image, String> {
    let (path, body) = match p {
        Provider::Gemini => (
            "/v1beta/interactions",
            json!({
                "model": model,
                "input": [{"type": "text", "text": prompt}],
                "response_format": {
                    "type": "image",
                    "mime_type": "image/jpeg",
                    "aspect_ratio": match aspect { Aspect::Wide => "16:9", Aspect::Tall => "9:16", Aspect::Square => "1:1" },
                    "image_size": "2K"
                }
            }),
        ),
        Provider::OpenAi => (
            "/v1/images/generations",
            json!({
                "model": model,
                "prompt": prompt,
                "size": match aspect { Aspect::Wide => "1536x1024", Aspect::Tall => "1024x1536", Aspect::Square => "1024x1024" },
                "n": 1
            }),
        ),
    };
    let req = authorized(p, agent(300).post(&format!("{base}{path}")).header("Content-Type", "application/json"), key);
    let mut resp = req.send(body.to_string()).map_err(|e| format!("{}: {e}", p.label()))?;
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .with_config()
        .limit(64 << 20)
        .read_to_string()
        .map_err(|e| format!("{}: {e}", p.label()))?;
    if !(200..300).contains(&status) {
        return Err(problem(p, status, &text, key));
    }
    let json: Json = serde_json::from_str(&text).map_err(|_| format!("{}: the answer is not readable", p.label()))?;
    find_image(&json).ok_or_else(|| format!("{}: the answer has no picture", p.label()))
}

pub fn generate(p: Provider, key: &str, model: &str, prompt: &str, aspect: Aspect) -> Result<Image, String> {
    generate_at(p.base(), p, key, model, prompt, aspect)
}

/// Whether the service accepts the key (a free request that lists models).
pub fn check_key_at(base: &str, p: Provider, key: &str) -> Result<(), String> {
    let path = match p {
        Provider::Gemini => "/v1beta/models?pageSize=1",
        Provider::OpenAi => "/v1/models",
    };
    let req = match p {
        Provider::Gemini => agent(30).get(&format!("{base}{path}")).header("x-goog-api-key", key),
        Provider::OpenAi => agent(30).get(&format!("{base}{path}")).header("Authorization", &format!("Bearer {key}")),
    };
    let mut resp = req.call().map_err(|e| format!("{}: {e}", p.label()))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    Err(problem(p, status, &text, key))
}

pub fn check_key(p: Provider, key: &str) -> Result<(), String> {
    check_key_at(p.base(), p, key)
}

/// A picture being made in the background.
pub struct CloudJob {
    pub provider: Provider,
    pub prompt: String,
    result: Arc<Mutex<Option<Result<Image, String>>>>,
}

impl Editor {
    /// Starts a picture with the saved key of `p`; the result is imported into the project.
    pub fn cloud_image(&mut self, p: Provider, prompt: &str, aspect: Aspect) -> Result<(), String> {
        if self.cloud.is_some() {
            return Err("A picture is already being made".into());
        }
        if prompt.trim().is_empty() {
            return Err("Describe the picture first".into());
        }
        let key = crate::secrets::get(p.secret()).ok_or_else(|| format!("Save the API key of {} first (Preferences > 생성형 AI)", p.label()))?;
        let model = match p {
            Provider::Gemini => self.prefs.gemini_model.clone(),
            Provider::OpenAi => self.prefs.openai_model.clone(),
        };
        let model = if p.models().contains(&model.as_str()) || !model.is_empty() { model } else { p.models()[0].to_string() };
        let result = Arc::new(Mutex::new(None));
        let (r, prompt_text) = (result.clone(), prompt.trim().to_string());
        std::thread::Builder::new()
            .name("cloud-image".into())
            .spawn(move || {
                let res = generate(p, &key, &model, &prompt_text, aspect);
                *r.lock() = Some(res);
            })
            .map_err(|_| "The request could not start".to_string())?;
        self.cloud = Some(CloudJob { provider: p, prompt: prompt.trim().to_string(), result });
        Ok(())
    }

    pub(crate) fn poll_cloud(&mut self) {
        let finished = self.cloud.as_ref().and_then(|j| j.result.lock().take());
        let Some(res) = finished else { return };
        let Some(job) = self.cloud.take() else { return };
        match res {
            Ok(img) => {
                let dir = crate::comfy::generated_dir(&self.dirs.data);
                let name = format!(
                    "{}-{}.{}",
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                    match job.provider { Provider::Gemini => "nano-banana", Provider::OpenAi => "chatgpt-image" },
                    img.ext
                );
                let path: PathBuf = dir.join(name);
                if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, &img.bytes)) {
                    self.error(format!("The picture could not be saved: {e}"));
                    return;
                }
                let bin = self.bin;
                self.import(vec![path.clone()], bin);
                self.cloud_last = Some(path);
                self.info(format!("{}: 그림을 프로젝트에 넣었습니다", job.provider.label()));
            }
            Err(e) => self.error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// 1x1 PNG.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    /// A one-shot HTTP server: answers with `reply` (status, body) and hands back the request.
    fn serve(status: u16, reply: String) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = s.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(h) = text.find("\r\n\r\n") {
                    let len = text[..h]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                        .unwrap_or(0);
                    if buf.len() >= h + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
            String::from_utf8_lossy(&buf).to_string()
        });
        (base, handle)
    }

    #[test]
    fn a_gemini_picture_is_found_wherever_the_answer_keeps_it() {
        let reply = json!({"steps": [{"type": "model_output", "content": [{"type": "text", "text": "ok"}, {"type": "image", "data": PNG}]}]}).to_string();
        let (base, server) = serve(200, reply);
        let img = generate_at(&base, Provider::Gemini, "gm-key", "gemini-nano-banana-2.1", "a fox", Aspect::Tall).unwrap();
        assert_eq!(img.ext, "png");
        assert!(img.bytes.starts_with(&[0x89, b'P']));
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1beta/interactions"), "{request}");
        assert!(request.to_ascii_lowercase().contains("x-goog-api-key: gm-key"));
        assert!(request.contains("\"model\":\"gemini-nano-banana-2.1\""));
        assert!(request.contains("\"aspect_ratio\":\"9:16\""));
        assert!(request.contains("a fox"));
    }

    #[test]
    fn an_openai_picture_comes_back_as_b64_json() {
        let (base, server) = serve(200, json!({"data": [{"b64_json": PNG}]}).to_string());
        let img = generate_at(&base, Provider::OpenAi, "sk-secret", "gpt-image-2.5-flare", "a cat", Aspect::Wide).unwrap();
        assert_eq!(img.ext, "png");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/images/generations"), "{request}");
        assert!(request.contains("Bearer sk-secret"));
        assert!(request.contains("\"size\":\"1536x1024\""));
    }

    #[test]
    fn an_error_is_explained_and_never_shows_the_key() {
        let (base, server) = serve(401, json!({"error": {"message": "Incorrect API key provided: sk-secret"}}).to_string());
        let err = generate_at(&base, Provider::OpenAi, "sk-secret", "m", "p", Aspect::Square).err().unwrap();
        server.join().unwrap();
        assert!(err.contains("401"), "{err}");
        assert!(err.contains("Incorrect API key"), "{err}");
        assert!(!err.contains("sk-secret"), "{err}");
    }

    #[test]
    fn a_reply_without_a_picture_is_reported() {
        let (base, server) = serve(200, json!({"data": [{"note": "nothing here"}]}).to_string());
        let err = generate_at(&base, Provider::OpenAi, "k", "m", "p", Aspect::Wide).err().unwrap();
        server.join().unwrap();
        assert!(err.contains("no picture"), "{err}");
    }

    #[test]
    fn the_key_check_follows_the_status() {
        let (base, server) = serve(200, "{\"data\":[]}".into());
        assert!(check_key_at(&base, Provider::OpenAi, "good").is_ok());
        let req = server.join().unwrap();
        assert!(req.starts_with("GET /v1/models"));
        let (base, server) = serve(403, json!({"error": {"message": "API key not valid"}}).to_string());
        let err = check_key_at(&base, Provider::Gemini, "bad").err().unwrap();
        server.join().unwrap();
        assert!(err.contains("403") && err.contains("not valid"), "{err}");
    }
}
