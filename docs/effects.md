# Effects, transitions and graphics

**Updated:** 2026-10-02 (version 0.7.0)

OpenPremier's components are original implementations: their names follow the vocabulary editors
know, but the parameters and processing are our own and no output is copied from another product.
Video effects and transitions run on the GPU (WGSL shaders through wgpu); audio effects run in
the floating-point mixer. Importers map components found in other applications' projects to the
closest one here and list every replacement in the import report.

The catalog lives in `crates/op-core/src/catalog.rs`; this page is generated from it.

## Video effects (73)

| Category | Components |
|---|---|
| Adjust | Channel Mixer, Extract, Levels, ProcAmp |
| Blur & Sharpen | Directional Blur, Gaussian Blur, Radial Blur, Sharpen, Unsharp Mask |
| Channel | Invert |
| Color Correction | Brightness & Contrast, Color Balance, Duotone, Leave Color, Lumetri Color, Tint |
| Distort | Corner Pin, Lens Distortion, Mirror, Offset, Ripple, Spherize, Transform, Twirl, Wave Warp, Zoom Pulse |
| Generate | Lens Flare, Light Leaks, Ramp |
| Image Control | Black & White, Color Replace, Gamma Correction |
| Keying | Color Key, Luma Key, Track Matte Key, Ultra Key |
| Noise & Grain | Film Grain, Noise |
| Perspective | Basic 3D, Drop Shadow |
| Stylize | CRT Screen, Digital Glitch, Emboss, Find Edges, Glow, Halation, Halftone, Kaleidoscope, Mosaic, Motion Tile, Neon Edges, Oil Paint, Old Film, Pencil Sketch, Posterize, RGB Split, Radiant Glow, Replicate, Solarize, Strobe Light, Threshold, VHS Tape, Vignette |
| Time | Posterize Time |
| Transform | Camera Shake, Cinematic Bars, Crop, Edge Feather, Horizontal Flip, Vertical Flip |
| Transition | Linear Wipe, Radial Wipe |
| Video | Timecode |

## Video transitions (41)

| Category | Components |
|---|---|
| Dissolve | Additive Dissolve, Blur Dissolve, Cross Dissolve, Dip to Black, Dip to White, Film Dissolve, Ink, Luma Fade, Non-Additive Dissolve |
| Iris | Iris Box, Iris Cross, Iris Diamond, Iris Round |
| Light | Film Burn, Flash, Light Leak |
| Slide | Center Split, Push, Slide, Smooth Slide, Split, Stretch, Whip |
| Spin | Kaleidoscope, Spin |
| Stylized | Chromatic Split, Glitch, Pixelate, Shatter |
| Wipe | Barn Doors, Checker Wipe, Clock Wipe, Hexagons, Inset, Radial Wipe, Random Blocks, Venetian Blinds, Wipe |
| Zoom | Cross Zoom, Zoom In, Zoom Out |

## Audio effects (27)

| Category | Components |
|---|---|
| Amplitude and Compression | Amplify, Hard Limiter, Noise Gate, Single-band Compressor |
| Delay and Echo | Delay |
| Filter and EQ | Bandpass, Bass, Highpass, Lowpass, Notch Filter, Parametric Equalizer, Simple Parametric EQ, Treble |
| Modulation | Auto-Pan, Chorus/Flanger, Phaser, Tremolo |
| Reverb | Studio Reverb |
| Special | Bitcrusher, Distortion, Invert, Pitch Shifter, Telephone and Radio |
| Stereo Imagery | Fill Left with Right, Fill Right with Left, Stereo Expander, Swap Channels |

## Graphics

Text, Shape and Caption components draw on graphics clips. Captions animate word by word with
twelve styles (Classic, Boxed, Karaoke, Highlight Box, Pop, One Word, Typewriter, Bounce In,
Neon, Creator, Fade In and Underline) driven by the time of each word.

## Testing

Every video effect and transition is rendered by the GPU tests, which CI requires on Linux
(software Vulkan) and macOS (Metal), and which also run on Direct3D 12 and OpenGL. Audio effects
have unit tests for level, pitch, gating, quantization and band limits.
