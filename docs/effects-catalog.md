# Premiere Pro 2024 effects, transitions, generators, and audio catalog

**Status:** installation inventory is derived; visible UI catalog and processing math are not fully validated.<br>
**Target:** Premiere Pro 2024 compatibility, not the materially changed 2026 catalog.<br>
**Complete raw name inventory:** [premiere_plugins_inventory.md](evidence/generated/premiere_plugins_inventory.md).

## 1. What the inventory proves

The analyzed folder contains:

- 151 Premiere video effect/transition plugin names exposed through an AE-compatible plugin family;
- 291 shared After Effects effect-library names, many of which may be internal or unavailable in the Premiere UI;
- 37 importer names, 23 exporter names, 10 camera-RAW source-setting modules and 40 other plugin modules;
- factory preset evidence for 58 filter presets/components and selected `MatchName` parameter layouts;
- 325 Lumetri presets with a 98-entry component layout plus layouts for Motion, Text, Shape, Mosaic, Twirl and selected legacy effects;
- two mask fixtures exposing ordered parameters and a provisional bounded binary path layout.

File presence does **not** prove UI visibility, licensing, platform availability, GPU acceleration or valid use in Premiere. The UI-visible 2024 catalog must be captured from a clean official installation and reconciled with this inventory before GATE-FX-001 passes.

## 2. Catalog record schema

Every in-scope component requires one record with:

| Field | Requirement |
|---|---|
| Stable identity | Source `MatchName`/class ID plus OpenPremier UUID; display name is not identity |
| Kind/scope | Video effect, video transition, audio effect, audio transition, generator, fixed effect; clip/source/track/master |
| Version/availability | Premiere build, OS, optional component and legacy/obsolete visibility |
| Parameters | Stable ID/order, type, unit, min/max/default, enum labels, animation and spatial behavior |
| Processing contract | Input/output pixel or sample domain, alpha, color space, premultiplication, channel layout, temporal neighborhood |
| Ordering | Interaction with fixed effects, masks, adjustment layers, nesting and source effects |
| GPU/CPU behavior | Observable parity result; implementation backend is not copied from Adobe |
| Serialization | `.prproj` ownership, opaque payload, keyframe fields and unknown preservation |
| Oracle | Fixture hash, exact action, output hash/statistics and tolerance |
| Status | Inventory-only, parameter-mapped, behavior-validated, pixel/sample-validated |

An effect is not “implemented” merely because it has the same name or visually plausible output.

## 3. Fixed/intrinsic components

### Video

| Component | Observed identity/parameters | Remaining validation |
|---|---|---|
| Motion | `AE.ADBE Motion`; Position, Scale, Scale Width, Uniform Scale, Rotation, Anchor Point in factory preset | Per-version extra parameters, transform order, pixel aspect, field handling, resampling, bounds and precision |
| Opacity | `AE.ADBE Opacity`; opacity, blend mode, masks; separate `AE.ADBE AEMask` fixtures observed | Exact project serialization, 27 blend equations/color domain, mask math, effect order |
| Time Remapping | Speed/time curve | Object graph, interpolation, reverse/hold, frame sampling and audio behavior |
| Vector Motion / graphics transforms | Observed in graphic components | Coordinate spaces, parenting and responsive design behavior |

### Audio

Volume, Channel Volume and Panner are mandatory fixed components. Clip versus track versus master ownership, dB-to-linear mapping, pan law, channel linking and automation interpolation require E4/E5 measurements.

### Mask preset evidence

`MaskPresets.prfpset` contains ellipse and rectangle fixtures. Both expose 11 ordered parameters: tracking; three unnamed hidden fields; group end; path; feather; opacity; expansion; an unnamed checkbox; and an unnamed bounded float. Path, Feather, Opacity and Expansion have direct labels. Adobe's archived UI documentation makes the checkbox a strong **Inverted candidate**, but the serialization mapping remains unproven until a controlled UI diff; the final bounded float remains unknown.

Mask Path uses control type 22 and a 124-byte base64 payload. The two fixtures support a provisional little-endian header plus four records containing point flags, anchors and two handles. This does not validate arbitrary point counts, open paths, multiple masks, inversion, tracking data, coordinate transforms, feather falloff or interpolation. The detailed observation is in [premiere-2024-static-data-audit.md](premiere-2024-static-data-audit.md).

## 4. Required video effects

Priority 0 is the minimum professional interchange/render set; it does not redefine the full 2024 inventory.

| Effect | Identity/evidence | Mandatory validation |
|---|---|---|
| Lumetri Color | `AE.ADBE Lumetri`; 98-parameter factory layout | Every section, LUT embedding/reference, curves/blobs, color management, HDR and numerical parity |
| Gaussian Blur | `GaussianBlur` and legacy variants observed | Radius/sigma relation, repeat-edge, dimensions, alpha/color behavior, GPU parity |
| Ultra Key | `UltraKey` observed | Key color, matte generation/cleanup/spill/output modes and color domain |
| Sharpen / Unsharp Mask | both observed | Kernel, amount/radius/threshold, edge/alpha behavior |
| Crop | `Crop` observed | Percent/pixel units, zoom, edge feather and coordinate order |
| Transform | `Transform` observed | Matrix order, shutter-angle motion blur, sampling and anchor semantics |
| Corner Pin | `Corner_Pin` observed | Normalized/project coordinates, interpolation and out-of-bounds sampling |
| Warp Stabilizer | `Stabilizer` observed | Analysis metadata, unsupported/offline behavior; algorithmic parity scope decision |
| Track Matte Key | `TrackMatteKey` observed | Matte source selection, alpha/luma, reverse, track movement and missing source |
| Basic 3D | `Basic_3D` observed | Perspective math, shading/specular, sampling and clipping |
| Noise | multiple variants observed | Distribution, monochrome/color, seeding and temporal determinism |
| Posterize Time | `Posterize_Time` observed | Frame selection at fractional rates and nested/time-remapped sequences |

The full installed 151-name set includes color correction, blur/sharpen, distort, generate, image control, keying, perspective, stylize, time, transform, VR effects and numerous legacy effects. It is normative as an **inventory input**, not yet as a support promise.

## 5. Video transitions observed/required

Core parity set:

- Cross Dissolve
- Dip to Black
- Dip to White
- Film/Additive/Non-Additive Dissolve where present in the 2024 UI
- Morph Cut
- Push, Slide, Split, Whip and Center Split
- Linear, Radial and Gradient Wipe
- Venetian Blinds, Inset and Block Dissolve
- VR Chroma Leaks, Gradient Wipe, Iris Wipe, Light Leaks, Light Rays, Mobius Zoom, Random Blocks and Spherical Blur where visible

The plugin inventory contains identifiers such as `CrossDissolve`, `DipToBlack`, `DipToWhite`, `MorphCut`, `NonAdditiveDissolve`, `Gradient_Wipe`, `Radial_Wipe`, `Venetian_Blinds`, `Split` and `Whip`. Category and display-name mapping must be verified in the 2024 UI.

Every transition is tested for two-sided and one-sided placement, alignment, odd/even frame duration, insufficient handles, repeated frames, reversed order, nested sequences, alpha and color-space behavior. Default duration and default transition are user preferences; Adobe's current public documentation identifies Cross Dissolve and Constant Power as defaults, but the target 2024 profile must be tested.

## 6. Generators and synthetic media

The installation exposes importer/generator-style names including Bars and Tone, Black Matte/Black Video, Color Matte, Transparent Matte, Universal Counting Leader/Leader, Frame Generator, captions/transcripts, Simple Text, shapes and graphic groups.

For each generator, validate:

- project item versus timeline item representation;
- finite/infinite source duration and default duration;
- sequence-size/rate inheritance;
- color-space and alpha tagging;
- editable parameters and localization;
- `.prproj` identity and relink behavior;
- render determinism and test-pattern numerical values.

Adobe templates and graphics assets are never copied. Equivalent generators use original implementation and original artwork.

## 7. Lumetri Color parameter surface

The observed 98-entry layout is committed in [premiere_effect_layouts.md](evidence/generated/premiere_effect_layouts.md). Named groups include:

- Basic Correction: input LUT, HDR white, white balance, exposure, contrast, highlights, shadows, whites, blacks and saturation;
- Creative: look/intensity, faded film, sharpen, vibrance, saturation and tint controls;
- RGB and hue/saturation curves;
- Color Wheels;
- HSL Secondary key/refine/correction;
- Vignette;
- arbitrary blobs and embedded LUT payloads.

Several entries have empty names or opaque binary types. The layout may vary with component version. No formula for Temperature, Tint, tonal ranges, curves, wheels, HSL keying or vignette is considered known from the parameter table alone.

## 8. Blend modes

Target inventory: Normal, Dissolve; Darken, Multiply, Color Burn, Linear Burn, Darker Color; Lighten, Screen, Color Dodge, Linear Dodge/Add, Lighter Color; Overlay, Soft Light, Hard Light, Vivid Light, Linear Light, Pin Light, Hard Mix; Difference, Exclusion, Subtract, Divide; Hue, Saturation, Color and Luminosity.

Each mode requires equations validated against linear/nonlinear working spaces, premultiplied/unpremultiplied alpha, values below zero/above one, opacity and 8/10/16/float sources. HSL-like modes require a defined color model; Photoshop-style formulas are not assumed automatically.

## 9. Audio effects and transitions

Target categories from Adobe's public audio library and local inventory:

- Amplitude/compression: Amplify, Channel Mixer/Volume, DeEsser, Dynamics, Dynamics Processing, Hard Limiter, Multiband Compressor, Single-band Compressor, Tube-modeled Compressor.
- Delay/echo: Analog Delay, Delay, Multitap Delay.
- Filter/EQ: Bandpass, Bass, FFT Filter, Graphic EQ 10/20/30 bands, Highpass, Lowpass, Notch, Parametric EQ, Scientific Filter, Treble.
- Modulation: Chorus/Flanger, Flanger, Phaser.
- Noise restoration: Automatic Click Remover, DeHummer, DeNoise, DeReverb.
- Reverb: Convolution Reverb, Studio Reverb, Surround Reverb.
- Special/stereo/time: Distortion, channel fill/swap/invert, Guitar Suite, Loudness Radar/Meter, Mastering, Vocal Enhancer, Stereo Expander, Pitch Shifter, Remix.
- Audio transitions: Constant Gain, Constant Power, Exponential Fade.

The local `dvaaudiofilters` data directory contains 84 WAV resources: binaural azimuth responses and convolution/surround-reverb impulse responses. Filenames support the existence of those resource families but do not reveal DSP equations, normalization, channel mapping or UI availability. `PrivateAudioFilterConfig.xml` declares two private VST3 descriptors only and is not a catalog of built-in effects.

Sequence presets statically demonstrate at least two serialized initial-track shapes: version-5 wrapped records and version-8 direct-array records with sends, panner assignments, track IDs and volume. Four projects with sequences also expose 256 `AutomationMode` fields with value `1` and 22 `AutomationSafeFlags` fields with value `0`. These observations do not map enum/bit meanings or validate live routing and automation behavior. The derived preset schema is recorded in [premiere_preset_schema_inventory.md](evidence/generated/premiere_preset_schema_inventory.md); the behavioral gap is specified in [compatibility-research.md](compatibility-research.md).

Mandatory audio oracle tests use impulses, silence, DC, logarithmic sweeps, stepped levels, inter-channel phase and multichannel channel-ID signals. Record latency, tail, state reset, sample-rate dependence, denormal behavior, peak/RMS/LUFS conventions and automation accuracy. “Noise Reduction” is a family/category, not one sufficiently specified algorithm.

## 10. Pixel/sample conformance

For deterministic effects, the default target is max absolute/relative error plus PSNR/SSIM and alpha-specific comparison. Non-deterministic or content-analysis effects require an explicit perceptual/statistical scope or are declared non-parity. Tests run on CPU oracle where possible and every supported GPU backend.

Inputs cover integer and float formats, RGB/YUV range/matrix/transfer/primaries, premultiplied/straight alpha, odd sizes, borders, NaN/Inf handling, interlacing, pixel aspect and temporal discontinuities. Output comparison is performed before display encoding when the oracle permits it.

## 11. Original looks, presets and import equivalents

OpenPremier also ships original effects that editors commonly reach for (glow, radiant glow,
RGB split, camera shake, digital glitch, film grain, vignette, cinematic bars, lens distortion,
radial blur, motion tile, light leaks, old film, VHS, strobe, kaleidoscope), motion and light
transitions (zoom, spin, stretch, smooth slide, blur dissolve, luma fade, flash, light leak, film
burn, glitch, chromatic split) and animation presets. Their names, parameters and processing are
our own; none claims to reproduce another product's output.

Added in 0.6.0, also original:

- video effects: Halftone (one ink or CMY screens), Duotone, Oil Paint (Kuwahara filter), Pencil
  Sketch (Sobel lines with cross hatching), Neon Edges, Halation (tinted highlight bloom), Lens
  Flare (source, streak, ghosts and ring), Ripple, Zoom Pulse and CRT Screen;
- video transitions: Hexagons, Shatter, Ink, Pixelate and Kaleidoscope;
- audio effects: Tremolo, Auto-Pan, Phaser, Bitcrusher, Distortion, Noise Gate, Pitch Shifter
  (two-head delay-line shifter with complementary sin^2 windows) and Telephone and Radio (band
  limiting, presence peak and speaker saturation for telephone, radio, megaphone, walkie-talkie
  and underwater sounds).

Captions (0.6.0) are a graphic component, Caption (`op.graphic.caption`): the caption text, the
time of each word (an internal parameter written by transcription and caption import), one of
twelve animated styles and the usual text appearance. The renderer lays the words out in lines
and draws each word with its own state at the current time (color, size, lift, visibility,
highlight box, underline), so the animation follows the speech.

Every video effect and transition is rendered by the GPU tests, which CI requires on Linux (software
Vulkan) and macOS (Metal);
the audio effects have unit tests for level, pitch (an octave up and down), gating, quantization
and band limits.

Importers (`.prproj` and FCP XML) resolve a foreign component in this order
(`op_core::catalog::resolve_foreign`):

1. an attested interchange identity (`match_names`);
2. the same display name, also with a version suffix (for example "Gaussian Blur 2");
3. an equivalent by purpose, recognized from keywords in the identity or display name (for
   example third-party glow, shake, glitch or zoom-transition plug-ins).

Equivalents start from their own defaults unless a parameter layout is attested (Fast Blur) or
the interchange format names the parameters (FCP XML). Every replacement is listed in the import
report; unresolved components stay opaque and disabled (DM-FX-003). `.prproj` transition items
are read with the same field names as clip items and the component identity they reference; this
is not yet validated against a fixture with populated transitions (PR-X-003).

## 12. Public references

- [Adobe: Video and audio transitions overview](https://helpx.adobe.com/premiere/desktop/add-video-effects/apply-video-transitions/transitions-overview.html)
- [Adobe: Audio effects library](https://helpx.adobe.com/premiere/desktop/add-audio-effects/apply-audio-effects/audio-effects-library.html)
- [Adobe: Transition effects](https://helpx.adobe.com/premiere/desktop/add-video-effects/effects-and-transitions-library/transition-effects.html)
- [Adobe: Immersive video effects and transitions](https://helpx.adobe.com/premiere/desktop/edit-projects/edit-vr-content/immerse-video-effects-and-transitions.html)
