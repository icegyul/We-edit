# Rust workspace

The application is implemented as a Rust workspace with dependency flow from reusable models and
editing policy toward application services and UI:

- `op-core`: canonical model, exact time, parameters, validation, and history;
- `op-timeline`: semantic editing operations and navigation policy;
- `op-project`: native project I/O and interchange import/export;
- `op-media`: FFmpeg probe, decode, conform, thumbnail, and encode paths;
- `op-render`: wgpu compositor, effects, transitions, graphics, and scopes;
- `op-audio`: mixer, DSP, meters, and device output;
- `op-application`: editor state, commands, playback, autosave, and export;
- `op-ui`: egui panels, docking, monitors, timeline, dialogs, and localization;
- `openpremier`: executable, diagnostics, portable mode, and packaged self-test.

Use the root `cargo xtask` commands so native FFmpeg and libclang paths are configured correctly.
All workspace crates are currently private implementation units and cannot be published to
crates.io accidentally.
