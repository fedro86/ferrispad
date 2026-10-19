---
id: T0051
title: Clear the tab bar's + button highlight after it moves
status: review
created: 2026-10-19
severity: minor
area: ui
depends-on: [T0028]
issue: 79
---

## Goal

After clicking `+`, a new tab is added and the `+` button shifts right, but it
stays highlighted until the next mouse movement, even though the pointer is no
longer over it. The highlight should always match what is under the pointer.

## In scope

- After `rebuild()`, recompute the hover from the current pointer position
  (hit-test it against the new layout) instead of keeping or clearing stale
  hover state. Use the `update_hover(&mut hover, hit)` helper from T0028.
- Do this for every hover target, so the same fix covers tabs that slide under
  the pointer after a close.

## Out of scope

- Any other tab-bar drawing change.

## What was done

- `TabBarState` gained a plain `pointer: Option<(i32, i32)>` field: the last
  pointer position over the bar, widget-relative. It is written by the
  `Move`, `Drag` and `Push` events the bar already handles, and set to `None`
  on `Leave`. No timers, no polling: 0% idle CPU is unchanged.
- The bar also handles `Event::Hide` (sent once by FLTK when the bar is
  hidden, i.e. the start page after the last tab closes): it clears `pointer`
  and `hover`. A hidden bar gets no `Move`/`Leave`, so without this a stale
  position (or the old stale `+` highlight) would survive until the bar is
  shown again.
- New `relayout(st)` = `compute_layout(st)` + re-hit-test `pointer` against
  the new layout and store the result with `update_hover`. `pointer == None`
  (pointer outside the bar) means no hover.
- Every place that recomputed the layout now calls `relayout`: `rebuild`,
  `ensure_active_visible`, `handle_resize`, `set_diff_tab`, `clear_diff_tab`
  and the two scroll-arrow clicks. So the rule covers every hover target and
  every layout change, not only `rebuild`.
- One rule replaces two special cases: `rebuild`'s "drop tab/group hovers"
  (`HitResult::refers_to_tab_or_group`) and `clear_diff_tab`'s "drop a
  diff-tab hover" are gone, since re-hit-testing already handles both.
- **Deviation:** T0028's test `only_tab_and_group_hovers_are_invalidated_by_rebuild`
  was removed with the `refers_to_tab_or_group` method it tested (the method
  had no other use). `pointer_outside_the_bar_means_no_hover_after_rebuild`
  now covers every hover kind instead. T0028's other `update_hover` tests are
  unchanged and green.

## Red-test evidence

The tests were written first, with `relayout` carrying the old rule (drop
tab/group hovers, keep the rest), and run against it:

```
---- ui::tab_bar::tests::hover_is_recomputed_after_rebuild stdout ----
assertion `left == right` failed
  left: PlusButton
 right: None
---- ui::tab_bar::tests::plus_click_hovers_the_new_tab_under_the_pointer stdout ----
assertion `left == right` failed
  left: PlusButton
 right: Tab { index: 0, is_close: false }
---- ui::tab_bar::tests::closing_a_tab_hovers_the_tab_that_slides_under_the_pointer stdout ----
assertion `left == right` failed
  left: None
 right: Tab { index: 1, is_close: false }
---- ui::tab_bar::tests::pointer_outside_the_bar_means_no_hover_after_rebuild stdout ----
assertion `left == right` failed
  left: DiffTab { is_close: false }
 right: None
test result: FAILED. 5 passed; 4 failed; 0 ignored; 0 measured; 448 filtered out
```

With the fix: `test result: ok. 9 passed; 0 failed`.

## How to test

```bash
nix develop -c cargo test --lib ui::tab_bar::tests
```

- `hover_is_recomputed_after_rebuild`: hover on `PlusButton`, the pointer over
  empty space in the new layout, so after the rebuild the hover is `None`.
- `plus_click_hovers_the_new_tab_under_the_pointer`: no tabs, pointer on `+`;
  after a tab is added, `+` moves to the right edge and the hover is the new
  tab under the pointer.
- `closing_a_tab_hovers_the_tab_that_slides_under_the_pointer`: three tabs,
  pointer on the second; after closing the first, the hover is the tab that
  slid under the pointer (index 1, formerly the third).
- `pointer_outside_the_bar_means_no_hover_after_rebuild`: with no known
  pointer, every kind of stale hover becomes `None`.

Manual (in the running app, `nix develop -c cargo run`, window at least
~800px wide so tabs keep their full width):

1. With exactly one tab open, put the pointer on `+` (right edge, it
   highlights). Without moving the mouse, press Ctrl+W: the start page
   appears and the tab bar hides. Move the pointer to the middle of the
   window (away from where the bar was) and press Ctrl+N. Expected: the bar
   comes back with `+` NOT highlighted (before the fix it stayed highlighted
   until the pointer entered the bar).
2. Open three tabs and click the first one to make it active. Move the
   pointer onto the middle of the second tab (it highlights), then press
   Ctrl+W without moving the mouse. Expected: the tab that slid under the
   pointer is drawn hovered (before the fix no tab was highlighted until the
   next mouse move).
3. Move the pointer off the tab bar. Expected: nothing in the bar is
   highlighted.

## Acceptance criteria

- [x] Red test fails before and passes after; T0028's tests stay green
      (except the removed `refers_to_tab_or_group` test, see deviation).
- [x] Gates green. Still no timers or polling (0% idle CPU).

## Affected files

- `src/ui/tab_bar.rs`: `TabBarState::pointer`, `relayout`, `rebuild`,
  `clear_diff_tab`, the layout call sites, `Move`/`Drag`/`Push`/`Leave`/`Hide`
  handling, tests.

## Notes

- Origin: T0028 report, 2026-10-19 (an older cosmetic quirk, kept out of that
  zero-behaviour-change refactor).
- On current `master`, with at least one tab `+` is anchored to the right
  edge, and with zero tabs the bar is hidden behind the start page. So the
  visible "`+` stays highlighted" case is the hide/show path in manual step 1;
  the tab cases (close, reorder, scroll, diff tab appearing, resize) go
  through `relayout`.
- `Event::Hide` handling is UI-only (needs a real FLTK widget), so it is
  covered by manual step 1, not by a unit test.
