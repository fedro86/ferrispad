---
id: T0029
title: Consolidate the scrollbar FFI helper and fix contradictory SAFETY docs
status: review
created: 2026-08-24
severity: minor
area: ui
depends-on: [T0027]
issue: 62
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

- The `Pane` refactor (T0027). This branch is stacked on T0027's branch
  (`ticket/T0027`, PR #56), which moved both split-panel copies next to each
  other.

## What was done

- **New module `src/ui/scrollbar.rs`** — the one place that reaches the
  built-in scrollbars of `TextDisplay`/`TextEditor` (`Fl_Text_Display`) and
  `Tree` (`Fl_Tree`). Public API:
  - `style_scrollbars(&widget, track, thumb)` — flat-styles every scrollbar
    child of a widget;
  - `style_scrollbar(&mut sb, track, thumb)` — styles one scrollbar;
  - `text_display_vscroll_value(&display)` — vertical scrollbar value, bounded
    on `DisplayExt` so it cannot be called on a `Tree` (whose order differs).
- **No `unsafe` left.** Instead of redeclaring `Fl_Group_children` /
  `Fl_Group_child` and reconstructing a `Scrollbar` from a raw child pointer
  with `Scrollbar::from_widget_ptr` (an unchecked type assumption), the helper
  uses fltk-rs's safe, checked casts: `Group::from_dyn_widget_ptr` and
  `Scrollbar::from_dyn_widget` (C++ `dynamic_cast`). A widget or child of the
  wrong type yields `None` instead of a mistyped handle.
- **Invariant determined empirically and documented once** (module docs +
  `TEXT_DISPLAY_VSCROLLBAR` constant), and pinned by a unit test that builds
  real widgets and reads each child's scrollbar type:

  | Widget            | child 0              | child 1              |
  |-------------------|----------------------|----------------------|
  | `Fl_Text_Display` | horizontal scrollbar | vertical scrollbar   |
  | `Fl_Tree`         | vertical scrollbar   | horizontal scrollbar |

  This matches the bundled FLTK 1.4 source (fltk-sys 1.5.14:
  `Fl_Text_Display.cxx` creates `mHScrollBar` before `mVScrollBar`;
  `Fl_Tree.cxx` creates `_vscroll` before `_hscroll`). So the old
  `get_vscrollbar_value_raw` (index 1) was **correct**; the wrong statement was
  only the `theme.rs` comment "Child 0 = vertical scrollbar, child 1 =
  horizontal scrollbar". It never caused a runtime bug, because that site styles
  both children.
- **All five copies replaced**, not just the three the audit listed: the two in
  `split_panel.rs` and the one in `theme.rs`, plus identical copies found in
  `dialogs/readonly_viewer.rs` and `tree_panel.rs` (leaving them would have
  contradicted "one FFI helper").
- **Notes item folded in:** the duplicated track/thumb factor derivation
  (`theme.rs` and `DialogTheme::from_theme_bg`) is now one pure function,
  `dialogs::scrollbar_colors(base, is_dark)`, with unit tests.
  `DialogTheme::style_scroll` reuses `style_scrollbar`. Colors are unchanged:
  the editor still derives from the editor background, dialogs from the dialog
  background.
- Behaviour change: none intended. `style_scrollbars` styles every child that
  is a scrollbar (dynamic-cast checked) instead of "the first two children".
  For these widgets those are the same two scrollbars.

### Evidence the invariant test discriminates

With `TEXT_DISPLAY_VSCROLLBAR` temporarily set to `0`, the test fails:

```
thread 'ui::scrollbar::tests::child_index_invariant_matches_bundled_fltk' panicked at src/ui/scrollbar.rs:112:13:
assertion `left == right` failed
  left: Horizontal
 right: Vertical
```

With the correct value `1`, it passes.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::scrollbar::tests   # child-index invariant, real FLTK widgets
nix develop -c cargo test --lib ui::dialogs::tests     # scrollbar_colors derivation
nix develop -c cargo test                              # full suite
```

### Manual (scrollbar styling and scroll sync are UI-only; this part is manual)

1. `nix develop -c cargo run`, open a file longer than one screen, with a dark
   syntax theme.
2. Expect both editor scrollbars (vertical and, with a long line and wrap off,
   horizontal) to be flat, with a track darker than the editor and a lighter
   thumb, as before. Switch to a light theme in Settings: both restyle (track
   slightly darker, thumb darker still).
3. Open the file tree (sidebar) in a folder with enough entries to scroll:
   its scrollbar(s) are flat and themed the same way.
4. Open a file over the large-file threshold read-only (the read-only viewer):
   its vertical scrollbar is flat and themed.
5. Trigger a split diff view (e.g. a plugin or MCP `propose edit` that opens a
   side-by-side review) on a file long enough to scroll:
   - mouse-wheel over the left pane: both panes scroll together by 3 lines per
     notch;
   - drag the left pane's vertical scrollbar and release: the right pane
     snaps to the same first line; do the same from the right pane.
   Before and after: identical behaviour.

## Acceptance criteria

- [x] One FFI helper; all call sites use it (five, not three — see above).
- [x] A single, correct, documented child-index invariant, verified by a unit
      test against the real widgets.
- [ ] Scroll sync verified in the split view (manual, step 5 above).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/scrollbar.rs` — new: the helper, the invariant, the test.
- `src/ui/mod.rs` — registers the module.
- `src/ui/split_panel.rs` — drops `get_vscrollbar_value_raw`,
  `get_vscrollbar_value`, `style_scrollbars_raw`; uses the helper.
- `src/ui/theme.rs` — drops its FFI copy and the wrong comment; uses
  `scrollbar_colors` + `style_scrollbars`.
- `src/ui/dialogs/mod.rs` — new `scrollbar_colors`; `DialogTheme` uses it;
  `style_scroll` uses `style_scrollbar`; unit tests.
- `src/ui/dialogs/readonly_viewer.rs`, `src/ui/tree_panel.rs` — drop their
  FFI copies; use `style_scrollbars`.

## Notes

- Origin: UI audit (MODERATE).
- Soundness review of the old SAFETY comments: no real unsoundness found. The
  old code relied on an unchecked assumption (that children 0 and 1 are
  `Fl_Scrollbar`s) before `Scrollbar::from_widget_ptr`; that holds for
  `Fl_Text_Display` and `Fl_Tree` in FLTK 1.4. The new code checks it with
  `dynamic_cast`, so the assumption no longer needs to be trusted.
- The invariant test constructs FLTK widgets in a unit test. That works
  headless (construction does not open a display). All construction is kept in
  one test because FLTK's "current group" is process-global.
