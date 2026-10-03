# Changelog

OpenPremier follows semantic versioning. User-visible changes are grouped by release.

## 0.8.0 - 2026-10-03

### Added

- AI assistants can edit with OpenPremier through the Model Context Protocol: `OpenPremier --mcp` serves Claude Code, Codex, Cursor and other MCP clients with 28 tools (project and media, sequences, adding, moving, cutting and speeding up clips, effects and their parameters, transitions, titles, automatic captions, markers, looking at a rendered frame, exporting, any menu command, undo). With Preferences > AI Assistants on, the tools edit the open project live (only from this computer, with a random key; off by default), each change one undo step; otherwise they work on a project of their own. See docs/assistants.md.
- Every common picture format imports and pastes: besides PNG, JPEG, TIFF, WebP, BMP, GIF, TGA, EXR, DPX and PSD, now SVG, AVIF, HEIC/HEIF, JPEG XL, JPEG 2000, ICO, DDS, QOI, PCX, PNM, SGI, XBM/XPM, Sun Raster and Radiance HDR. The import dialog has an Images filter.
- Pasting (Ctrl+V) with the pointer over the timeline puts the pasted media there: at the time under the pointer, on the track under it when it is free; video files bring their sound to free audio tracks. Without an open sequence, the pasted media starts one.

### Changed

- Exports are about three and a half times faster when the encoder runs on the processor (ProRes, DNxHR, PNG, and H.264/HEVC without a hardware encoder): FFmpeg encoded on a single core, now it uses all of them. A one-second 1080p clip with 100 effects exports at 40 frames per second instead of 11 on the test computer, and ten stacked layers of ten effects at 21 instead of 6.
- Exports read each frame back from the graphics card while the next one renders, instead of waiting for it.
- A heavy-load benchmark (`cargo test --release --test stress_render -- --ignored`) measures one clip with 100 effects and ten stacked layers of ten effects, on the graphics card or on the processor's rasterizer like a computer without one; see docs/performance.md.
- The Hand tool shows a hand (open, and closed while dragging) instead of the system's four-arrow cursor.
- Transcription choices read Caption Language: Same as spoken (transcribe) or Translate to English. Tests showed that choosing a spoken language other than the real one does not translate.
- The interface code is split into smaller modules (dialogs and the main window each in several files).

### Fixed

- TGA and ICO pictures did not decode ("the image has no picture").
- A one-frame GIF became a one-frame clip instead of a still.

## 0.7.0 - 2026-10-02

### Added

- Performance profiles in Preferences > Performance: a five-step slider from Ultra Performance to Maximum Quality replaces the Performance Mode checkbox. A profile sets interface animations and smooth scrolling, how often the interface redraws while playing and while background work runs, playback and paused resolution, the decoded frame cache (never more than a quarter of the memory), how far playback decodes ahead, how long playback waits for a late frame, and timeline thumbnails and waveforms. Ultra Performance is meant for old computers and ones without a graphics card; Maximum Quality redraws at the display's refresh rate with richer animations. Exports are never affected.
- Automatic hardware check: the processor, its cores and threads, the memory and the graphics card (dedicated, integrated or none) choose a recommended profile. On the first start a Performance Setup window shows what was found and offers Use Recommended Settings or Choose Manually; Preferences shows the recommendation and Use Recommended at any time.
- Software Only (processor) in Preferences > Graphics API, like Premiere Pro's Mercury Playback Engine Software Only: the interface and preview are drawn by the processor's rasterizer (WARP on Windows, llvmpipe on Linux), for computers without a usable graphics card or with broken drivers.
- The Razor tool shows translucent scissors at the pointer over the timeline, so the clip underneath stays visible, with the red cut line between the blades.
- Custom performance settings: Preferences > Performance > Custom Settings changes each thing a profile sets (interface animations, smooth scrolling, redraws during playback, frames decoded ahead, timeline thumbnails, audio waveforms and preparing effects in the background) one by one; changing one, or a preview resolution or the frame cache, switches to Custom, and the slider goes back to a profile.
- Link Media (File > Link Media, and offered when a project opens with files that moved or were renamed), like Premiere Pro's: each missing file can be located by hand, and the other missing files in the same folder are linked with it; Search Automatically looks in the project folder, the user's media folders and every drive by file name, and when several files share a name it takes the one whose duration, size and audio streams match. Linking is one undo step and keeps each clip's interpretation and proxy; Offline All keeps working without them.
- Captions panel (Window > Captions; in the Editing and Graphics workspaces): every caption of the sequence with its start time and editable text, and one set of style controls (caption style, font, font style, size, text and highlight colors, stroke, box, vertical position, maximum width, animation strength, all caps and shadow) that changes all captions at once, or only the selected ones. Clicking a caption's time selects it and moves the playhead to it.
- Transcription choices: the spoken language (offered in the program's language) and Captions In: the language spoken or an English translation.
- Audio Gain (G) has Premiere Pro's four choices: Set Gain to, Adjust Gain by, Normalize Max Peak to (one gain for the selection, so its loudest peak reaches the level) and Normalize All Peaks to (each clip its own gain), and shows the selection's peak amplitude.
- Edit > Remove Attributes: puts Motion, Opacity, Volume, Channel Volume and Panner back to their defaults and removes effects from the selected clips, choosing which.

### Changed

- Faster transcription: about a third less time for the same result. The speech model's text decoder keeps what it has already read instead of reading the whole sentence again for every word, the audio of each 30-second window is analyzed once however many tries it takes, and silence is skipped without analysis.
- Better language detection: the spoken language is judged on up to three 30-second stretches with the most sound instead of the first 30 seconds, which were often music or silence and made Spanish speech come out as English. The status bar says which language was heard.
- Lighter: about a fifth less memory at rest (155 MB against 199 MB in 0.6.0 on the test computer) and half the processor time at start; the program file is 8 % smaller (whole-program optimization).
- A cleaner look: the interface uses the system's own font (Segoe UI on Windows, San Francisco on macOS, the desktop's sans-serif on Linux) with semibold panel names and window titles, controls have a faint rim that brightens under the pointer, windows and menus have rounder corners and softer shadows, and sliders fill up to their value. Preferences > Interface Font goes back to the classic font.

- The program opens on computers without a graphics card: the window takes the best adapter that can draw it (dedicated, then integrated, then the software rasterizer).
- Faster start: only the shaders every preview needs compile at start; effect shaders compile two per idle frame from the Balanced profile up, and on first use in the lighter profiles (with software rendering, compiling them all at start took several seconds of every core).
- The Preferences window scrolls when it is taller than the screen.

### Fixed

- Paste Attributes with Effects copied a graphic's text or caption onto the target clips as if it were an effect.
- The underline of the active tab of a narrow panel (such as Audio Meters) ran into the neighbouring panel.
- The Essential Graphics placeholder text wraps in a narrow panel instead of being cut off.

## 0.6.0 - 2026-10-02

### Added

- Animated captions, in the style of short-form video apps. Graphics > Captions > Transcribe and Create Captions turns the speech in the sequence into captions automatically: OpenAI's Whisper speech model (Tiny, Base or Small, downloaded once on request) runs on this computer, detects the language or uses the chosen one, and times every word. Captions go on their own Captions track, one clip per caption, with the chosen number of words per caption.
- Twelve caption styles, animated word by word as each word is said: Classic, Boxed, Karaoke (words fill with color), Highlight Box, Pop, One Word, Typewriter, Bounce In, Neon, Creator (bold capitals), Fade In and Underline. Style, font, size, colors, outline, box, shadow, position and animation strength are in Effect Controls; Apply Caption Style to All copies one caption's look to every caption in the sequence.
- Captions can also be imported from SRT and WebVTT files (word times are spread over each caption), exported to SRT or WebVTT, or added by hand (New Caption).

- Proxies, as in Premiere Pro: Proxy > Create Proxies in the Project panel and the Clip menu makes 540p H.264 copies of the selected clips in the background (progress and Cancel in the status bar), and Toggle Proxies in the Program Monitor (also View > Enable Proxies) switches playback between proxies and originals. Proxies keep the original's frame numbering and colors, a keyframe every half second and no reordered frames, so scrubbing and reverse playback are fast; export always uses the original media. Remove Proxies detaches them and deletes the files OpenPremier made.
- Ten video effects: Halftone, Duotone, Oil Paint, Pencil Sketch, Neon Edges, Halation, Lens Flare, Ripple, Zoom Pulse and CRT Screen.
- Five video transitions: Hexagons, Shatter, Ink, Pixelate and Kaleidoscope.
- Eight audio effects: Tremolo, Auto-Pan, Phaser, Bitcrusher, Distortion, Noise Gate, Pitch Shifter (up to two octaves up or down) and Telephone and Radio (telephone, radio, megaphone, walkie-talkie and underwater voices).
- docs/roadmap.md lists what the editor does today and what comes next; docs/drift-comparison.md reviews the Drift editor's rendering and features.

### Fixed

- WAV files that do not name their channel layout (written by many recorders and speech synthesizers) played silent: preparing their audio failed with "Input changed" on every frame.

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
