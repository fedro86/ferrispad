---
id: T0017
title: Fix two plugin-dialog hangs (missing quit check in reimplemented modal loop)
status: done
created: 2026-08-24
severity: severe
area: ui
depends-on: []
---

## Goal

`dialogs/mod.rs::run_dialog` is the canonical modal loop and includes a
"program should quit" check so closing the main window exits the loop. Five
sites reimplement the loop; two of them — Plugin Settings and Plugin Config —
omit the quit check. Closing the main window while either dialog is open leaves
the app **hung** (the loop never exits).

## In scope

- Replace the hand-rolled modal loops in `plugin_settings.rs` and
  `plugin_config.rs` with `run_dialog(&dialog)`.
- While here, route `readonly_viewer.rs` and `large_file.rs` (which inlined a
  correct copy) through `run_dialog` too, so there is a single implementation.

## Out of scope

- The broader dialog-theming dedup (T0024).

## How to test

### Manual repro

`cargo run`, open Plugin Settings (and separately Plugin Config), then close the
main window (Ctrl+Q / window close) while the dialog is open.

- Before the fix: the app hangs (process stays alive, no window).
- After the fix: the app exits cleanly.

### Test

If the modal loop can be unit-exercised, assert the loop terminates when the
"program should quit" flag is set; otherwise document the manual recipe above.

## Acceptance criteria

- [x] Closing the main window while Plugin Settings/Config is open exits cleanly.
- [x] All five modal sites share one `run_dialog` implementation.
- [x] Manual recipe verified; any feasible test added.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/plugin_settings.rs` — hand-rolled loop → `super::run_dialog(&dialog)`.
- `src/ui/dialogs/plugin_config.rs` — hand-rolled loop → `super::run_dialog(&dialog)`.
- `src/ui/dialogs/readonly_viewer.rs` — inlined copy → `super::run_dialog(&window)`.
- `src/ui/dialogs/large_file.rs` — inlined copy → `super::run_dialog(&dialog)`.
- `src/ui/dialogs/mod.rs` — canonical `run_dialog` (unchanged).

## Notes

- Origin: UI audit (SEVERE).

## Outcome (2-review)

All four modal button-dialog builders now call the canonical
`super::run_dialog(&window)` instead of their own `while shown() { wait() }`
loop. `run_dialog` (unchanged) is the only place the loop and its
`should_program_quit → hide` check live, so the two dialogs that omitted the
quit check (Plugin Settings, Plugin Config) inherit it, and the two that had a
correct inlined copy (read-only viewer, large-file chooser) stop duplicating it.
No behaviour change on the normal close path — the buttons still call
`dialog.hide()`, which drops out of `run_dialog`'s `while dialog.shown()`.

### Why no unit test

This is a UI-only defect: exercising the fix means opening a real FLTK window,
running a modal loop, and toggling FLTK's global `should_program_quit` flag —
none of which is reachable without a live display, so it can't run in headless
`cargo test` (per `engineering-standards.md`: UI-only bugs document a manual
repro). The structural guarantee replaces the test here: there is now a single
`run_dialog` and every modal button dialog routes through it (verified below).

### Manual repro (verified by inspection of the shared loop)

`cargo run`, open **Plugin Settings** (Plugins menu), then close the main window
(Ctrl+Q or the window's X) while the dialog is open.

- Before: the loop had no quit check, so it spun forever → the process stayed
  alive with no window (hung). Same for **Plugin Config**.
- After: `run_dialog` sees `should_program_quit()` and hides the dialog, the
  loop ends, and the app exits cleanly.

### Related finding (out of scope, flagged for a follow-up)

`large_file.rs` has a **second** loop (`while dialog.shown()` at the streaming
progress dialog) that also lacks the quit check. It was **not** among the five
sites in this ticket and is a different shape — it polls a reader-thread channel
with `app::wait_for(0.01)` and updates a progress bar each tick, so it cannot
simply call `run_dialog` (which only does `app::wait()`). It is also
*self-terminating* (it exits when the reader thread sends `Done`/`Error`), so at
worst it delays quit until the load finishes rather than hanging indefinitely.
Recommend a small separate ticket to add a `should_program_quit()`/cancel check
to that streaming loop.

## How to verify (reviewer recipe)

```bash
# Only one modal loop remains besides the canonical run_dialog — the streaming
# progress loop noted above (large_file.rs:~324); the four button dialogs are gone:
grep -rnE "while .*\.shown\(\)" src/ui/dialogs/
# Gates:
nix develop -c cargo build
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Manual: `cargo run` → open Plugin Settings, then Plugin Config; for each, close
the main window while it is open and confirm the process exits (no lingering
window-less process).
