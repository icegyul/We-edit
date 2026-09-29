# Keyboard shortcut and command-dispatch specification

**Status:** English default inventory plus localized/static physical-layout comparison extracted; runtime behavior only partially validated.<br>
**Normative dataset:** [premiere_shortcuts_default.json](evidence/generated/premiere_shortcuts_default.json) (370 entries).<br>
**Source:** EV-KBD-001, English Premiere Pro 2024 default `.kys` data.

## 1. Scope and caveats

The committed matrix records `context`, internal command identifier and key combination. It is a factual inventory from one installation, not proof that each command is active in every workspace or that macOS/localized physical-key mappings are identical.

The broader [keyboard-locale inventory](evidence/generated/premiere_keyboard_locales.md) compares 11 localized Premiere Defaults files and 10 physical-layout fragments. The English map is a 370-pair superset of the localized files in this installation, while raw bindings vary materially by locale. Parity therefore requires a versioned map per locale/layout, not one English table rendered with localized labels.

- **KBD-001:** Bindings are stored against logical commands, not hard-coded directly into tools.
- **KBD-002:** Application/global and panel contexts are distinct.
- **KBD-003:** When the focused panel has a binding for a key, that panel binding takes precedence over an application binding; this is documented publicly by Adobe but still needs 2024 black-box confirmation.
- **KBD-004:** User changes create a named custom map without mutating the shipped default.
- **KBD-005:** A command may have multiple bindings; conflicts and intentionally unbound commands are representable.
- **KBD-006:** Key serialization separates physical key, produced character, modifiers, numpad identity and operating-system reservation.
- **KBD-007:** Text/number entry, menus, modal dialogs, drag gestures and IME composition may suppress editor commands according to a validated focus policy.
- **KBD-008:** Stable command identity is `(context, command identifier)`. Serialized virtual key, produced character, physical position and localized display label are separate fields.
- **KBD-009:** `.ksvlayout` is parsed as a bounded XML fragment and source anomalies are preserved with diagnostics.

## 2. Core compatibility bindings (Windows)

| Area | Key | Command/behavior |
|---|---|---|
| Transport | `J` | Shuttle reverse; repeated press increases reverse speed |
| Transport | `K` | Shuttle stop; chord modifier for slow/frame stepping |
| Transport | `L` | Shuttle forward; repeated press increases forward speed |
| Transport | `Space` | Play/stop toggle in active monitor/timeline context |
| Navigation | `Left` / `Right` | Step one frame backward/forward |
| Marks | `I` / `O` | Set In / Out |
| Timeline | `S` | Toggle snapping |
| Tools | `V` | Selection |
| Tools | `A` | Track Select Forward; reverse variant remains dataset/context dependent |
| Tools | `B` | Ripple Edit |
| Tools | `N` | Rolling Edit |
| Tools | `R` | Rate Stretch |
| Tools | `C` | Razor |
| Tools | `Y` | Slip |
| Tools | `U` | Slide |
| Tools | `P` | Pen |
| Tools | `H` | Hand |
| Tools | `Z` | Zoom |
| Edit | `,` | Insert |
| Edit | `.` | Overwrite (`overlay` in internal command name) |
| Edit | `Delete` | Clear selection |
| Edit | `Shift+Delete` | Ripple Delete |
| Edit | `Ctrl+K` | Add Edit |
| Edit | `Ctrl+Shift+K` | Add Edit to all tracks |
| Transition | `Ctrl+D` | Apply default video transition |
| Transition | `Ctrl+Shift+D` | Apply default audio transition |
| Panels | `Shift+1` | Project panel |
| Panels | `Shift+5` | Effect Controls |
| Panels | `Shift+6` | Audio Clip Mixer |
| Panels | `Shift+7` | Effects |
| Panels | `Shift+8` | Media Browser |
| Configuration | `Ctrl+Alt+K` | Keyboard Shortcuts dialog |

The full dataset, rather than this convenience subset, is the inventory oracle.

## 3. J-K-L state machine to validate

| Input from stopped state | Candidate observable result | Evidence |
|---|---|---|
| `L` | forward at 1x | Adobe public documentation |
| repeated `L` | progressively faster forward shuttle | Adobe public documentation; exact speed ladder open |
| `J` | reverse at 1x | Adobe public documentation |
| repeated `J` | progressively faster reverse shuttle | Exact speed ladder open |
| `K` | stop | Adobe public documentation |
| hold `K+L` | slow forward | Adobe public documentation; exact rate/audio policy open |
| hold `K+J` | slow reverse | Adobe public documentation; exact rate/audio policy open |
| hold `K`, tap `L`/`J` | step one frame | Historical/public documentation; 2024 verification required |
| opposite shuttle key while moving | reduce speed, cross zero, then reverse | Hypothesis requiring test |

KBD-X-001 records playhead movement, rendered frame sequence, audio output, speed indicator, key-repeat behavior and stop position for every transition in the state machine.

## 4. Dispatch model

The candidate dispatch order is:

1. Operating system reserves or delivers the event.
2. Active modal/popup/menu/IME consumes it if applicable.
3. Focused editable control handles text/navigation commands.
4. Focused panel command map is queried.
5. Application command map is queried.
6. Tool transient modifiers and drag-state handling are applied.
7. The command validates enablement and executes as one transaction or produces no mutation.

Steps 2-6 remain a hypothesis until tested. The UI must expose the resolved command and conflict in the shortcut editor rather than relying on undocumented toolkit behavior.

## 5. Required test dimensions

- Premiere 24.0 and latest 24.x patch on Windows; macOS map separately.
- US English, Spanish and Japanese keyboard layouts; dead keys and AltGr.
- Focus in Project, Source Monitor, Program Monitor, Timeline, Effect Controls, Effects, text field, numeric field and modal dialog.
- Key down/up, auto-repeat, simultaneous chord order, sticky modifier, numpad and function keys.
- Custom assignment, duplicate assignment, panel-over-global conflict, unbind, reset and named preset round-trip.
- Playback active/stopped, drag active, trim mode and text edit mode.

## 6. Public references

- [Adobe: About keyboard shortcuts](https://helpx.adobe.com/premiere/desktop/get-started/keyboard-shortcuts/about-keyboard-shortcuts.html)
- [Adobe: Navigate Premiere using the keyboard](https://helpx.adobe.com/premiere/desktop/get-started/set-up-accessibility-features/navigate-premiere-using-the-keyboard.html)
- [Adobe: Play an active sequence and J-K-L transport](https://helpx.adobe.com/premiere/desktop/render-and-export/render-sequences-for-playback/play-active-sequence-in-program-monitor.html)
