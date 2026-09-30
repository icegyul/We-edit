# Release process

OpenPremier 0.x releases are explicitly **technical-alpha prereleases**. Publishing a build
does not validate Premiere feature, project, pixel, performance, or workflow parity.

## Prerequisites

1. `CHANGELOG.md` describes user-visible changes.
2. `cargo xtask lint` and `cargo xtask test` pass.
3. `cargo xtask dist` passes both packaged self-tests locally.
4. `Cargo.toml` contains the intended semantic version and `Cargo.lock` is current.
5. Clean-room, dependency-license, and security-sensitive changes have been reviewed.

## Publishing

Create and push an annotated tag matching the workspace version:

```text
git tag -a v0.4.0 -m "OpenPremier 0.4.0"
git push origin v0.4.0
```

The release workflow verifies the tag/version match, builds the portable Windows ZIP, builds the
AppImage in manylinux_2_28, runs package startup checks, generates `SHA256SUMS.txt`, and publishes a
GitHub prerelease with generated notes. A failed job must be fixed and rerun; never upload an
untested replacement manually under the same filename.

## Repository setup checklist

- Enable private vulnerability reporting and branch protection for the default branch.
- Require the Windows and Linux CI checks before merging.
- Disable force-push and branch deletion on the default branch.
- Keep workflow permissions read-only by default; only the publish job receives `contents: write`.
- Optionally enable immutable releases after the release workflow has been exercised successfully.
