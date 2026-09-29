# compatibility open-questions research

**Research date:** 2026-09-28<br>
**Target:** Premiere Pro 2024 / 24.x<br>
**Outcome:** static search exhausted for the four highest-priority gaps; controlled 24.x fixtures are now required

## 1. Evidence boundary

This pass searched five workstation-local .prproj files, the installed declarative preset/keyboard/workspace corpus, and Adobe's public documentation. It did not execute the portable package, inspect JavaScript implementation bodies, or disassemble/decompile executable modules.

Evidence is separated as follows:

- **E2/E3 local observation:** serialized facts in the five inspected projects and declarative presets;
- **E1 public contract:** concepts documented by Adobe, including APIs newer than the 2024 target;
- **hypothesis:** a useful candidate that still requires a one-variable 24.x experiment;
- **unknown:** no serialization or behavior claim is made.

Public UXP pages marked “Since: 25.6” are deliberately not treated as evidence for the 2024 .prproj schema.

## 2. Object identity: corrected model

Treating every numeric ObjectID in a document as one global namespace is disproved by the corpus. The root graph has direct definitions under PremiereData, while Project/Node/Properties/ProjectViewState.List contains an inline serialization island that reuses the same numeric values.

| Fixture | XML bytes | All ObjectID attributes | Direct root definitions | Inline attributes | Numeric values colliding across the whole tree | Direct sequences |
|---|---:|---:|---:|---:|---:|---:|
| Broadcaster template | 697,789 | 1,115 | 943 | 172 | 171 | 9 |
| Social Media template | 5,855,107 | 5,446 | 3,299 | 2,147 | 2,146 | 10 |
| Standard template | 255,727 | 483 | 140 | 343 | 141 | 2 |
| Recent user autosave | 110,078 | 209 | 92 | 117 | 93 | 1 |
| Empty user project | 71,567 | 155 | 38 | 117 | 39 | 0 |

Within each fixture, direct root definitions have unique numeric IDs. The view-state island's references resolve against IDs present inside that island, but its wrapper also carries an ObjectID value reused by a nested entry. Therefore the exact scoping role of the wrapper attribute remains unknown.

Required parser rule: retain an explicit serialization-scope path with every identity and reference. Never resolve by bare integer across the whole XML tree. A future Rust adapter must reject ambiguous cross-scope resolution rather than choosing the first match.

## 3. Transitions

The five projects contain 171 TransitionItems descriptors. Their observed child fields are only Index and MediaType; none contains a populated transition object. Static searches of the declarative corpus found shortcut commands for applying default audio/video transitions, but no project serialization fixture.

Adobe's post-target [AddTransitionOptions](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/addtransitionoptions) contract identifies four useful semantic dimensions: start versus end, duration in tick time, forced single-sided versus two-sided placement, and alignment. [VideoClipTrackItem](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/videocliptrackitem) also exposes add/remove operations and start/end transition position. These are requirements for the canonical model, not proof of 2024 field names or numeric encodings.

Still unknown for 24.x:

- serialized class/tag and effect identity;
- ownership by track, edit point, or clip edge;
- duration/alignment representation and rounding;
- handle-shortage and repeated-frame flags;
- one-sided/two-sided conversion after trim, move, unlink, or delete;
- audio transition graph and channel-layout behavior.

## 4. Time remapping

No inspected project contains an element or value matching TimeRemap, Time Remapping, ClipSpeed, or an equivalent speed/reverse object. Rate-like fields found in the corpus are media, frame, sample, or export rates, not clip time transforms.

Adobe's post-target VideoClipTrackItem API exposes speed and reverse state. Adobe's current [time-interpolation documentation](https://helpx.adobe.com/premiere/desktop/edit-projects/change-clip-speed/apply-time-interpolation-methods-to-adjust-clip-speed.html) distinguishes frame sampling, frame blending, and optical flow. This establishes the canonical concepts to preserve, but the page was updated in 2026 and cannot validate the 2024 serialization or render math.

The model must independently represent:

- sequence-time to source-time mapping;
- constant rate, reverse, hold, and piecewise ramp;
- frame-generation policy;
- audio duration/pitch policy;
- retiming of clip keyframes, transitions, captions, and linked A/V.

## 5. Mask parameters

The static mask presets still provide only two closed four-point shapes. Adobe's archived 2015 [Premiere reference](https://helpx.adobe.com/archive/en/premiere-pro/cc/2015/premiere_pro_reference.pdf) documents the mask controls Path, Inverted, Expansion, Opacity, and Feather. This makes the unnamed checkbox following Expansion a strong **Inverted candidate**, but it does not prove the preset's parameter ordering or numeric serialization. The final unnamed bounded float remains unknown.

Custom/open paths, variable point counts, multiple masks, tracking data, path animation, coordinate space, feather falloff, inversion encoding, and mask-combination behavior remain unvalidated.

## 6. Audio automation

The four projects with sequences contain 256 AutomationMode fields, all serialized as 1, and 22 AutomationSafeFlags fields, all serialized as 0. The empty project contains neither. This proves field presence and the observed default-domain values only; it does not establish their enum labels or bit assignments.

Adobe's current [Audio Track Mixer automation documentation](https://helpx.adobe.com/premiere/desktop/add-audio-effects/apply-audio-effects/audio-track-mixer-automation-modes.html) defines five behavioral states: Off, Read, Write, Latch, and Touch, plus Safe During Write and Automatch behavior. Those semantics are the state-machine checklist. Assigning serialized value 1 to Read is a plausible hypothesis, not a validated mapping.

Still unknown:

- numeric enum and safe-flag bit mapping in 24.x;
- clip, track, submix, and master ownership;
- write cadence, thinning, interpolation, and sample accuracy;
- Automatch timing and loop-boundary behavior;
- volume/pan/send units and channel-dependent pan law;
- persistence after Write-to-Touch switching.

## 7. Required controlled fixture set

Each experiment starts from a minimal baseline, changes exactly one property, saves to a new file, and records the application build, OS, locale, preference state, compressed hash, decompressed structural diff, reopen result, and resave diff.

| Priority | Fixture family | One-variable variants | Gate evidence produced |
|---:|---|---|---|
| 1 | Identity scope | add/remove/reorder one Project-panel column; duplicate one saved view | scope boundaries and reference resolution |
| 2 | Video transition | start/end; center/start/end alignment; 1/2/3/10-frame duration; one/two sided; enough/no handles | transition object graph, units, rounding, flags |
| 3 | Audio transition | Constant Power/Gain/Fade on mono, stereo, 5.1 | ownership, channel and duration graph |
| 4 | Constant speed | 50%, 200%, reverse; maintain audio pitch on/off | constant time transform and A/V policy |
| 5 | Time-remap curve | two speed keys; ramp; 0% hold; reverse crossing | curve records, interpolation, source mapping |
| 6 | Frame generation | sampling, blending, optical flow | persisted policy and render oracle |
| 7 | Mask | invert only; one moved point; added/deleted point; open/closed path; second mask | parameter IDs, path flags, point layout, ownership |
| 8 | Audio automation | Off/Read/Write/Latch/Touch; Safe During Write; one fader/pan step | enum/bit mapping and keyframe ownership |

## 8. Conclusion

No additional installed static file found in this pass can validate transition objects, time-remap serialization, mask variability, or automation behavior. More filename/string inventory would increase volume without raising the evidence level. The next useful step is an E4 black-box fixture run against a licensed, official Premiere Pro 24.0 and final 24.x installation. Until those results exist, the compatibility compatibility gate remains blocked: the existing alpha must not be treated as validated behavior or authorize parity claims and writable `.prproj` support.
