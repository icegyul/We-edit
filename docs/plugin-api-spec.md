# Plugin and scripting API specification

**Status:** draft. OpenFX behavior is based on the public 1.5 reference; host capabilities and scripting IDL are not validated. No plugin loader is authorized.

## 1. Goals and non-goals

OpenPremier needs three distinct extension surfaces:

1. **OpenFX image-effect host** for industry-standard compiled video effects.
2. **Audio plugin host** for a separately licensed/validated standard such as VST3 and platform-specific formats where permitted.
3. **Sandboxed automation API** using WebAssembly as the canonical portable runtime.

These are separate APIs. OpenFX is not an audio API, and scripting plugins do not receive native render callbacks. Compatibility with Adobe UXP/ExtendScript source code is not promised; only selected workflow concepts may be mapped.

## 2. Common extension principles

- Stable IDs and semantic versioning for every host API and capability.
- Least privilege: no filesystem, network, process, clipboard, device or project-write access without declared capability and user grant.
- Project mutations occur through transactions and validated commands.
- No extension receives Rust references, C++ objects, UI widget pointers or unrestricted GPU device handles.
- Time, memory, recursion, thread, I/O and output-size budgets are enforced.
- Extension identity, version, binary hash, granted capabilities and migration state are recorded.
- Unknown/missing extensions preserve project parameters and report unavailable rendering rather than deleting data.
- Headless operation is first-class; UI is optional and separated from processing.

## 3. OpenFX host profile

### 3.1 ABI boundary

OpenFX plugins expose a C ABI entry surface. A small `op-openfx-sys` boundary would own raw pointers, null checks, string lifetime, property-array conversion, panic containment and suite tables. Safe Rust code sees validated handles and typed property access.

Third-party native plugins remain unsafe code in the process even when the host is Rust. The default architecture target is an out-of-process worker per trust/vendor group. In-process loading, if offered, is an explicit performance/trust option.

### 3.2 Lifecycle state machine

The host must validate at least this sequence against the standard and conformance plugins:

```text
discover module
  -> enumerate plugin entries
  -> setHost
  -> load
  -> describe
  -> describeInContext(context)
  -> createInstance
  -> instanceChanged / begin-end change as applicable
  -> beginSequenceRender
  -> render (possibly many frames/tiles/threads)
  -> endSequenceRender
  -> destroyInstance
  -> unload
```

Every action's permitted thread, handle lifetime, property set, status codes and reentrancy are documented before implementation. Invalid action order is rejected without calling arbitrary plugin code.

### 3.3 Target contexts

Initial conformance target:

- Filter
- General
- Generator
- Transition

Paint and Retime contexts remain mandatory research items because they interact with masks/time mapping. Context support is advertised truthfully per plugin and host version.

### 3.4 Suites to map

| Suite | compatibility requirement |
|---|---|
| Property | Typed dimensions, mutability, lifetime and bounds for every property used |
| Image Effect | Clip definition, instance, image fetch/release, render window, project/format properties |
| Parameter | All supported scalar, point, color, choice, string, group/page/custom parameter types and animation |
| Memory | Allocation ownership, failure, limits and cross-process strategy |
| Multi-thread | Honest CPU/thread counts, cancellation and thread-safety contract |
| Message | Structured logging and user-visible message policy without modal calls from render threads |
| Progress | Cancellation and progress semantics |
| Timeline | Time/range behavior and headless availability |
| Interact | Optional parameter UI overlay boundary; accessibility/fallback required |
| OpenGL/GPU | Disabled until device/context sharing and isolation are validated |

The supported suite/version table is a compatibility contract. Returning a suite and then only partially implementing it is forbidden.

### 3.5 Image contract

The host profile must specify supported components, bit depths, row orientation/stride, bounds/region of definition, render window, pixel aspect, premultiplication, fielding, frame rate, frame range, temporal access, tiling, multi-resolution, render scale and identity optimization.

Conversion is explicit. A plugin requesting an unsupported format receives the standard failure path rather than an undocumented conversion. CPU buffers are guarded and bounded. GPU extension suites remain off until their interoperability can be tested across Vulkan/D3D12/Metal backends.

### 3.6 Render safety

- Worker process receives immutable render request and bounded shared surfaces.
- Watchdog applies deadline; cancellation is cooperative first, process termination last.
- A crash marks that plugin binary/instance failed and returns a diagnostic frame/error; the editor survives.
- Deterministic export records plugin/version/hash and host profile.
- Render cache key includes all declared parameters, source identities, time, render scale, color metadata and plugin binary identity.
- Plugin file/network/process access follows OS sandbox policy where achievable and is disclosed otherwise.

## 4. Audio plugin profile

Audio hosting is specified independently with sample format/layout, block sizes, sample rate, transport/time info, automation resolution, state serialization, latency, tail, bypass, offline rendering and UI separation. VST3 or platform SDK headers/binaries enter the repository only after GATE-LIC-001.

Plugins run outside the real-time device callback where isolation requires IPC; buffering/latency is compensated and measured. A plugin that cannot meet real-time constraints may be restricted to offline rendering.

## 5. WebAssembly automation model

### 5.1 Package manifest

Each package declares:

```text
id, name, vendor, version
minimum/maximum host API
entrypoints: command | importer | exporter | panel-model | background-job
capabilities
memory/fuel/deadline limits
configuration/state schema versions
signature/hash metadata
```

No capability is granted because a plugin name is trusted. Grants are stored per plugin identity and scope.

### 5.2 Capability examples

- `project.read`
- `project.write`
- `media.metadata.read`
- `media.decode.request`
- `export.submit`
- `filesystem.open-user-selected`
- `filesystem.read-project-relative`
- `network.connect:<origin>`
- `ui.panel-model`
- `clipboard.read` / `clipboard.write`

Broad arbitrary filesystem and unrestricted process execution are not baseline capabilities.

### 5.3 Versioned API modules

| Module | Representative operations |
|---|---|
| `host` | Version/capability query, localized diagnostics, cancellation |
| `project` | Snapshot/read entities, begin transaction, validate/commit/abort |
| `timeline` | Query tracks/items, submit semantic edit commands, markers |
| `media` | Query descriptors, request user-mediated import/relink, thumbnails |
| `effects` | Query catalog and parameter schemas, add/configure known components |
| `export` | Describe presets, submit/cancel jobs, progress events |
| `storage` | Plugin-scoped key/value and user-selected file handles |
| `ui` | Declarative panel view-model and commands, no native widget pointers |
| `events` | Versioned subscriptions with bounded queues and coalescing |

Entity handles include project revision and become stale after invalidating mutations. Transactions use optimistic revision checks. Scripts cannot retain direct mutable access.

### 5.4 Determinism and async behavior

Calls that can block return jobs/futures. Event callbacks are bounded and non-reentrant by default. Time, randomness and locale are explicit host services so tests can be deterministic. Headless scripts may not depend on panel creation.

The host serializes plugin state as versioned plugin-owned bytes plus a declared schema/version, with a size limit. Migration runs in a restricted context and can fail without corrupting the project.

## 6. JavaScript compatibility option

QuickJS may be embedded behind the same capability/transaction API for easier scripting, but WebAssembly remains canonical. JavaScript does not receive hidden APIs or wider authority. ExtendScript's ECMAScript 3 surface and Adobe object model are not copied wholesale; a migration shim would require a separately scoped compatibility matrix.

Adobe's current UXP documentation distinguishes UXP core APIs from Premiere DOM APIs and exposes projects, sequences, tracks, clips, markers, effects and export concepts. Those concepts inform workflow coverage, not binary/source compatibility.

### 6.1 Installed manifest observations

The audited installation contains ten Premiere-relevant UXP manifests plus one nested Photoshop-only manifest. At least three declarative shapes coexist: legacy `uiEntryPoints`, manifest-v5 `entrypoints`, and case-variant `entryPoints`. Observed entrypoint kinds include panel, command, home/welcome/picker and popover. Premiere-related IDs include text, importer, export settings/queue, quick export, progress, preset manager, cloud-media storage and creative-copilot surfaces.

Eight CEP manifest files separately declare extension-bundle IDs, extension IDs and host/version ranges. Workspace files persist both UXP and CEP-backed panel identities.

These observations establish migration requirements only:

- normalize imported manifest keys without treating their case variants as interchangeable on export;
- separate plugin ID, entrypoint ID/type, host compatibility, manifest-schema version and plugin version;
- preserve unavailable external-panel identity/state in workspace data;
- never execute a package while scanning its manifest;
- do not promise source or runtime compatibility with UXP/CEP.

Only manifests were inspected; JavaScript/HTML implementation bodies were excluded from the audit.

## 7. UI extensions

Plugin UI uses a versioned declarative model rendered by the host design system. Required controls include text, number, checkbox, enum, color, file picker, list/tree/table, progress and custom image/canvas with accessible fallback. HTML/CSS/browser compatibility is not a baseline promise.

Custom native OpenFX interacts or audio-editor windows are isolated from workspace layout and must provide a generic parameter fallback. A plugin cannot spoof host permission dialogs or capture global shortcuts without explicit registration.

## 8. Discovery, trust and updates

- Search paths are explicit and user-configurable; discovery never executes plugin render code in the editor process.
- Scanner workers collect identity/capabilities and cache by binary hash.
- Quarantine tracks crashes/timeouts; reset is user-controlled.
- Duplicate IDs follow a deterministic precedence rule and are visible in diagnostics.
- Plugin updates do not migrate project state until successfully validated; rollback retains the previous binary/state association where licensing permits.

## 9. Conformance suite required before implementation gate closes

OpenFX tests include: minimal filter/generator/transition, every advertised parameter type, animation, tiles, temporal clip access, render scale, multi-thread stress, cancellation, memory failure, malformed properties, wrong lifecycle calls, crash, hang and missing dependency.

Wasm tests include: capability denial/grant, stale handle, transaction conflict, fuel/memory/deadline exhaustion, event flood/coalescing, deterministic replay, state migration, malformed package and headless execution.

Security tests treat project/plugin files as hostile. Fuzz parsers and ABI property conversion separately from actual third-party binaries.

## 10. Public references

- [OpenFX Generic Core API](https://openfx.readthedocs.io/en/main/Reference/ofxCoreAPI.html)
- [OpenFX documentation](https://openfx.readthedocs.io/)
- [Adobe Premiere UXP overview](https://developer.adobe.com/premiere-pro/uxp/)
- [Adobe: UXP core APIs versus Premiere DOM APIs](https://developer.adobe.com/premiere-pro/uxp/resources/fundamentals/apis/)
