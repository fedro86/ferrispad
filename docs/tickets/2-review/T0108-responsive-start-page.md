---
id: T0108
title: Responsive start page — one column when narrow, scroll when short, a minimum window size while shown
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 198
---

## Goal

Narrowing the window while the start page is shown clips everything. The
two-column body (buttons + Sessions/Recent cards | Wiki buttons + Changelog)
is cut on both sides, and the centred "FerrisPad" title sits over the cut
columns. User report with a screenshot. The user treats clipping as a
blocking layout defect.

## Cause

- `ui/start_page.rs` lays the body out as one fixed row:
  `LEFT_W + 20 + RIGHT_W` (644 px) plus centring spacers. A narrower window
  centres that row and cuts both sides.
- `show()` grows the window once when the page appears
  (`start_page.rs:300-311`); nothing stops the user from shrinking it
  afterwards.
- A short window cuts the bottom the same way: the body has a fixed height
  and the page cannot scroll.

## In scope

- **The layout rule, as pure functions.** One column width (two buttons
  plus a gap; the button width is wide enough for the measured labels).
  Two columns need `2 × column + gap + 2 × margin`; below that, one column
  with the left block above the right block, centred.
- **Vertical scrolling.** The page content sits in a vertical `Scroll`,
  styled with the theme (`ui::scrollbar::style_scrollbar`), so a short
  window scrolls instead of clipping.
- **Relayout on resize.** Crossing the column threshold rebuilds the page
  (a `StartPageRelayout` message, routed like the theme rebuild). Within a
  layout, the content just follows the new width. No timer.
- **A minimum window size only while the start page is shown**
  (`size_range`). The one-column width plus the window's chrome; the height
  of the title plus the first button rows. The window grows to it if it is
  smaller. When a tab opens, the editor's normal (unconstrained) minimum
  comes back.

## Out of scope

- Changing what the page shows (buttons, cards, changelog entries).
- The themed scrollbar components of T0105. This uses the existing
  `style_scrollbar` helper.

## How to test

### Regression tests (red first)

```bash
nix develop -c cargo test start_page_layout
xvfb-run -a nix develop -c cargo test --test start_page_layout
```

- Unit (`ui::start_page`): the column rule at its boundaries; the minimum
  sizes follow the constants (no hard-coded 380).
- UI binary `tests/start_page_layout.rs`, with private config and data dirs:
  the start page in a window 1100, 760, 500 and 380 px wide, light and dark.
  No visible widget may extend past the window horizontally, and a
  vertical overflow is allowed only with the scrollbar shown. Before the
  fix, at 500 px the two columns lie outside the window.

### Manual (Xvfb by Claude, then the user)

At 1100 / 760 / 500 / 380 px wide and a short height, light and dark:
screenshots, inspected edge by edge; nothing cut. Narrowing below the
minimum is refused while the start page shows; after opening a tab the
window can shrink again.

Red run of the UI test on the old page (excerpt):

```text
test start_page_fits_every_width ... FAILED
dark 500x600: 'New File' at -72,182 150x38 outside the 500x600 window
dark 500x600: 'New Session' at -72,232 150x38 outside the 500x600 window
dark 500x600: 'Recent Sessions' at -64,306 296x30 outside the 500x600 window
dark 500x600: 'Wiki FerrisPad' at 260,182 312x38 outside the 500x600 window
...
```

After the fix the test passes, including the resize phase (1100 → 500 →
1100 → 420×380 on a shown page, answering `StartPageRelayout` as dispatch
does). The 3 unit tests pass. Gates: 669 passed (with a display), clippy
`-D warnings` and fmt clean.

### Xvfb results (debug build, light and dark, real app with tabs on)

The window is resized with `xdotool windowsize`. Screenshots are in
`/home/fedro/Code/personal/ferrispad-wt/uitest-T0087/t108-{Dark,Light}-<W>x<H>.png`,
inspected edge by edge:

| Window | Layout | Edges |
|---|---|---|
| 1100×760 | two columns, centred | content well inside, symmetric |
| 760×620 | two columns | columns 58–702 of 760 |
| 500×600 | one column, left block above right | 88–400, centred in the 488 px left by the scrollbar; vertical scrollbar |
| 380×420 | one column | 28–340 of the 368 usable px; scrolling to the bottom shows the whole Changelog card, "more…" and the bottom margin (`t108-Dark-380-bottom.png`) |
| 760×300 | two columns, short | the content scrolls; nothing cut horizontally |

- The scrollbar uses the theme's `scroll_track` / `scroll_thumb`, as the
  editor's does.
- **Minimum size:** while the start page shows, `WM_NORMAL_HINTS` says
  "program specified minimum size: 364 by 271" (`xprop`). After Ctrl+T
  (a tab opens) it says **1 by 1**, and the window can be made 300×200.
  Xvfb has no window manager, so the hint is not *enforced* there; a real
  WM, and on Wayland the compositor via the xdg-toplevel minimum size,
  enforces it. Pending user verification on a desktop.
- No panic or assertion in the debug build's log.

## What was done

- `ui/start_page.rs`:
  - Pure rule: `column_width(button_w)`, `two_columns_min_width`
    (= 2 columns + 3 gaps) and `one_column_min_width` (= 1 column + 2 gaps).
    `columns_for(content_w)` and `page_columns(w, h, …)` take off the
    vertical scrollbar that a too-short page needs before deciding.
    `min_page_height()` is the title plus the first button rows.
  - The grid button width is `BTN_W`, or wider when a measured label needs
    it (`grid_button_width`), so the minimum follows the font metrics.
  - `show()` builds the content in a vertical `Scroll`, at least as tall as
    the page, so the spacers still centre it when there is room. The body is
    a row of two columns or one centred column (left block above right
    block; the Changelog card then takes its content height).
  - A hidden page with no size yet uses its parent's size as the estimate.
  - The `Scroll`'s resize callback makes the content follow the new width,
    and sends `StartPageRelayout` when the column layout would change.
  - `enforce_min_window_size()` sets `size_range` to the page minimum plus
    the window's chrome around the page (menu, status bar, side panels),
    and grows the window if it is smaller. `hide()` resets it to 1×1.
    This replaces the old one-time grow.
- `Message::StartPageRelayout`, routed in `main.rs` to
  `dispatch::relayout_start_page`, which rebuilds the page like the theme
  path does. `sync_start_page` applies the minimum after each layout of
  the page. No timer.

## Review note (ferrispad-a3, applied): one relayout request per build

- A window drag across the threshold fires one resize per motion event.
  Each sent a `StartPageRelayout`, and each rebuilt the page, including
  `session::list_sessions()` and `gather_recent_files()` reading the disk.
- The resize callback now sends at most one request per build, using a flag
  captured in the closure.
- The flag cannot stick:
  - `StartPageRelayout` is routed unconditionally;
  - while the page is visible, handling it rebuilds the page, which
    installs a new callback with a fresh flag;
  - while the page is hidden no relayout is needed, and the next `show()`
    rebuilds anyway.
  So it needs no clearing on dequeue. This branch is based on `master`,
  which does not have T0096's `AppState::note_dequeued`.
- Test: in the resize phase, three more resizes past the threshold before
  the request is handled give **one** request. Red with the flag bypassed:
  `10 relayout requests for one build`.

## Merge of master after 0.9.10 (2026-11-27)

Master brought T0114, which sizes the left cards so the changelog card
shows all its entries (`left_card_h`, `changelog_card_h`,
`changelog_content_h`), and T0093's `DirtyChanged` message. The conflicts
were resolved like this:
- `messages.rs`: both variants are kept.
- `start_page.rs`:
  - the cards use T0114's `left_card_h`;
  - one rule, `changelog_h(columns, left_card_h, entries)`, gives the
    changelog height: beside the left cards it spans both
    (`changelog_card_h`); alone, in one column, it is
    `changelog_content_h`;
  - both test modules are kept.
- **A defect the merge exposed.** T0108's own one-column height,
  `30 + 1 + 22·n + 25 + 12`, is 200 px for the 6 entries, and T0114
  measured 222 px of content. In one column the last entry would have been
  clipped, the bug T0114 fixed for two columns. New test
  `start_page_layout_changelog_shows_every_entry` (both layouts, 1 to
  `MAX_RECENT` recent items). It is red on that formula:
  `One, 1 item(s): 200 px < 222 px`.

Gates after the merge: 762 passed (UI tests required, under Xvfb);
clippy `-D warnings` and fmt are clean.

## Merge of T0116 and master (2026-11-28)

T0116 (#240) added the recent-files list: `StartPage::show()` takes
`recent_list: &[String]`, and the page remembers it (`last_recent`), so
`sync_start_page` redraws it when the list changes. Then master moved on
(T0095, T0112, T0073, T0104). One conflict, in `start_page.rs`:
- both new fields are kept (`min_page`, `last_recent`);
- the data comes from T0116: `recent_files::start_page_entries(recent_list)`,
  not `gather_recent_files()`, so a relayout no longer reads the disk for
  the recent files;
- a Recent Files row is fitted to `column_w - 2·8 - 8`. T0116 used
  `LEFT_W - 24`, but `LEFT_W` is gone: the left column is `column_w` wide in
  both layouts;
- `dispatch::relayout_start_page` passes `&state.recent_files`, like the
  other two `show()` calls.

`tests/start_page_layout.rs` now builds the page with a full Recent Files
card: four long paths, including the FLTK specials '@' and '&'. T0116's
`recent_files_row_label_fits_the_card` and this ticket's
`start_page_layout_changelog_shows_every_entry` both pass. Gates (UI tests
on a private Xvfb, T0122's environment): 797 passed, clippy `-D warnings`
and fmt clean.

## Acceptance criteria

- [x] Nothing is clipped at any width down to the start-page minimum; one
      column below the threshold; vertical scrolling when short.
- [x] The minimum window size applies only while the start page shows
      (hint set and reset; enforcement by a real WM is pending user
      verification).
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; Xvfb screenshots at the four widths,
      light and dark, edge-checked.
- [ ] Pending user verification on a desktop (the report's case; Wayland
      minimum size).

## Affected files

- `src/ui/start_page.rs` — the layout rule, the scroll, relayout, minimum
  size
- `src/app/domain/messages.rs`, `src/main.rs`, `src/dispatch.rs` —
  `StartPageRelayout`
- `tests/start_page_layout.rs` — new UI test binary

## Notes

- User report; assigned by the orchestrator (ferrispad-5a), target 0.9.11.
- Wayland: `size_range` maps to the xdg-toplevel minimum size; the
  compositor enforces it. Checked on X11/Xvfb only.
