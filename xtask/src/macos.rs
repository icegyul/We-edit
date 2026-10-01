//! macOS package: `dist/OpenPremier-<version>-macos-<arch>.dmg` with `OpenPremier.app`, the
//! FFmpeg libraries it links inside `Contents/Frameworks`, the licenses and the third-party
//! notices. Built on a Mac with Homebrew's FFmpeg (`brew install ffmpeg dylibbundler`). The app
//! is signed ad hoc (Apple silicon does not run unsigned code) and started with `--self-test`
//! from its final layout before it is packed.

use std::path::PathBuf;

use crate::util::{self, Result};

#[cfg(target_os = "macos")]
pub fn package() -> Result<PathBuf> {
    use std::fs;
    use std::process::Command;

    crate::dist::release_build()?;
    let version = util::version()?;
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    let name = format!("{}-{version}-macos-{arch}", util::APP_NAME);
    let stage = util::dist_dir().join(&name);
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    let app = stage.join(format!("{}.app", util::APP_NAME));
    let contents = app.join("Contents");
    for d in ["MacOS", "Resources", "Frameworks"] {
        fs::create_dir_all(contents.join(d))?;
    }
    let exe = contents.join("MacOS").join(util::APP_NAME);
    fs::copy(util::root().join("target/release").join(util::BIN), &exe)?;
    fs::write(contents.join("Info.plist"), info_plist(&version))?;

    // icon: an iconset from the generated PNG sizes
    let icons = util::root().join("assets/icons");
    let iconset = stage.join("AppIcon.iconset");
    fs::create_dir_all(&iconset)?;
    for (size, src) in [
        ("16x16", 16),
        ("16x16@2x", 32),
        ("32x32", 32),
        ("32x32@2x", 64),
        ("128x128", 128),
        ("128x128@2x", 256),
        ("256x256", 256),
        ("256x256@2x", 512),
        ("512x512", 512),
        ("512x512@2x", 1024),
    ] {
        fs::copy(
            icons.join(format!("openpremier-{src}.png")),
            iconset.join(format!("icon_{size}.png")),
        )?;
    }
    util::run(
        Command::new("iconutil")
            .args(["-c", "icns", "-o"])
            .arg(contents.join("Resources/AppIcon.icns"))
            .arg(&iconset),
    )?;
    fs::remove_dir_all(&iconset)?;

    // FFmpeg and everything it links, rewritten to load from the bundle
    util::run(
        Command::new("dylibbundler")
            .args(["-od", "-b", "-cd", "-ns", "-x"])
            .arg(&exe)
            .arg("-d")
            .arg(contents.join("Frameworks"))
            .args(["-p", "@executable_path/../Frameworks/"]),
    )?;
    util::run(
        Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&app),
    )?;
    crate::dist::write_docs(&stage, "macos")?;
    crate::dist::self_test(&exe)?;

    // the disk image shows the app next to a link to Applications
    std::os::unix::fs::symlink("/Applications", stage.join("Applications"))?;
    let dmg = util::dist_dir().join(format!("{name}.dmg"));
    if dmg.exists() {
        fs::remove_file(&dmg)?;
    }
    util::run(
        Command::new("hdiutil")
            .args(["create", "-volname", util::APP_NAME, "-srcfolder"])
            .arg(&stage)
            .args(["-ov", "-format", "UDZO"])
            .arg(&dmg),
    )?;
    Ok(dmg)
}

#[cfg(not(target_os = "macos"))]
pub fn package() -> Result<PathBuf> {
    Err("the macOS package is built on a Mac".into())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn info_plist(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundleExecutable</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>io.github.openpremier.OpenPremier</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.video</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>OpenPremier Project</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>CFBundleTypeExtensions</key><array><string>opproj</string></array>
    </dict>
  </array>
</dict>
</plist>
"#,
        name = util::APP_NAME
    )
}
