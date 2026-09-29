# CMake integration boundary

Cargo is the primary build graph. The root CMake file exposes convenience targets for release
builds and tests and remains the integration point for future audited C/C++ SDK discovery or small
ABI shims. It does not duplicate Rust dependency management or define a second application build.
