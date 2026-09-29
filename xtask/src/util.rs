//! Shared helpers: paths, running commands, verified downloads.

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const APP_NAME: &str = "OpenPremier";
pub const APP_ID: &str = "io.github.openpremier.OpenPremier";
pub const BIN: &str = "openpremier";

/// Repository root.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn dist_dir() -> PathBuf {
    root().join("dist")
}

pub fn third_party() -> PathBuf {
    root().join("third_party")
}

/// Workspace version from the root Cargo.toml.
pub fn version() -> Result<String> {
    let text = fs::read_to_string(root().join("Cargo.toml"))?;
    let mut in_pkg = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_pkg = l == "[workspace.package]";
            continue;
        }
        if in_pkg && let Some(v) = l.strip_prefix("version") {
            let v = v.trim().trim_start_matches('=').trim().trim_matches('"');
            return Ok(v.to_string());
        }
    }
    Err("no version in [workspace.package]".into())
}

pub fn run(cmd: &mut Command) -> Result {
    eprintln!("> {}", describe(cmd));
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run {}: {e}", cmd.get_program().to_string_lossy()))?;
    if !status.success() {
        return Err(format!("{} failed ({status})", cmd.get_program().to_string_lossy()).into());
    }
    Ok(())
}

pub fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run {}: {e}", cmd.get_program().to_string_lossy()))?;
    if !out.status.success() {
        return Err(format!(
            "{} failed: {}",
            describe(cmd),
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn describe(cmd: &Command) -> String {
    let mut s = cmd.get_program().to_string_lossy().into_owned();
    for a in cmd.get_args() {
        s.push(' ');
        s.push_str(&a.to_string_lossy());
    }
    s
}

pub fn have(program: &str) -> bool {
    let probe = if cfg!(windows) { "where" } else { "which" };
    Command::new(probe)
        .arg(program)
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn download(url: &str, to: &Path) -> Result {
    eprintln!("downloading {url}");
    if let Some(p) = to.parent() {
        fs::create_dir_all(p)?;
    }
    let resp = ureq::get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut reader = resp.into_body().into_reader();
    let tmp = to.with_extension("part");
    let mut f = fs::File::create(&tmp)?;
    std::io::copy(&mut reader, &mut f)?;
    f.flush()?;
    drop(f);
    fs::rename(tmp, to)?;
    Ok(())
}

/// Downloads `url` to `to` unless a file with the expected SHA-256 is already there.
pub fn fetch_verified(url: &str, sha256: &str, to: &Path) -> Result {
    if to.exists() && sha256_file(to)?.eq_ignore_ascii_case(sha256) {
        return Ok(());
    }
    download(url, to)?;
    let got = sha256_file(to)?;
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = fs::remove_file(to);
        return Err(format!("checksum mismatch for {url}: expected {sha256}, got {got}").into());
    }
    Ok(())
}

/// `tar` understands .zip and .tar.xz on Windows 10+ and Linux.
pub fn tar() -> Command {
    if cfg!(windows) {
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        Command::new(system.join("System32").join("tar.exe"))
    } else {
        Command::new("tar")
    }
}

pub fn size_text(path: &Path) -> String {
    let n = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    format!("{:.1} MB", n as f64 / 1_048_576.0)
}

#[cfg(unix)]
pub fn make_executable(path: &Path) -> Result {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> Result {
    Ok(())
}

/// Prepends a directory to a PATH-like variable.
pub fn prepend_path(var: &str, dir: &Path) -> OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(old) = std::env::var_os(var) {
        paths.extend(std::env::split_paths(&old));
    }
    std::env::join_paths(paths).unwrap_or_default()
}
