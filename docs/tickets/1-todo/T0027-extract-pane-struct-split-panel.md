---
id: T0027
title: Extract a Pane struct in split_panel.rs
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`SplitPanel` has ~30 fields, of which the `left_*` and `right_*` groups
(display, buffer, style buffer, label, markers) are hand-mirrored twins.
`show_request_with_syntax`, `apply_theme`, `update_display_fonts`, and
`apply_syntax_diff_pane(is_left: bool, ...)` all write everything twice. With a
`Pane` type owning one side's widgets + methods, the struct drops to ~15 fields
and those methods roughly halve (estimate 1331 → ~800 lines) with no loss of
function.

## In scope

- Introduce a `Pane` struct (one side's display/buffer/style/label/markers) with
  methods; store `left: Pane, right: Pane`.
- Replace the `is_left` boolean-flag branching with per-`Pane` method calls.

## Out of scope

- The scrollbar FFI dedup (T0029) and diff-map logic (which is good as-is).

## How to test

### Regression / manual

Split-view diff still renders identically (syntax colors + diff backgrounds),
theme applies to both panes, fonts update on both. `cargo test` green; manual
diff view check.

## Acceptance criteria

- [ ] `Pane` struct in place; left/right mirroring gone from the four methods.
- [ ] Split/diff view visually identical to before.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs:302-369` (struct), `639-788`, `885-...`, `1072-1198`.

## Notes

- Origin: UI audit (MODERATE). `start_page.rs:325-479` shows the codebase
  already knows this style of helper decomposition.
