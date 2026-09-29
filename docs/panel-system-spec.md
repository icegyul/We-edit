# Panel, workspace, and window-system specification

**Status:** draft. Sixteen installed workspace layouts and their persistence vocabulary are statically mapped; exact UI behavior and Rust-toolkit feasibility remain unvalidated.

## 1. Layout model

The workspace is represented as a serializable forest of native windows. Each window owns a docking tree:

```text
Window
└── DockNode
    ├── Split { axis, ratio, first, second }
    └── TabGroup { ordered panel instances, active tab, presentation }
```

This model is now supported by static workspace evidence, not only architectural preference. The audited `prop.map version="4"` files persist monitor information, top-level frames, nested binary splitters, orientation, placement, tab IDs, active-tab index, visibility, floating-frame reanimation and size/position. Embedded panel state remains opaque and versioned.

- **UI-LAYOUT-001:** A panel instance has stable type ID, instance ID, title, minimum/preferred size and versioned state.
- **UI-LAYOUT-002:** A panel can move within a tab group, become a new adjacent split, move to another native window, or become a floating window.
- **UI-LAYOUT-003:** Edge drop zones create splits; center/tab zones create groups. The preview exactly matches the committed result.
- **UI-LAYOUT-004:** A floating window can itself contain a full docking tree, not only one panel.
- **UI-LAYOUT-005:** Closing the last panel closes its floating window; closing a floating window closes/hides its contained panels according to measured behavior.
- **UI-LAYOUT-006:** Split ratios respect minimum sizes and survive resize without accumulating rounding drift.
- **UI-LAYOUT-007:** Tab order, active tab, collapsed/stacked state where supported and focused panel persist in the workspace.
- **UI-LAYOUT-008:** Maximize/restore panel under pointer or focus is reversible and does not destroy the docking tree.

Adobe's public documentation describes edge docking zones, central grouping zones, tabbed/stacked presentation, Ctrl/Command-drag floating and floating groups on a secondary monitor. Exact 2024 hit regions, modifiers and stacked-group availability require black-box tests.

## 2. Multi-monitor and HiDPI

- **UI-WIN-001:** Every top-level window records logical bounds, monitor identity, DPI scale and maximized/fullscreen state.
- **UI-WIN-002:** On restore, unavailable/off-screen windows are clamped to a visible work area without overwriting the saved ideal layout.
- **UI-WIN-003:** Moving a window across mixed-DPI monitors updates scale, text, icons, hit targets and render surfaces without a full application restart.
- **UI-WIN-004:** Monitor add/remove, rotation, taskbar/dock work-area changes and remote-desktop reconnect are handled.
- **UI-WIN-005:** Video-monitor fullscreen output is distinct from a floating Program Monitor panel.
- **UI-WIN-006:** Keyboard focus, drag capture and menus remain associated with the correct native window.

Workspace persistence includes monitor configuration according to Adobe's public panel documentation. Pixel-identical placement is not promised when the hardware topology changes.

## 3. Focus and command routing

One panel is active and receives panel-context commands. A visible focus indicator is theme-aware. Pointer hover does not necessarily change keyboard focus; maximize-under-pointer and scroll-under-pointer are separate behaviors to test.

Focus travels through window -> tab group -> panel -> control. Modal dialogs and text/number editors temporarily own command routing. Closing/hiding a focused panel moves focus by a deterministic rule. Focus changes are application/session state and do not mutate the project.

## 4. Required panel inventory

Target 2024 inventory to validate against Window/workspace menus:

- Project
- Media Browser
- Libraries
- Info
- Effects
- Markers
- History
- Source Monitor
- Program Monitor
- Reference Monitor
- Effect Controls
- Audio Clip Mixer
- Audio Track Mixer
- Metadata
- Timeline
- Tools
- Audio Meters
- Lumetri Color
- Lumetri Scopes
- Essential Graphics / Properties, depending on 2024 build
- Essential Sound
- Text (transcripts and captions)
- Productions
- Events
- Progress
- Frame.io review
- Capture
- Timecode

Panel names and availability vary by build, platform, installed services and feature deprecations. A panel absent from one configuration is not removed from the catalog without version evidence.

The derived installed-layout inventory and panel frequencies are recorded in [premiere_workspace_inventory.md](evidence/generated/premiere_workspace_inventory.md). Stored IDs include native panel names, per-project instance IDs, UXP identities such as `com.adobe.dva.text -> mainPanel`, and CEP-backed panel IDs. Display labels are not stable identity.

## 5. Primary panel contracts

### Project panel

- List, icon and freeform/storyboard views where present.
- Nested bins, rename, labels, search/filter, sort and metadata columns.
- Hover scrub and J-K-L/I-O in icon view where supported.
- Drag sources for monitors/timeline; relink, proxy, interpret footage, reveal and metadata commands.
- Selection is shared with relevant inspectors but does not decode media synchronously on the UI thread.

Open items: multi-select rename rules, duplicate names, freeform persistence, exact column schema, source-effect tab behavior and productions locking.

### Source Monitor

- Displays master/source items and source sequences independently of the active program sequence.
- Own source playhead, In/Out, markers and transport state.
- Insert/overwrite uses source patches and targeted sequence tracks.
- Gang, comparison, audio waveform, overlays and button-editor behavior require separate records.

### Program Monitor

- Displays the active sequence at its playhead with transport, resolution, overlays, markers and safe margins.
- J-K-L/Space commands apply according to focus and shared transport policy.
- Frame presentation uses a GPU texture without full-frame CPU copy.
- Dropping media onto monitor edit zones and direct manipulation overlays remain to validate.

### Timeline

- Virtualized canvas for track headers, rulers, markers, items, transitions, keyframes and rubber bands.
- Horizontal time transform is exact and stable during zoom; vertical virtualization supports large track counts.
- Pointer hit testing is deterministic at subpixel zoom and mixed DPI.
- Tool/status, targets, patches, locks, sync locks, mute/solo/output and track heights are visible and keyboard-accessible.
- Behavioral operations are defined in `timeline-behavior.md`, not inside the widget.

### Effect Controls

- Ordered component stack with fixed and standard effects, bypass/reset, parameter editors, animation toggles and per-parameter keyframes.
- Mini-timeline shares clip/sequence time mapping explicitly.
- Value and velocity graphs, temporal/spatial interpolation, masks and direct-manipulation overlays require testable contracts.
- Multiple selection and source-versus-clip effects must not silently edit the wrong owner.

### Effects

- Searchable tree of presets, audio/video effects and transitions.
- Default-transition marker, favorites/custom bins and drag/double-click application rules.
- Availability reflects plugin scan state without blocking workspace restore.

### Lumetri Color and Scopes

- Lumetri sections mirror the parameter record, including LUT selection and reset/bypass.
- Scopes include waveform, parade, vectorscope and histogram configurations validated for scaling/color management.
- Scope analysis runs asynchronously and labels the analyzed color domain.

### Audio mixers/meters

- Clip mixer follows clip automation ownership; Track Mixer exposes track inserts, sends, routing, mute/solo/record and master.
- Faders/meters specify dB scale, peak hold, clipping, channel order and automation mode.
- UI polling never runs DSP or touches the real-time callback with locks.

## 6. Workspaces

The audited installation contains 16 named workspace layouts: All Panels, Assembly, Audio, Captions, Color, Editing, Effects, Essentials, Learning, Libraries, Metalogging, Production, Review, Social, Text Based Editing and Vertical. Installation/build differences must still be recorded; static file presence does not prove menu visibility or behavior.

Observed layouts range from 11 to 34 distinct stored tab IDs and from 7 to 11 split nodes. This variation requires saving complete topology rather than applying a common layout with different visibility flags.

- **UI-WS-001:** A workspace stores layout/window topology, panels and appropriate panel state, not project content.
- **UI-WS-002:** Save As, reset-to-saved, import/export and delete/rename behavior require validation.
- **UI-WS-003:** Switching workspace preserves document selection/playhead unless Premiere evidence shows otherwise.
- **UI-WS-004:** Missing plugin panels become placeholders or are omitted with a recoverable diagnostic.
- **UI-WS-005:** Workspace schema is versioned and migrated transactionally; corrupted layout falls back without losing the saved file.
- **UI-WS-006:** XML containment, panel type identity, panel instance identity and extension entrypoint identity remain distinct in the canonical layout model.
- **UI-WS-007:** An unavailable extension-backed panel restores as a recoverable placeholder carrying opaque state; it is not silently discarded from the saved workspace.

## 7. Visual system

The interface uses an original design system with dark hierarchy and configurable accent, never copied Adobe assets. Tokens cover surface levels, text emphasis, focus/accent, selection, warnings, clip labels, spacing, radii, stroke, typography, icon size and animation duration.

Minimum acceptance:

- WCAG-oriented contrast review for text/focus/state;
- 100-300% UI scaling and mixed-DPI screenshots;
- color-blind-safe state cues not dependent only on hue;
- keyboard access to all non-canvas controls and named accessibility nodes;
- reduced-motion and high-contrast modes;
- original SVG icon set with documented license.

## 8. Rust UI feasibility gate

Before choosing a toolkit, build an isolated disposable prototype outside production crates and measure:

1. nested docking and tab grouping;
2. tear-off/redock across two mixed-DPI monitors;
3. workspace serialize/restore after topology change;
4. IME/text/numeric editing and full shortcut routing;
5. screen-reader tree and keyboard-only navigation;
6. 10,000-item virtual timeline at 60 Hz;
7. direct display of the selected `wgpu` texture/device;
8. native menus/dialogs, clipboard and drag/drop;
9. device loss and window recreation.

`egui`/`egui_dock` is a candidate, not a decision. If it fails mandatory accessibility, text or docking tests, evaluate another Rust toolkit. Qt 6 remains a fallback through a narrow bridge only after license and QRhi stability review.

## 9. Public references

- [Adobe: Dock, group, and undock panels](https://helpx.adobe.com/premiere/desktop/get-started/tour-the-workspace/dock-group-undock-panels.html)
- [Adobe: Customize panels](https://helpx.adobe.com/premiere/desktop/get-started/tour-the-workspace/customize-panels.html)
- [Adobe: Navigate panels with the keyboard](https://helpx.adobe.com/premiere/desktop/get-started/set-up-accessibility-features/navigate-premiere-using-the-keyboard.html)
- [egui multi-viewport documentation](https://docs.rs/egui/latest/egui/index.html#viewports)
- [egui_dock documentation](https://docs.rs/egui_dock/latest/egui_dock/)
