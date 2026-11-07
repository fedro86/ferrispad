---
id: T0074
title: Prepare the 0.9.9 release (dependency updates)
status: review          # todo | review | done
created: 2026-11-07
severity: ~
area: process
depends-on: [T0072]
issue: 138
---

## Goal

Ship what landed since 0.9.8 as 0.9.9, mainly the dependency updates of T0072
(`toml` 1.x, `dirs` 7, `open` 5.4, `memmap2`, `pulldown-cmark`). Pushing the
stable tag also runs the `flatpark` job in `release.yml`, which sends the new
version to FlatPark, so Flatpak users get it with `flatpak update`.

## In scope

- `CHANGELOG.md`: a `## [0.9.9]` section — Fixed (Preview in Browser on
  Windows with square brackets in the path, from `open` 5.4.4) and Changed
  (dependency updates, smaller binary).
- `./scripts/bump-version.sh -y 0.9.9`, plus `Cargo.lock`.
- Start-page changelog card: "Updated dependencies, smaller binary" on top,
  six entries.

## Out of scope

- T0073 (Rust Lint shortcut clash): it is a fix in the plugins repo and does
  not need a FerrisPad release.
- Creating or pushing the tag: done after this ticket lands on `master`, with
  the user's OK.

## How to test

1. `nix develop -c cargo run`: the start page shows v0.9.9 and the new top
   changelog entry.
2. `awk '/^## \[0.9.9\]/,/^## \[0.9.8\]/' CHANGELOG.md` prints the section.
3. The gates pass.
4. After the tag (`./scripts/release.sh`):
   - the GitHub Release `0.9.9` has the Linux amd64/arm64 (raw + `.deb`),
     Windows and macOS assets and the four `.sig` files;
   - the `flatpark` job is green and FlatPark lists 0.9.9.

## Acceptance criteria

- [ ] The user reviewed the CHANGELOG section.
- [ ] Version 0.9.9 in `Cargo.toml`, `Cargo.lock` and `build-releases.sh`.
- [ ] After the tag: all assets present and signed; FlatPark updated.

## Notes

- Commits since 0.9.8: T0070 (README), T0071 (FlatPark aarch64 mirror),
  T0072 (dependency bumps). Only T0072 changes the binary.
