# Derived Premiere workspace inventory

**Source scope:** 16 workspace-layout XML files in the audited installation.<br>
**Evidence level:** E3 for static structure within this installation; not behavioral validation.

No source XML or embedded panel-state payload is reproduced here.

## Workspace topology counts

| Workspace | Distinct tab IDs | Split nodes | Frame nodes | Frame proxies |
|---|---:|---:|---:|---:|
| All Panels | 34 | 8 | 7 | 1 |
| Assembly | 14 | 7 | 5 | 2 |
| Audio | 17 | 7 | 7 | 0 |
| Captions | 18 | 8 | 7 | 1 |
| Color | 17 | 8 | 7 | 1 |
| Editing | 19 | 8 | 6 | 2 |
| Effects | 19 | 8 | 7 | 1 |
| Essentials | 17 | 7 | 6 | 1 |
| Learning | 11 | 11 | 7 | 3 |
| Libraries | 11 | 8 | 6 | 2 |
| Metalogging | 15 | 8 | 7 | 1 |
| Production | 19 | 8 | 7 | 1 |
| Review | 29 | 9 | 6 | 3 |
| Social | 17 | 7 | 6 | 1 |
| Text Based Editing | 22 | 11 | 6 | 4 |
| Vertical | 17 | 7 | 6 | 1 |

The differing split/tab counts show that workspaces are distinct persisted layouts, not merely visibility presets.

## Panel presence across 16 workspaces

| Stored panel ID/family | Workspace count |
|---|---:|
| `MiniAudioMixer` | 16 |
| `Program Monitor` | 16 |
| `Source Monitor` | 16 |
| `Tools` | 16 |
| project-panel instance family | 16 |
| `Timeline` | 15 |
| Libraries CEP panel | 15 |
| `Effect Controls` | 13 |
| `Effects` | 13 |
| `Media Browser` | 13 |
| `Color` | 12 |
| `Essential Sound` | 12 |
| `Graphics` | 12 |
| `MarkerList` | 12 |
| `AudioClipMixer` | 11 |
| `History` | 10 |
| `Info` | 10 |
| `PrProductionFolder` | 10 |
| `MetadataEditor` | 5 |
| `Scopes` | 5 |
| Frame.io panel | 5 |
| `AudioMixer` | 3 |
| `Captions` | 2 |
| `Events` | 2 |
| `Progress` | 2 |
| `Capture`, `EditToTape`, `ReferenceMonitor`, `Timecode`, Titler families | 1 each |

Project-panel IDs include per-project UUID-like instance state and must be normalized to a panel family only for catalog reporting; the original instance ID must be preserved on round-trip.

## Persisted vocabulary

Observed structural keys include:

- workspace version and monitor information;
- top-level frame, regular frame and frame proxy;
- nested splitter with orientation and placement;
- first/second child branches;
- ordered `TabIDs` and `CurrTab` index;
- visibility;
- frame reanimation/floating state;
- saved size and position;
- escaped per-panel state, including UXP plugin ID/version and entrypoint.

## Design requirements derived from the evidence

- `UI-WS-DATA-001`: save native-window forest, monitor association and geometry separately from project content.
- `UI-WS-DATA-002`: represent each window as a binary split tree whose leaves are ordered tab groups.
- `UI-WS-DATA-003`: give every panel both a stable type ID and an instance ID.
- `UI-WS-DATA-004`: preserve opaque, versioned state for unavailable native/UXP/CEP-equivalent panels.
- `UI-WS-DATA-005`: validate tab index, split ratios, geometry and monitor references before restore; recover without overwriting the saved layout.
- `UI-WS-DATA-006`: never treat source workspace geometry as a portable pixel-perfect contract across monitor/DPI topologies.
