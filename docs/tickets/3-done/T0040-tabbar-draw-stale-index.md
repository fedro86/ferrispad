---
id: T0040
title: Bounds-check the tab-bar draw path against a stale layout index
status: done
created: 2026-08-25
severity: minor
area: ui
depends-on: []
---

## Goal

`tab_bar.rs` draw path indexes `st.tabs[*index]` where `*index` comes from a
`LayoutItem::Tab` in `st.layout`. This is the same layout-index → tabs-index
pattern that T0020 hardened for the drag/release handlers, but on the redraw
path. If the layout is momentarily stale versus the tab vector (a tab closed by
a plugin/reload before the layout is rebuilt), the next redraw panics
out of bounds.

## In scope

- Replace the direct `&st.tabs[*index]` in the draw loop with a bounds-checked
  `.get()`; skip drawing that layout item if the index is stale.

## Out of scope

- The drag/release sites (already fixed in T0020).
- The broader hover/layout cleanup (T0028).

## How to test

### UI-only — no unit test is feasible

Same constraint as T0020: the draw path needs a live FLTK widget and a redraw
with a layout that is out of sync with `st.tabs`, unreachable under headless
`cargo test` (`engineering-standards.md`). Verify structurally that the direct
index is gone.

### Manual repro

Hard to force deterministically (requires a redraw between a tab removal and a
layout rebuild). The change is a defensive `.get()`; the acceptance is that no
direct index remains and normal drawing is unchanged.

## Acceptance criteria

- [ ] No direct `st.tabs[..]` on a layout index in the draw loop.
- [ ] Normal tab drawing is unchanged (all tabs still render).
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tab_bar.rs` — the `LayoutItem::Tab { index, .. }` draw arm (~line 1009).

## Notes

- Origin: related finding surfaced while doing T0020 (UI audit). Trivial
  defensive change; authorized to land straight through review.
