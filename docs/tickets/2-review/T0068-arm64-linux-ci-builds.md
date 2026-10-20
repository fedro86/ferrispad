---
id: T0068
title: Build and sign ARM64 Linux releases in CI
status: review
created: 2026-10-20
severity: ~
area: process
depends-on: [T0067]
issue: 123
---

## Goal

Linux on ARM64 (Raspberry Pi, Armbian, ARM laptops and servers) has no
prebuilt FerrisPad. Users build from source (issue #29). Publish ARM64 Linux
builds from the release workflow, signed like the x86_64 ones, so ARM users
can download, install and self-update.

## In scope

- **`.github/workflows/release.yml`.** Build Linux on the GitHub-hosted
  `ubuntu-24.04-arm` runner, which is free for public repos, alongside the
  x86_64 job. A matrix or a second job produces:
  - `FerrisPad-v<ver>-linux-arm64`, the raw binary;
  - `FerrisPad-v<ver>-ubuntu-arm64.deb`, via `cargo deb`.
- **`sign-binaries`.** Sign the ARM64 binary with platform id `linux-arm64`,
  and attach the `.sig`.
- **Updater.** Map aarch64 Linux to `linux-arm64`. This builds on T0067's
  architecture-aware asset name.
- **Docs.** `RELEASE_PROCESS.md` and `.claude/CLAUDE.md`: the asset list, the
  `linux-arm64` platform id, and the four-`.sig` check. The user-facing docs
  (README, website, wiki) switch from "build from source" to ARM64 downloads
  in the release that first ships them, not before.

## Out of scope

- An aarch64 FlatPark package. That is a follow-up once the asset exists: add
  an `only-arches: [aarch64]` extra-data source, a `build.arches` entry, and an
  arch-aware `resolve-update.sh`.
- Windows on ARM.

## What was done

- **`release.yml`, build.** `build-linux` is a matrix:
  - `ubuntu-latest` produces amd64 assets;
  - `ubuntu-24.04-arm` produces arm64 assets.
  Both build the raw binary `FerrisPad-v<ver>-linux-<arch>` and the `.deb`
  `FerrisPad-v<ver>-ubuntu-<arch>.deb`, in artifacts `linux-bin-<arch>` and
  `linux-deb-<arch>`. `fail-fast: false`, so one architecture failing does not
  cancel the other.
- **`release.yml`, signing.** `sign-binaries` downloads `linux-bin-*` (merged;
  `continue-on-error` so a missing leg does not stop the other signatures),
  then signs `linux-amd64` and `linux-arm64` with their platform ids.
- **`release.yml`, publishing.** `create-release` attaches
  `artifacts/linux-deb-*/*` and `artifacts/linux-bin-*/*`.
- **`ci.yml`.** A new `test-arm64` job runs `cargo test --locked` on
  `ubuntu-24.04-arm` on every PR and push to master, so an ARM build break
  shows up before a tag.
- **Updater.** `asset_name_for("linux", "aarch64")` is now `linux-arm64`, and
  the tests are updated. The asset lookup matches on the name's **suffix**
  (`ends_with`) instead of `contains`: `contains("linux-arm64")` also matches
  `….linux-arm64.sig`, which only worked because GitHub lists assets
  alphabetically.
- **Lint.** `actionlint` is clean on both workflows. Its shellcheck notes are
  the same 9 as on master.

## How to test

1. The PR's CI runs `test (linux arm64)` on the ARM runner and passes. This
   proves FerrisPad builds and its tests pass on aarch64 Linux.
2. At the next release tag, expect `FerrisPad-v<ver>-linux-arm64`,
   `FerrisPad-v<ver>-ubuntu-arm64.deb` and `FerrisPad-v<ver>-linux-arm64.sig`,
   next to the unchanged amd64 assets.
3. On an ARM64 machine or VM, or with `qemu-user` on x86:
   `file FerrisPad-v…-linux-arm64` reports aarch64, and the binary starts.
4. An ARM64 install of the previous version sees the update and installs the
   ARM64 binary.

## Acceptance criteria

- [ ] Every release carries signed ARM64 Linux assets.
- [ ] The updater on aarch64 Linux uses them.
- [ ] Docs, site and wiki list ARM64 downloads instead of "build from source".

## Notes

- Origin: user, 2026-10-20 ("per il momento gli diciamo solo di compilare… poi
  magari lo metteremo fra le compilazioni in CI").
- Check that the FLTK build deps (README Debian list) install on the ARM
  runner image, as #29 showed they do on Debian trixie arm64.
