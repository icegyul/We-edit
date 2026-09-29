//! Native build dependencies.
//!
//! * FFmpeg 8.1 development files and shared libraries (GPL build from BtbN/FFmpeg-Builds),
//!   verified against the checksums published with each build. Every platform builds and ships
//!   the same FFmpeg release. `FFMPEG_DIR` overrides the download.
//! * libclang for bindgen on Windows (the `libclang` wheel on PyPI, pinned by SHA-256).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{self, Result, third_party};

const FFMPEG_BASE: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";
const LIBCLANG_WHEEL: (&str, &str) = (
    "https://files.pythonhosted.org/packages/0b/2d/3f480b1e1d31eb3d6de5e3ef641954e5c67430d5ac93b7fa7e07589576c7/libclang-18.1.1-py2.py3-none-win_amd64.whl",
    "4dd2d3b82fab35e2bf9ca717d7b63ac990a3519c7e312f19fa8e86dcc712f7fb",
);
const WHEEL_DLL: &str = "libclang-18.1.1.data/platlib/clang/native/libclang.dll";

pub fn ffmpeg_dir() -> PathBuf {
    std::env::var_os("FFMPEG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| third_party().join("ffmpeg"))
}

fn ffmpeg_archive() -> Result<(String, &'static str)> {
    let (os, ext) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => ("win64", ".zip"),
        ("windows", "aarch64") => ("winarm64", ".zip"),
        ("linux", "x86_64") => ("linux64", ".tar.xz"),
        ("linux", "aarch64") => ("linuxarm64", ".tar.xz"),
        (os, arch) => {
            return Err(format!(
                "no FFmpeg build for {os}/{arch}; install FFmpeg 8 and set FFMPEG_DIR"
            )
            .into());
        }
    };
    Ok((format!("ffmpeg-n8.1-latest-{os}-gpl-shared-8.1"), ext))
}

pub fn ensure_ffmpeg() -> Result {
    if std::env::var_os("FFMPEG_DIR").is_some() {
        return Ok(());
    }
    let dest = ffmpeg_dir();
    if dest.join("include").join("libavcodec").exists() {
        return Ok(());
    }
    let (name, ext) = ffmpeg_archive()?;
    let work = third_party();
    fs::create_dir_all(&work)?;
    let sums = work.join("checksums.sha256");
    util::download(&format!("{FFMPEG_BASE}/checksums.sha256"), &sums)?;
    let file = format!("{name}{ext}");
    let expected = fs::read_to_string(&sums)?
        .lines()
        .find(|l| l.split_whitespace().nth(1) == Some(file.as_str()))
        .and_then(|l| l.split_whitespace().next().map(str::to_string))
        .ok_or_else(|| format!("{file} is not in the published checksums"))?;
    let archive = work.join(&file);
    util::fetch_verified(&format!("{FFMPEG_BASE}/{file}"), &expected, &archive)?;
    util::run(util::tar().arg("-xf").arg(&archive).arg("-C").arg(&work))?;
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(work.join(&name), &dest)?;
    fs::remove_file(&archive)?;
    fs::remove_file(&sums)?;
    eprintln!("FFmpeg 8.1 ready in {} (SHA-256 verified)", dest.display());
    Ok(())
}

fn libclang_dir() -> Result<Option<PathBuf>> {
    if !cfg!(windows) {
        return Ok(None);
    }
    if let Some(p) = std::env::var_os("LIBCLANG_PATH") {
        return Ok(Some(PathBuf::from(p)));
    }
    let dir = third_party().join("libclang");
    if dir.join("libclang.dll").exists() {
        return Ok(Some(dir));
    }
    fs::create_dir_all(&dir)?;
    let wheel = third_party().join("libclang.whl");
    util::fetch_verified(LIBCLANG_WHEEL.0, LIBCLANG_WHEEL.1, &wheel)?;
    let mut zip = zip::ZipArchive::new(fs::File::open(&wheel)?)?;
    let mut entry = zip.by_name(WHEEL_DLL)?;
    let mut out = fs::File::create(dir.join("libclang.dll"))?;
    std::io::copy(&mut entry, &mut out)?;
    drop(entry);
    fs::remove_file(&wheel)?;
    eprintln!("libclang ready in {}", dir.display());
    Ok(Some(dir))
}

/// Makes sure native dependencies exist and configures a cargo command to use them.
pub fn configure(cmd: &mut Command) -> Result {
    ensure_ffmpeg()?;
    let ff = ffmpeg_dir();
    cmd.env("FFMPEG_DIR", &ff);
    if let Some(clang) = libclang_dir()? {
        cmd.env("LIBCLANG_PATH", clang);
    }
    runtime_env(cmd, &ff);
    Ok(())
}

/// Lets programs started from cargo find the FFmpeg shared libraries.
pub fn runtime_env(cmd: &mut Command, ff: &Path) {
    if cfg!(windows) {
        cmd.env("PATH", util::prepend_path("PATH", &ff.join("bin")));
    } else {
        cmd.env(
            "LD_LIBRARY_PATH",
            util::prepend_path("LD_LIBRARY_PATH", &ff.join("lib")),
        );
    }
}

/// Shared libraries to ship next to the program.
pub fn runtime_libraries() -> Result<Vec<PathBuf>> {
    let ff = ffmpeg_dir();
    let mut out = Vec::new();
    if cfg!(windows) {
        for e in fs::read_dir(ff.join("bin"))? {
            let p = e?.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")) {
                out.push(p);
            }
        }
    } else {
        for e in fs::read_dir(ff.join("lib"))? {
            let p = e?.path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            // the versioned sonames (libavcodec.so.62), not the development symlinks
            if name.contains(".so.") && name.matches('.').count() == 2 {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        return Err(format!("no FFmpeg libraries in {}", ff.display()).into());
    }
    Ok(out)
}
