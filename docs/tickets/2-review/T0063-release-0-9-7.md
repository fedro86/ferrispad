---
id: T0063
title: Prepare the 0.9.7 release (Escape fix)
status: review
created: 2026-10-20
severity: ~
area: process
depends-on: [T0062]
issue: 113
---

## Goal

0.9.6 shipped with a bug: pressing Escape in the editor hides the main window
(T0062). Ship the fix as patch release 0.9.7.

0.9.6 is not re-tagged or overwritten, because:
- clients that already updated would never see a change under the same
  version;
- the signatures and the sha256 pinned by packagers (FlatPark) would change
  under the same file name.

## In scope

- `CHANGELOG.md`: a `## [0.9.7]` section with the Escape fix. CI extracts the
  release notes from it.
- `./scripts/bump-version.sh -y 0.9.7`: `Cargo.toml`, `Cargo.lock`,
  `scripts/build-releases.sh`, auto-commit.
- Start-page changelog card: add the Escape fix on top and keep six entries.
- `packaging/flatpak/` metainfo: `<release version="0.9.7">`.

## Out of scope

- Creating or pushing the `0.9.7` tag. That happens after the merge, only
  with the user's explicit OK.
- The FlatPark package pin (T0047). It moves to 0.9.7 on its own branch
  once the release exists.

## How to test

1. Run `nix develop -c cargo build --release && ./target/release/FerrisPad`.
   Expect the start page to show v0.9.7 with "Escape no longer closes the
   editor" at the top of the changelog card.
2. In the same build, run the T0062 recipe: type in a tab and press Escape.
   Expect the window to stay.
3. Run `awk '/^## \[0.9.7\]/,/^## \[0.9.6\]/' CHANGELOG.md`. Expect only the
   0.9.7 section, which is what CI extracts.
4. The gates pass.

## Acceptance criteria

- [ ] The user reviewed the CHANGELOG section.
- [ ] Version 0.9.7 in `Cargo.toml`, `Cargo.lock` and `build-releases.sh`; gates green.
- [ ] No tag created by this ticket.

## Notes

- Stacked on T0062: land T0062 first, then this ticket.
- After landing:
  1. tag `0.9.7` (no `v` prefix) on master, with the user's OK;
  2. check that the release has the three `.sig` files;
  3. re-pin T0047's manifest to the 0.9.7 asset before the FlatPark PR is
     opened. Once listed, FlatPark's update check does this by itself.
