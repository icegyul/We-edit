# Rust-first architecture map

**Status:** draft; implemented alpha architecture, not validated as Premiere-compatible behavior.<br>
**Applies after:** every row in `compatibility-validation.md` is validated.

## 1. Decision drivers

The editor must preserve project information it does not understand, maintain frame/sample-accurate deterministic edits, keep media and plugin failures outside the document model, share GPU resources without unnecessary readbacks, and remain responsive with long projects. Compatibility behavior belongs in specifications and tests, not in UI widgets.

Rust is the default implementation language. Native C/C++ is limited to versioned FFI adapters for established ecosystems that do not expose a suitable Rust ABI. Unsafe code is isolated in those adapter crates and never accepted in the canonical model.

## 2. Planned dependency direction

```text
op-ui ────────────────┐
op-cli / automation ─┼──> op-application ──> op-core
                      │          │              ^
                      │          ├──> op-project┤
                      │          ├──> op-media──┤
                      │          ├──> op-render─┤
                      │          ├──> op-audio──┤
                      │          └──> op-plugins┘
                      └─────────────────────────┘

Native adapters: FFmpeg C ABI, OpenColorIO C/C++ shim, OpenFX C ABI,
optional platform audio/codec SDKs. They may not depend on op-ui.
```

`op-core` is pure, deterministic, serializable Rust with no filesystem, network, codec, GPU, audio-device, or GUI dependency. All time-changing operations are transactions that either satisfy invariants or fail without mutation.

## 3. Planned packages

| Boundary | Responsibility | Forbidden dependencies |
|---|---|---|
| `op-core` | Project entities, time, commands, selection-independent edit operations, undo events, render/audio plans | GUI, FFmpeg, GPU, filesystem |
| `op-project` | Native format, `.prproj` preservation graph, OTIO/FCP XML adapters, migrations, validation | GUI, decoder, renderer |
| `op-timeline` | Editing policy/state machine: targets, sync locks, links, snapping, trim transactions | GUI toolkit and pixels |
| `op-media` | Probe, demux, decode, proxies, thumbnails, waveforms, caches, hardware-device negotiation | Document mutation |
| `op-render` | Frame graph, compositing, effects, color pipeline, scopes, readback/export surfaces | UI behavior, project parsing |
| `op-audio` | Sample graph, resampling, routing, automation, meters, device ring buffers | GUI, project XML |
| `op-plugins` | OpenFX host, Wasm scripting, capability broker, crash/time/memory isolation | Direct project mutation outside transactions |
| `op-application` | Services, scheduling, commands, autosave, preferences, collaboration boundaries | Toolkit-specific widget types in core APIs |
| `op-ui` | Docking, panels, input routing, accessibility, visualization | Codec/project parsing logic |
| `xtask` | Reproducible developer, packaging, evidence and conformance tasks | Runtime application dependency |

Package names and license boundaries remain provisional until GATE-ARCH-001 and GATE-LIC-001 pass.

## 4. GPU and frame pipeline

`wgpu` is the leading abstraction candidate because its native backends cover Vulkan, Direct3D 12, Metal, and OpenGL. Selection is conditional on a benchmark proving the required external-memory/video interop and scheduling behavior on supported GPUs.

Planned frame stages:

1. Build an immutable render plan for sequence time `t` from a project snapshot.
2. Request decoded source frames by media time and declared color metadata.
3. Import or upload planes to the selected GPU device.
4. Convert to a scene-linear working space with explicit range, matrix, transfer and primaries.
5. Evaluate standard effects in stack order.
6. Apply fixed Motion/Vector Motion, masks and Opacity in the specified order.
7. Composite tracks bottom-to-top with premultiplied alpha.
8. Apply display/view transform for monitoring or output transform for export.
9. Keep the monitor result on the same GPU; read back only where an encoder/plugin boundary requires it.

“32-bit float processing” is defined as FP32 shader arithmetic for effect math and an explicitly selected intermediate texture format. It does not mean that every source or every surface must be RGBA32F. The specification must identify which passes may use RGBA16F, quantify error against RGBA32F, and require no clipping outside nominal display range until output mapping.

CUDA is not a primary renderer API. Vendor APIs may be used for decode/encode or specialized compute through optional adapters after interop and licensing tests. A portable Vulkan/D3D12/Metal path remains mandatory.

## 5. Media boundary

FFmpeg is the leading demux/decode/encode dependency, accessed through a narrow adapter that owns all raw pointers and translates to project-neutral media descriptors. Runtime capability discovery replaces compile-time assumptions. The adapter records library versions and hardware device chosen in diagnostic output.

Decode workers are keyed by source/stream and feed bounded caches. Requests carry generation IDs so stale work can be cancelled after a seek. Cache budgets are measured in bytes, not frame counts. Proxies and originals share an immutable media identity with separately validated time mapping.

## 6. Audio boundary

The engine processes planar `f32` internally unless a parity test requires `f64` accumulation for a stage. It supports mono, stereo, 5.1 and adaptive channel layouts without silently downmixing. Every conversion requires an explicit channel map.

The audio device callback only consumes a lock-free bounded ring buffer and updates atomics; it never allocates, locks, decodes, parses, logs, or calls plugins. Playback position is derived from samples accepted by the device plus measured latency. Offline export uses the same graph without the device buffer.

Miniaudio, CPAL and direct platform backends remain candidates. ASIO distribution and VST3 hosting require separate license and ABI review.

## 7. UI strategy

The preferred result is an all-Rust UI. `egui` plus a docking layer is a candidate because it supports multiple native viewports and detachable dock surfaces, but selection requires a proof of concept that passes:

- multi-monitor mixed-DPI movement and persistence;
- tabs, nested splits, floating windows, redocking, maximization and workspace restore;
- keyboard focus/context routing, IME, screen-reader accessibility and high-contrast themes;
- stable text metrics and large-timeline virtualization;
- sharing the renderer device/texture without full-frame CPU copies;
- 60 Hz interaction under representative project load.

If no Rust toolkit passes, Qt 6 may be used behind a small C ABI adapter while core, application logic, renderer and panels remain Rust. Qt's RHI/private API stability and dual-license obligations must be accepted explicitly. The toolkit decision is not made by aesthetic preference alone.

## 8. Plugin and scripting isolation

OpenFX is hosted through its C ABI in a dedicated adapter. Untrusted binary plugins cannot be made memory-safe by Rust; optional out-of-process hosting is therefore the default target for third-party plugins, with shared GPU/CPU surfaces where available.

Automation uses a versioned, capability-based WebAssembly host written in Rust. Scripts receive handles and transactions rather than pointers. Filesystem, network, clock and process access are denied unless declared and granted. QuickJS may be an optional compatibility layer behind FFI, but it is not the canonical API.

## 9. Scheduling and failure domains

| Domain | Failure policy |
|---|---|
| UI/application thread | Never waits on decode, render, plugin or disk I/O; retains last valid preview |
| Project worker | Atomic load/save with validation and recovery copy; parser limits depth/size/counts |
| Decode workers | Cancel stale seeks; quarantine corrupt sources; surface structured diagnostics |
| GPU scheduler | Device-loss recovery rebuilds transient resources from immutable plans |
| Audio real-time thread | No blocking/allocating; underrun is counted and reported |
| Plugin host | Time, memory and capability budgets; crash disables instance, not editor |
| Export worker | Deterministic snapshot; resumable job metadata; no mutation from live edits |

## 10. Performance budgets to validate

- 4K UHD 59.94 fps playback frame deadline, including decode and composite, on a published baseline machine.
- Audio callback deadline with at least 2x measured headroom at configured block size.
- Timeline pan/zoom/trim input-to-present latency below one refresh interval at 60 Hz for 10,000 visible items.
- Project open/save memory and time linear in serialized data, with hard limits for hostile input.
- Zero full-frame CPU readback on the monitor path.
- Bounded cache and history memory with observable eviction policy.

Exact numbers and hardware tiers must be frozen by the architecture validation corpus before implementation.

## 11. Primary references

- [wgpu documentation](https://wgpu.rs/doc/wgpu/)
- [FFmpeg documentation](https://ffmpeg.org/documentation.html)
- [OpenFX 1.5 reference](https://openfx.readthedocs.io/en/main/Reference/ofxCoreAPI.html)
- [OpenColorIO documentation](https://opencolorio.readthedocs.io/)
- [OpenTimelineIO documentation](https://opentimelineio.readthedocs.io/en/latest/)
- [Qt Rendering Hardware Interface](https://doc.qt.io/qt-6/qrhi.html), retained only as a fallback comparison
