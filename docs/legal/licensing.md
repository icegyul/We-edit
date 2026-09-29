# Licensing plan

## Default

Repository content is intended to be licensed under `GPL-3.0-or-later`. The full license text is stored in the root `LICENSE` file.

## Possible LGPL boundaries

An independently reusable library may use `LGPL-3.0-or-later` only when all of the following are documented before its first implementation commit:

1. It has a stable public boundary and is useful outside the editor.
2. It can be built and tested without GPL-only packages.
3. Every dependency is compatible with LGPL distribution.
4. File headers, package metadata, generated artifacts, and notices agree on the license.
5. Contributors explicitly accept the split.

Likely candidates are a neutral timeline model, timecode library, or `.prproj` preservation library. The application, editor orchestration, UI, and integrated engine remain GPL by default.

## Dependency review

Before the architecture gate closes, record each direct and bundled native dependency, version policy, license/SPDX expression, dynamic/static linkage, codec/patent exposure, redistributable binaries, and platform-specific obligations. FFmpeg configuration is distribution-sensitive; OpenFX, OpenColorIO, VST3, ASIO, codec SDKs, GPU SDKs, fonts, and model/data files each require explicit review.

No patent grant, trademark permission, or third-party codec right is implied by GPL or LGPL.
