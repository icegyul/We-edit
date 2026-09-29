# Clean-room interoperability policy

OpenPremier documents and reimplements observable behavior and interoperability formats without using Adobe source code or protected implementation assets. This policy is not legal advice.

## Allowed research

- Use a licensed official copy of Premiere Pro and observe user-visible behavior.
- Read public documentation and standards.
- Create minimal projects and media, save/export them, and compare the resulting data and pixels.
- Inspect data files created by the researcher: `.prproj`, `.kys`, presets, interchange XML/JSON, logs, and exported media.
- Record minimal functional facts such as names, identifiers, parameter ranges, object references, timings, and state transitions.
- Measure outputs with original test patterns and publish derived statistics or tolerances.

## Prohibited research

- Disassemble, decompile, debug, patch, instrument, or inspect the memory of Adobe executables, libraries, or plugins.
- Circumvent access controls, signatures, licensing, encryption, or copy protection.
- Copy Adobe code, shaders, icons, images, sounds, LUTs, Looks, templates, help text, translations, or UI assets.
- Commit Adobe factory projects, presets, keyboard-layout files, binaries, SDK material with redistribution restrictions, or third-party projects.
- Claim exact compatibility where the evidence is partial.

## Contribution protocol

Every compatibility claim identifies its source, Premiere build, operating system, locale, fixture hash, evidence level, and acceptance test. Private fixtures remain private. The committed specification is the only bridge from observation to later implementation.

SDKs or plugin headers with special terms require a separate license review before use. Public documentation may describe behavior without importing restricted SDK material into the repository.
