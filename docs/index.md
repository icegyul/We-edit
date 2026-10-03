# Technical documentation

OpenPremier 0.7.0 is an early alpha with working builds for Windows, Linux and macOS. This documentation records the plan, the implemented architecture and how it is tested.

## Project documentation

| Document | Purpose |
|---|---|
| [roadmap.md](roadmap.md) | What editors can do today and what comes next, in order |
| [implementation-status.md](implementation-status.md) | Implemented modules, verified checks and known limits |
| [drift-comparison.md](drift-comparison.md) | Review of the Drift editor's rendering and features, and what we adopted |
| [release-process.md](release-process.md) | Package and release procedure |
| [architecture.md](architecture.md) | Rust subsystem boundaries and dependency direction |
| [data-model.md](data-model.md) | Project, timeline, effect and audio data model |
| [keyboard-shortcuts.md](keyboard-shortcuts.md) | Default shortcuts and dispatch rules |
| [timeline-behavior.md](timeline-behavior.md) | Editing, trimming, snapping, links and history |
| [effects.md](effects.md) | Effects, transitions, audio effects and graphics |
| [assistants.md](assistants.md) | Editing with AI assistants (Model Context Protocol) |
| [performance.md](performance.md) | Heavy-load benchmark results and what they mean for smaller computers |
| [plugin-api-spec.md](plugin-api-spec.md) | OpenFX and WebAssembly extension design |

## Clean-room policy

OpenPremier is developed independently and contains no code, assets or data taken from Adobe
products; see [legal/clean-room.md](legal/clean-room.md). Notes from studying other products are
kept outside this repository.
