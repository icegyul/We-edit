# Premiere Pro 2024 static-data audit

**Audit date:** 2026-09-28<br>
**Sources:** workstation-local folder labelled `Adobe Premiere Portable 2024` plus two user-created project fixtures on the same workstation<br>
**Method:** clean-room inspection of declarative interoperability data only<br>
**Status:** useful E2/E3 schema evidence; not a validated behavioral oracle

## Scope and exclusions

This pass inventories filenames and parses project, preset, keyboard, workspace, metadata and extension-manifest data. It does not disassemble, decompile, debug or inspect implementation code in `.exe`, `.dll`, `.aex` or `.prm` files. JavaScript/JSX implementation bodies, Adobe artwork, LUTs, media and localized resources are not copied into this repository. Only derived facts, hashes, counts and interoperability fields are retained.

The source installation contains 15,166 files totalling 4,297,867,158 bytes. Relevant declarative families include 427 JSON, 56 XML, 41 `.kys`, 21 `.prfpset`, 3 factory `.prproj`, 10 `.ksvlayout`, 365 `.sqpreset`, 1,028 `.epr` and 16 workspace-layout XML files. Presence does not prove runtime availability or UI visibility.

## `.prproj` findings

Five gzip/XML projects were inspected: three factory templates, one recent user autosave, and one independently created empty project. All use `PremiereData Version="3"`; observed `Project` serialization versions are 41 and 42.

| Fixture | XML bytes | All `ObjectID` attributes | Direct root definitions | Inline attributes | Colliding numeric values | Sequences |
|---|---:|---:|---:|---:|---:|---:|
| Broadcaster template | 697,789 | 1,115 | 943 | 172 | 171 | 9 |
| Social Media template | 5,855,107 | 5,446 | 3,299 | 2,147 | 2,146 | 10 |
| Standard template | 255,727 | 483 | 140 | 343 | 141 | 2 |
| Recent user autosave | 110,078 | 209 | 92 | 117 | 93 | 1 |
| Empty user project | 71,567 | 155 | 38 | 117 | 39 | 0 |

Critical correction: numeric identity is not file-global. Direct definitions under `PremiereData` are unique within the root graph, while `Project/Node/Properties/ProjectViewState.List` contains an inline serialization island that reuses those numeric values. Identity discovery must traverse the complete tree **and retain its serialization scope**; a flat integer-keyed dictionary would misresolve objects. XML containment, scope and reference edges are separate concepts.

Across the five fixtures, 171 `TransitionItems` descriptors expose only `Index` and `MediaType`; no fixture contains a populated transition object. Transition serialization therefore remains unknown. Four projects with sequences expose 256 `AutomationMode` values, all `1`, and 22 `AutomationSafeFlags` values, all `0`; these are opaque observed codes, not proven enum labels. The social template exposes graphic/text/shape components but does not broaden transition or time-remap evidence. See [compatibility-research.md](compatibility-research.md).

## Effect-preset findings

The 21 `.prfpset` files are plain UTF-8 XML, not gzip. Ten locales each contain Factory and Lumetri preset files; `MaskPresets.prfpset` is shared.

- Factory Presets: 58 filter presets/components, 512 keyframe records and parameter-control codes 1-7. Explicit keyframe records use code 2; start records use codes 0 and 2.
- Lumetri Presets: 325 presets, all with `MatchName` `AE.ADBE Lumetri`; 27,300 video parameters and 4,550 arbitrary parameters.
- Mask Presets: two fixtures, `Ellipse Preset` and `Rect Preset`, both with `MatchName` `AE.ADBE AEMask` and 11 ordered parameters.

Observed mask parameter order is: Track; three unnamed hidden fields; group end; Mask Path; Mask Feather; Mask Opacity; Mask Expansion; unnamed checkbox; unnamed bounded float. The last two meanings are not assigned without a controlled UI experiment.

The two mask-path values decode to a 124-byte little-endian payload. Observed layout:

```text
offset  size  observation
0       4     ASCII bytes `kcin`
4       4     fixture flag: ellipse=1, rectangle=0
8       4     point count=4
12      112   four 28-byte point records
```

Each point record contains a 32-bit flag followed by six float32 values interpreted from the two fixtures as anchor `(x,y)`, first handle `(x,y)` and second handle `(x,y)`. This interpretation is provisional until validated with variable-point, open/closed and custom Bezier masks. It is sufficient to require bounded binary parsing and opaque preservation.

## Workspace and panel findings

Sixteen workspace XML files use `prop.map version="4"`. Their layout state contains monitor information, top-level frames, binary splits, orientation, placement, frame proxies, tab IDs, active-tab index, visibility, floating-frame reanimation and geometry. This directly supports a persisted forest of native windows containing split nodes and tab groups.

The workspaces are All Panels, Assembly, Audio, Captions, Color, Editing, Effects, Essentials, Learning, Libraries, Metalogging, Production, Review, Social, Text Based Editing and Vertical. Details and panel-frequency counts are in [premiere_workspace_inventory.md](evidence/generated/premiere_workspace_inventory.md).

Panel state embeds both native IDs and extension-backed IDs. Observed UXP panel identities include `com.adobe.dva.text -> mainPanel` and `com.adobe.dva.uxp.importer -> mainPanel`; CEP-backed IDs are also stored. The serializer must treat these as versioned external panel identities and preserve unavailable panels as recoverable placeholders.

## Keyboard-map and physical-layout findings

The installation contains 41 `.kys` maps. Eleven localized Premiere Defaults maps use schema version 5 on Windows; the English map contains 370 context-command pairs, while localized maps contain 308-332 and change raw bindings by locale. Ten `.ksvlayout` files describe six-row physical keyboards as multi-root XML fragments. Character and numpad codes use distinct high-bit domains, while modifiers are explicit in `.kys` records.

The filename/declared-locale sets are inconsistent: `nb.ksvlayout` declares `de_DE`; `en` and `zh` defaults have no matching fragment; `sv` has a fragment but no matching default map directory. These are preserved source facts, not normalized assumptions. Full counts are in [premiere_keyboard_locales.md](evidence/generated/premiere_keyboard_locales.md).

## Sequence and editing-mode presets

All 365 `.sqpreset` files are XML object graphs with root `PremiereData Version="3"`. They carry frame size, pixel aspect, field enum, frame duration, audio sample duration, channel type, adaptive channel count, editing-mode GUID, preview codec and initial track data. Newer version-8 VR presets store a direct JSON array of richer audio-track records, while version-5 presets wrap track records under `SerializerWrappedObject`.

Using 254,016,000,000 ticks/second, observed frame rates are 23.976024, 24, 25, 29.97003, 30, 48, 50, 59.94006 and 60 fps. Observed audio rates are 32 kHz and 48 kHz. Most presets contain four initial audio tracks; VR/Ambisonics presets contain three richer track records, and eight presets contain eight tracks. Fifty-seven older presets omit `AudioTracks`.

`Adobe Editing Modes.xml` declares 52 editing modes and cross-platform GUID/name mappings, supported frame-rate rational sets, frame rectangles and pixel-aspect rational sets. IDs are opaque identities; localized display names are not identities.

One sequence preset contains a nonnumeric frame-duration value, `84SQ667200`. It is recorded by path and hash in the evidence manifest. A parser must diagnose it and preserve the original token; silently coercing it is forbidden.

## Export presets

All 1,028 `.epr` files have XML signatures. 1,027 parse as `PremiereData Version="3"`; one contains an unescaped `<` inside preset-comment text and is not well-formed XML.

The valid corpus contains 18 exporter class IDs, 34 file-type IDs, 487 distinct parameter identifiers and parameter type codes 1-12 (code 12 is sparse). Export parameters form an `ObjectID`/`ObjectRef` graph of containers and typed values with metadata such as optional, hidden, disabled, slider, password, multiline, ordinal and constrained-list state.

Numeric exporter/file-type identities often have a useful big-endian FourCC rendering (for example `HEVC`, `H264`, `MXF `, `MooV`, `WAVE`, `AIFF`, `DPX `, `PNG `, `TIFF`, `JPEG`), but the numeric value is canonical and the ASCII rendering is diagnostic only.

Observed basic parameter IDs cover width, height, tick-duration FPS, field type, PAR, codec, profile/level, deep-color switch, bitrate controls, sample rate, channel count, audio codec, captions and post-encode destinations. Unknown identifiers and values must round-trip losslessly.

## Metadata schemas

Two XMP definition files declare:

- project namespace `http://ns.adobe.com/premierePrivateProjectMetaData/1.0/`: 63 properties, split into 33 external and 30 internal; types are 45 text, 13 private timecode, 4 boolean and 1 private label;
- file namespace `http://ns.adobe.com/premierePrivateFileProperties/1.0/`: 7 internal properties; 6 text and 1 URI.

The private timecode type declares integer qualifiers for minimum, maximum, offset, time display and frame rate. Project-panel columns therefore need stable qualified keys and typed values rather than a fixed set of UI strings.

## Extension manifests

Ten Premiere-relevant UXP manifest files expose multiple schema generations: legacy `uiEntryPoints` and manifest-v5 `entrypoints`/`entryPoints`. Observed built-in plugin IDs include text, importer, export settings/queue, quick export, progress, preset manager, cloud-media storage and creative-copilot surfaces. Eight CEP manifests expose legacy extension IDs and host-version ranges.

Only manifests were inspected. Their JavaScript/HTML implementation was not analyzed. OpenPremier must define its own extension ABI and UI model rather than emulating private Adobe runtime internals.

## Architectural consequences

1. Keep compatibility locked: this pass improves schema coverage but does not validate editing behavior, rendering math or write compatibility.
2. Build any future project/preset reader as a bounded, two-pass, loss-preserving graph parser; Rust remains the preferred implementation language after the gate opens.
3. Represent ticks and rational geometry exactly; never normalize unknown numeric text on import.
4. Version audio-track JSON adapters independently because at least two shapes coexist.
5. Model workspace persistence as windows -> split tree -> tab groups -> panel instances, with opaque versioned panel state.
6. Treat malformed factory data as part of the threat/compatibility model and emit source-located diagnostics.
7. Repeat this audit against licensed official Premiere Pro 24.0 and final 24.x installations before implementation or compatibility claims.
