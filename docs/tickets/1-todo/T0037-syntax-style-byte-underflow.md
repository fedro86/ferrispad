---
id: T0037
title: Guard the u8 underflow on the syntax style byte path
status: todo
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
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

Feed the diff-pane syntax path a style buffer containing a byte `< 'A'` and
assert no panic in a debug build.

- Before: debug panic on subtract-overflow.
- After: byte skipped/clamped, no panic.

## Acceptance criteria

- [ ] No unguarded `byte - b'A'` on the style path.
- [ ] Regression test added and green in debug.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs:913`.

## Notes

- Origin: plugin/services + UI audits (MINOR, debug-only).
