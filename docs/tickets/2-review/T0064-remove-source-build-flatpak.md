---
id: T0064
title: Remove the source-build Flatpak packaging
status: review
created: 2026-10-20
severity: ~
area: process
depends-on: [T0047]
issue: 115
---

## Goal

`packaging/flatpak/` built FerrisPad from source inside Flatpak, and was
written for the Flathub submission that Flathub closed. FlatPark (T0047) is
now the Flatpak channel: it installs the official release binary. The
source-build manifest has no remaining job.

It is also already broken. `cargo-sources.json` still vendors the
pre-`ureq` dependency set, so an offline build fails until it is regenerated,
and it would have to be regenerated on every `Cargo.lock` change. Its
metainfo duplicates the FlatPark one and needed a manual `<release>` on every
release. Remove it, together with the references to Flathub that are no longer
true.

## In scope

- Delete `packaging/flatpak/`: the manifest, the metainfo,
  `cargo-sources.json` and `flatpak-cargo-generator.py`.
- `docs/guides/BUILD_GUIDE.md`:
  - drop "Option 3: Build Flatpak", so the FlatPark section becomes Option 3;
  - say that there is no Flatpak build from source;
  - fix the `--verify` note, which talked about a local `master` build.
- Update dialog (`src/ui/dialogs/update.rs`): inside a Flatpak it said updates
  come from "your software store (Flathub)". It now says "through Flatpak
  (your software store or `flatpak update`)".
- `src/app/services/updater.rs`: same fix in the `is_flatpak()` doc comment.
- `.gitignore`:
  - drop the `.flatpak-builder/` and `/build-dir/` entries;
  - reword the icons comment, which said the Flatpak build installed the icons
    (the `.deb` and the window icon still use them).

## Out of scope

- `CHANGELOG.md`: its 0.9.6 entry mentions `packaging/flatpak/`, and that is
  history.
- Closed tickets in `3-done/`.
- The user's local Flatpak install. The user has already uninstalled the
  `master` dev build, and its `ferrispad-origin`/`debug-origin` remotes left
  with it. The local `.flatpak-builder/` cache (92 MB, ignored by git) was
  deleted on 2026-10-20 at the user's request.

## How to test

1. `git ls-files packaging` lists only `packaging/flatpark/…` files.
2. `grep -rni "flathub\|packaging/flatpak/\|cargo-sources" --exclude-dir=target --exclude-dir=.git --exclude-dir=tickets . | grep -v CHANGELOG.md`
   prints nothing.
3. In `docs/guides/BUILD_GUIDE.md`, the Linux build options are 1 `.deb`,
   2 standalone binary and 3 Flatpak (FlatPark).
4. The gates pass. The update-dialog text only shows inside a Flatpak, so it
   is checked by reading the diff. It appears in a build after the next
   release, once installed from FlatPark: Help → Check for Updates.

## Acceptance criteria

- [ ] `packaging/flatpak/` is gone, and nothing in the repo points at it
      except the CHANGELOG and closed tickets.
- [ ] No user-facing text claims FerrisPad comes from Flathub.
- [ ] Gates green.

## Notes

- Origin: user, 2026-10-20 ("packaging/flatpak e .flatpak-builder sono ancora
  necessari?" → "rimuovi quindi le cartelle non necessarie").
- What is lost: building unreleased code inside Flatpak. FlatPark only
  installs release binaries, so Flatpak-specific behaviour (`is_flatpak()`) is
  checked on a release.
