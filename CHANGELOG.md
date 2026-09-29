# Changelog

OpenPremier follows semantic versioning. User-visible changes are grouped by release.

## 0.1.0 - 2026-09-29

### Added

- Rust editor application with dockable panels, timeline tools, FFmpeg media I/O, GPU rendering, audio mixing and project interchange.
- Portable Windows x64 ZIP and Linux x86-64 AppImage packages.
- Spanish and English interface catalogs.
- Package self-test and SHA-256 checksums.
- Original violet-and-white OP identity.

### Changed

- Timeline Ctrl/Cmd+wheel zoom uses discrete, cursor-anchored steps without inertial continuation.
- Product name, executable, package names and application identifiers standardized as OpenPremier.

### Known limitations

- `.prproj` import is read-only and covers a subset of project structures.
- OpenFX, VST3 and WebAssembly scripting are not included in this release.
- A macOS package is not available yet.
