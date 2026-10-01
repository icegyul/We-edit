# Compatibility validation

**Status: IN PROGRESS**<br>
**Application version: 0.5.1 alpha**<br>
**Full compatibility claim: not yet validated**<br>
**Last audit: 2026-09-29**

## 1. Purpose

This validation process prevents implementation guesses about proprietary behavior from being presented as compatible specifications or production-safe project I/O. OpenPremier is developed independently. Implemented behavior is documented separately from observed compatibility evidence, and writable `.prproj` support remains outside the current release until round-trip preservation is verified.

The status changes to `VALIDATED` only in a reviewed change that satisfies every mandatory row below. A status changes only when the corresponding evidence and repeatable acceptance tests are available.

## 2. Evidence levels

| Level | Meaning | Can validate normative behavior? |
|---|---|---|
| E0 | Hypothesis or architectural preference | No |
| E1 | Public primary documentation | Only for the documented claim |
| E2 | One inspected data file or one observation | No; establishes a lead |
| E3 | Multiple independent fixtures or versions agree | Schema facts only |
| E4 | Reproducible black-box experiment with controlled input and measured output | Yes |
| E5 | Automated differential corpus with stable oracle, tolerances, and regression results | Yes; required for math/round-trip |

“Validated” requires E4 or E5 for behavior. Static inventory facts may use E3 when the scope and sample population are explicit. A missing value is not inferred as a default.

## 3. Mandatory matrix

| Gate ID | Deliverable | Current evidence | Remaining acceptance criteria | Status |
|---|---|---|---|---|
| GATE-FMT-001 | `.prproj` container and object graph | Five gzip/XML samples; root graph plus colliding inline view-state ID island identified; 62-class generated catalog; E2-E3 | Resolve every identity-scope boundary; corpus covering Premiere 24.0/final 24.x; malformed-input cases | Partial |
| GATE-FMT-002 | Lossless unknown-node preservation | Design only; E0 | Read/write/reopen differential corpus with byte/semantic classification | Pending |
| GATE-FMT-003 | Sequence, track, clip, link, transition, marker, proxy, multicam, caption, production mappings | Clips/markers partially observed; 171 empty transition descriptors plus post-target semantic contract; E1-E3 | One minimal 24.x fixture per feature and round-trip results | Pending |
| GATE-FMT-004 | Effect, mask, keyframe, time-remap, audio automation XML | Motion/Lumetri layouts; two mask-path fixtures; likely-but-unmapped Inverted checkbox; keyframe codes 0/2; automation codes 1/0; E1-E3 | Custom/open/multiple masks, all interpolation codes, spatial tangents, remap, clip/track automation | Partial |
| GATE-FMT-005 | Sequence/editing-mode/export preset schemas | 365 sequence presets, 52 editing modes, 1,028 export presets; malformed fixtures recorded; E3 | Official 24.0/final-24.x comparison; controlled import/export round trips and enum mappings | Partial |
| GATE-TL-001 | Timeline editing state machine | Common operations drafted; E1-E2 | Measured matrix for selection/link/target/sync-lock/lock combinations | Pending |
| GATE-TL-002 | Snapping and trim edge cases | Approximate threshold only; E2 | DPI/zoom-independent target priority and tie behavior | Pending |
| GATE-KBD-001 | Default Windows key map | 41 maps, 11 localized defaults, 10 physical-layout fragments; English 370-pair index; E3 | Verify precise Premiere 24 builds, runtime physical/character translation, duplicate/conflict semantics | Partial |
| GATE-KBD-002 | Shortcut dispatch behavior | Adobe public docs describe global vs panel context; E1 | Black-box focus/context tests, auto-repeat, chords, text-entry suppression | Pending |
| GATE-FX-001 | Complete Premiere Pro 2024 effect/transition/generator inventory | 151 Premiere plugin names plus shared library names; E2 | Reconcile visible UI catalog in clean profile and installed optional components | Partial |
| GATE-FX-002 | Parameter contracts | Motion/Lumetri and selected presets; E2-E3 | Every in-scope effect: types, units, ranges, defaults, animation, alpha/color behavior | Pending |
| GATE-FX-003 | Pixel parity | No controlled oracle corpus | E5 corpus by pixel format/color space/GPU; published tolerances | Pending |
| GATE-AUD-001 | Audio routing and automation | Public five-mode behavior; two preset track-record shapes; XML exposes opaque mode/safe codes 1/0; E1-E3 | Map enums/bits and measure mono/stereo/5.1/adaptive routing plus clip/track/master automation | Pending |
| GATE-AUD-002 | DSP parity | No controlled oracle corpus | Impulse, sweep, step, latency and null/difference tests per effect | Pending |
| GATE-UI-001 | Panel/workspace inventory | 16 persisted layouts, panel IDs/frequencies and split/tab vocabulary; E3 | Clean-profile UI walk-through on 24.x; focus/docking/multi-monitor tests | Partial |
| GATE-UI-002 | Rust UI feasibility | Candidate libraries identified; E1 | Prototype outside production crates passes docking, HiDPI, IME, accessibility and GPU-sharing benchmark | Pending |
| GATE-PLUG-001 | OpenFX host contract | OpenFX 1.5 reference; E1 | Required/optional suite table, lifecycle state machine, conformance plugins, threading tests | Pending |
| GATE-PLUG-002 | Scripting API | Capability model plus UXP/CEP manifest-shape inventory; E0-E2 | Versioned IDL, deterministic tests, sandbox threat model, migration/versioning rules | Pending |
| GATE-DM-001 | Canonical data model | Entity draft | Map every supported `.prproj` and OTIO concept; invariants and migrations accepted | Pending |
| GATE-META-001 | Project/file metadata registry | 63 project and 7 file XMP definitions; E2 | Controlled read/edit/write tests, qualifier semantics, public/private namespace policy | Partial |
| GATE-ARCH-001 | Architecture map and budgets | Rust-first proposal | Benchmarks and dependency/license review; ADRs accepted | Pending |
| GATE-LEGAL-001 | Clean-room provenance | Policy and hashes recorded | Repeat observations using a licensed official installation; counsel review before distribution claims | Pending |
| GATE-LIC-001 | License boundaries | GPL/LGPL intent recorded | Dependency license audit and crate-level licensing decision | Pending |

Progress is not computed as a misleading percentage. A row is either `Validated`, `Partial`, `Pending`, or explicitly `Out of scope`. Full validation requires every mandatory row to be `Validated`.

## 4. Required shape of a validated specification

Every normative feature must contain:

1. Stable requirement ID.
2. Supported Premiere version/build, OS, locale, hardware, and project settings.
3. Preconditions and exact user action or serialized input.
4. Observable result, including selection, playhead, undo, focus, error, and persisted-data effects.
5. Boundary and failure cases.
6. Source/evidence level and fixture hash.
7. Acceptance test and numerical tolerance where applicable.
8. Unknown-field preservation rule for project data.

## 5. Minimal validation corpus

The corpus must include synthetic assets whose expected values are known and redistributable:

- frame-counted color bars, ramps, alpha wedges, zone plates, single-pixel impulses, and checkerboards;
- 8/10/12/16-bit integer and 16/32-bit float sources where supported;
- Rec.601, Rec.709, Rec.2020, PQ, HLG, ACEScg and tagged/untagged media;
- CFR and VFR media at 23.976/24/25/29.97/30/50/59.94/60 fps, including drop-frame timecode;
- mono, stereo, 5.1 and adaptive audio at 44.1/48/96 kHz with impulses, sweeps and phase tests;
- nested sequences, adjustment layers, multicam, proxies, offline media, captions and productions;
- every temporal/spatial keyframe interpolation and every transition alignment/handle condition.

Private Adobe-generated outputs may serve as local oracles by hash but are not committed. Tests committed to the repository must use original or freely licensed assets.

## 6. Validation procedure

1. Resolve every pending row with reviewed evidence.
2. Freeze the compatibility documents at a tagged baseline.
3. Record architecture and license decisions as ADRs.
4. Change this file to `Status: VALIDATED` in the same review that adds the final evidence manifest.
5. Reconcile the existing alpha against the validated baseline; delete, redesign, or qualify behavior that does not conform.
6. Only then authorize public compatibility/parity claims and writable `.prproj` support.
