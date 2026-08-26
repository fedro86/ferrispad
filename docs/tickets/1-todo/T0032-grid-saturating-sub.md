---
id: T0032
title: Use saturating_sub for terminal grid row/col math
status: todo
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
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

Unit-test the grid with a degenerate 0×0 (or minimal) size and assert the row/col
math doesn't panic.

- Before: `attempt to subtract with overflow` in debug if reached with zero.
- After: saturates to 0, no panic.

## Acceptance criteria

- [ ] No raw `- 1` on grid dimensions.
- [ ] Regression test added and green.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/terminal/grid.rs:122,174`.

## Notes

- Origin: plugin/services audit (MINOR, latent).
