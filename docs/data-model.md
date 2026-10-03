# Canonical data model specification

**Status:** draft. All IDs below are normative requirements only after the compatibility gate is validated.

## 1. Principles

- **DM-001:** The internal model is not the `.prproj` XML tree. Import/export adapters map between the canonical model and format-specific preservation graphs.
- **DM-002:** Entities use stable opaque IDs, never memory addresses or array positions.
- **DM-003:** Time is rational and exact. No persistent timeline position is a binary float.
- **DM-004:** Unknown source-format data survives an unedited round-trip and is invalidated only when an edit makes preservation impossible.
- **DM-005:** Mutations occur through validated transactions with reversible semantic events.
- **DM-006:** Media identity is separate from path and from a decoded stream instance.
- **DM-007:** Channel layout, color space, alpha interpretation, pixel aspect ratio and field order are explicit, never silently defaulted after import.

## 2. Top-level graph

```text
Project
├── project_id, format_version, created/modified metadata
├── settings (video, audio, color, time display, ingest, scratch policy)
├── bins -> ProjectItem IDs
├── media_assets -> MediaAsset IDs
├── sequences -> Sequence IDs
├── shared effects/presets/styles
├── markers and metadata schemas
└── foreign_payloads -> adapter-owned preservation records

Sequence
├── edit_rate and start timecode
├── video/audio/data track stacks
├── sequence markers
├── working color/audio settings
└── nested references by Sequence ID
```

Cycles in bin hierarchies and nested sequences are invalid. Import may retain the foreign cycle as opaque data but must not expose an endlessly recursive canonical graph.

## 3. Time model

- **DM-TIME-001:** `RationalTime { value: i128, rate_num: u64, rate_den: u64 }` is the conceptual persistent representation. A normalized common tick domain may be used only when every supported frame and sample rate remains exact and overflow bounds are proven.
- **DM-TIME-002:** Intervals are half-open `[start, end)`; duration is non-negative.
- **DM-TIME-003:** Sequence time, source/media time, and effect-parameter time are distinct types or domains and cannot be mixed implicitly.
- **DM-TIME-004:** Drop-frame affects timecode labels, not media duration or edit math.
- **DM-TIME-005:** The observed Premiere tick constant `254,016,000,000 ticks/s` is an adapter fact, not the universal core time type.
- **DM-TIME-006:** Import adapters retain the original numeric token and sentinel alongside any validated rational interpretation. Nonnumeric or out-of-range source tokens are diagnostics, not values to coerce.

## 4. Media model

`MediaAsset` contains one or more immutable stream descriptors and a set of mutable locations: original, proxy, alternate and offline candidates. A stream descriptor includes codec identity, duration, start time, rate, dimensions, pixel format, sample format, channel layout, timecode and full color metadata.

Relink changes a location, not the asset ID. Conformance caches are keyed by content identity plus interpretation settings. A proxy mapping must prove source-time equivalence; otherwise it is rejected or explicitly marked approximate.

### 4.1 Metadata schema

Metadata values are addressed by a qualified schema namespace plus stable property key, not by localized column label. A schema record declares value type, internal/external visibility, qualifiers and adapter provenance. The audited private project namespace contains 63 property keys and the private file namespace contains 7; this is evidence for an extensible registry, not a closed canonical enum.

- **DM-META-001:** Unknown namespaces, properties and qualifiers remain typed opaque values and preserve source spelling/order where required.
- **DM-META-002:** Timecode metadata retains frame-rate, display, offset and bounds qualifiers as exact integers/rationals.
- **DM-META-003:** File path/URI metadata is separate from the canonical media-location record and cannot silently relink an asset.
- **DM-META-004:** Internal/external is source-schema metadata, not an authorization boundary.

## 5. Timeline model

```text
TrackStack
  Track { id, kind, order, lock, sync_lock, output, routing, items }

TimelineItem = Clip | Gap | Transition | Composition | Caption | Generator

Transition
  edit_relation { left_clip?, right_clip?, edge, alignment, duration, single_sided }
  component_identity
  foreign_payload

Clip
  timeline_range
  source_id
  source_range
  time_transform
  links/groups
  component_stack
  channel mapping
  labels/metadata
```

- **DM-TL-001:** Items on a non-overlap track may not overlap, except where the format explicitly represents a transition relationship or a composition type.
- **DM-TL-002:** A clip's `TimeTransform` maps sequence-local time to source time and represents constant speed, reverse, frame hold and piecewise time remapping.
- **DM-TL-003:** A/V links and user groups are different relations. Breaking one does not break the other.
- **DM-TL-004:** Track targeting and current UI selection are session/application state, not intrinsic clip state; only persisted format state explicitly observed in evidence may enter the project.
- **DM-TL-005:** Nested sequences are references with an explicit source range and cannot be flattened on import.
- **DM-TL-006:** Transition duration, alignment, edge and single/two-sided state are independent fields. A source adapter may not derive one from another unless its format contract proves that relation.

## 6. Effects and animation

`ComponentInstance` has a stable effect identity, enabled/bypass state, ordered parameters, masks, foreign payload and declared processing domain. Parameter values are typed: boolean, integer, scalar, angle, point, color, enum, curve, string, path, binary/opaque and structured group.

An animated parameter contains keyframes with:

- time domain and exact time;
- typed value;
- temporal interpolation;
- incoming/outgoing velocity and influence where defined;
- spatial tangents for spatial parameters;
- source-format opaque fields that are not yet semantically decoded.

- **DM-FX-001:** Parameter order is preserved even when names collide or are empty.
- **DM-FX-002:** Effect stack order is significant.
- **DM-FX-003:** Unknown effects are retained as disabled/unrendered opaque instances and produce a visible compatibility warning; they are never dropped silently.
- **DM-FX-004:** “Current value” and animated curve are both retained when the source format contains both.
- **DM-FX-005:** Color values include encoding/space semantics; a packed integer is not assumed to be sRGB without evidence.

## 7. Audio model

Audio tracks and clips carry explicit channel layouts and routing maps. Automation has clip, track and master scopes. Gain units distinguish linear amplitude, power and dB. Pan law is project/sequence state when the source format exposes it.

Track automation state is modeled as `Off | Read | Write | Latch | Touch | Foreign(code)`, with independent safe-during-write flags and Automatch policy. The labels are a public behavioral contract; source numeric codes remain adapter-owned until mapped by controlled fixtures.

- **DM-AUD-001:** No import path silently reduces multichannel audio to stereo.
- **DM-AUD-002:** Plugin latency and tail are modeled for compensation and export.
- **DM-AUD-003:** Sample-accurate automation is evaluated in the same graph for live playback and offline export.
- **DM-AUD-004:** Channel type and adaptive-channel numeric enums remain opaque adapter values until fixtures establish their semantic mapping.
- **DM-AUD-005:** Observed `.prproj` values `AutomationMode=1` and `AutomationSafeFlags=0` remain foreign codes; their likely UI labels are not serialized facts.

## 8. Transactions and history

Each command validates preconditions against a project revision, emits a new immutable revision plus semantic event records, and either commits completely or leaves the previous revision unchanged. A drag gesture may publish preview states but commits one undo step. Undo restores document state; playhead/viewport restoration follows the behavior specification rather than being assumed.

History persistence, collaboration/merge semantics and autosave compaction remain open decisions. A snapshot-only design is not accepted until memory behavior on large projects is measured.

## 9. Preservation graph

Each importer may attach `ForeignPayload { format, owner_id, path/key, raw_value, dependencies, invalidation_rule }`. For XML, preservation must retain unknown elements, attributes, ordering where significant, text, references and namespace information. Raw payload is bounded and never executable.

`owner_id` is a compound identity containing source format, serialization-scope path, identity kind and raw value. Numeric `ObjectID` alone is not globally unique in an observed `.prproj`.

When an edit invalidates foreign data, export must report the exact discarded path and reason. Save cannot claim lossless round-trip if any unacknowledged discard occurred.

## 10. Required validation

The model is accepted only after every supported `.prproj` concept has one of three explicit mappings: canonical, preserved opaque, or declared unsupported with a save prohibition. OTIO is used to test editorial interchange concepts, but because OTIO references rather than embeds media and does not represent all Premiere state, it is not the canonical project model.
