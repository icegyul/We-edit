# Roadmap

**Updated:** 2026-10-02 (version 0.7.0)<br>
**Companion documents:** [implementation-status.md](implementation-status.md) (technical status and
validation gaps), [CHANGELOG.md](../CHANGELOG.md) (what each release changed),
[drift-comparison.md](drift-comparison.md) (lessons from another open-source editor).

This file is the user-facing plan: what an editor can already do, and what comes next, in order of
priority. Items move from "Next" to "Done" in the release that ships them.

## Done

### Editing

- Multitrack timeline: insert, overwrite, lift, extract, razor, ripple, roll, slip, slide, rate
  stretch, nesting, snapping, linked selection, sync and track locks, markers, keyframes.
- Premiere-style panels and workspaces: Project, Source, Program, Timeline, Effect Controls,
  Effects, History, Audio Mixer, Meters, Markers, Info, Lumetri, Scopes, Graphics, Tools.
- Copy and paste images from the browser, other programs or the file manager (Ctrl+V) (0.4).
- Reversed clips (Speed/Duration > Reverse Speed), also read from Premiere projects (0.5.1).

### Media and performance

- FFmpeg decoding and encoding; export to H.264, HEVC, ProRes 422 HQ, ProRes 4444, DNxHR HQ and
  PNG sequences in QuickTime, with pause, live preview and a chosen output frame rate.
- Hardware decoding (Direct3D 11, VideoToolbox, VAAPI) in Automatic, Always or Never mode; the
  automatic mode moves a file to the graphics card when the processor falls behind (0.5).
- Hardware encoders (NVENC, AMF, Quick Sync, Media Foundation, VideoToolbox) with an automatic
  retry on the software encoder when one fails (0.5.1).
- Decoder read-ahead with in-order gap filling; exports of camera footage 20 times faster (0.5).
- Backwards reading in blocks: reversed clips and reverse playback no longer seek once per
  frame (0.6).
- **Proxies** (0.6): Create Proxies / Remove Proxies in the Project panel and the Clip menu,
  Toggle Proxies in the Program Monitor and View menu. Proxies are 540p H.264 with a keyframe every
  half second, keep the original's frame numbering and color, and are never used for export.
- Playback and paused resolution (Full, 1/2, 1/4, 1/8) and choice of graphics API.
- Performance profiles (0.7): Ultra Performance, Performance, Balanced, Quality and Maximum
  Quality, recommended from the processor, memory and graphics card at first start; Software Only
  rendering and automatic fallback to the processor's rasterizer, so the program runs on
  computers without a graphics card; effect shaders compiled in idle time; custom settings
  changed one by one.
- **Link Media** (0.7): missing files located by hand (with the others in the same folder) or
  found by searching the computer by name, duration, size and audio streams.

### Captions (0.6)

- Graphics > Captions > Transcribe and Create Captions: local speech recognition (Whisper Tiny,
  Base or Small through candle, downloaded once on request) with language detection and word
  times; captions on their own track, one clip per caption.
- Twelve animated styles driven by the word times: Classic, Boxed, Karaoke, Highlight Box, Pop,
  One Word, Typewriter, Bounce In, Neon, Creator, Fade In, Underline. Everything is editable in
  Effect Controls; Apply Caption Style to All restyles the sequence.
- SRT and WebVTT import and export; New Caption for typed captions.
- Captions panel (0.7): every caption with its time and editable text, and style controls for all
  captions or the selected ones; spoken language and English translation choices; transcription
  about a third faster and language detection over the most voiced parts.

### Effects

- 73 video effects, 42 transitions and 27 audio effects, including Lumetri Color, keyers, blurs,
  distortions, light and film looks, and (0.6) Halftone, Duotone, Oil Paint, Pencil Sketch, Neon
  Edges, Halation, Lens Flare, Ripple, Zoom Pulse, CRT Screen, the Hexagons, Shatter, Ink,
  Pixelate and Kaleidoscope transitions, and Tremolo, Auto-Pan, Phaser, Bitcrusher, Distortion,
  Noise Gate, Pitch Shifter and Telephone and Radio audio effects.
- Drag-and-drop animation presets; foreign effects in imported projects replaced by the closest
  equivalent.

### Projects and platforms

- Native `.opproj`, OTIO, FCP XML and EDL interchange; partial read-only `.prproj` import.
- Windows ZIP, Linux AppImage and macOS disk images (Apple silicon and Intel), installable and
  updatable from [OpenHub](https://github.com/xXKuroiKenshiXx/OpenHub).
- Spanish and English interface, `.kys` keyboard shortcut import, crash recovery, log viewer.

## Next

Ordered by how much they improve everyday editing. Items marked (Drift) are features the Drift
editor shows are valuable for the same audience; see [drift-comparison.md](drift-comparison.md).

### 1. Smoother playback on modest hardware

- **Zero-copy hardware frames:** hand Direct3D 11 / VAAPI / VideoToolbox surfaces to the renderer
  without the round trip through system memory (Drift does this for D3D11 and VAAPI). Needs
  wgpu-hal interop per backend.
- **Render cache for heavy sections** (Premiere's "Render In to Out"): render effect-heavy ranges
  once to an intermediate file and play that.
- **Automatic proxies on import** as a preference (with a size choice: 540p, 720p, 1080p).
- **Effect thumbnails** in the Effects panel rendered on the current frame (Drift).

### 2. Titles, captions and graphics

- **Caption editor:** split and merge captions and edit their times in the Captions panel
  (0.7 lists them with editable text and styles them all at once).
- **Exact word timing:** align each word to the audio (Whisper cross-attention or forced
  alignment) instead of spreading a timed phrase over its words; faster transcription on the
  GPU (candle with CUDA or Metal).
- **Caption style presets** with saved custom looks, emoji and keyword highlighting.
- **Text animators:** per-character, per-word and per-line presets (fade, rise, pop, typewriter,
  karaoke highlight) (Drift has about 40).
- **Stickers and emoji** as graphics, and Lottie/SVG animated graphics (Drift).

### 3. Effects

- **Temporal effects** that look at neighbouring frames: echo/motion trail, frame blending,
  optical-flow slow motion.
- **Look templates:** one-click stacks of effects, transitions and audio that follow the music's
  beats (needs beat detection) (Drift).
- **Effect packages:** a data format (JSON parameters plus WGSL passes) so new effects and
  transitions can be added without rebuilding the program, with an add-on browser (Drift ships a
  similar JSON + GLSL format and a signed add-on market).
- More transitions: page curl, morph cut, film roll, light rays.
- Masks with tracking (Premiere's mask tracking) and planar tracking.

### 4. Audio

- Loudness normalization to a target (LUFS) and a true-peak meter.
- Auto-ducking of music under speech.
- Voice clean-up: noise reduction, de-esser, de-reverb; a local AI denoiser as an option (Drift
  uses DeepFilterNet).
- VST3 plug-in hosting (needs a separate license decision).

### 5. Assisted editing (local, optional models)

- Scene detection on import to split long recordings.
- Beat detection for cutting to music.
- Subject cut-out (segmentation) and background removal; depth-based effects (Drift uses SAM 2,
  RVM and Video Depth Anything).
- Auto-reframe for vertical formats.

### 6. Compatibility and platform

- `.prproj` writing once lossless round trips are demonstrated with original fixtures.
- OpenFX plug-in hosting and a WebAssembly scripting API.
- macOS notarization and signed Windows builds; Flatpak.
- Accessibility, multi-monitor docking, HiDPI edge cases.

## Not planned

- Accounts, subscriptions, watermarks or telemetry.
- Copying another product's code, assets or exact effect output (see the clean-room policy in
  [legal/clean-room.md](legal/clean-room.md)).
