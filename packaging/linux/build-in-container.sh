#!/bin/bash
# Builds the AppImage inside quay.io/pypa/manylinux_2_28_x86_64 (glibc 2.28), so the result runs
# on desktop distributions released since 2018. /work is a persistent volume holding the sources
# (/work/src), the Rust toolchain, the cargo cache and the build directory.
set -euo pipefail

dnf -y -q install alsa-lib-devel clang-devel llvm-devel pkgconf-pkg-config xz file >/dev/null

export CARGO_HOME=/work/cargo
export RUSTUP_HOME=/work/rustup
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_TARGET_DIR=/work/target
toolchain="${RUST_VERSION:-stable}"
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$toolchain" --no-modify-path
fi
rustup toolchain install "$toolchain" --profile minimal >/dev/null
rustup default "$toolchain" >/dev/null

cd /work/src
cargo xtask linux-inside
