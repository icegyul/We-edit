# Drift: architecture and feature review

**Reviewed:** 2026-10-02, [CutWire-Studios/Drift](https://github.com/CutWire-Studios/Drift) at
commit `809bbaf` (last release 0.6.0)<br>
**License:** GPL-3.0, like OpenPremier. Nothing below was copied; this is a description of
approaches and features, used to plan our own work ([roadmap.md](roadmap.md)).

Drift is a free desktop editor aimed at short-form creators (Reels, Shorts, game clips, school
projects). It is written in C++ with Qt 6 and QML, uses FFmpeg for media and ships for Linux
(Flathub, AppImage), Windows (Microsoft Store, installer, portable ZIP), macOS (Homebrew, disk
image) and Android.

## How Drift renders

| Area | Drift | OpenPremier |
|---|---|---|
| GPU API | OpenGL through Qt (`QOpenGLContext` on an offscreen surface, one GL thread, framebuffer pool) | wgpu: Direct3D 12, Vulkan, Metal or OpenGL, chosen in Preferences |
| Compositor | One compositor (`GpuCompositor`) for preview and export, so the preview matches the file | Same principle: one renderer for the monitors and export |
| Effects | Data-driven packages: `effect.json` (parameters and a list of passes with intermediate buffers at chosen scales) plus GLSL fragment shaders per pass | Effects compiled into the program: catalog entries in Rust, WGSL passes |
| Transitions | Same package format (`transition.json` + `main.frag`) | Rust dispatch + WGSL |
| Text and vector graphics | Skia (Ganesh) inside the same GL context; Lottie and SVG animations | Our own text rasterizer (fontdb + ab_glyph) and shape drawing |
| Preview display | The compositor's GL texture is shown by a `QQuickItem` without copying (shared context) | The rendered texture is drawn by egui through wgpu without copying |
| Hardware decode | D3D11VA, VAAPI, CUDA, VideoToolbox and Android MediaCodec, with a vendor check so decoding stays on the GPU that draws | D3D11VA/DXVA2, VideoToolbox, VAAPI; automatic switch when the processor falls behind |
| Zero-copy decode | D3D11 surfaces imported into GL with `WGL_NV_DX_interop2` (planes stay YUV, Drift's own conversion shader); VAAPI through EGL on proven drivers | Not yet: hardware frames are copied to memory, then converted on the GPU (NV12/P010) |
| Proxies | Low-resolution H.264 copies (short GOP, no B-frames, source timestamps kept) | Since 0.6: 540p H.264, keyframe every 0.5 s, no B-frames, original frame numbering |
| Reversed clips | Pre-rendered reversed copies (`ReverseProxyCache`) read forwards | Since 0.6: backwards reading in decoded blocks (one seek per block) |
| Temporal effects | Frame history blending in the compositor (Time Echo, Motion Trail) | Not yet |
| Hybrid laptops | Writes the Windows per-application GPU preference (integrated or discrete) | wgpu asks for the high-performance adapter |

The main ideas worth adopting were the proxy layout (done), avoiding per-frame seeks backwards
(done differently), zero-copy hardware frames and a data-driven effect package format (both on the
roadmap).

## What Drift offers

### Effects and transitions (packages in the repository)

- **63 video effects:** adjustments (brightness, contrast, gamma, hue, saturation, temperature),
  stylize (bloom, blur, grayscale, pixelate, ripple, sepia, sharpen, VHS, vignette), glitches
  (block, digital, scanline, RGB split), looks (cinematic grade, duotone, halation, halftone comic,
  oil paint, pencil sketch, edge neon, film burn, light leak, lens flare, star filter, Super 8, VHS
  CRT, bokeh), motion (beat shake, zoom pulse, shockwave, spin blur, motion trail, time echo, droste
  zoom, wave warp, water ripple, kaleidoscope), chroma key, and AI-based depth effects (fog,
  occlusion, depth of field, relighting) and face effects (big eyes, slim, swap, swirl, 3D masks,
  props).
- **28 transitions:** crossfade and dips, wipes, push, zoom, radial zoom blur, honeycomb, Voronoi
  shatter, triangle mosaic, ink bleed, liquid smudge, matrix rain, particle wind, plasma burn,
  pixelate, RGB displacement, kaleidoscope spin, lens-flare wipe, VHS and analog interference.
- **23 look templates:** multi-effect stacks timed to music (Beat Drop, Glitch Cut, Neon Night,
  Retro Tape, ...).
- **About 40 text animations:** per-character and per-word fades, pops, bounces, karaoke styles.
- **30 audio effects:** voice changers (chipmunk, deep, robot, alien, wobble), telephone,
  megaphone, walkie-talkie, underwater, muffled, bitcrush, 8-bit, tape, vinyl, chorus, flanger,
  phaser, tremolo, auto-pan, echo, widen, compressor, limiter, gate, de-esser, EQ, leveler.

### Other features

- Captions generated from speech with a local Whisper model, speaker diarization and voice
  activity detection; a caption editor.
- Subject cut-out with a click (SAM 2), background matting (RVM), depth estimation, face landmarks
  and object detection, all through ONNX Runtime with optional model downloads.
- Beat detection, scene detection, auto-reframe, music ducking under speech, loudness
  normalization with a true-peak meter, DeepFilterNet noise reduction.
- Stickers and emoji, Lottie and SVG clips, 3D model clips.
- Transform layers (group several tracks without nesting), composites, adjustment layers.
- A signed add-on market for effect packages and an MCP server so AI agents can edit projects.
- Android build.

## Where OpenPremier differs

- Premiere-like organization, panels, shortcuts (`.kys` import) and project interchange
  (`.prproj`, FCP XML, OTIO, EDL); Drift has its own simpler, creator-oriented interface.
- Professional delivery formats (ProRes 422 HQ/4444, DNxHR) and broadcast tools (Lumetri Color,
  scopes, track mattes, keyers).
- A modern GPU API (wgpu) instead of OpenGL, which Apple deprecates.

## Gaps adopted into the roadmap

| Gap | Status |
|---|---|
| Proxies | Done in 0.6 |
| Fast reverse playback | Done in 0.6 (block decoding) |
| Duotone, halftone, oil paint, sketch, neon edges, halation, lens flare, ripple, zoom pulse, CRT | Done in 0.6 |
| Hexagon, shatter, ink, pixelate, kaleidoscope transitions | Done in 0.6 |
| Tremolo, auto-pan, phaser, bitcrusher, distortion, gate, pitch shifter, telephone/radio voices | Done in 0.6 |
| Zero-copy hardware frames | Next (performance) |
| Temporal effects (echo, trails) | Next (effects) |
| Effect thumbnails on the current frame | Next |
| Effect packages and add-ons | Next |
| Captions, speech to text, text animators, stickers | Next |
| Beat and scene detection, templates, ducking, loudness | Next |
| Segmentation, matting, depth, face effects | Later (optional local models) |
