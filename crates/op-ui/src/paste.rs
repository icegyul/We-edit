//! Pasting media from the system clipboard with Ctrl+V: an image copied in a web browser or an
//! image editor, media files copied in the file manager, or a link or path to an image. Copied
//! pixels and downloaded images are saved to a folder (asked for, unless "Always save here" is
//! set in the paste dialog or in Preferences), imported into the project and, when a sequence is
//! being edited, placed at the playhead above the material there.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

/// Largest image downloaded from a link.
const MAX_DOWNLOAD: u64 = 200 << 20;

/// Extensions of files pasted as media (anything the importer reads).
const MEDIA_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "tga", "exr", "dpx", "mp4", "mov",
    "m4v", "mkv", "avi", "webm", "mxf", "mts", "m2ts", "mpg", "mpeg", "wav", "mp3", "aac", "m4a",
    "flac", "ogg", "aif", "aiff",
];

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"];

pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    /// Straight RGBA, 8 bits per channel.
    pub rgba: Vec<u8>,
}

/// What the clipboard offers.
pub enum Found {
    Files(Vec<PathBuf>),
    Bitmap(Bitmap),
    Url(String),
}

/// The contents of a pasted image, ready to be written.
pub enum Payload {
    Bitmap(Bitmap),
    Bytes { bytes: Vec<u8>, ext: &'static str },
}

impl Payload {
    pub fn ext(&self) -> &'static str {
        match self {
            Payload::Bitmap(_) => "png",
            Payload::Bytes { ext, .. } => ext,
        }
    }
}

/// A pasted image waiting to be saved: its data (downloaded in the background for links) and,
/// once chosen, the folder it goes to.
pub struct Job {
    pub id: u64,
    pub name: String,
    /// Where it comes from, for the dialog.
    pub source: String,
    pub data: Arc<Mutex<Option<Result<Payload, String>>>>,
    pub folder: Option<PathBuf>,
    /// Put the imported clip on the timeline.
    pub place: bool,
    pub preview: Option<egui::TextureHandle>,
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_media(p: &Path) -> bool {
    MEDIA_EXT.contains(&ext_of(p).as_str())
}

/// A web link that points at an image file.
fn image_url(t: &str) -> Option<String> {
    let t = t.trim();
    if !(t.starts_with("http://") || t.starts_with("https://")) || t.contains(char::is_whitespace) {
        return None;
    }
    let path = t.split(['?', '#']).next().unwrap_or(t);
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    IMAGE_EXT.contains(&ext.as_str()).then(|| t.to_string())
}

/// The first `<img src="http...">` of copied web content.
fn img_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<img") {
        let tag_start = from + i;
        let tag_end = lower[tag_start..].find('>').map(|e| tag_start + e)?;
        let tag = &html[tag_start..tag_end];
        let tag_lower = &lower[tag_start..tag_end];
        if let Some(s) = tag_lower.find("src=") {
            let rest = &tag[s + 4..];
            let quote = rest.chars().next()?;
            let url = if quote == '"' || quote == '\'' {
                rest[1..].split(quote).next()?
            } else {
                rest.split(char::is_whitespace).next()?
            };
            let url = url.replace("&amp;", "&");
            if url.starts_with("http://") || url.starts_with("https://") {
                return Some(url);
            }
        }
        from = tag_end;
    }
    None
}

/// Reads the clipboard. Plain text that is not a link or path to media is left to the normal
/// paste (clips copied inside the program).
pub fn read() -> Option<Found> {
    let mut cb = arboard::Clipboard::new().ok()?;
    if let Ok(files) = cb.get().file_list() {
        let media: Vec<PathBuf> = files.into_iter().filter(|p| is_media(p)).collect();
        if !media.is_empty() {
            return Some(Found::Files(media));
        }
    }
    if let Ok(img) = cb.get_image()
        && img.width > 0
        && img.height > 0
    {
        return Some(Found::Bitmap(Bitmap {
            width: img.width,
            height: img.height,
            rgba: img.bytes.into_owned(),
        }));
    }
    if let Ok(text) = cb.get_text() {
        let t = text.trim().trim_matches('"');
        if let Some(url) = image_url(t) {
            return Some(Found::Url(url));
        }
        let p = PathBuf::from(t);
        if !t.is_empty() && !t.contains('\n') && p.is_file() && is_media(&p) {
            return Some(Found::Files(vec![p]));
        }
    }
    if let Ok(html) = cb.get().html()
        && let Some(url) = img_src(&html)
    {
        return Some(Found::Url(url));
    }
    None
}

/// The image format of downloaded bytes, from their signature.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG") {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"BM") {
        Some("bmp")
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        Some("tif")
    } else {
        None
    }
}

/// Downloads an image (runs on a background thread).
pub fn download(url: &str) -> Result<Payload, String> {
    let resp = ureq::get(url)
        .header("User-Agent", "OpenPremier")
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_body()
        .into_reader()
        .take(MAX_DOWNLOAD)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let ext = sniff(&bytes).ok_or_else(|| "the link is not an image".to_string())?;
    Ok(Payload::Bytes { bytes, ext })
}

/// A file name that is not taken yet in `folder`.
pub fn unique_path(folder: &Path, name: &str, ext: &str) -> PathBuf {
    let clean: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim();
    let clean = if clean.is_empty() { "Image" } else { clean };
    let mut p = folder.join(format!("{clean}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = folder.join(format!("{clean} ({n}).{ext}"));
        n += 1;
    }
    p
}

/// Writes the image; copied pixels become a PNG.
pub fn save(payload: &Payload, folder: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = unique_path(folder, name, payload.ext());
    match payload {
        Payload::Bitmap(b) => {
            let img = image::RgbaImage::from_raw(b.width as u32, b.height as u32, b.rgba.clone())
                .ok_or("the copied image is incomplete")?;
            img.save_with_format(&path, image::ImageFormat::Png)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Payload::Bytes { bytes, .. } => {
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(path)
}

/// A file name for a link: its last path segment without the extension.
pub fn name_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let last = path.rsplit('/').next()?;
    let stem = last.rsplit_once('.').map(|(s, _)| s).unwrap_or(last);
    let stem = stem.replace("%20", " ");
    (!stem.is_empty()).then_some(stem)
}

/// A small preview of the pasted pixels.
pub fn preview_image(p: &Payload) -> Option<egui::ColorImage> {
    let (w, h, rgba) = match p {
        Payload::Bitmap(b) => (b.width as u32, b.height as u32, b.rgba.clone()),
        Payload::Bytes { bytes, .. } => {
            let img = image::load_from_memory(bytes).ok()?.to_rgba8();
            (img.width(), img.height(), img.into_raw())
        }
    };
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    let k = (480.0 / w as f32).min(270.0 / h as f32).min(1.0);
    let (tw, th) = (
        ((w as f32 * k) as u32).max(1),
        ((h as f32 * k) as u32).max(1),
    );
    let small = image::imageops::thumbnail(&img, tw, th);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [tw as usize, th as usize],
        small.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_web_content() {
        assert_eq!(
            image_url("https://example.com/a/cat.PNG?x=1").as_deref(),
            Some("https://example.com/a/cat.PNG?x=1")
        );
        assert!(image_url("https://example.com/page").is_none());
        assert!(image_url("hello world").is_none());
        let html =
            r#"<html><body><IMG alt="x" SRC="https://cdn.example.com/p.jpg?a=1&amp;b=2"></body>"#;
        assert_eq!(
            img_src(html).as_deref(),
            Some("https://cdn.example.com/p.jpg?a=1&b=2")
        );
        assert_eq!(
            name_from_url("https://x.org/img/my%20photo.jpeg?s=2").as_deref(),
            Some("my photo")
        );
        assert_eq!(sniff(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(sniff(b"<html>"), None);
    }

    #[test]
    fn saves_pixels_as_png_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let p = Payload::Bitmap(Bitmap {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        });
        let a = save(&p, dir.path(), "Pasted").unwrap();
        let b = save(&p, dir.path(), "Pasted").unwrap();
        assert!(a.ends_with("Pasted.png") && b.ends_with("Pasted (2).png"));
        let back = image::open(&a).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(1, 0).0, [0, 0, 255, 255]);
        assert!(preview_image(&p).is_some());
    }
}
