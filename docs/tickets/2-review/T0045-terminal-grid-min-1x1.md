---
id: T0045
title: Clamp TerminalGrid dimensions to at least 1×1
status: review
created: 2026-09-23
severity: minor
area: robustness
depends-on: [T0032]
issue: 47
---

## Goal

`TerminalGrid` accepts a 0-row or 0-col size in `new()` and `resize()`, and
several methods assume at least one row/column. T0032 patched two raw `- 1`
subtractions, but a fresh `TerminalGrid::new(0, 0)` still panics on the very
first `newline()`: `cursor_row == scroll_bottom == 0`, so `scroll_up` runs and
indexes `self.cells[0]` on an empty `Vec`. Today no caller produces a
degenerate size (`terminal_panel.rs` clamps to `.max(20)/.max(5)` and
`.max(10)/.max(3)`), so this is latent — but every method has to defend against
it individually. Enforcing the invariant once, at the two places a size enters
the grid, removes the whole class.

## In scope

- `TerminalGrid::new(cols, rows)`: clamp both to `>= 1`.
- `TerminalGrid::resize(new_cols, new_rows)`: same clamp.
- Document the invariant ("`rows >= 1 && cols >= 1`") on the struct.
- Revisit the T0032 zero-size tests: they build grids that can no longer
  exist. Rewrite them to assert the clamp (e.g. `new(0, 0)` yields a 1×1 grid
  and `newline()` / `clear_line_to_cursor()` do not panic).

## Out of scope

- Reverting the T0032 `saturating_sub` changes — they stay as defence in depth.
- Any change to how `terminal_panel.rs` computes the size from the widget.

## How to test

### Regression test

```bash
nix develop -c cargo test --lib terminal::grid::tests
```

- `newline_on_fresh_zero_size_grid_does_not_panic` — `TerminalGrid::new(0, 0)`
  then `newline()`: no panic, grid is 1×1.
- `resize_to_zero_clamps_to_one` — `resize(0, 0)` on an 80×24 grid, then
  `put_char('x')` and `newline()`: no panic, grid is 1×1.

Red run — **executed** on the pre-fix code:

```
test result: FAILED. 14 passed; 2 failed
  newline_on_fresh_zero_size_grid_does_not_panic   panicked at grid.rs:145:38
  resize_to_zero_clamps_to_one                     panicked at grid.rs:145:38
```

(`grid.rs:145` is `self.cells[0].clone()` in `scroll_up` — index out of
bounds on the empty `cells` Vec.)

Green after the fix: all `terminal::` tests pass (26); full `cargo test`,
`clippy -D warnings`, `fmt --check` clean.

## What was done

- `TerminalGrid::new` and `TerminalGrid::resize` clamp both dimensions with
  `.max(1)` before building/adjusting the cells.
- Struct doc states the invariant: `rows >= 1 && cols >= 1`.
- The two T0032 zero-size tests built grids that can no longer exist:
  - `newline_on_zero_row_grid_does_not_underflow` still passes unchanged in
    body (a cursor past the last row leaves `newline` a no-op); comment
    updated.
  - `clear_line_to_cursor_on_zero_col_grid_does_not_underflow` asserted an
    empty row; it now asserts the clamp (`cols == 1`) and that clearing that
    single column works.
- T0032's `saturating_sub` changes are kept as defence in depth.

## Acceptance criteria

- [x] `new()` and `resize()` never produce a grid with `rows == 0` or
      `cols == 0`.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`,
      and `cargo fmt --check` all pass with zero warnings.

## Affected files

- `src/app/services/terminal/grid.rs` — `new()`, `resize()`, struct doc,
  tests.

## Notes

- Origin: review of T0032, which listed the remaining `scroll_up` panic under
  its Notes as out of scope.
