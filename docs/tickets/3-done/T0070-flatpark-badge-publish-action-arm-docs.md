---
id: T0070
title: FlatPark badge in README, publish-action, ARM64 downloads in docs
status: done
created: 2026-10-20
severity: ~
area: process
depends-on: [T0069]
issue: 127
---

## Goal

Three follow-ups now that FerrisPad is on FlatPark and 0.9.8 ships ARM64
builds:
- **Badge.** Show the official "Get it on FlatPark" badge in the README, as the
  website already does (fedro86/fedro-websites#4).
- **Faster Flatpak updates.** FlatPark's update check runs twice a day.
  [`flatpark/publish-action`](https://github.com/flatpark/publish-action)
  notifies FlatPark when a release is published, so the Flatpak updates within
  minutes.
- **ARM64 downloads.** The README still tells ARM64 Linux users to build from
  source. 0.9.8 is the first release with ARM64 assets, so point them at the
  downloads.

## In scope

- `README.md`: the badge above the FlatPark commands; ARM64 rows for the
  `ubuntu-arm64.deb` and the `linux-arm64` binary.
- `.github/workflows/release.yml`: a `flatpark` job after `create-release`.
  - Why after `create-release`: the release is created by `GITHUB_TOKEN`,
    which never triggers an `on: release` workflow (FlatPark's
    `docs/developers/publish-action.md`, "If a workflow creates your
    releases").
  - Its settings:
    - `continue-on-error: true`, so FlatPark can never fail a release;
    - `permissions: {}`, because it needs no token;
    - stable tags only (`!contains(github.ref_name, '-')`), so no
      `-rc`/`-beta` reaches Flatpak users;
    - the action pinned to the v1.0.2 commit
      `b67fca1b80b095541bc011967e0a35292bed14d8`.
  - What the action does: one `curl` POST of `{app_id, tag, repository}` to
    `hooks.flatpark.org/release`, which FlatPark verifies against its catalog
    and the public GitHub release.

## Out of scope

- The website and wiki ARM64 text, which live outside this repo. They are
  updated in the same pass, in fedro-websites and the wiki.
- An aarch64 FlatPark package: a FlatPark-side PR adding an aarch64
  extra-data source.

## What was done

- **README.**
  - The badge sits above the FlatPark commands.
  - The ARM64 "build from source" row is gone: the Debian/Ubuntu and
    any-Linux rows now list `-ubuntu-arm64.deb` and `-linux-arm64` next to
    amd64.
  - The FlatPark row says x86_64, because the FlatPark package is x86_64
    only.
  - New line: the Linux builds need glibc 2.39+ (Ubuntu 24.04, Debian 13,
    Fedora 40 and later). The binaries are built on Ubuntu 24.04, and
    `objdump -T` shows GLIBC_2.39 as the highest symbol version. Debian 12 and
    bookworm-based Raspberry Pi OS must build from source.
- **`release.yml`.** The `flatpark` job, as specified in scope.
- **0.9.8 release check of T0068**, run on 2026-10-20:
  - all release jobs green, both Linux matrix legs included;
  - 12 assets, including `linux-arm64`, `ubuntu-arm64.deb` and
    `linux-arm64.sig`;
  - `file` reports `ELF 64-bit … ARM aarch64`, and `NEEDED` matches the amd64
    build;
  - the `.deb` reads `Architecture: arm64`, depends on `libc6 (>= 2.39)`, and
    ships `/usr/bin/FerrisPad` and the desktop file;
  - the arm64 `.sig` verifies with the updater's own `verify_binary_signature`
    and the embedded public key, through a temporary ignored test that was not
    committed. The same signature is rejected with platform id `linux-amd64`.
- **Outside this repo, same pass.**
  - Website: fedro86/fedro-websites#6, a Linux list with an x86_64/ARM64
    switch.
  - Wiki: the ARM64 notes now point to the downloads.

## How to test

1. `nix shell nixpkgs#actionlint -c actionlint -shellcheck= .github/workflows/release.yml`
   is clean.
2. Read the README on the PR. Expect the badge to render and link to the
   FlatPark app page. Expect every ARM64 file name to exist on the 0.9.8
   release.
3. The real check of the `flatpark` job is the next stable tag. Expect it to
   end green with "FlatPark accepted", and a FlatPark `auto/release-com.ferrispad.FerrisPad`
   PR to appear within minutes.

## Acceptance criteria

- [ ] Badge in README.
- [ ] `flatpark` job present, unprivileged, pinned, stable-only, non-blocking.
- [ ] README ARM64 rows match the 0.9.8 assets.

## Notes

- Origin: user, 2026-10-20 ("procedi anche con il badge nel readme e publish
  action di flatpark").
