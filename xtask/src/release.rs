//! `cargo xtask release <version>`: tags the release and publishes the packages in dist/ on
//! GitHub with the notes from docs/releases/<version>.md. It shows what it will do and asks for
//! confirmation before anything leaves this machine.

use std::io::BufRead;
use std::process::Command;

use crate::util::{self, Result};

pub fn release(args: &[String]) -> Result {
    let wanted = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .ok_or("usage: cargo xtask release <version> [--yes]")?;
    let version = util::version()?;
    if wanted.trim_start_matches('v') != version {
        return Err(format!("the workspace version is {version}, not {wanted}").into());
    }
    let tag = format!("v{version}");
    let dist = util::dist_dir();
    let files = [
        dist.join(format!("{}-{version}-windows-x64.zip", util::APP_NAME)),
        dist.join(format!("{}-{version}-x86_64.AppImage", util::APP_NAME)),
        dist.join("SHA256SUMS.txt"),
    ];
    for f in &files {
        if !f.exists() {
            return Err(format!("{} is missing; run `cargo xtask dist` first", f.display()).into());
        }
    }
    let notes = util::root().join(format!("docs/releases/{version}.md"));
    if !notes.exists() {
        return Err(format!("{} is missing", notes.display()).into());
    }
    if !util::have("gh") {
        return Err("the GitHub CLI (gh) is required".into());
    }
    let dirty = util::output(
        Command::new("git")
            .current_dir(util::root())
            .args(["status", "--porcelain"]),
    )?;
    if !dirty.trim().is_empty() {
        return Err("the working tree has uncommitted changes".into());
    }
    eprintln!("Release {tag}:");
    for f in &files {
        eprintln!("  {} ({})", f.display(), util::size_text(f));
    }
    eprintln!("  notes: {}", notes.display());
    if !args.iter().any(|a| a == "--yes") {
        eprint!("Publish this release on GitHub? [y/N] ");
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            return Err("cancelled".into());
        }
    }
    let exists = util::output(
        Command::new("git")
            .current_dir(util::root())
            .args(["tag", "--list", &tag]),
    )?;
    if exists.trim().is_empty() {
        util::run(Command::new("git").current_dir(util::root()).args([
            "tag",
            "-a",
            &tag,
            "-m",
            &format!("OpenPremier {version}"),
        ]))?;
    }
    util::run(
        Command::new("git")
            .current_dir(util::root())
            .args(["push", "origin", &tag]),
    )?;
    let mut gh = Command::new("gh");
    gh.current_dir(util::root())
        .args([
            "release",
            "create",
            &tag,
            "--title",
            &format!("OpenPremier {version}"),
            "--notes-file",
        ])
        .arg(&notes);
    for f in &files {
        gh.arg(f);
    }
    util::run(&mut gh)
}
