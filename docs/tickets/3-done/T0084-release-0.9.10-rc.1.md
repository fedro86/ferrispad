---
id: T0084
title: Prepare the 0.9.10-rc.1 release candidate
status: done            # todo | review | done
created: 2026-11-26
severity: ~
area: process
depends-on: [T0077, T0078, T0079, T0080, T0081, T0082, T0083]
issue: 165
---

## Goal

Ship what landed since 0.9.9 as the **pre-release** 0.9.10-rc.1. It
contains the custom themes and theme manager, the current-line highlight, the
blue selection, the remembered window geometry, the title-bar close fix and
the Settings layout fix (T0077–T0083, issues #150 and #151). The RC goes
to testers only, through the GitHub pre-release and the updater's beta
channel. Stable users must not see it.

## In scope

- `CHANGELOG.md`: a `## [0.9.10-rc.1]` section (Added / Changed / Fixed).
  `release.yml` extracts the release notes from it.
- Version `0.9.10-rc.1` in `Cargo.toml`, `Cargo.lock` and
  `scripts/build-releases.sh`, the same files `bump-version.sh` edits. The
  README has no version-specific links.
- Start-page changelog card: five new entries on top, six in total.

## Out of scope

- Creating and pushing the tag. That is done after this ticket lands on
  `master`, once the website fix below is deployed.
- Windows 11 and macOS checks of T0077/T0080/T0081/T0082. They are optional
  and run on the RC binaries.

## Where the RC must not appear (checked 2026-11-26)

| Channel | How it picks "latest" | RC tag effect |
|---------|-----------------------|---------------|
| GitHub Release | `release.yml` marks tags with `-alpha`/`-beta`/`-rc` as pre-release | pre-release |
| FlatPark webhook | `flatpark` job: `if: !contains(github.ref_name, '-')` | skipped |
| FlatPark twice-daily check | `resolve-update.sh`: `GET /releases/latest` (no pre-releases) | ignored |
| In-app updater, stable channel | `GET /releases/latest` | ignored |
| In-app updater, beta channel | `GET /releases` (includes pre-releases) | **offered (intended)** |
| ferrispad.com | was `GET /tags`, first entry: **would show the RC** | fixed by fedro86/fedro-websites#15 (`/releases/latest`), merged 2026-11-26; deploy before tagging |

`is_newer_version` uses `semver`: `0.9.9 < 0.9.10-rc.1 < 0.9.10`. RC users
are therefore offered 0.9.10 when it ships, and stable users are never
offered the RC.

## How to test

1. `nix develop -c cargo build`, then start `target/debug/FerrisPad` with a
   temporary `XDG_CONFIG_HOME`. The start page shows `v0.9.10-rc.1` and the
   new changelog entries, each fitting inside the card.
2. `awk '/^## \[0.9.10-rc.1\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md` prints
   the section; this is the same extraction `release.yml` does.
3. The gates pass.
4. After the tag (`VERSION=0.9.10-rc.1; git tag -a $VERSION -m "Release
   $VERSION"; git push origin $VERSION`):
   - the GitHub Release `0.9.10-rc.1` is marked **Pre-release** and has the
     Linux amd64/arm64 (raw + `.deb`), Windows and macOS assets plus the
     `.sig` files;
   - the `flatpark` job is **skipped**;
   - `curl -s https://api.github.com/repos/fedro86/ferrispad/releases/latest | jq -r .tag_name`
     still prints `0.9.9`;
   - www.ferrispad.com/download still offers 0.9.9.

## Acceptance criteria

- [x] The user approved merging and the release candidate (2026-11-26).
- [x] Version 0.9.10-rc.1 in `Cargo.toml`, `Cargo.lock` and
      `build-releases.sh`.
- [x] The start page shows the new version and entries without clipping
      (Xvfb screenshot).
- [ ] After the tag: a pre-release with all assets signed; FlatPark, the
      website and `/releases/latest` unchanged. **Not met:** the release is a
      pre-release, the Linux and macOS assets are signed, FlatPark was skipped
      and `/releases/latest` and the website stayed on 0.9.9, but there are
      no Windows assets: `build-windows` failed with MSVC 14.52 (run
      45821907548). Fixed by T0085, shipped as 0.9.10-rc.2.

## What was done (2026-11-26)

- `CHANGELOG.md` `## [0.9.10-rc.1]`: Added (custom themes and theme manager,
  current-line highlight, window geometry), Changed (blue selection), Fixed
  (unsaved changes on title-bar close, Settings layout). It opens with a
  one-line note that this is a pre-release.
- Version bumped in `Cargo.toml`, `Cargo.lock` (the `FerrisPad` package
  entry) and `scripts/build-releases.sh`.
- Start page: "Custom themes & theme manager", "Current-line highlight",
  "Window size remembered", "Blue, readable selection", "Save prompt on
  title-bar close", then "Updated dependencies, smaller binary".
- Gates: fmt and clippy clean; `cargo test` 658 passed, 0 failed.
- Xvfb/X11 (driven by Claude), Noto Sans, 1100×740 window, light and dark:
  `ferrispad-wt/uitest-T0084/start-Light.png` and `start-Dark.png`. The
  start page shows `v0.9.10-rc.1`; every changelog line fits inside the
  card with margin. The longest new entry is shorter than the 0.9.9 entry
  that already fitted on the user's GNOME desktop.

## Notes

- Commits since 0.9.9: T0075 (README badges) and T0077–T0083. Only
  T0077–T0083 change the binary.
- Version scheme follows the earlier RCs (`0.9.5-rc.1`): the RC version is
  in `Cargo.toml`, so the RC binary reports itself as `0.9.10-rc.1`.
