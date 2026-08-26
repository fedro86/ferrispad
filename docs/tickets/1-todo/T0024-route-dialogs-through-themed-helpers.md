---
id: T0024
title: Route message/alert dialogs through the themed helper + one modal loop
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: [T0017]
---

## Goal

`show_themed_message` was introduced to replace FLTK's unthemed
`dialog::message_default`/`alert_default`, but it is called from only two sites
while ~40 sites still use the raw unthemed FLTK dialogs. The result is
inconsistent theming across the app. Likewise `run_dialog` is the canonical
modal loop but is reimplemented at several sites (T0017 fixed the two buggy
ones).

## In scope

- Migrate the ~40 raw `dialog::{alert,message,choice2,input}_default` call sites
  to the themed helpers (`show_themed_message` / a themed choice/input variant,
  adding the latter if missing).
- Ensure all modal dialogs use `run_dialog`.

## Out of scope

- The color-derivation mismatch between dialogs and the tab bar (T0030).

## How to test

### Manual

`cargo run` in dark mode; trigger a representative set of the migrated dialogs
(find not-found, plugin manager errors, readonly viewer prompts, goto-line
errors).

- Before: raw FLTK gray dialogs.
- After: themed dialogs matching the app.

## Acceptance criteria

- [ ] No remaining `*_default` FLTK dialog calls outside the themed helpers.
- [ ] All modals use `run_dialog`.
- [ ] Manual check across several dialogs.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/mod.rs:391-403` — `run_dialog` / `show_themed_message`.
- ~40 sites incl. `plugin_manager.rs`, `readonly_viewer.rs`, `split_panel.rs`,
  `tree_panel.rs`, `goto_line.rs` (see UI audit for the full list).

## Notes

- Origin: UI audit (SEVERE for consistency). Large but mechanical; can be split
  per-file if it grows past one sitting.
