//! Release packages.
//!
//! * Windows: `dist/OpenPremier-<version>-windows-x64.zip`, a folder with the program, the
//!   FFmpeg libraries it links, the licenses and the third-party notices. It runs from any
//!   folder; a `portable.txt` file next to the program keeps settings in that folder.
//! * Linux: the AppImage (see `linux.rs`).
//! * macOS: a disk image with the app bundle (see `macos.rs`).
//!
//! Each package is started with `--self-test` from its final layout before it is accepted.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{self, Result};
use crate::{configured_cargo, deps};

/// FFmpeg libraries the program links (the others in the FFmpeg build are not needed).
pub const FFMPEG_LIBS: [&str; 5] = ["avcodec", "avformat", "avutil", "swresample", "swscale"];

pub fn dist(args: &[String]) -> Result {
    if cfg!(target_os = "macos") {
        let dmg = crate::macos::package()?;
        eprintln!(
            "\nmacOS package: {} ({})",
            dmg.display(),
            util::size_text(&dmg)
        );
    } else if cfg!(windows) {
        let zip = windows_zip()?;
        if !args.iter().any(|a| a == "--windows-only") {
            crate::linux::appimage_task()?;
        }
        eprintln!(
            "\nWindows package: {} ({})",
            zip.display(),
            util::size_text(&zip)
        );
    } else {
        crate::linux::inside(args)?;
    }
    write_checksums()?;
    Ok(())
}

/// Builds the release binary with the FFmpeg configuration.
pub fn release_build() -> Result {
    let mut c = configured_cargo(&["build", "--release", "--locked", "--package", util::BIN])?;
    util::run(&mut c)
}

fn windows_zip() -> Result<PathBuf> {
    release_build()?;
    let version = util::version()?;
    let name = format!("{}-{version}-windows-x64", util::APP_NAME);
    let dist = util::dist_dir();
    let stage = dist.join(&name);
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(stage.join("licenses"))?;
    let exe = util::root()
        .join("target/release")
        .join(format!("{}.exe", util::BIN));
    fs::copy(&exe, stage.join(format!("{}.exe", util::APP_NAME)))?;
    for lib in deps::runtime_libraries()? {
        let file = lib.file_name().unwrap().to_string_lossy().to_string();
        if FFMPEG_LIBS
            .iter()
            .any(|l| file.starts_with(&format!("{l}-")))
        {
            fs::copy(&lib, stage.join(&file))?;
        }
    }
    write_docs(&stage, "windows")?;
    self_test(&stage.join(format!("{}.exe", util::APP_NAME)))?;
    let zip = dist.join(format!("{name}.zip"));
    zip_dir(&stage, &zip, &name)?;
    Ok(zip)
}

/// License, readme and third-party notices shipped with every package.
pub fn write_docs(dir: &Path, platform: &str) -> Result {
    let root = util::root();
    fs::copy(root.join("LICENSE"), dir.join("LICENSE.txt"))?;
    let ff_license = deps::ffmpeg_dir().join("LICENSE.txt");
    if ff_license.exists() {
        fs::create_dir_all(dir.join("licenses"))?;
        fs::copy(&ff_license, dir.join("licenses/FFmpeg-LICENSE.txt"))?;
    }
    let version = util::version()?;
    let start = if platform == "windows" {
        format!(
            "Start {}.exe. To keep settings, caches and autosaves in this folder instead of your user profile, create an empty file named portable.txt next to it.",
            util::APP_NAME
        )
    } else if platform == "macos" {
        format!(
            "Drag {name}.app to Applications and open it. The app is not notarized: the first time, right-click it and choose Open (or allow it in System Settings > Privacy & Security).",
            name = util::APP_NAME
        )
    } else {
        "Make the AppImage executable (chmod +x) and start it. Integrate it with your desktop using your AppImage launcher of choice.".to_string()
    };
    let readme = format!(
        "OpenPremier {version}\r\n\
         A free and open source video editor for Windows and Linux.\r\n\r\n\
         {start}\r\n\r\n\
         Supported structures from Adobe Premiere Pro (.prproj) import read-only; Final Cut Pro XML and OpenTimelineIO interchange open through File > Open Project.\r\n\
         The keyboard shortcuts match the Premiere Pro defaults; your own .kys file can be imported in Edit > Keyboard Shortcuts.\r\n\r\n\
         License: GNU General Public License version 3 or later (LICENSE.txt).\r\n\
         Third-party components and their licenses: THIRD-PARTY-NOTICES.txt.\r\n\r\n\
         Adobe and Premiere Pro are trademarks of Adobe Inc. OpenPremier is an independent project and is not affiliated with Adobe.\r\n"
    );
    fs::write(dir.join("README.txt"), readme)?;
    fs::write(dir.join("THIRD-PARTY-NOTICES.txt"), third_party_notices()?)?;
    Ok(())
}

/// Every Rust crate in the lock file with its license, plus FFmpeg.
fn third_party_notices() -> Result<String> {
    let out = util::output(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(util::root())
            .args(["metadata", "--format-version", "1", "--locked"]),
    )?;
    let meta: serde_json::Value = serde_json::from_str(&out)?;
    let members: Vec<String> = meta["workspace_members"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut rows: Vec<(String, String, String, String)> = meta["packages"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|p| !members.iter().any(|m| Some(m.as_str()) == p["id"].as_str()))
                .map(|p| {
                    let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
                    (
                        s("name"),
                        s("version"),
                        if s("license").is_empty() {
                            s("license_file")
                        } else {
                            s("license")
                        },
                        s("repository"),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    rows.sort();
    rows.dedup();
    let mut text = String::new();
    text.push_str("OpenPremier includes or links the following third-party software.\r\n\r\n");
    text.push_str("FFmpeg 8.1 (https://ffmpeg.org), shared libraries built by https://github.com/BtbN/FFmpeg-Builds with GPL components.\r\n");
    text.push_str(
        "License: GNU General Public License version 3 or later (licenses/FFmpeg-LICENSE.txt).\r\n",
    );
    text.push_str("Source code: https://ffmpeg.org/releases/ and https://github.com/BtbN/FFmpeg-Builds (build scripts).\r\n\r\n");
    text.push_str("Speech models for automatic captions are not included: OpenAI Whisper (tiny, base or small; MIT License, https://github.com/openai/whisper) is downloaded from https://huggingface.co/openai when the user asks for captions.\r\n\r\n");
    text.push_str("Rust crates (name, version, license, repository):\r\n\r\n");
    for (n, v, l, r) in rows {
        text.push_str(&format!("{n} {v}\r\n    License: {l}\r\n"));
        if !r.is_empty() {
            text.push_str(&format!("    {r}\r\n"));
        }
    }
    Ok(text)
}

/// Runs the packaged program's self-test without the development FFmpeg on the search path.
pub fn self_test(program: &Path) -> Result {
    let mut cmd = Command::new(program);
    cmd.arg("--self-test");
    if cfg!(windows) {
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        cmd.env("PATH", system.join("System32"));
    } else {
        cmd.env_remove("LD_LIBRARY_PATH");
        cmd.env_remove("DYLD_LIBRARY_PATH");
        cmd.env_remove("DYLD_FALLBACK_LIBRARY_PATH");
    }
    let out = cmd
        .output()
        .map_err(|e| format!("cannot start {}: {e}", program.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    eprint!("{text}");
    if !out.status.success() || !text.contains("self-test passed") {
        return Err(format!(
            "self-test of {} failed: {}",
            program.display(),
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(())
}

fn zip_dir(dir: &Path, to: &Path, prefix: &str) -> Result {
    let file = fs::File::create(to)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));
    let mut files = Vec::new();
    collect(dir, &mut files)?;
    files.sort();
    for f in files {
        let rel = f.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
        zip.start_file(format!("{prefix}/{rel}"), options)?;
        zip.write_all(&fs::read(&f)?)?;
    }
    zip.finish()?;
    Ok(())
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result {
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// dist/SHA256SUMS.txt for every package in dist/.
pub fn write_checksums() -> Result {
    let dist = util::dist_dir();
    let mut lines = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dist)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.ends_with(".zip") || name.ends_with(".AppImage") || name.ends_with(".dmg") {
            lines.push(format!("{}  {name}", util::sha256_file(&p)?));
        }
    }
    fs::write(dist.join("SHA256SUMS.txt"), lines.join("\n") + "\n")?;
    eprintln!("{}", lines.join("\n"));
    Ok(())
}
