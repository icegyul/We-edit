# Contributing to OpenPremier

OpenPremier accepts implementation, tests, documentation, original assets, and clean-room
compatibility evidence. Version 0.1.x is a technical alpha: contributions must describe what is
implemented without claiming unverified Premiere parity.

## Development setup

Install Git and Rust 1.98.1 or newer, clone the repository, and run:

```text
cargo xtask build --release
cargo xtask test
cargo xtask lint
```

The task runner downloads checksum-verified FFmpeg development files and libclang where needed.
Direct `cargo` commands may fail to find those native dependencies; use `cargo xtask cargo -- ...`
when a custom Cargo invocation is required.

## Pull requests

Keep each change focused. Add tests for behavior, update the relevant specification/requirement ID,
and run lint plus the full test suite. Packaging changes must also pass the final-layout
`--self-test`. User-facing changes belong under `Unreleased` in `CHANGELOG.md`.

Do not commit build output, downloaded dependencies, private media, local tool configuration,
credentials, crash dumps, or personal notes.

## Clean-room compatibility work

Never copy Adobe code, binaries, artwork, UI text, icons, LUTs, templates, translations, or other
copyrighted assets. Use public documentation and controlled observations with original synthetic
fixtures. Record exact versions, environment, steps, output, hashes, and uncertainty. Private
Adobe-generated files may be local oracles but must not be committed.

Writable `.prproj` support stays disabled until the round-trip requirements in
`docs/compatibility-validation.md` are validated.
