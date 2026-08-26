---
id: T0028
title: Collapse tab_bar hover fields into a single hover: HitResult
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`TabBarState` carries eight parallel `hover_*` booleans/indices; `Event::Move`
builds an eight-tuple, matches eight ways to fill it, and compares eight fields;
`Event::Leave` resets eight fields. `HitResult` already encodes exactly this
state. Replacing the eight fields with `hover: HitResult` (+ `#[derive(PartialEq)]`)
removes ~80 lines with zero behaviour change — the single highest lines-removed
per-risk refactor in the file. Also fold the byte-identical `draw` arms
(ScrollLeft/ScrollRight, Tab/DiffTab) into helpers while here if cheap.

## In scope

- Replace the eight `hover_*` fields with one `hover: HitResult`.
- Rewrite `Event::Move`/`Event::Leave` to set/compare the single field.
- (Optional, if low-risk) extract `draw_arrow(dir)` and share the Tab/DiffTab
  draw body.

## Out of scope

- The stale-index panic (T0020) — separate ticket, same file.

## How to test

### Manual

`cargo run`; hover tabs, close buttons, plus button, group labels, collapsed
chips, scroll arrows, diff tab — every hover highlight behaves as before.

## Acceptance criteria

- [ ] Eight hover fields replaced by one `HitResult`; ~80 lines removed.
- [ ] All hover highlights behave identically.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tab_bar.rs:184-191` (fields), `1913-2004` (Move/Leave),
  `1324-1417` (optional draw dedup).

## Notes

- Origin: UI audit (MODERATE, flagged as the best ratio refactor in tab_bar).
