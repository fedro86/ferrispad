---
id: T0067
title: The updater offers the x86_64 binary on ARM64 Linux
status: review
created: 2026-10-20
severity: high
area: updater
depends-on: []
issue: 121
---

## Goal

`updater::get_platform_asset_name()` (`src/app/services/updater.rs`) picks the
release asset by operating system only. On any Linux it returns
`linux-amd64`.

FerrisPad built from source on ARM64 Linux (the documented path for ARM, see
issue #29) therefore:
1. sees the next release as an available update;
2. downloads `FerrisPad-v<ver>-linux-amd64`;
3. accepts its valid ed25519 signature, because the binary is genuine, but for
   x86_64;
4. replaces itself with it.

After the restart FerrisPad no longer starts (`Exec format error`). The same
applies to any non-x86_64 Linux and to Windows on ARM built from source.

Updates must only ever install a binary for the running architecture. When
none is published, there is no self-update: say so and link the release page.

## In scope

- **Red test first.** A unit test on the asset-name logic, taking OS and
  architecture as parameters (a pure function), showing that
  `("linux", "aarch64")` yields `linux-amd64` today. The fix makes it yield
  "no asset" (or `linux-arm64` once T0068 publishes one).
- **Fix.** Make the asset name depend on `std::env::consts::ARCH`:
  - x86_64 Linux: `linux-amd64`;
  - macOS (any arch): `macos-universal`;
  - x86_64 Windows: `windows-x64.exe`;
  - anything else: no self-update.
  The update dialog then shows the release link instead of the install button.
- Check both callers: `services/updater.rs` (download) and
  `ui/dialogs/update.rs` (asset lookup).

## Out of scope

- Publishing ARM64 builds (T0068).
- `.deb` installs, where self-replace fails because `/usr/bin` is not
  writable. That is a separate issue.

## What was done

- **Pure helper.** `services/updater.rs` gained `asset_name_for(os, arch) ->
  Option<&'static str>`, fed with `std::env::consts::{OS, ARCH}`:
  - any macOS: `macos-universal`;
  - x86_64 Windows: `windows-x64.exe`;
  - x86_64 Linux: `linux-amd64`;
  - anything else: `None`.
- **`get_platform_asset_name()`** now returns `Option`, with two callers:
  - `download_and_verify` fails with "No release binary is published for
    <os> <arch>" instead of downloading another architecture's file;
  - the Update Available dialog (`ui/dialogs/update.rs`) finds no direct
    asset. It already handles that case: the install button is disabled and
    reads "Manual Update Only", and the release link stays.
- **Red / green.**
  - Step 1 extracted `asset_name_for` with the old behaviour (architecture
    ignored, always `Some`).
  - `asset_name_never_offers_another_architectures_binary` then failed:
    `left: Some("linux-amd64") right: None` for `("linux", "aarch64")`.
  - With the fix, it passes. `asset_name_for_published_platforms` pins the
    three published assets.

## How to test

1. `nix develop -c cargo test updater`. The new test fails on master
   (`aarch64` maps to `linux-amd64`) and passes with the fix.
2. The gates pass.

## Acceptance criteria

- [ ] No architecture ever gets another architecture's binary.
- [ ] Unsupported architectures get a clear message and the release link, not
      a broken install.
- [ ] A regression test that failed before the fix.

## Notes

- Found 2026-10-20 while adding the "ARM64: build from source" notes to the
  README, website and wiki.
- Who is exposed: anyone running a from-source build on a non-x86_64 Linux or
  on Windows. The fix ships in a release, so builds made before it keep the
  bug until rebuilt.
