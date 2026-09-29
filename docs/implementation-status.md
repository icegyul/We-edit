# Implementation status

**Snapshot:** 2026-09-28<br>
**Application version:** 0.1.0 technical alpha<br>
**Primary language/UI:** Rust 2024 / egui 0.36<br>
**Compatibility validation:** in progress; full parity has not been established

This file records repository reality. It does not replace the normative Premiere-compatibility
specifications or upgrade static observations into validated behavior.

## Verified build baseline

The following commands completed on the Windows development host using the checked-in `xtask`
flow:

| Check | Result |
|---|---|
| `cargo xtask lint` | rustfmt and Clippy passed with warnings denied |
| `cargo xtask test` | 115 tests passed; no failures or ignored tests |
| Windows release package | final-layout `--self-test` passed |
| Linux AppImage | WSL `--self-test` passed; startup passed in Ubuntu 22.04 and Fedora 42 containers |

The self-test exercises packaged FFmpeg discovery, required encoders, GPU adapter creation, and a
known compositor pixel. It is a startup/integration check, not a performance or parity benchmark.

## Implemented subsystem map

| Subsystem | Present behavior | Important limit |
|---|---|---|
| Core model | Exact rational time, items/bins, tracks, clips, transitions, links, parameters, validation, undo/redo | Foreign concepts not represented by the canonical model remain incomplete |
| Timeline | Selection, insert/overwrite, razor, lift/extract, ripple delete/trim, roll, slip, slide, rate stretch, nesting, snapping, track controls | Premiere cross-product behavior is not fully measured |
| Project I/O | Atomic native `.opproj` read/write; OTIO/FCP XML/EDL interchange; gzip/XML `.prproj` import | `.prproj` is read-only and mappings are intentionally partial |
| Media | FFmpeg probe, software decode, audio conform/peaks, thumbnails, proxy/export paths; optional hardware encoders | Hardware decode/interoperability and codec coverage are not yet validated as a matrix |
| Rendering | wgpu compositor, color conversion, blend modes, implemented effects/transitions, graphics, scopes | No differential pixel-parity corpus; OpenColorIO integration remains pending |
| Audio | Sample-accurate floating-point mixer, meters, device output, EQ/dynamics/delay/reverb foundations | VST3 hosting, exhaustive routing, 5.1/adaptive parity, and DSP oracle tests remain pending |
| UI | Fifteen dockable panels, saved workspace layouts, monitors, timeline, effect controls, dialogs, English/Spanish catalogs | Multi-monitor, accessibility, IME, HiDPI edge cases, and automated interaction coverage remain incomplete |
| Packaging | Portable Windows ZIP, Linux AppImage, checksums, licenses, final-layout self-test | No macOS package is currently available |

## Highest-priority pending work

1. Complete controlled E4/E5 fixtures for `.prproj` identity scopes, masks, retiming, automation,
   transitions, malformed input, and unknown-node preservation. Do not add `.prproj` writing until
   lossless round-trip evidence exists.
2. Build an automated timeline behavior matrix covering linked selection, track targeting, sync
   locks, locked tracks, focus contexts, trim boundaries, and snapping at multiple DPI/zoom values.
3. Create pixel/audio oracle corpora across bit depths, YUV/RGB formats, color spaces, alpha,
   effects, transitions, audio layouts, and supported GPUs; publish tolerances and performance
   budgets.
4. Implement and conformance-test the missing extensibility layer: isolated OpenFX hosting and a
   capability-based WebAssembly scripting API. VST3 requires a separate ABI/license decision.
5. Finish production platform work: hardware decode, zero-copy decoder/GPU interop, cache pressure
   tests, crash recovery, accessibility, multi-monitor docking, and macOS dependency packaging.
6. Run the clean-room observations against a licensed official Premiere Pro 2024 installation and
   obtain legal/dependency-license review before any public compatibility release.

## Recently fixed interaction contract

Timeline Ctrl/Cmd+wheel zoom now consumes raw wheel events. Each detent applies exactly one 1.20×
step (or its inverse), preserves the time under the pointer, and stops when the event stream stops.
It does not use egui's smooth/inertial zoom accumulator. See `TL-VIEW-001` through `TL-VIEW-004`
in [timeline-behavior.md](timeline-behavior.md).
