---
id: T0021
title: Preserve the split panel's dragged height across a mode toggle
status: todo
created: 2026-08-24
severity: moderate
area: ui
depends-on: []
---

## Goal

`SplitPanel::current_height()` always returns the constant default height,
because `Message::SplitViewResize` writes the dragged height straight into the
parent `Flex` via `parent.fixed(...)` without telling the panel. So
`SplitViewToggleMode` reads `current_height()` and snaps the panel back to the
default, discarding the user's drag. The panel's height state lives in FLTK, not
in the panel.

## In scope

- Store the current height in the `SplitPanel` (update it when
  `SplitViewResize` fires) so `current_height()` returns the actual height.
- Have the mode toggle read that stored height instead of the constant.

## Out of scope

- The `Pane` extraction refactor (T0027).

## How to test

### Manual repro

`cargo run`, open the split view, drag its divider to a non-default height, then
toggle the split mode.

- Before the fix: the panel snaps back to the default height.
- After the fix: the panel keeps the dragged height.

## Acceptance criteria

- [ ] `current_height()` reflects the user's dragged height.
- [ ] Mode toggle preserves the dragged height.
- [ ] Manual recipe verified.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs:1063-1069` — `current_height()`.
- `src/dispatch.rs:870-879` — `SplitViewResize` (store the height).
- `src/dispatch.rs:903-904` — `SplitViewToggleMode` (read stored height).

## Notes

- Origin: UI audit (MODERATE).
