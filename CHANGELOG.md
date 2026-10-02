# Changelog

OpenPremier follows semantic versioning. User-visible changes are grouped by release.

## 0.6.0 - 2026-10-02

### Added

- Proxies, as in Premiere Pro: Proxy > Create Proxies in the Project panel and the Clip menu makes 540p H.264 copies of the selected clips in the background (progress and Cancel in the status bar), and Toggle Proxies in the Program Monitor (also View > Enable Proxies) switches playback between proxies and originals. Proxies keep the original's frame numbering and colors, a keyframe every half second and no reordered frames, so scrubbing and reverse playback are fast; export always uses the original media. Remove Proxies detaches them and deletes the files OpenPremier made.
- Ten video effects: Halftone, Duotone, Oil Paint, Pencil Sketch, Neon Edges, Halation, Lens Flare, Ripple, Zoom Pulse and CRT Screen.
- Five video transitions: Hexagons, Shatter, Ink, Pixelate and Kaleidoscope.
- Eight audio effects: Tremolo, Auto-Pan, Phaser, Bitcrusher, Distortion, Noise Gate, Pitch Shifter (up to two octaves up or down) and Telephone and Radio (telephone, radio, megaphone, walkie-talkie and underwater voices).
- docs/roadmap.md lists what the editor does today and what comes next; docs/drift-comparison.md reviews the Drift editor's rendering and features.

### Changed

- Reversed clips and reverse playback read the source backwards in blocks of frames: each block costs one seek to a keyframe instead of one per frame (75 frames read backwards went from 76 seeks to under 20 in the test).
- The decoder follows the direction frames are actually requested in, so a reversed clip in a sequence that plays forward reads ahead the right way.

## 0.5.1 - 2026-10-01

### Fixed

- Exporting H.264 or HEVC on a Mac failed with "Invalid argument" from the VideoToolbox encoder. It now runs without B-frames (the likely cause) and may use its software path; if it still fails, the software encoder takes over (below).
- When a hardware encoder fails during an export on any platform, the export starts again with the software encoder instead of stopping.
- Export errors name the step that failed (for example the video encoder in use) instead of only FFmpeg's short message, and the log records which encoder an export uses.
- The default export folder is the user's videos folder on every system (Movies on macOS, the localized XDG folder on Linux) instead of the home folder when no folder is named "Videos"; pasted images default to the pictures folder in the same way.
- The empty Project panel shows Cmd+I instead of Ctrl+I on macOS.
- On Linux systems without the VAAPI libraries (libva), hardware decoding could close the program; it is now only tried when those libraries are installed, and the processor decodes otherwise.
- Premiere Pro project import reads reversed clips (Reverse Speed).

### Changed

- A new end-to-end test exports a clip with sound and effects in every format (H.264, HEVC, ProRes 422 HQ, ProRes 4444, DNxHR HQ and PNG) and reads each file back. Continuous integration runs it, and the effect and transition pixel tests, on a real graphics adapter on Linux and macOS instead of skipping them.

## 0.5.0 - 2026-09-30

### Added

- macOS package: a disk image (`.dmg`) with `OpenPremier.app` for Apple silicon (arm64) and Intel (x86_64) Macs, with FFmpeg bundled inside the app. Keyboard shortcuts are shown with Cmd and Option, and Show in Folder opens Finder.
- Performance Mode in Preferences (Performance): turns off interface animations and smooth scrolling and redraws the interface less often while waiting for background work. Editing and rendering are not affected.
- Hardware decoding in Preferences: Automatic (the default) decodes on the processor and moves a video to the graphics card's decoder when the processor cannot keep up in real time; Always and Never force one or the other. Hardware decoding uses Direct3D 11/DXVA2 on Windows, VideoToolbox on macOS and VAAPI on Linux, and falls back to the processor when the card cannot decode a file.
- Graphics API choice in Preferences: Automatic, Direct3D 12 (Windows), Metal (macOS), Vulkan or OpenGL. If the chosen API cannot start, the next one is tried.

### Changed

- Much faster playback and export of camera files: the decoder keeps a longer read-ahead and decodes every frame in order instead of skipping ahead, which made it seek back to the previous keyframe again and again. Exporting a 4K HEVC 10-bit clip to 1080p went from about 4 to about 80 frames per second on the test machine.
- Export encodes on its own thread while the next frame renders, and reads the rendered planes back from the graphics card in one batch with reused buffers.
- 8-bit and 10-bit semi-planar frames (NV12/P010, what hardware decoders produce) are converted to RGB on the graphics card.
- During playback the interface is redrawn when the next video frame is due (at most 60 times a second) instead of at the screen's refresh rate, which lowers processor and graphics card use on high refresh rate screens.



### Added

- Paste media from the system clipboard with Ctrl+V: images copied in a web browser or an image editor, media files copied in the file manager, and links or paths to images (links are downloaded in the background). The image is saved to a folder, imported into the project and, while a sequence is edited, placed at the playhead on the first free video track above the material there and selected.
- The paste dialog shows a preview, the file name and the destination folder, with an "Always save here" option; the folder and the option can be changed later in Preferences (Pasted Images).
- A randomized test applies thousands of timeline edits (overwrite, insert, razor, delete, ripple delete, lift, extract, trims, moves, transitions and track locks) and checks that the project stays valid and that locked tracks never change.

### Changed

- The log viewer opens as a regular pop-up of a fixed size instead of growing to the full height of the window, and has a Close button.
- Slimmer OP logo: the bowl of the P is drawn lighter than the stem.

### Fixed

- A ripple trim of a clip whose linked partner had been moved out of sync could cut through the partner and make it overlap its neighbor; each linked clip now gets the added or removed time at its own edge.

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
