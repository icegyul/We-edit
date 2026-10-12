//! Downloading one file with progress and cancel, shared by the model downloads.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Saves `url` as `dest` (through `dest.part`, so a broken download never looks like a file).
/// `on_bytes` hears every chunk. "cancelled" is returned when `cancel` is set.
pub fn fetch_file(
    url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let part = std::path::PathBuf::from(format!("{}.part", dest.display()));
    let resp = ureq::get(url)
        .header("User-Agent", "We edit")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut out = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
        on_bytes(n as u64);
    }
    drop(out);
    std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))
}
