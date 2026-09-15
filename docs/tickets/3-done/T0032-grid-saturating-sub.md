---
id: T0032
title: Use saturating_sub for terminal grid row/col math
status: done
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
issue: 32
---

## Goal

`services/terminal/grid.rs` uses raw `self.rows - 1` and `self.cols - 1` in two
spots, in a file that otherwise uses `saturating_sub` a dozen times. These are
not currently reachable (callers clamp dimensions at `.max(20)/.max(5)` and
`.max(10)/.max(3)`), but a zero would underflow-panic — one refactor away from a
crash. Make them consistent with the rest of the file.

## In scope

- Replace the two raw `- 1` with `saturating_sub(1)`.

## Out of scope

- The escape-sequence DoS (T0010).

## How to test

### Regression test

```bash
nix develop -c cargo test --lib terminal::grid::tests
```

- `newline_on_zero_row_grid_does_not_underflow` — 0×0 grid, cursor off
  `scroll_bottom`, `newline()` must not panic and leaves the cursor alone.
- `clear_line_to_cursor_on_zero_col_grid_does_not_underflow` — 0-col × 1-row
  grid, `clear_line_to_cursor()` must be a no-op.
- `clear_line_to_cursor_clears_through_cursor_inclusive` — pin test (green
  before and after): on `ABCDE` with cursor at col 2 the line becomes `   DE`.

Red run — **executed**, by reverting the two `grid.rs` source lines (tests kept)
in an isolated copy of the tree and running the command above. The test profile
keeps overflow checks on: `Cargo.toml` has no `[profile]` override.

```
RED   (two source lines reverted)   test result: FAILED. 12 passed; 2 failed
  newline_on_zero_row_grid_does_not_underflow
      panicked at grid.rs:125:37: attempt to subtract with overflow
  clear_line_to_cursor_on_zero_col_grid_does_not_underflow
      panicked at grid.rs:184:46: attempt to subtract with overflow
  clear_line_to_cursor_clears_through_cursor_inclusive ... ok   (pin: green both sides)

GREEN (with the fix)                test result: ok. 14 passed; 0 failed
```

To reproduce the red side yourself: revert `grid.rs:125` to `self.rows - 1` and
`grid.rs:184` to `0..=self.cursor_col.min(self.cols - 1)`, then re-run.

> Heads-up when running this recipe: `line_feed_flood_trims_in_bounded_time`
> (pre-existing, from T0010) is a wall-clock test — 1M `scroll_up` under a 2 s
> bound in a debug build. On an idle machine it takes ~0.45 s, but under CPU
> load it fails spuriously ("1M line-feeds took 4.31s"). It is unrelated to this
> ticket; see T0044.

## What was done

- `newline` (`grid.rs:125`): `self.rows - 1` → `self.rows.saturating_sub(1)`.
- `clear_line_to_cursor` (`grid.rs:184`): `0..=self.cursor_col.min(self.cols - 1)`
  → `0..self.cursor_col.saturating_add(1).min(self.cols)`. A literal
  `saturating_sub(1)` here is not enough: with `cols == 0` it yields `0..=0` and
  then indexes an empty row (index-out-of-bounds instead of overflow). The
  exclusive range is the same set of columns for `cols >= 1` and empty for
  `cols == 0`.

## Acceptance criteria

- [x] No raw `- 1` on grid dimensions (verified: the only remaining `rows - 1` /
      `cols - 1` strings in the terminal module are inside comments).
- [x] Regression test added; it failed before the fix, for the bug's reason
      (overflow panic), and passes after.
- [x] `cargo test` / `clippy --all-targets --all-features -D warnings` / `fmt
      --check` clean — locally and on CI (run `34978163419`, green).

## Affected files

- `src/app/services/terminal/grid.rs:125,184` (ticket originally said 122,174 —
  those are the enclosing function lines).

## Notes

- Origin: plugin/services audit (MINOR, latent).
- On a 0-row grid `newline()` with the cursor *on* `scroll_bottom` still panics
  inside `scroll_up` (`cells[0]` index), a separate latent issue not covered by
  this ticket's scope. Note that this is the *default* state of a fresh
  `TerminalGrid::new(0, 0)` (`cursor_row == scroll_bottom == 0`), so "a 0-row
  grid is safe now" is not true in general — only the non-scrolling branch is.
  Follow-up: T0045 clamps the grid to at least 1×1 in `new()`/`resize()`.
- The "not currently reachable" claim was checked: `terminal_panel.rs:302-303`
  clamps to `.max(20)/.max(5)`, `:727-728` to `.max(10)/.max(3)`, and
  `char_metrics()` returns `.max(1)` on both axes — so no caller can produce a
  0-row or 0-col grid, and there is no divide-by-zero on the way in either.
- Tracking issue #32, PR #33. Verified by the user on 2026-09-23 and landed on
  `master`.
