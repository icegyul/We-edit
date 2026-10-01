//! `cargo xtask <task>`: developer, packaging and release tasks.

mod deps;
mod dist;
mod linux;
mod macos;
mod release;
mod util;

use std::process::Command;

use util::Result;

const HELP: &str = "\
cargo xtask <task>

  deps              download FFmpeg 8.1 (and libclang on Windows) into third_party/
  build [--release] build OpenPremier
  run [args]        build and start OpenPremier (release profile)
  test              run every test
  lint              rustfmt check and clippy with warnings as errors
  cargo <args>      run cargo with the native dependencies configured
  dist              release build and packages in dist/:
                      Windows: OpenPremier-<version>-windows-x64.zip
                      Linux:   OpenPremier-<version>-x86_64.AppImage
  appimage          the Linux AppImage (on Windows, through the WSL build distro)
  release <version> check, tag and publish a GitHub release with the packages (asks first)
";

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn configured_cargo(args: &[&str]) -> Result<Command> {
    let mut cmd = cargo();
    cmd.current_dir(util::root()).args(args);
    deps::configure(&mut cmd)?;
    Ok(cmd)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = args.first().map(String::as_str).unwrap_or("help");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    let result = match task {
        "deps" => deps::ensure_ffmpeg().and_then(|_| {
            let mut probe = cargo();
            deps::configure(&mut probe)
        }),
        "build" => {
            let release = rest.iter().any(|a| a == "--release");
            let mut a = vec!["build", "--package", util::BIN];
            if release {
                a.push("--release");
            }
            configured_cargo(&a).and_then(|mut c| util::run(&mut c))
        }
        "run" => configured_cargo(&["run", "--release", "--package", util::BIN, "--"])
            .and_then(|mut c| util::run(c.args(&rest))),
        "test" => {
            configured_cargo(&["test", "--workspace"]).and_then(|mut c| util::run(c.args(&rest)))
        }
        "lint" => util::run(
            cargo()
                .current_dir(util::root())
                .args(["fmt", "--all", "--check"]),
        )
        .and_then(|_| {
            configured_cargo(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ])
            .and_then(|mut c| util::run(&mut c))
        }),
        "cargo" => {
            let refs: Vec<&str> = rest.iter().map(String::as_str).collect();
            configured_cargo(&refs).and_then(|mut c| util::run(&mut c))
        }
        "dist" => dist::dist(&rest),
        "appimage" => linux::appimage_task(),
        "linux-inside" => linux::inside(&rest),
        "release" => release::release(&rest),
        _ => {
            print!("{HELP}");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
