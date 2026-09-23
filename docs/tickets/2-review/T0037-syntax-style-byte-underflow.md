---
id: T0037
title: Guard the u8 underflow on the syntax style byte path
status: review
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
issue: 42
---

## Goal

`split_panel.rs:913` computes `(syntax_bytes[i] - b'A') as usize` with no
lower-bound check. Any style byte below `'A'` (a `\0`, a space, a padding byte)
panics with "attempt to subtract with overflow" in debug builds. Release wraps
and the `< main_style_table.len()` guard on the next line catches it, so this is
debug-only — but it bites during development.

## In scope

- Bounds-check the style byte before subtracting (skip/clamp bytes below `'A'`),
  so the path is panic-free in debug too.

## Out of scope

- The `Pane` refactor (T0027) touching the same file.

## How to test

### Regression test

```bash
nix develop -c cargo test --lib split_panel::tests
```

- `style_bytes_below_a_fall_back_to_diff_style` — syntax style bytes
  `"\0 @"` (all below `'A'`) over diff backgrounds Normal/Added/Removed must
  produce the diff-only styles `"ABC"`, not panic.
- `valid_style_bytes_combine_with_diff_background` — pin (green before and
  after): `"A"` combines with its syntax colour into the first combined entry
  (`'G'`, after the 6 base diff styles); `"Z"` (outside the table) and a
  position past the end of the syntax string fall back to the diff style.

Red run — **executed**. The loop body was first moved, unchanged, out of the
FLTK-bound `SplitPanel::apply_syntax_diff_pane` into a pure
`combine_syntax_diff` function so it can be unit-tested. On that code:

```
test result: FAILED. 1 passed; 1 failed
  style_bytes_below_a_fall_back_to_diff_style
      panicked at src/ui/split_panel.rs:247:29: attempt to subtract with overflow
```

Green after the fix: both tests pass; full `cargo test`, `clippy -D warnings`,
`fmt --check` clean.

## What was done

- Extracted `combine_syntax_diff(syntax_bytes, diff_map, main_style_table,
  sdm) -> String` from `apply_syntax_diff_pane` (pure; the method now just
  calls it).
- Replaced the unguarded `syntax_bytes[i] - b'A'` + manual length checks with
  one checked chain: `get(i)` → `checked_sub(b'A')` → `main_style_table.get(..)`
  → colour. Any failing step means "no syntax colour", i.e. the diff-only base
  style — the same result release builds already produced by wrapping into an
  out-of-table index.
- Other `b'A'` arithmetic in the codebase was checked: it all *adds* to `'A'`
  or (in `terminal_panel.rs`) subtracts after an `is_ascii_uppercase()` guard.

## Acceptance criteria

- [x] No unguarded `byte - b'A'` on the style path.
- [x] Regression test added and green in debug; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs` — new `combine_syntax_diff`, `apply_syntax_diff_pane`
  calls it, new `tests` module.

## Notes

- Origin: plugin/services + UI audits (MINOR, debug-only).
