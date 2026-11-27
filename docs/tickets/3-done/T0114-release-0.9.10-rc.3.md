---
id: T0114
title: Prepare the 0.9.10-rc.3 release candidate
status: done            # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: [T0090, T0092, T0094]
issue: 228
---

## Goal

Ship what landed since 0.9.10-rc.2 as the **pre-release** 0.9.10-rc.3. It
contains:

- the plugin security hardening (T0090, T0092, T0094);
- the dependency updates (T0076);
- the highlighting stalls on long lines and on unclosed comments (T0087,
  T0096);
- the session, tab and title fixes (T0091, T0093, T0099, T0103);
- the theme and dialog fixes (T0088, T0097, T0098, T0105, T0107);
- the settings defaults (T0100).

The RC goes to testers only, through the GitHub pre-release and the
updater's beta channel. Stable users must not see it.

## In scope

- Version `0.9.10-rc.3` in `Cargo.toml`, `Cargo.lock` and
  `scripts/build-releases.sh`, the same files `bump-version.sh` edits. The
  edits are made without its auto-commit or tag.
- `CHANGELOG.md`: a `## [0.9.10-rc.3]` section (Security, Fixed, Note).
  `release.yml` extracts the release notes from it.
- Start-page changelog card (`src/ui/start_page.rs`, `CHANGELOG_ENTRIES`):
  two new entries on top, six in total.
- Start-page defect found while driving it: with 0–1 recent sessions/files
  (a fresh install) the changelog card clipped its sixth entry. It is as
  tall as the two left cards, which were sized for their recent rows only.
  rc.1 and rc.2 shipped with only five of the six entries visible
  (`uitest-T0084/start-Light.png`).

## Out of scope

- Creating and pushing the tag. The orchestrator does that after this
  ticket lands on `master`.
- Process-only tickets without a user-visible change: T0089 (macOS test
  harness) and T0113 (test guard). T0086 gets at most one line.

## Where the RC must not appear (checked 2026-11-27)

| Channel | How it picks "latest" | RC tag effect |
|---------|-----------------------|---------------|
| GitHub Release | `release.yml` `create-release`: a tag matching `-(alpha\|beta\|rc)` sets `prerelease: true` | pre-release |
| Release gate (T0086) | `release-gate`: `publish=true` only for a version tag | publishes (it is a tag) |
| FlatPark webhook | `flatpark` job: `publish == 'true' && !contains(github.ref_name, '-')` | skipped |
| FlatPark twice-daily check | `resolve-update.sh`: `GET /releases/latest` (no pre-releases) | ignored |
| In-app updater, stable channel | `GET /releases/latest` | ignored |
| In-app updater, beta channel | `GET /releases` (includes pre-releases) | **offered (intended)** |
| ferrispad.com | `GET /releases/latest` (fedro86/fedro-websites#15, since rc.1) | ignored |

`/releases/latest` returns `0.9.9` on 2026-11-27. `is_newer_version` uses
`semver`: `0.9.10-rc.2 < 0.9.10-rc.3 < 0.9.10`. So rc.1/rc.2 testers on the
beta channel are offered rc.3, and stable users are never offered it.

## How to test

1. `nix develop -c cargo build`, then start `target/debug/FerrisPad` with a
   temporary `XDG_CONFIG_HOME`. The start page shows `v0.9.10-rc.3` and the
   new changelog entries, each fitting inside the card.
2. `awk '/^## \[0.9.10-rc.3\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md` prints
   the section. This is the same extraction `release.yml` does.
3. The gates pass.
4. After the tag (`VERSION=0.9.10-rc.3; git tag -a $VERSION -m "Release
   $VERSION"; git push origin $VERSION`):
   - `release-gate` reports `publish=true`;
   - the GitHub Release `0.9.10-rc.3` is marked **Pre-release** and has the
     Linux amd64/arm64 (raw + `.deb`), Windows (`.exe` + `.zip`) and macOS
     (raw + `.dmg`) assets plus the `.sig` files;
   - the `flatpark` job is **skipped**;
   - `curl -s https://api.github.com/repos/fedro86/ferrispad/releases/latest | jq -r .tag_name`
     still prints `0.9.9`;
   - www.ferrispad.com/download still offers 0.9.9.
   This is also the first real tag through T0086's release gate. Record the
   run in T0086 (its open acceptance box) and here.

## Acceptance criteria

- [x] The user approved the CHANGELOG text and the release candidate
      (2026-11-27).
- [x] Version 0.9.10-rc.3 in `Cargo.toml`, `Cargo.lock` and
      `build-releases.sh`.
- [x] The CHANGELOG section extracts with the `awk` above.
- [x] The start page shows the new version and all six entries without
      clipping (Xvfb screenshots), with 0–1 recent items too
      (`changelog_card_shows_every_entry`).
- [ ] After the tag:
  - a pre-release with all assets signed;
  - FlatPark, the website and `/releases/latest` unchanged;
  - T0086's tag path recorded as working.

## Affected files

- `Cargo.toml`, `Cargo.lock`, `scripts/build-releases.sh`: version
- `CHANGELOG.md`: the `[0.9.10-rc.3]` section
- `src/ui/start_page.rs`: `CHANGELOG_ENTRIES`, card heights

## What was done (2026-11-27)

- Version `0.9.10-rc.3` in `Cargo.toml`, `Cargo.lock` (the `FerrisPad`
  package entry) and `scripts/build-releases.sh`. Edited directly, so
  `bump-version.sh` neither committed nor tagged.
- `CHANGELOG.md` `## [0.9.10-rc.3] - 2026-11-27`:
  - an intro (pre-release only; website, FlatPark and the stable updater
    stay on 0.9.9);
  - Security: T0090, T0092, T0094, the web-only links of T0090/T0092, and
    the T0076 dependency versions, checked against `Cargo.lock`;
  - Fixed: T0087, T0096, T0099, T0103, T0093, T0091, T0107, T0098, T0097,
    T0105, T0088, T0100, with T0100's behaviour change;
  - Note: the T0102 decision.
  Each claim was checked against its ticket. The UI names were checked in
  the code: the diagnostic link is opened with a double-click and is not
  called "Details" (only the Plugin Manager button is), and the Settings
  button is "Save". The `awk` extraction prints the section (27 lines).
- Review wording fixes (a3, orchestrator):
  - The T0099 entry now says exactly what T0099 fixed and the user
    verified: two windows on the same session are closed one after the
    other, and the unsaved untitled text of both comes back.
  - It no longer promises that unsaved text is never lost. T0101, which
    stays for 0.9.11, documents a remaining path.
  - "A program with an approved name in any other folder" replaces "a
    program with the same name somewhere else".
- Start page: "Plugin sandbox hardening", "No stalls on long lines", then
  "Custom themes & theme manager", "Current-line highlight", "Window size
  remembered", "Blue, readable selection".
- Start-page card heights. The red test `changelog_card_shows_every_entry`
  failed on the unchanged layout: `1 recent item(s): changelog card 194 px
  < 222 px needed`. The card sizes now go through small helpers:
  - `recent_card_h` is the old formula;
  - `changelog_content_h` is what the changelog card needs: card margins,
    header, separator, 22 px per entry, "more...", and the pads;
  - `changelog_card_h` spans both left cards;
  - `left_card_h` is `max(recent_card_h, half of what the changelog
    needs)`.
  With 0–1 recent items each left card grows from 83 to 97 px, so the
  columns stay aligned. With 2 or more items nothing changes (113 px and
  up). `left_cards_keep_their_recent_rows` guards the recent rows.
- Xvfb/X11 (driven by Claude), Noto Sans, 1100×740 window, editor font
  16 and 18, light and dark: `ferrispad-wt/uitest-T0114/start-{Light,Dark}-{16,18}.png`.
  Checked edge by edge:
  - all six entries are inside the card;
  - "more..." sits below the last entry with margin;
  - the longest entry ends well inside the right border;
  - the bottom of Recent Files lines up with the bottom of Changelog;
  - no label is clipped.

## Notes

- T0102 (watch the OS appearance at runtime) was rejected: it needs a
  background listener, against the 0%-CPU-when-idle rule. The CHANGELOG says
  when a change of the OS appearance is picked up instead.
- T0100 changes behaviour: an old `settings.json` without `"tabs_enabled"`
  now starts with tabs on, like a fresh install. The CHANGELOG says so.
- The security entries describe the protection, not how the old checks could
  be bypassed. The tickets (T0090/T0092/T0094) hold the details.

## User verification

2026-11-27: the user approved the rc.3 CHANGELOG text, including the two
lines narrowed in review (two windows, plugin program paths), and asked to
merge and tag. Start-page screenshots:
`ferrispad-wt/uitest-T0114/start-{Light,Dark}-{16,18}.png`. The post-tag
items stay open until the tag run is checked.
