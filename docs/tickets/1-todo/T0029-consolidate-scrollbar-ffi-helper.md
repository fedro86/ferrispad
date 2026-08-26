---
id: T0029
title: Consolidate the scrollbar FFI helper and fix contradictory SAFETY docs
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

The same `unsafe extern "C"` scrollbar-child FFI block and child-index walk is
redeclared verbatim in three places, and the comments contradict each other
about which child index is the vertical vs horizontal scrollbar — within a
single function, one comment says "child 0 = vertical" and the SAFETY comment
says "child 0 = horizontal". `get_vscrollbar_value_raw` hardcodes index 1 and is
correctness-critical for scroll sync, so a wrong invariant here silently
desyncs the split view. One copy, one correct documented invariant.

## In scope

- Extract the FFI declaration + child-walk into one helper used by all three
  sites.
- Determine the actual child-index invariant empirically and document it once,
  correctly; remove the contradictory comments.

## Out of scope

- The `Pane` refactor (T0027).

## How to test

### Manual

`cargo run`, open split diff view, scroll one pane and confirm the other tracks
(vertical scroll sync depends on the correct index).

- Before: two contradictory comments; risk of index error on edit.
- After: one helper, one verified invariant; scroll sync still correct.

## Acceptance criteria

- [ ] One FFI helper; three call sites use it.
- [ ] A single, correct, documented child-index invariant.
- [ ] Scroll sync verified in the split view.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs:31-57,1287-1313` and `src/ui/theme.rs:95-118` — the
  three copies; `theme.rs:78` vs `:96` — the contradiction.

## Notes

- Origin: UI audit (MODERATE). Also fold the duplicated scroll-track/thumb
  factor derivation (`theme.rs:80-91`) into the `DialogTheme` fields it mirrors.
