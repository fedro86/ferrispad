---
id: T0069
title: Prepare the 0.9.8 release (ARM64 builds, updater arch fix)
status: done
created: 2026-10-20
severity: ~
area: process
depends-on: [T0067, T0068]
issue: 125
---

## Goal

Ship what landed since 0.9.7 as 0.9.8:
- the updater architecture fix (T0067);
- ARM64 Linux builds (T0068);
- the store-neutral Flatpak update message (T0064);
- the plain tagline (T0065).

This is also the first release built by the arm64 matrix leg, so it is the
real check of T0068.

## In scope

- `CHANGELOG.md`: a `## [0.9.8]` section with Added (ARM64 Linux builds,
  Flatpak on FlatPark), Fixed (updates on other architectures, the Flatpak
  update dialog) and Changed (tagline).
- `./scripts/bump-version.sh -y 0.9.8`, plus `Cargo.lock`.
- Start-page changelog card: "ARM64 Linux builds" and "Flatpak on FlatPark" on
  top, six entries.
- `ferrispad.desktop` and the FlatPark copy end with a newline. FlatPark's
  reviewer noted it in flatpark/flatpark#773.

## Out of scope

- Creating or pushing the tag, which needs the user's OK.
- The user-facing ARM docs: README, website and wiki still say "build from
  source" until the 0.9.8 assets exist. They switch right after the release
  passes the ARM64 check below.

## How to test

1. `nix develop -c cargo run`: the start page shows v0.9.8 with the new
   changelog entries.
2. `awk '/^## \[0.9.8\]/,/^## \[0.9.7\]/' CHANGELOG.md` prints the section.
3. The gates pass.
4. After the tag, the release must have all of these:
   - `FerrisPad-v0.9.8-linux-arm64`, `FerrisPad-v0.9.8-ubuntu-arm64.deb` and
     `FerrisPad-v0.9.8-linux-arm64.sig`, next to the amd64, macOS and Windows
     assets;
   - `file` on the arm64 binary reporting aarch64;
   - the four `.sig` files.

## Acceptance criteria

- [ ] The user reviewed the CHANGELOG section.
- [ ] Version 0.9.8 in `Cargo.toml`, `Cargo.lock` and `build-releases.sh`.
- [ ] After the tag: ARM64 assets present and signed.

## Notes

- After the release: re-point the README, website and wiki ARM64 notes to the
  new downloads.
