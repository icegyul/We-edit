# Changelog

OpenPremier follows semantic versioning. User-visible changes are grouped by release.

## 0.3.0 - 2026-09-30

### Added

- Sixteen new video effects of our own: Glow, Radiant Glow (a wide, soft multi-octave glow), RGB Split, Camera Shake, Digital Glitch, Film Grain, Vignette, Cinematic Bars, Lens Distortion, Radial Blur, Motion Tile, Light Leaks, Old Film, VHS Tape, Strobe Light and Kaleidoscope.
- Twelve new video transitions: Zoom In, Zoom Out, Spin, Stretch, Smooth Slide, Blur Dissolve, Luma Fade, Flash, Light Leak, Film Burn, Glitch and Chromatic Split. The motion transitions mirror the image at its edges, so zooms and spins never show empty borders.
- Animation presets in the Effects panel (Presets): slow zooms, punch and pop entrances and exits, fades, slides from each side, spins, blur in/out, impact shake, handheld camera, glitch burst and ready-made looks. They are dragged onto a clip or applied with a double-click like any effect, and their keyframes adapt to the clip length.
- Effect Controls has a divider between the parameters and the keyframe timeline that can be dragged and is remembered, plus a button to show or hide the timeline.

### Changed

- Importing Premiere Pro projects (`.prproj`) and XML now recognizes effects and transitions it does not implement under their own identity and uses our closest equivalent (for example popular glow, blur, shake, glitch and zoom plug-ins), instead of leaving them unrendered. The import report counts these replacements and lists each one.
- `.prproj` import reads Opacity, maps transition items to transitions at the clip edges they cover, and reads Fast Blur settings into Gaussian Blur.
- XML import maps filters by name with their parameter values and keyframes, including Basic Motion scale and rotation and Opacity fades.

### Fixed

- Long effect and parameter names in Effect Controls are shortened (full name on hover) instead of running over the values and buttons, in effect headers and group titles too.

## 0.2.0 - 2026-09-30

### Added

- Exports can be paused and resumed; paused time is not counted in the elapsed and remaining estimates.
- While exporting, the Program monitor shows the frames being rendered with the export progress, and the bottom right corner of the window shows the progress with Pause and Cancel (details such as frames, elapsed and remaining time, speed and encoder on hover). When the export ends, an Open Folder button appears there.
- The output frame rate can be chosen in Export Settings (Ctrl+M); the sequence rate stays the default.
- Crash recovery: shortly after each change the project is written to a recovery file in the background, and an emergency copy plus a crash report are written if the program fails. The recovered project is offered at the next start.
- Program log with an in-app viewer (Help > Show Log...) offering level filter, search, copy and clear. The log file can be switched on or off and its detail level chosen in Preferences; the previous session's log is kept and crash reports are always written.
- Full color picker for color parameters and mattes: saturation and brightness square, hue and opacity strips, RGB fields and hexadecimal code.
- Closing the program during an export asks whether to stop it or keep exporting.

### Changed

- Interface Scale in Preferences is applied with an Apply button instead of resizing the whole interface while the slider moves.
- Projects open on a background thread with a progress indicator; autosave and recovery files are written in the background.
- Panel tabs have gray panel icons, more spacing and a flat look with an underline on the active tab.
- The Tools panel starts as a compact strip without a tab bar. Workspace layouts saved by 0.1.0 are replaced once by the new default layout.
- Dialogs, pop-ups and undocked panels open centered on the program window.
- The panel tab context menu and more status messages follow the interface language.
- Effect Controls and the monitor transport controls adapt to narrow panels: long names are shortened (full name on hover) and less important buttons move to the settings menu instead of overlapping.
- Thinner OP logo.

### Fixed

- A failing panel, dialog, decoder, thumbnail or audio mix no longer closes the program; the affected part is restored or skipped and the problem is logged.
- Graphics validation errors are logged instead of ending the program.
- An export to a folder that cannot be written, for example one protected by Windows Controlled folder access, reports a clear error instead of "No such file or directory".
- Minimizing the window no longer squeezes the panels; their sizes are kept for when it is restored.

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
