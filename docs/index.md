# Technical documentation

OpenPremier 0.2.0 is an early alpha with working builds for Windows and Linux. This documentation records the implemented architecture, interoperability research and the tests still required before broader compatibility claims can be made.

## Project documentation

| Document | Purpose |
|---|---|
| [implementation-status.md](implementation-status.md) | Implemented modules, verified checks and known limits |
| [release-process.md](release-process.md) | Package and release procedure |
| [compatibility-validation.md](compatibility-validation.md) | Compatibility evidence and remaining validation work |
| [architecture.md](architecture.md) | Rust subsystem boundaries and dependency direction |
| [data-model.md](data-model.md) | Project, timeline, effect and audio data model |
| [prproj-spec.md](prproj-spec.md) | Observed `.prproj` container and XML structure |
| [keyboard-shortcuts.md](keyboard-shortcuts.md) | Keyboard command map and dispatch rules |
| [timeline-behavior.md](timeline-behavior.md) | Editing, trimming, snapping, links and history |
| [effects-catalog.md](effects-catalog.md) | Effect, transition and audio inventory |
| [panel-system-spec.md](panel-system-spec.md) | Panels, docking, workspaces and multi-monitor behavior |
| [plugin-api-spec.md](plugin-api-spec.md) | OpenFX and WebAssembly extension design |
| [premiere-2024-static-data-audit.md](premiere-2024-static-data-audit.md) | Clean-room static observations |
| [compatibility-research.md](compatibility-research.md) | Open questions and controlled-fixture plan |

## Evidence

The [evidence manifest](evidence/manifest.md) records the source and scope of each observation. Only derived technical facts are committed under `evidence/generated/`; proprietary Adobe programs and assets are not included.

Generated indexes cover keyboard bindings, plugin names, effect layouts, `.prproj` classes, workspace topology and preset schemas. When an observation is incomplete or disputed, it remains documented as such until a reproducible test resolves it.
