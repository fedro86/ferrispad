---
id: T0028
title: Collapse tab_bar hover fields into a single hover: HitResult
status: done
created: 2026-08-24
severity: minor
area: ui
issue: 68
depends-on: [T0030]
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

## What was done

Stacked on T0030 (branch `ticket/T0030`, PR #64), which moved `ThemeRgb` out of
`tab_bar.rs` into `src/ui/color.rs`. Only `src/ui/tab_bar.rs` changes.

- `HitResult` now derives `Debug, Clone, Copy, PartialEq` and *is* the hover
  state: `TabBarState` has one `hover: HitResult` field instead of
  `hover_tab_index`, `hover_close`, `hover_plus`, `hover_group_label`,
  `hover_collapsed_chip`, `hover_scroll_left`, `hover_scroll_right`,
  `hover_diff_tab`.
- The hover state machine is a pure function, `update_hover(&mut hover, hit)
  -> bool` (true = changed, redraw). `Event::Move` calls it with the hit-test
  result, `Event::Leave` with `HitResult::None`. `Push`/`Drag`/`Released`
  never touched hover and still don't.
- The partial resets keep their exact old scope:
  - `rebuild()` used to clear only the tab/close/group-label/chip fields; it
    now clears the hover only when `HitResult::refers_to_tab_or_group()` (tab
    index or group id, which a rebuild can invalidate). Plus, scroll and diff
    tab hover survive a rebuild, as before.
  - `clear_diff_tab()` clears the hover only when it is `DiffTab`.
- Draw dedup, as helpers next to `draw_tab_bar`:
  - `draw_scroll_arrow(..., glyph, enabled, is_hover, colors)` replaces the two
    ScrollLeft/ScrollRight arms, which differed only in the glyph.
  - `draw_tab_background`, `draw_tab_title`, `draw_close_button` are shared by
    the Tab and DiffTab arms. The arms keep their own rules: a regular tab
    always draws its `×` and highlights it only when the pointer is on the
    close button; the diff tab draws its `×` only when active or hovered and
    highlights it when the pointer is anywhere on the diff tab.
- Six unit tests for `update_hover` and `refers_to_tab_or_group`
  (`ui::tab_bar::tests`).

Size: the non-test part of `tab_bar.rs` goes from 1962 to 1872 lines (-90),
plus 83 lines of tests.

### The one non-visible difference

The old `Event::Move` folded `DiffTab { is_close }` into a single
`hover_diff_tab` bool, so moving between the diff tab's body and its close
button did not redraw. `hover` stores the full hit, so that move now asks for
one redraw. The diff tab draws the same either way, so nothing on screen
changes, and the redraw only happens while the mouse moves (idle CPU is
unaffected).

### Hover bugs looked for

I checked whether any hover could point at something that is gone:

- Tab closed or tabs reordered: `rebuild()` clears tab and group hovers, and
  `draw` only compares the hover with the indices in the layout, so an
  out-of-range index cannot panic or highlight the wrong tab.
- Diff tab removed: `clear_diff_tab()` clears it.

No defect found, so there is no red test. There is one cosmetic quirk from
before this change, and it is left as is because the ticket is zero-behaviour-change:
after clicking `+`, the plus button moves right, but it stays highlighted until
the next mouse motion, which then fixes it. Clearing every hover in `rebuild()`
would remove this. It would also drop the highlight when the plus button is
still under the pointer. That is a behaviour change for a separate ticket if
wanted.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::tab_bar::tests
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

The first command runs 6 tests, all pass.

### Manual (the drawing is manual-only: FLTK drawing has no test harness)

1. `nix develop -c cargo run`. Open 3 or 4 files so there are several tabs.
2. Move the mouse over an **inactive tab body**: its `×` turns to the
   text colour, with no square behind it. Move onto the `×`: a highlight square
   appears behind it. Move off the tab: both go back to normal.
3. Move over the **`+` button**: the circle becomes lighter and the `+` turns
   to the text colour. Move off: back to normal.
4. Right-click a tab, add it to a new group, and give the group a name. Hover the **group
   label pill**: it gets brighter. Collapse the group (click the label) and hover the
   **collapsed chip**: it gets brighter.
5. Open enough tabs to overflow the bar so the **scroll arrows** appear. Hover
   an enabled arrow: it is highlighted. Hover a disabled (faded) arrow: no
   highlight. Click through to the end and check both ends.
6. Open a **diff tab** (e.g. an MCP edit proposal or a split-view diff). Hover
   anywhere on it while it is inactive: its `×` appears with the highlight
   square. Move off: the `×` disappears. With it active, the `×` is always
   shown and highlighted only while hovered.
7. Move the mouse out of the tab bar from any hovered item: the highlight
   clears at once.
8. Close a tab with its `×`: the tab that slides under the pointer is not
   highlighted until the mouse moves, then it highlights normally. This is
   the same as before.

## Acceptance criteria

- [x] Eight hover fields replaced by one `HitResult`; ~80 lines removed (-90 non-test).
- [x] All hover highlights behave identically (manual recipe above).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tab_bar.rs` — `HitResult`, `TabBarState`, `rebuild`,
  `clear_diff_tab`, `draw_tab_bar` + new draw helpers, `Event::Move`/`Leave`,
  new `tests` module.

## Notes

- Origin: UI audit (MODERATE, flagged as the best ratio refactor in tab_bar).
