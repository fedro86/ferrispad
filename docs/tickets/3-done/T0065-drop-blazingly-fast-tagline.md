---
id: T0065
title: Drop the "blazingly fast" tagline
status: done
created: 2026-10-20
severity: ~
area: ui
depends-on: []
issue: 117
---

## Goal

"A blazingly fast, minimalist notepad written in Rust" reads as boilerplate,
and readers take it for AI-written marketing and leave. Replace it everywhere
with a plain tagline: **"A minimalist text editor written in Rust"**.

## In scope

The occurrences:

| File | Where it shows |
|---|---|
| `Cargo.toml` | `description` |
| `build.rs` | Windows "File description" in the file properties |
| `ferrispad.desktop` | `Comment=`, the tooltip in Linux menus and the `.deb` |
| `packaging/flatpark/…/com.ferrispad.FerrisPad.desktop` | `Comment=`, already submitted this way in flatpark/flatpark#773 |
| `src/ui/dialogs/about.rs` | About dialog |
| `src/main.rs` | first line of `--help` |
| `.github/discussions_welcome.md` | Discussions welcome text |

## Out of scope

- The website (www.ferrispad.com, `fedro86/fedro-websites`), which is a
  separate repo with its own change.
- The GitHub wiki, which is updated separately.
- CHANGELOG history and closed tickets.

## How to test

1. `grep -rni blazing --exclude-dir=target --exclude-dir=.git --exclude-dir=tickets .`
   prints nothing.
2. `nix develop -c cargo run`, then Help → About. Expect it to read "A minimalist
   text editor written in Rust".
3. `nix develop -c cargo run -- --help`. Expect the first line to be
   "FerrisPad 0.9.7 — A minimalist text editor".
4. The gates pass.

## Acceptance criteria

- [ ] No "blazingly" left in the repo (outside history).
- [ ] About, `--help` and the desktop entry use the new tagline.

## Notes

- Origin: user, 2026-10-20 ("togli Blazingly fast… è proprio la firma del AI").
- The release binaries pick it up at the next release.
