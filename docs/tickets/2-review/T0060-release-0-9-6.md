---
id: T0060
title: Prepare the 0.9.6 release (CHANGELOG + version bump)
status: review
created: 2026-10-19
severity: ~
area: process
depends-on: []
issue: 104
---

## Goal

Ship everything landed since 0.9.5 (63 commits: the security audit T0001–T0016,
the UI/robustness batches T0017–T0059, the TLS stack upgrade and the CI gates)
as release 0.9.6. It is also the version the first FlatPark package (T0047) will
point at. This ticket prepares the release on its branch; the tag is pushed
separately, after the merge and only with the user's explicit OK.

## In scope

- `CHANGELOG.md`: a new `## [0.9.6] - <date>` section on top, in the existing
  Keep-a-Changelog style (bold lead-in per bullet; Added / Fixed / Changed / Security).
  CI extracts the GitHub release notes from it, so it must read well for users.
  - Group by what users notice, not by ticket.
  - Put a **Security** section first, for the audit fixes: plugin sandbox escape,
    terminal_view shell injection, PTY use-after-free, registry path sanitisation,
    plugin instruction-limit bypass, download size caps, TLS stack upgrade.
  - Notable fixes: Ctrl+Z/Ctrl+A/Ctrl+M with mismatched keyboard layouts; dialogs
    that could open hidden and freeze the app; opening a missing path from the
    CLI; themed dialogs and file chooser; live Settings theme preview; regex find
    anchors/empty matches; Find Next/Prev switching.
  - Added: Cancel button for large-file loading.
  - Changed: HTTP client is now `ureq` with `rustls` on `ring`. The squash commit
    on master wrongly says "upgrade minreq to 3", so describe it correctly here.
    Mention the stricter https-only redirects.
  - Changed: regex case folding is ASCII-only, with a hint in the dialog.
  - Do not list internal refactors or ticket workflow changes, unless users notice
    them.
- Run `./scripts/bump-version.sh -y 0.9.6` on the ticket branch. It updates
  `Cargo.toml`, `docs/js/main.js`, `docs/index.html`, `README.md` and
  `scripts/build-releases.sh`, and commits. Also update `Cargo.lock`'s own
  package version if the script doesn't.
- The start page's embedded changelog card should show 0.9.6 items, if it reads
  CHANGELOG.md or a constant. Check.

## Out of scope

- Creating or pushing the `0.9.6` tag, and anything outward-facing beyond the
  ticket branch and draft PR.
- FlatPark packaging (T0047).

## How to test

1. `nix develop -c cargo build --release && ./target/release/FerrisPad`: Help →
   About shows 0.9.6, and the start page changelog shows the new items.
2. `awk '/^## \[0.9.6\]/,/^## \[0.9.5\]/' CHANGELOG.md` prints a complete section.
   This is what CI extracts.
3. `grep -rn '0\.9\.5' README.md docs/index.html docs/js/main.js scripts/build-releases.sh`
   prints no stale download links.
4. Gates green.

## Acceptance criteria

- [ ] CHANGELOG section reviewed by the user.
- [ ] Version 0.9.6 everywhere the bump script touches; gates green.
- [ ] No tag created by this ticket.

## Notes

- After this lands: tag `0.9.6` (no `v` prefix) on master, push it, and watch the
  release workflow (first run of `release.yml` with the Node 24 actions from T0042
  and with ureq/ring on Windows/macOS). Check that every binary has its `.sig`.
