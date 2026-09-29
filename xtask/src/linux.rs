//! The Linux AppImage.
//!
//! On Linux (`cargo xtask appimage`, or `linux-inside` in the build container) the release binary
//! is built with an RPATH of `$ORIGIN/../lib`, assembled with the FFmpeg libraries into an AppDir
//! and packed with appimagetool and the static type2 runtime, both pinned by SHA-256.
//!
//! On Windows the same steps run inside the `OpenPremier-Build` WSL distribution, in a
//! manylinux_2_28 container, so the AppImage runs on distributions with glibc 2.28 or newer. The
//! result is then started on the WSL distribution itself and in clean Ubuntu 22.04 and Fedora 42
//! containers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dist::{self, FFMPEG_LIBS};
use crate::util::{self, Result};
use crate::{configured_cargo, deps};

const APPIMAGETOOL: (&str, &str) = (
    "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage",
    "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0",
);
const RUNTIME: (&str, &str) = (
    "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64",
    "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d",
);
const WSL_DISTRO: &str = "OpenPremier-Build";
const IMAGE: &str = "quay.io/pypa/manylinux_2_28_x86_64";
const ICON_SIZES: [u32; 8] = [16, 32, 48, 64, 128, 256, 512, 1024];

pub fn appimage_task() -> Result {
    if cfg!(windows) {
        via_wsl()
    } else {
        inside(&[])
    }
}

fn appimage_name() -> Result<String> {
    Ok(format!(
        "{}-{}-x86_64.AppImage",
        util::APP_NAME,
        util::version()?
    ))
}

/// "C:\OpenPremier" -> "/mnt/c/OpenPremier"
fn wsl_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    match s.split_once(":/") {
        Some((drive, rest)) => format!("/mnt/{}/{rest}", drive.to_ascii_lowercase()),
        None => s,
    }
}

fn wsl(script: &str) -> Result {
    let mut c = Command::new("wsl");
    c.args(["-d", WSL_DISTRO, "--", "bash", "-lc", script]);
    util::run(&mut c)
}

fn rustc_version() -> String {
    util::output(Command::new("rustc").arg("--version"))
        .ok()
        .and_then(|v| v.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_else(|| "stable".into())
}

fn via_wsl() -> Result {
    let root = util::root();
    let dist = util::dist_dir();
    fs::create_dir_all(&dist)?;
    // the sources as git sees them: tracked and new files, never ignored ones
    let list = util::output(Command::new("git").current_dir(&root).args([
        "ls-files",
        "-co",
        "--exclude-standard",
        "-z",
    ]))?;
    let list_path = dist.join(".sources");
    fs::write(&list_path, list)?;
    let name = appimage_name()?;
    let win_root = wsl_path(&root);
    let script = format!(
        r#"set -euo pipefail
mkdir -p ~/opp-build/src
find ~/opp-build/src -mindepth 1 -maxdepth 1 ! -name third_party -exec rm -rf {{}} +
cd "{win_root}"
tar --null -T "{win_root}/dist/.sources" -cf - | tar -xf - -C ~/opp-build/src --no-same-owner
podman run --rm -v ~/opp-build:/work -e RUST_VERSION={rust} {IMAGE} bash /work/src/packaging/linux/build-in-container.sh
cp ~/opp-build/src/dist/{name} "{win_root}/dist/{name}"
"#,
        rust = rustc_version()
    );
    wsl(&script)?;
    fs::remove_file(&list_path)?;
    smoke_tests(&name)?;
    let out = dist.join(&name);
    eprintln!(
        "\nLinux package: {} ({})",
        out.display(),
        util::size_text(&out)
    );
    Ok(())
}

/// Starts the AppImage on the WSL distribution (self-test with its GPU) and in clean containers
/// of other distributions (start-up and library check).
fn smoke_tests(name: &str) -> Result {
    let image = format!("{}/dist/{name}", wsl_path(&util::root()));
    wsl(&format!(
        r#"set -e; cp "{image}" /tmp/{name}; chmod +x /tmp/{name}; cd /tmp; APPIMAGE_EXTRACT_AND_RUN=1 ./{name} --self-test"#
    ))?;
    for (img, setup) in [
        (
            "docker.io/library/ubuntu:22.04",
            "apt-get update -qq >/dev/null && apt-get install -y -qq libasound2 >/dev/null",
        ),
        (
            "registry.fedoraproject.org/fedora:42",
            "dnf -y -q install alsa-lib >/dev/null",
        ),
    ] {
        wsl(&format!(
            r#"set -e; podman run --rm -v /tmp/{name}:/app/{name}:ro {img} bash -c '{setup} && cd /tmp && APPIMAGE_EXTRACT_AND_RUN=1 /app/{name} --version'"#
        ))?;
    }
    Ok(())
}

/// Builds the AppImage on Linux (natively or in the build container).
pub fn inside(_args: &[String]) -> Result {
    let root = util::root();
    let mut build = configured_cargo(&["build", "--release", "--locked", "--package", util::BIN])?;
    build.env("RUSTFLAGS", "-C link-arg=-Wl,-rpath,$ORIGIN/../lib");
    util::run(&mut build)?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let bin = target.join("release").join(util::BIN);

    let appdir = target.join("AppDir");
    if appdir.exists() {
        fs::remove_dir_all(&appdir)?;
    }
    let usr = appdir.join("usr");
    for d in [
        "bin",
        "lib",
        "share/applications",
        "share/metainfo",
        "share/mime/packages",
        "share/doc/openpremier",
    ] {
        fs::create_dir_all(usr.join(d))?;
    }
    let app_bin = usr.join("bin").join(util::BIN);
    fs::copy(&bin, &app_bin)?;
    // keep symbols for readable crash reports, drop debug information
    if util::have("strip") {
        util::run(Command::new("strip").arg("--strip-debug").arg(&app_bin))?;
    }
    util::make_executable(&app_bin)?;
    for lib in deps::runtime_libraries()? {
        let file = lib.file_name().unwrap().to_string_lossy().to_string();
        if FFMPEG_LIBS
            .iter()
            .any(|l| file.starts_with(&format!("lib{l}.so.")))
        {
            fs::copy(&lib, usr.join("lib").join(&file))?;
        }
    }
    let pkg = root.join("packaging/linux");
    let id = util::APP_ID;
    fs::copy(
        pkg.join(format!("{id}.desktop")),
        usr.join(format!("share/applications/{id}.desktop")),
    )?;
    fs::copy(
        pkg.join(format!("{id}.desktop")),
        appdir.join(format!("{id}.desktop")),
    )?;
    fs::copy(
        pkg.join(format!("{id}.metainfo.xml")),
        usr.join(format!("share/metainfo/{id}.metainfo.xml")),
    )?;
    fs::copy(
        pkg.join(format!("{id}.mime.xml")),
        usr.join(format!("share/mime/packages/{id}.xml")),
    )?;
    for size in ICON_SIZES {
        let dir = usr.join(format!("share/icons/hicolor/{size}x{size}/apps"));
        fs::create_dir_all(&dir)?;
        fs::copy(
            root.join(format!("assets/icons/openpremier-{size}.png")),
            dir.join(format!("{id}.png")),
        )?;
    }
    fs::copy(
        root.join("assets/icons/openpremier-256.png"),
        appdir.join(format!("{id}.png")),
    )?;
    fs::copy(
        root.join("assets/icons/openpremier-256.png"),
        appdir.join(".DirIcon"),
    )?;
    fs::copy(pkg.join("AppRun"), appdir.join("AppRun"))?;
    util::make_executable(&appdir.join("AppRun"))?;
    dist::write_docs(&usr.join("share/doc/openpremier"), "linux")?;

    // every library must resolve from the AppDir or the system
    let ldd = util::output(Command::new("ldd").arg(&app_bin))?;
    eprint!("{ldd}");
    if ldd.contains("not found") {
        return Err("the program has unresolved libraries".into());
    }

    let tools = util::third_party().join("appimage");
    let tool = tools.join("appimagetool-x86_64.AppImage");
    let runtime = tools.join("runtime-x86_64");
    util::fetch_verified(APPIMAGETOOL.0, APPIMAGETOOL.1, &tool)?;
    util::fetch_verified(RUNTIME.0, RUNTIME.1, &runtime)?;
    util::make_executable(&tool)?;
    let dist_dir = util::dist_dir();
    fs::create_dir_all(&dist_dir)?;
    let out = dist_dir.join(appimage_name()?);
    let _ = fs::remove_file(&out);
    let mut pack = Command::new(&tool);
    pack.env("ARCH", "x86_64")
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .env("NO_APPSTREAM", "1")
        .arg("--no-appstream")
        .arg("--runtime-file")
        .arg(&runtime)
        .arg(&appdir)
        .arg(&out);
    util::run(&mut pack)?;
    util::make_executable(&out)?;

    let version = util::output(
        Command::new(&out)
            .env("APPIMAGE_EXTRACT_AND_RUN", "1")
            .arg("--version"),
    )?;
    eprintln!("{}", version.trim());
    if !version.contains(&util::version()?) {
        return Err("the AppImage does not start".into());
    }
    // a headless GPU is not always available where packages are built
    match dist::self_test(&out) {
        Ok(()) => {}
        Err(e) => eprintln!("note: self-test skipped here ({e})"),
    }
    eprintln!("AppImage: {} ({})", out.display(), util::size_text(&out));
    Ok(())
}
