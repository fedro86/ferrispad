---
id: T0026
title: Convert menu.rs closures to add_emit + a single shortcut table
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`menu.rs` has 41 hand-rolled `menu.add(..., move |_| s.send(Message::X))`
closures (~250 lines of `let s = *s;` boilerplate) where `add_emit` — already
used in `tab_bar.rs` — collapses each to one line. Worse, menu path strings are
duplicated between `BUILTIN_SHORTCUTS` and the `menu.add` calls and matched by
linear string scan, so a typo in either silently yields no shortcut with no
compile error and no test.

## In scope

- Replace the 41 closures with `menu.add_emit(label, shortcut, flag, sender,
  msg)`.
- Drive both the menu path and its shortcut from a single source (one table)
  so the two can't desync; ideally make a missing/typo'd entry a compile or
  test failure.

## Out of scope

- Changing the menu structure or shortcuts themselves.

## How to test

### Regression / manual

Add a test asserting every menu entry's path resolves to a shortcut-table entry
(no orphans). Manually verify a few accelerators still fire.

- Before: a path typo silently drops the shortcut.
- After: it's caught by the test / type system.

## Acceptance criteria

- [ ] Menu built with `add_emit`; boilerplate gone.
- [ ] Path + shortcut share one source of truth; desync is detectable.
- [ ] Test added; manual accelerator check done.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/menu.rs:45-68,308-322,325-635` — shortcut table + the closures.

## Notes

- Origin: UI audit (SEVERE). `tab_bar.rs:1530-1580` already shows the `add_emit`
  pattern to copy.
