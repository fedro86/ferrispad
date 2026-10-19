---
id: T0021
title: Preserve the split panel's dragged height across a mode toggle
status: done
created: 2026-08-24
severity: moderate
area: ui
depends-on: [T0027, T0029, T0030]
issue: 69
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

## What was done

- New pure type `DockedHeight` in `src/ui/split_panel.rs`, held by `SplitPanel`
  as `docked_height`. It owns the docked (non-tab) panel height: it starts at
  `DEFAULT_DOCKED_HEIGHT` (header + labels + 250 px content + action bar, the
  old constant sum) and `drag_to(mouse_y, col_y, col_h)` clamps, stores and
  returns the dragged height. No FLTK in it, so it is unit-tested directly.
- `SplitPanel::current_height()` returns the stored height (0 when hidden, as
  before). A new `SplitPanel::drag_docked_height()` forwards to `drag_to`.
- `Message::SplitViewResize` (`src/dispatch.rs`) now asks the panel for the
  height (`lw.split_panel.drag_docked_height(...)`) instead of computing it
  inline, so the panel is told about every drag. `SplitViewToggleMode` and
  `SplitViewShow` were already reading `current_height()`; they now get the
  dragged height with no change of their own.
- Side effect (consistent with the goal): re-opening a docked split view later
  in the same run also uses the last dragged height, since `SplitViewShow`
  reads the same `current_height()`. The height is not persisted across runs.
- Small, explicit addition: the old inline clamp
  `(…).clamp(100, col_h / 2)` panicked (`min > max`) when the parent column is
  shorter than 200 px. Moving it into `drag_to` would have carried a known panic
  into new code, so the upper bound is now `max(col_h / 2, 100)`: in a very
  short column the 100 px minimum wins. Covered by its own red test below.

## Red-test evidence

The tests were written against `DockedHeight` wired into dispatch but still
reproducing the old behaviour (`get()` returning the constant, `drag_to` not
storing, the old clamp). `nix develop -c cargo test --lib split_panel::tests`:

```
test ui::split_panel::tests::drag_in_a_column_shorter_than_twice_the_minimum_does_not_panic ... FAILED
test ui::split_panel::tests::dragged_height_is_what_redocking_restores ... FAILED
test ui::split_panel::tests::drag_clamps_between_minimum_and_half_the_column ... FAILED

---- ui::split_panel::tests::drag_in_a_column_shorter_than_twice_the_minimum_does_not_panic stdout ----
panicked at src/ui/split_panel.rs:392:35:
min > max. min = 100, max = 75

---- ui::split_panel::tests::dragged_height_is_what_redocking_restores stdout ----
panicked at src/ui/split_panel.rs:1285:9:
assertion `left == right` failed
  left: 328
 right: 400

---- ui::split_panel::tests::drag_clamps_between_minimum_and_half_the_column stdout ----
assertion `left == right` failed
  left: 328
 right: 100

test result: FAILED. 6 passed; 3 failed; 0 ignored; 0 measured; 378 filtered out
```

`328` is the default docked height: the drag was discarded, which is the bug.
After the fix all 9 `split_panel` tests pass.

## How to test

### Automated

```bash
nix develop -c cargo test --lib split_panel::tests
```

Expect 9 passed, including `docked_height_starts_at_the_default`,
`dragged_height_is_what_redocking_restores`,
`drag_clamps_between_minimum_and_half_the_column` and
`drag_in_a_column_shorter_than_twice_the_minimum_does_not_panic`.

### Manual (the FLTK layout itself is manual-only)

1. `nix develop -c cargo run`.
2. Open a split view in docked (bottom panel) mode, e.g. from a plugin that
   proposes a diff, or an MCP edit proposal.
3. Drag the divider above the panel up, so the panel is clearly taller than
   its default (about a third of its maximum).
4. Toggle the split mode to tab mode (the toggle button in the split panel's
   action bar). The panel fills the editor area.
5. Toggle back to docked mode.
   - Expected: the panel returns at the height you dragged it to in step 3.
   - Before the fix: it snapped back to the default height.
6. Optional: shrink the window to a very small height and drag the divider:
   the app must not crash.

## Acceptance criteria

- [x] `current_height()` reflects the user's dragged height.
- [x] Mode toggle preserves the dragged height.
- [ ] Manual recipe verified.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs` — `DockedHeight`, `SplitPanel::docked_height`,
  `current_height()`, `drag_docked_height()`, tests.
- `src/dispatch.rs` — `SplitViewResize` routes the drag through the panel.

## Notes

- Origin: UI audit (MODERATE).
- Stacked on T0029 (#65, itself on T0027 #56) and T0030 (#64): the branch
  starts with a merge of `ticket/T0030` into `ticket/T0029`.
