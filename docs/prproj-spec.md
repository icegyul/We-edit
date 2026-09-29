# `.prproj` interoperability specification

**Status:** partially observed; read/write implementation is prohibited.<br>
**Target scope:** Adobe Premiere Pro 2024 project files on Windows, exact builds still to be recorded.<br>
**Evidence:** EV-PR-001 through EV-PR-004 and generated class/layout indexes.

## 1. Critical schema correction

A 2024 `.prproj` does **not** serialize as a simple nested document with one `<Project>` containing direct `<Media>`, `<Sequence>`, `<Track>` and `<Filter>` children. The observed files serialize an identity/reference graph rooted under:

```xml
<PremiereData Version="3">
  <Project ObjectRef="1"/>
  <Project ObjectID="1" ClassID="..." Version="...">...</Project>
  <!-- more object definitions and references -->
</PremiereData>
```

Definitions are **not restricted to direct children** of `PremiereData`. The four inspected projects contain nested serialized subgraphs, including `ProjectViewState`, column lists and column objects embedded below project-property containers. A loader must index every `ObjectID` and `ObjectUID` across the complete XML tree before resolving references. Any design based on either a simplified semantic tree or a flat direct-child table is rejected because it cannot preserve identity, sharing, nesting, unknown objects or reference topology.

## 2. Container

| Requirement | Observation | Confidence |
|---|---|---|
| PR-CNT-001 | EV-PR-001..004 start with gzip magic `1F 8B` and decompress to XML | E3 |
| PR-CNT-002 | Decompressed XML is UTF-8 and begins with an XML declaration in observed files | E3 |
| PR-CNT-003 | Root is `PremiereData` with `Version="3"` | E3 |
| PR-CNT-004 | Three factory templates use gzip header OS byte `0x13` | E3 for observation only |
| PR-CNT-005 | Whether Premiere requires that nonstandard OS byte is unvalidated | Open |
| PR-CNT-006 | CRC32 and ISIZE must be checked before XML parsing; decompressed size is bounded | Design requirement |

No writer may claim compatibility until projects recompressed with controlled gzip header variants are opened, edited, saved and reopened by the targeted official builds. The observed header is evidence, not yet a writer rule.

## 3. Secure parse profile

- Reject encrypted, concatenated or trailing-data variants unless validated.
- Bound compressed bytes, decompressed bytes, object count, element depth, attribute length, text length, reference count and cumulative opaque payload.
- Disable DTDs, external entities, XInclude, stylesheet execution and network/file resolution.
- Treat every path, URL, JSON string and base64 blob as untrusted data.
- Parse integers with overflow checks; preserve unrecognized numeric text without coercion.
- Resolve references in a separate validation pass and report dangling, duplicate and type-incompatible references.
- Never load a plugin or media file merely because a project references it.

## 4. Object identity and references

Observed identity attributes:

| Attribute | Meaning in observed graph |
|---|---|
| `ObjectID` | Integer identity scoped to a serialization graph/island |
| `ObjectRef` | Reference to an `ObjectID` in the applicable serialization scope |
| `ObjectUID` | UUID-like identity used by multiple project entities |
| `ObjectURef` | Reference to an `ObjectUID` |
| `ClassID` | GUID identifying serialized class |
| `Version` | Per-class serialization version, not application version |

- **PR-GRAPH-001:** Definitions and references are indexed before semantic mapping.
- **PR-GRAPH-001A:** Identity discovery traverses the complete XML document, including definitions nested below another definition. Every definition/reference retains an explicit serialization-scope path; XML parentage, scope and reference edges are recorded separately.
- **PR-GRAPH-001B:** A bare numeric `ObjectID` is never used as a document-global key. Ambiguous or cross-scope resolution is an error until that scope's rules are validated.
- **PR-GRAPH-002:** Duplicate identity, dangling reference and wrong expected target type are diagnostics with source location.
- **PR-GRAPH-003:** Unknown class/tag combinations remain opaque but referenceable.
- **PR-GRAPH-004:** Element order and repeated entries are preserved; collections are not collapsed into maps unless uniqueness is proven.

The observed catalog contains 62 tag/class families in [premiere_prproj_classes.md](evidence/generated/premiere_prproj_classes.md). It is a sample catalog, not a closed enum.

Across five fixtures, complete-tree traversal found 7,408 `ObjectID` attributes and 286 `ObjectUID` attributes. Of the `ObjectID` attributes, 4,512 are direct definitions in the root graph and 2,896 occur in inline project-view-state data. Direct root IDs are unique per fixture, but 2,590 numeric values collide when each document is incorrectly flattened. The exact role of the `ProjectViewState.List` wrapper's own `ObjectID` remains open. Observed `Project` serialization versions are 41 and 42; a folder or marketing-version label must not be converted into a presumed project serialization version.

## 5. Observed project graph

```text
Project
  -> RootProjectItem
     -> ProjectItemContainer / Items / Item[ObjectURef]
        -> BinProjectItem -> nested ProjectItemContainer
        -> ClipProjectItem -> MasterClip[ObjectURef]

MasterClip
  -> Clips / Clip[ObjectRef]
     -> VideoClip | AudioClip
        -> Clip / Source[ObjectRef]
           -> VideoMediaSource | AudioMediaSource
              -> MediaSource / Media[ObjectURef]
                 -> Media

Sequence
  -> TrackGroups / TrackGroup[Index]
     -> First: media-kind GUID
     -> Second[ObjectRef]: VideoTrackGroup | AudioTrackGroup | DataTrackGroup
```

Observed media-kind GUIDs:

- video: `228cda18-3625-4d2d-951e-348879e4ed93`
- audio: `80b8e3d5-6dca-4195-aefb-cb5f407ab009`
- data/captions: `d8143ffe-eec4-4d2a-a909-d5f7bf094dc5`

### 5.1 Media

Observed fields include `ActualMediaFilePath`, `FilePath`, `Title`, stream references, duration, frame rate, frame rectangle, still flag, codec type, alpha type and implementation identifiers. Field presence and precedence are not validated. Paths must be normalized only for lookup; original spelling/URI form is preserved.

### 5.2 Sequences and tracks

`Sequence` objects reference video, audio and data track groups. Track objects expose IDs/indexes, lock, sync-lock, mute/output state, media type, clip-item collections and transition-item collections. The exact persistence of UI source-patching and track-target state is open.

Observed item path:

```text
VideoClipTrack / AudioClipTrack
  -> ClipTrack / ClipItems / TrackItems / TrackItem[ObjectRef]
     -> VideoClipTrackItem | AudioClipTrackItem
        -> TrackItem / Start, End
        -> SubClip[ObjectRef]
        -> ComponentOwner / Components[ObjectRef]
```

The five inspected projects contain 171 `TransitionItems` descriptors with observed fields `Index` and `MediaType`, but no populated transition object. Their class, match-name, alignment, handle and duration serialization remains unmapped. Adobe's post-target UXP contract exposes start/end, tick duration, single/two-sided state and alignment; those concepts constrain the canonical model but do not establish 2024 XML fields. Details are in [compatibility-research.md](compatibility-research.md).

### 5.3 Markers

Observed marker owners reference marker objects whose payload contains JSON-like DVA marker data, including name/comment/start/duration fields. JSON schema, marker types, colors, chapter/web links, GUID semantics and round-trip escaping remain to validate.

## 6. Time and geometry

- **PR-TIME-001:** Observed Premiere timebase is `254,016,000,000` ticks per second.
- **PR-TIME-002:** Observed video `FrameRate` fields may encode ticks per frame; audio equivalents may encode ticks per sample. The owning class determines interpretation.
- **PR-TIME-003:** `TrackItem/Start` and `End` are sequence-domain times in observed fixtures.
- **PR-TIME-004:** `Clip/InPoint` and `OutPoint` are source/media-domain times in observed fixtures.
- **PR-TIME-005:** Sentinel values such as `-91445760000000000` and `-101606400000000000` are retained verbatim until their per-field semantics are validated.
- **PR-GEO-001:** Motion point values such as Position and Anchor Point appear as normalized `x:y` pairs in factory presets. Sequence/clip coordinate transformations and pixel-aspect behavior still require experiments.

No parser may infer field domain solely from the element name `FrameRate`, `Start` or `End`.

## 7. Components and parameters

Observed video path:

```text
VideoComponentChain
  -> ComponentChain / Components / Component[ObjectRef]
     -> VideoFilterComponent
        -> MatchName
        -> Component / Params / Param[ObjectRef]
           -> VideoComponentParam | PointComponentParam | ArbVideoComponentParam
```

`MatchName` is the stable identity candidate; display and instance names are presentation data and may be localized. Observed examples include `AE.ADBE Motion`, `AE.ADBE Lumetri`, `AE.ADBE Fast Blur`, `AE.ADBE Text` and `PR.ADBE Solarize`.

Observed parameter fields include `Name`, `ParameterID`, `ParameterControlType`, `StartKeyframe`, `Keyframes`, `IsTimeVarying`, bounds and current value. Control types observed in the preset corpus:

| Code | Observed representation |
|---:|---|
| 1 | integer |
| 2 | float |
| 3 | angle |
| 4 | checkbox |
| 5 | packed color-like integer; encoding unknown |
| 6 | point |
| 7 | popup index |
| 8 | float slider |
| 9, 10 | arbitrary/base64 payload |
| 11, 12 | group begin/end |
| 16 | hidden |
| 22 | arbitrary mask-path payload in the two mask-preset fixtures |

Codes are observations, not a complete enum. Empty parameter names and duplicate names are valid; order and `ParameterID` must be retained.

### 7.1 Mask preset payload

`MaskPresets.prfpset` contains ellipse and rectangle fixtures with `MatchName` `AE.ADBE AEMask`. Both expose 11 ordered parameters. Their Mask Path is a base64 `ArbVideoComponentParam` using control type 22.

The decoded 124-byte little-endian payload begins with bytes `6B 63 69 6E` (`kcin`), then a 32-bit fixture flag, a 32-bit point count of four, and four 28-byte point records. Each record contains a 32-bit flag plus six float32 values consistent across the two fixtures with an anchor and two control handles. This is a provisional structural interpretation, not a complete mask format. Unknown bytes and all original encoded text remain authoritative and must round-trip unchanged until custom/open/variable-point masks validate the schema.

## 8. Keyframes

Observed preset records use semicolon-separated keyframes with comma-separated fields similar to:

```text
time,value,unknown,interpolation,in_velocity,in_influence,out_velocity,out_influence[,spatial_fields];
```

The analyzed factory preset subset contains interpolation codes `0` and `2`: `StartKeyframe` records use both, while explicit `Keyframes` records observed in Factory Presets use code `2`. Neither code may be assigned a semantic label until a controlled project proves the mapping. Temporal Hold, Linear, Bezier, Auto Bezier, Continuous Bezier, Ease In/Out, spatial tangents and roving behavior each require a minimal fixture.

Arbitrary parameter blobs are opaque. Base64 decoding is bounded, and decoded content is never interpreted as executable data.

## 9. Preservation and save modes

Three explicit save capabilities are required:

1. **Read-only import:** creates a canonical project and reports unsupported data; never overwrites the `.prproj`.
2. **Preserving edit:** writes only when every modified entity has a validated mapping and all untouched foreign data can be retained.
3. **Native export:** produces a new `.prproj` only after writer conformance is validated for the target build.

- **PR-SAVE-001:** Default early behavior is read-only import.
- **PR-SAVE-002:** Save to the source path is forbidden until E5 round-trip validation.
- **PR-SAVE-003:** Unknown nodes/attributes/text/references are preserved with ordering and ownership.
- **PR-SAVE-004:** Export emits a machine-readable loss report. Any unacknowledged loss fails the operation.
- **PR-SAVE-005:** Atomic write uses a sibling temporary file, flush, parse verification, and replace while retaining a recovery copy.

Semantic comparison, not compressed-byte equality, is the primary oracle because gzip metadata and serializer formatting may differ. Byte-level diffs remain useful for minimizing experiments.

## 10. Required experiment matrix

| Experiment | Variable isolated | Required result |
|---|---|---|
| PR-X-001 | gzip OS byte and compression level | Open/edit/save outcome per 24.x build |
| PR-X-002 | one clip with linked A/V | Link container and IDs mapped |
| PR-X-003 | each video/audio transition alignment and no-handle case | Transition object graph and duration units |
| PR-X-004 | each temporal/spatial interpolation | Numeric code and tangent fields |
| PR-X-005 | opacity mask shape/feather/expansion/tracking | Mask graph and coordinate domain |
| PR-X-006 | speed, reverse, hold and ramp | Time-remap graph |
| PR-X-007 | clip, track and master audio automation | Units and component ownership |
| PR-X-008 | mono/stereo/5.1/adaptive sequences | Track/channel routing graph |
| PR-X-009 | nested, multicam and adjustment sequences | Source/reference topology |
| PR-X-010 | proxy attached/offline/relinked | Media identity and path precedence |
| PR-X-011 | captions, markers, productions and team metadata | Data-track and external-project behavior |
| PR-X-012 | unknown object injected into controlled fixture | Preservation and application tolerance |
| PR-X-013 | add/remove/reorder one Project-panel saved-view column | `ObjectID`/`ObjectRef` serialization-scope boundary |

Each test compares a baseline and exactly one changed property, records hashes, decompressed structural diff, application messages, resave behavior and semantic re-import.

## 11. Public API references and limitation

Adobe's public [Premiere UXP DOM](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/) documents application objects such as projects, sequences, tracks, clips and markers. It is useful for validating concepts, but it is **not** a published `.prproj` serialization schema. No official XSD or complete writable file-format specification has been identified. This document therefore distinguishes empirical observations from public API facts.
