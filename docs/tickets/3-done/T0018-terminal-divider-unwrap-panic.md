---
id: T0018
title: Fix terminal divider .unwrap() panic and restore the drag guard
status: done
created: 2026-08-24
severity: moderate
area: ui
depends-on: []
---

## Goal

The terminal panel's divider drag handler is a drifted copy of the split/tree
versions. It calls `f.window().unwrap().set_cursor(...)`, which panics if the
frame has no window (reachable during teardown or before `show()`), where the
other two copies use `if let Some(mut win) = div.window()`. It also dropped the
`dragging: Rc<Cell<bool>>` guard, so `Event::Leave` resets the cursor mid-drag
and `Event::Drag` can fire without a prior `Push`.

## In scope

- Replace the two `.unwrap()`s with the `if let Some(win) = f.window())` pattern.
- Restore the `dragging` guard so cursor changes and drag messages only happen
  during an actual drag.

## Out of scope

- Extracting the shared divider helper (T0023) — this ticket just makes the
  terminal copy correct; T0023 removes the duplication.

## How to test

### Manual repro

`cargo run`, show the terminal panel, then drag its divider and move the mouse
off the divider mid-drag; also toggle the panel closed while the cursor is over
the divider region.

- Before the fix: possible panic on teardown; cursor flickers/resets mid-drag.
- After the fix: no panic; drag behaves like the split/tree dividers.

## Acceptance criteria

- [x] No `.unwrap()` on `window()` in the terminal divider handler.
- [x] Drag guard restored; cursor/drag behaviour matches the other dividers.
- [x] Manual recipe verified (by inspection against the reference handlers).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/terminal_panel.rs` — `new_divider` handler rewritten to match
  `split_panel::new_divider`, plus a `use std::rc::Rc` already present.

## Notes

- Origin: UI audit (SEVERE — panic path). Reference copies:
  `split_panel.rs:588-630`, `tree_panel.rs:248-290`.

## Outcome (2-review)

`terminal_panel::new_divider`'s handler now mirrors the split/tree reference
exactly, keeping only the terminal-specific bits (`Cursor::WE`, `event_x`,
`Message::TerminalViewResize`):

- **No panic.** Both `f.window().unwrap()` calls (in `Enter` and `Leave`) become
  `if let Some(mut win) = f.window()`. `window()` is `None` during teardown or
  before `show()`, so the old code panicked exactly there.
- **Drag guard restored.** A `dragging: Rc<Cell<bool>>` is set on `Push`, checked
  on `Drag` (so a `Drag` without a prior `Push` sends nothing) and on `Leave` (so
  the cursor is not reset mid-drag), and cleared on a new `Released` arm (which
  also restores the default cursor). Previously `Push` was a bare `true`, `Drag`
  always fired, and there was no `Released` case.

**Gotcha handled:** this module already imports `Cell` from
`terminal::grid::{Cell, …}` (the grid cell struct), so the guard uses a
fully-qualified `std::cell::Cell::new(false)` instead of adding a colliding
`use std::cell::Cell`.

### Why no unit test

UI-only defect: the panic requires a real Frame whose `window()` returns `None`
during FLTK teardown, and the cursor/drag behaviour requires dispatched FLTK
events — neither is reachable under headless `cargo test`
(`engineering-standards.md`). The fix is verified structurally: the handler is
now byte-for-byte the reference shape apart from the documented terminal-specific
constants.

### Manual repro

`cargo run`, show the terminal panel, drag its divider and move the mouse off it
mid-drag; also toggle the panel closed while hovering the divider.

- Before: possible panic on teardown (`window().unwrap()` on `None`); cursor
  flickers/resets mid-drag.
- After: no panic; drag matches the split/tree dividers.

## How to verify (reviewer recipe)

```bash
# No .unwrap() on window() remains in the terminal divider:
grep -n "window().unwrap()" src/ui/terminal_panel.rs   # -> no matches
# The handler now matches the reference shape:
sed -n '/pub fn new_divider/,/^    }/p' src/ui/terminal_panel.rs
# Gates:
nix develop -c cargo build
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
nix develop -c cargo test
```
