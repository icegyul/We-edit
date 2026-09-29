# Timeline behavior specification

**Status:** draft with partial public/empirical evidence. Every `[open]` item blocks GATE-TL-001 or GATE-TL-002.

## 1. Terms and invariants

- A clip occupies a half-open sequence interval `[start, end)` and maps it to a source interval through a time transform.
- A cut/edit point is a boundary touched by an outgoing item, an incoming item, or both.
- Track targeting/source patching, sync lock, track lock, output enable, linked selection and current selection are separate states.
- Locked tracks cannot be mutated. A command either applies atomically to its validated set or reports why it cannot.
- Every pointer drag is a preview gesture followed by one committed history transaction; cancellation restores the exact prior document state.
- Frame snapping uses sequence frame boundaries. Audio/sample display modes and subframe edits require separate validation.

## 2. Tool selection

| ID | Tool | Default key | Observable contract |
|---|---|---:|---|
| TL-TOOL-001 | Selection | `V` | Select/move/edge-trim according to hit target; default tool |
| TL-TOOL-002 | Track Select Forward | `A` | Selects items at/right of hit time on one track; modifier expands to all tracks |
| TL-TOOL-003 | Track Select Backward | Dataset/modifier dependent | Mirror selection to the left; exact boundary inclusion open |
| TL-TOOL-004 | Ripple Edit | `B` | Changes one edit side and shifts subsequent affected items to remove/create duration |
| TL-TOOL-005 | Rolling Edit | `N` | Moves a shared cut; combined duration and later timeline positions remain constant |
| TL-TOOL-006 | Rate Stretch | `R` | Changes clip duration by changing speed while retaining chosen source extent |
| TL-TOOL-007 | Razor | `C` | Splits hit clip at snapped time; modifier cuts all eligible tracks |
| TL-TOOL-008 | Slip | `Y` | Changes source In/Out equally; sequence interval and duration remain fixed |
| TL-TOOL-009 | Slide | `U` | Moves selected clip while counter-trimming adjacent clips; outer boundaries remain fixed |
| TL-TOOL-010 | Pen | `P` | Adds/selects/moves timeline rubber-band keyframes |
| TL-TOOL-011 | Hand | `H` | Pans viewport without changing project |
| TL-TOOL-012 | Zoom | `Z` | Changes horizontal viewport scale around validated anchor |

Modifier behavior, linked-item propagation and invalid-operation feedback require a cross-product test rather than implementation by intuition.

## 3. Selection and links

- **TL-SEL-001:** Plain click selects the hit item and clears incompatible prior selection; modifier toggles/adds according to platform convention.
- **TL-SEL-002:** With Linked Selection enabled, selecting a clip selects linked A/V counterparts; the relation is not inferred merely from matching times.
- **TL-SEL-003:** Alt/Option click or drag can target one member independently without deleting the link; exact selection aftermath is open.
- **TL-SEL-004:** User groups and A/V links propagate movement differently and require independent tests.
- **TL-SEL-005:** Range/marquee selection boundary inclusion, track-header selection and transition selection are open.

## 4. Insert and overwrite

### Overwrite

- **TL-OW-001:** `.` from Source Monitor and default drag/drop perform overwrite into targeted or destination tracks.
- **TL-OW-002:** Covered destination material is removed; partial overlaps are trimmed; a destination clip spanning both sides is split into retained left/right segments.
- **TL-OW-003:** Source In/Out defines duration; without marks the effective source range follows monitor/project-item rules that remain to validate.
- **TL-OW-004:** Playhead final position, selection and linked A/V routing are open dimensions.

### Insert

- **TL-IN-001:** `,` or Ctrl-modified drop inserts source duration at the edit point.
- **TL-IN-002:** Directly affected tracks shift regardless of sync-lock; other unlocked tracks shift only when sync lock is enabled, consistent with Adobe's public description.
- **TL-IN-003:** Items crossing the insertion point may be split according to targeting/track policy. Exact behavior by media type is open.
- **TL-IN-004:** Sequence markers, track keyframes, captions and transitions require explicit shift tests.
- **TL-IN-005:** Track lock, source patch, insufficient destination track and channel-layout mismatch must produce deterministic routing or refusal.

## 5. Cut, clear, lift, extract and ripple delete

| ID | Operation | Candidate behavior | Open dimensions |
|---|---|---|---|
| TL-CUT-001 | Razor click | Split the hit item at pointer/snapped time | boundary click, transition, nested item, audio subframe |
| TL-CUT-002 | Shift+Razor | Split eligible items across tracks at one time | locks, target vs all, captions |
| TL-CUT-003 | Add Edit `Ctrl+K` | Split selected clips or targeted tracks at playhead | precedence when both selection and targets exist |
| TL-CLEAR-001 | Clear/Delete | Remove selected items and leave time gap | transitions, linked selection, markers |
| TL-LIFT-001 | Lift `;` | Remove targeted material in In-Out and preserve duration | partial clips, track keyframes, selection/playhead |
| TL-EXT-001 | Extract `'` | Remove In-Out and ripple-close affected tracks | sync-lock cross-product |
| TL-RDEL-001 | Ripple Delete | Remove selection/gap and close time where allowed | discontiguous selection, blockers, transitions |

Ripple is not permitted to desynchronize protected material silently. The exact warning/partial-application behavior when another sync-locked track contains material in the removal span must be measured.

## 6. Move and duplicate

- **TL-MOVE-001:** A normal move is an overwrite at the destination and removes the original interval as defined by the drag mode.
- **TL-MOVE-002:** Insert-modified move opens destination space; whether source space closes is tested separately for same-track and cross-track moves.
- **TL-MOVE-003:** Vertical moves preserve sequence time when snapping/modifier behavior requests it and never move video into audio tracks or vice versa.
- **TL-MOVE-004:** Linked and grouped items preserve relative offsets unless explicitly overridden.
- **TL-MOVE-005:** Duplicate-modified drag creates new item IDs and link/group relations according to a measured rule.
- **TL-MOVE-006:** Collision behavior with transitions, locked tracks and mixed item types is open.

## 7. Trimming

### Selection edge trim

Changes the selected head or tail without moving unrelated later items. It is bounded by available source media, minimum duration, locked state and collision policy. Synthetic/infinite media behavior is type-specific.

### Ripple trim

Moves one edge and shifts later affected items by the opposite duration delta. Track participation follows direct effect plus sync lock. Head-ripple semantics, markers and automation are open test dimensions.

### Rolling edit

The outgoing tail and incoming head move by equal/opposite deltas. Sequence duration and the following edit remain fixed. The allowable delta is the intersection of both source-handle ranges.

### Rate stretch

The timeline duration changes while the selected source extent is retained. Rate formula, frame sampling mode, audio pitch policy, reverse crossing and keyframe retiming require validation. The canonical choice set includes frame sampling, frame blending and optical flow because Adobe documents those policies, but their 2024 persistence and rendering behavior require separate fixtures.

### Slip

Source In and Out shift by the same delta; timeline range and neighbors do not move. It is bounded by source handles unless the media is infinite.

### Slide

The center clip moves while the preceding clip's tail and following clip's head compensate. The outer boundary of the three-item region stays fixed. One-sided/gap/transition cases remain open.

## 8. Snapping

- **TL-SNAP-001:** `S` toggles timeline snapping and UI state.
- **TL-SNAP-002:** Candidate targets include item edges, playhead, markers, sequence In/Out and possibly work-area/track keyframes; target set must be measured.
- **TL-SNAP-003:** The active dragged edge/anchor and every item in a multi-selection generate candidate alignments.
- **TL-SNAP-004:** Threshold is a screen-space distance influenced by UI scale, not a fixed time duration. The prior reference's approximate nine-pixel value is not validated.
- **TL-SNAP-005:** Tie-breaking and target priority must be deterministic.
- **TL-SNAP-006:** A modifier may invert snapping during a gesture; exact key and latch timing are open.

Validation records zoom, DPI scale, pointer coordinates, chosen target, indicator line and resulting exact time.

### Viewport navigation and wheel zoom

- **TL-VIEW-001:** With the pointer over the Timeline, each raw Ctrl/Cmd+wheel event changes the horizontal scale by exactly `1.20×`; a negative event applies `1 / 1.20×`.
- **TL-VIEW-002:** Wheel-delta magnitude does not change the size of one step. A line delta of `1` and a point delta of `120` each produce one step, preventing device-specific overshoot.
- **TL-VIEW-003:** The sequence time under the pointer remains at the same screen coordinate after zoom, except where the non-negative scroll boundary clamps the viewport.
- **TL-VIEW-004:** Timeline zoom does not consume a smooth-scroll or inertial accumulator. When no raw wheel event arrives, no additional zoom occurs.
- **TL-VIEW-005:** Horizontal scale is clamped to `0.02..=20,000` pixels per second. Normal wheel panning and Shift+wheel vertical navigation remain separate paths.

`TL-VIEW-001`, `TL-VIEW-002`, and the no-event portion of `TL-VIEW-004` have unit coverage. Pointer anchoring, platform input normalization, high-resolution touchpads, DPI behavior, and comparison against Premiere 2024 remain E4 validation work.

## 9. Transitions

- **TL-TR-001:** A transition is related to an edit point and up to two adjacent clips, not a free overlapping clip.
- **TL-TR-002:** Alignments include Center at Cut, Start at Cut, End at Cut and any custom-start representation observed in 2024.
- **TL-TR-003:** Default video/audio transitions and durations are preferences, not hard-coded constants.
- **TL-TR-004:** When handles are insufficient, visible warning, repeated-frame behavior and actual rendered sampling must be tested.
- **TL-TR-005:** Moving/deleting/trimming either side updates, shortens, one-sides or deletes the transition according to measured rules.
- **TL-TR-006:** `Ctrl+D`, `Ctrl+Shift+D` and `Shift+D` target selection/edit points using a priority rule still to validate.
- **TL-TR-007:** The transition command contract carries edge (start/end), duration, alignment and single/two-sided intent independently. Adobe's public API exposes these concepts only in a post-target 25.6 API, so 2024 behavior and serialization remain an E4 requirement.

## 10. Track controls

- Track lock prevents document edits on that track.
- Sync lock controls whether otherwise unaffected tracks shift during insert/ripple operations; a directly edited track shifts regardless of sync-lock state according to Adobe public documentation.
- Track targeting controls commands such as edit navigation, paste, Add Edit, Lift and Extract; source patching controls incoming channel routing. Their exact interaction is tested independently.
- Video output, audio mute, solo, record/voice-over and routing are playback states with separately specified persistence.

## 11. Navigation and history candidates

Up/Down navigate edit points in a target-dependent set; Left/Right step frames; Home/End navigate sequence bounds; `D` selects the item under the playhead under a priority rule. Whether undo restores playhead, selection, targets, workspaces and viewport is command-specific and remains open.

## 12. Conformance matrix

Every mutating operation is tested across at least:

```text
track: targeted/not × locked/unlocked × sync-lock on/off
item: selected/not × linked/unlinked × grouped/ungrouped
neighbors: none/gap/clip/transition × source handles enough/insufficient
time: frame boundary/subframe × at head/interior/tail
mode: snapping on/off × normal/modifier × playback stopped/active
```

The oracle captures project diff, selection, playhead, focus, undo stack, messages and serialized `.prproj` diff. Unsupported combinations must have an explicit refusal result.

## 13. Public references

- [Adobe: Tools panel](https://helpx.adobe.com/premiere/desktop/get-started/tour-the-workspace/tools-panel-and-options-panel.html)
- [Adobe: Rolling edits](https://helpx.adobe.com/premiere/desktop/edit-projects/trim-clips/perform-rolling-edits.html)
- [Adobe: Slip edits](https://helpx.adobe.com/premiere/desktop/edit-projects/trim-clips/perform-slip-edits.html)
- [Adobe: Snapping](https://helpx.adobe.com/premiere/desktop/edit-projects/change-clip-sequence/snap-clips.html)
- [Adobe: Sync lock](https://helpx.adobe.com/premiere/desktop/edit-projects/change-clip-sequence/sync-lock-to-prevent-changes.html)
