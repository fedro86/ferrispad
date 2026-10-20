---
id: T0066
title: README install section (deb, FlatPark, binary) and refresh
status: review
created: 2026-10-20
severity: ~
area: docs
depends-on: [T0047]
issue: 119
---

## Goal

The README only explains how to build from source. Someone who wants to
install FerrisPad has to guess which release file to pick, and Linux users
outside Debian/Ubuntu have no packaged option listed. Add an Install section
that matches the website, and refresh the parts that are stale since 0.9.5.

## In scope

- New **Install** section, before "Build from Source":
  - Windows `.zip`, macOS `.dmg`, the Debian/Ubuntu `.deb` with an `apt install`
    line, the FlatPark Flatpak for other distributions (with the two
    `flatpak` commands), and the standalone Linux binary;
  - how updates work for each: `flatpak update` for the Flatpak; for the
    portable builds, a self-replace that accepts only binaries with a valid
    ed25519 signature; for the `.deb`, installing the new `.deb`.
- Subtitle: "A fast, lightweight text editor built with Rust and FLTK" becomes
  "A minimalist text editor written in Rust", the tagline from T0065.
- Architecture:
  - the counts are now ~43,600 lines in 101 files (it said ~38,400 in 96);
  - the tree gains `services/http.rs` and `ui/layout_shortcut.rs`;
  - the updater channels are "stable/beta" (it said "stable/unstable").

## Out of scope

- The website, which is updated in fedro86/fedro-websites#3.
- The wiki, already updated.

## How to test

1. Read the README on the branch (GitHub renders it on the PR):
   - the Install table and the Flatpak commands read correctly;
   - every file name pattern matches the 0.9.7 release assets.
2. Check the commands:
   - `sudo apt install ./FerrisPad-v0.9.7-ubuntu-amd64.deb` works on
     Debian/Ubuntu;
   - the two `flatpak` commands work once flatpark/flatpark#773 is merged.

## Acceptance criteria

- [ ] Install section present and accurate.
- [ ] No stale counts or channel names.

## Notes

- **Land this only after flatpark/flatpark#773 is merged.** Until then the
  FlatPark commands fail.
- Found while writing this: on a `.deb` install the in-app updater tries to
  rename `/usr/bin/FerrisPad` and fails without root. The README now tells
  `.deb` users to install the new `.deb`. Making the updater detect a
  package-managed install (and not offer self-replace there) could be its own
  ticket.
