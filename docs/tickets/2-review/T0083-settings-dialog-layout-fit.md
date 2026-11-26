---
id: T0083
title: Settings dialog — labels must fit and the dialog must fit small screens
status: review          # todo | review | done
created: 2026-11-26
severity: moderate      # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0077]     # ordering only: stacked on the T0077–T0082 verification build
issue: 163              # GitHub issue number
---

## Goal

The Settings dialog (`src/ui/dialogs/settings_dialog.rs`) is laid out with
hard-coded pixel widths and heights. On real desktops this breaks.

- **(A) Truncated label.** With DejaVu Sans (a common Linux desktop font),
  "Show warning above (MB):" in *Large Files* is cut and runs into its input
  field ("Show warning above (MB50"). With Noto Sans it fits with 0 px to
  spare. Screenshot:
  `ferrispad-wt/uitest-fit/run/settings-dejavu-x1-s16-Light.png`.
- **(B) Dialog taller than the screen.** The dialog is a fixed 650 px tall,
  812 px at 125 % scaling. On an 800 px-tall work area (e.g. a 1366×768
  laptop at 125 %) **Save/Cancel are cut off**, so the user cannot save their
  settings. Screenshot:
  `ferrispad-wt/uitest-fit/run/settings-host-x1.25-s16-Dark.png`.

Both bugs are pre-existing (present on `master`). They were found during the
T0077–T0082 UI verification, after the user caught label overflow in the T0080
theme manager.

## In scope

- Size every label column, button and input row of the Settings dialog from
  the **measured** label text (`fltk::draw::measure` with the widget's label
  font and size) plus padding, never from fixed widths. Use the shared helper
  T0080 introduces for this, if it exists.
- Keep Save/Cancel always reachable. When the content is taller than the
  screen work area (`app::screen_work_area` of the screen the dialog opens
  on, minus decorations), the content area scrolls (themed scrollbar via
  `style_scroll`). The Save/Cancel row stays fixed at the bottom, outside the
  scroll.
- The layout decision is a pure, unit-testable function: given the content
  height, the button-row height and the work-area height, it returns the
  dialog height, whether the content scrolls, and the scroll viewport.

## Out of scope

- Redesigning the Settings dialog (sections, tabs, wording).
- Other dialogs. The T0080 dialogs are covered by T0080's own fit test; any
  further dialog is a follow-up.

## Edge cases (each one is covered by a test or a manual step below)

1. A wide desktop font (DejaVu Sans) and the +2 pt variant: every label fits
   inside its widget, and no sibling widgets overlap.
2. A work area smaller than the content, at 125 % / 150 % scaling or 768 px:
   the content scrolls, Save/Cancel are fully visible, and the dialog is never
   taller than the work area.
3. A work area larger than the content: no scrollbar, the same look as today.
4. Light and dark themes: the scrollbar and the scroll background are themed
   like the rest of the dialog.
5. Keyboard: Tab still reaches every field inside the scroll area, and the
   focused field scrolls into view.

## How to test

### Red test first

Apply the T0080 "label fits + no sibling overlap" helper (`tests/ui_dialogs.rs`)
to the **whole** Settings dialog, including the +2 pt font variant. Before
any layout change, run it under `xvfb-run` with DejaVu Sans as the sans font
and record the failure here: it must name the "Show warning above (MB):"
label. Then:

```bash
nix develop -c cargo test settings_dialog_layout
xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 nix develop -c cargo test --test ui_dialogs settings
```

- `settings_dialog_layout_scrolls_when_taller_than_work_area` (pure): content
  900, work area 768 → scrolls, dialog ≤ 768, buttons inside.
- `settings_dialog_layout_no_scroll_when_it_fits` (pure).
- `settings_dialog_labels_fit` (UI binary): the fit + overlap helper over the
  built dialog, at the default size and at +2 pt.

### Manual / Xvfb check (Claude drives this first; record results here)

Use Xvfb + openbox with the T0080 font setups (host Noto, forced Noto, forced
DejaVu), `FLTK_SCALING_FACTOR` 1, 1.25 and 1.5, a 1366×768 screen, light and
dark. Inspect every screenshot edge by edge and list what was checked:

1. No label is clipped or runs into its field. "Show warning above (MB):" is
   fully readable with DejaVu.
2. At 1366×768 and 125 %/150 %: Save/Cancel are visible. Scroll down, change
   a value, Save → the value is persisted.
3. At 1920×1080 and 100 %: no scrollbar, the same as before.
4. Tab through all fields: the focused field scrolls into view.

User (pending): the same on the real GNOME/Wayland desktop.

## Acceptance criteria

- [x] No clipped or overlapping text in the Settings dialog with Noto or
      DejaVu, at +0/+2 pt, at 100/125/150 % scaling.
- [x] Save/Cancel are always visible; the content scrolls when it is taller
      than the work area.
- [x] The red fit test was seen failing on the old layout, with the output
      recorded here.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass; CI is green on the PR.
- [x] The Xvfb screenshots are listed here, with an edge-by-edge note per
      setup.

## What was actually done

- **Red test** `tests/settings_dialog_layout.rs::settings_dialog_labels_fit`
  (UI binary, one `#[test]`). It builds the whole dialog with
  `build_settings_dialog` (T0080's split) on a light and a dark background,
  then runs `layout_failures(.., everything)`: inside labels at +0/+2 pt,
  outside labels at +0/+2 pt, and sibling overlaps. It also checks that
  Save/Cancel are inside the dialog and that the dialog is no taller than the
  work area. xvfb-run's default 640×480 screen forces the scrolling layout.
- **Shared helper strengthened** (`tests/support/ui_layout.rs`). It now
  also checks **outside** labels (`Align::Left` etc. without `Inside`), the
  gap the review found:
  - `outside_label_rect`, a pure function, mirrors
    `Fl_Group::draw_outside_label`: a box between the parent's edge and the
    widget, text aligned toward the widget, 3 px gap, no clipping;
  - `outside_label_failures` flags such a label when it leaves its parent
    group, runs into a sibling widget, or overlaps another outside label;
  - it is part of `layout_failures`, so the T0080 dialogs get it too (they
    pass);
  - pure tests `outside_label_rect_sides`,
    `outside_label_rect_skips_inside_and_centered` and
    `outside_label_rect_spills_and_collides`.
  Note: in this dialog "Show warning above (MB):" is an *inside* label of a
  150 px `Frame` placed left of its `Input`, so the existing inside-label
  check already caught it.

### Red run (before any layout change; Xvfb, FLTK_BACKEND=x11)

Default fonts (Noto Sans):
```
settings(239, 241, 245)/0:""/30:"Show warning above (MB):": label needs 156px at +0pt, widget is 150px
settings(239, 241, 245)/0:""/30:"Show warning above (MB):": label needs 186px at +2pt, widget is 150px
settings(239, 241, 245)/0:""/32:"Max editable size (MB):": label needs 163px at +2pt, widget is 150px
(the same three for the dark dialog)
test result: FAILED
```
DejaVu Sans (`FONTCONFIG_FILE=…/uitest-T0080/fonts-dejavu.conf`):
```
settings(239, 241, 245)/0:""/30:"Show warning above (MB):": label needs 170px at +0pt, widget is 150px
settings(239, 241, 245)/0:""/9:"Change...": label needs 84px at +2pt, widget is 80px
settings(239, 241, 245)/0:""/30:"Show warning above (MB):": label needs 196px at +2pt, widget is 150px
settings(239, 241, 245)/0:""/32:"Max editable size (MB):": label needs 171px at +2pt, widget is 150px
(the same four for the dark dialog)
test result: FAILED
```
Full output: `ferrispad-wt/uitest-fit/red-output.txt` (before the outside
check) and `red-output2.txt` (after it). After the fix the test passes with
both fonts.

### The fix

- **Measured widths** (`SettingsMetrics::measure`, in
  `settings_dialog.rs`). Widths come from T0080's `max_label_width`
  (installed fonts, +2 pt slack, padding):
  - each column is as wide as its widest row (section labels, check/radio
    rows with box and indent, "Manage themes…", the font row), never
    narrower than the historical 280 px;
  - the Large Files label cell (≥ 150 px), with its inputs placed after it;
  - "Change..." (≥ 80 px); the font input takes the rest of the row;
  - Save/Cancel (≥ 90 px);
  - the right column's x position and the dialog width follow from the
    column widths.
- **Scroll.** The content is a vertical `Scroll` (themed with
  `style_scroll`, flat background). Save/Cancel stay outside it, in a fixed
  row at the bottom.
  - After building, the dialog fits the work area of its screen:
    `settings_layout::dialog_height(content_h, work_h)`, which allows 40 px
    for the title bar.
  - When the content scrolls, the dialog is widened by the scrollbar
    (`dialog_width`), and `dialog_top` places it.
  - The window uses `make_resizable(false)` before resizing, so the children
    keep their measured positions.
- **Focus follows.** In the dialog loop (after each event, no timer), a
  focused widget inside the scroll that lies outside the viewport is
  scrolled into view (`settings_layout::scroll_to_show`).
- Pure tests (`cargo test settings_dialog_layout`):
  `settings_dialog_layout_scrolls_when_taller_than_work_area`,
  `settings_dialog_layout_no_scroll_when_it_fits`,
  `settings_dialog_layout_degenerate_work_area` (it found an `i32`
  overflow, fixed with saturating arithmetic),
  `settings_dialog_layout_columns_grow_with_measured_labels`,
  `settings_dialog_layout_scroll_follows_focus` and
  `settings_dialog_layout_width_and_top`.

### Verification recipe

```bash
nix develop -c cargo test settings_dialog_layout
xvfb-run -a env FLTK_BACKEND=x11 FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test settings_dialog_layout --test ui_dialogs
# and with a wide font:
FONTCONFIG_FILE=<a conf aliasing sans/Helvetica to DejaVu Sans> xvfb-run -a env ... (same)
```

### Xvfb/X11 results (driven by Claude)

Setup: debug build, Xvfb + openbox. Three font setups (host default =
Noto Sans; Noto Sans forced; DejaVu Sans forced, via `FONTCONFIG_FILE`),
`FLTK_SCALING_FACTOR` 1, 1.25 and 1.5, light and dark. Screenshots are in
`/home/fedro/Code/personal/ferrispad-wt/uitest-T0083/run-1366x768/` and
`run-1920x1080/`; the interaction run is in `run-interact/` (scripts
`uitest-T0083/shots.sh` and `interact.sh`).

1. All 18 combinations at 1366×768 and the 6 at 1920×1080: the dialog fits
   the screen (automated check) — **pass**. Checked edge by edge in
   `settings-dejavu-x1-Light`, `settings-dejavu-x1.25-Dark`,
   `settings-noto-x1.5-*`, `run-interact/01-top` and the 1920×1080 set:
   - section labels;
   - each radio and check label next to its box, with no touching and no
     clipping;
   - "Manage themes…" inside its button;
   - the font input, then a gap, then "Change...", fully inside its
     button;
   - "(6–96)" after the size field;
   - the info text (two lines);
   - "Show warning above (MB):" and "Max editable size (MB):" fully
     readable, with a gap before their inputs (the original bug, fixed with
     DejaVu);
   - Save/Cancel labels inside their buttons;
   - the right column clear of the dialog edge and of the scrollbar.
   All **pass**.
2. 1366×768 at 150 % with DejaVu: the dialog is 948×708 at y=58 and
   Save/Cancel are visible. The wheel scrolls the content (`02-scrolled`).
   Tab moves the focus to "Max editable size", which is scrolled into view
   (`03-tab-to-last-field`). Typing 300 and clicking Save persists
   `max_editable_size_mb = 300` — **pass**.
3. 1920×1080 at 100 %: no scrollbar, the same layout as before (620 px
   wide, 579 px tall, slightly shorter than the old fixed 650 because the
   empty bottom band is gone) — **pass**.
4. Tab through the fields: the focus cycles through the text inputs, and
   the focused one is always visible (`05-tab-10..40`) — **pass**. Note: in
   FerrisPad only text inputs take the keyboard focus, because
   `set_visible_focus(false)` is set app-wide (pre-existing). Check boxes,
   radios and choices were never keyboard-reachable; that is unchanged.
5. Light and dark: the scrollbar and the scroll background use the dialog
   theme — **pass**.
6. Cosmetic: at 1366×768 and 125 % the content exceeds the viewport by only
   a few pixels. The dialog is widened by the 12 px scrollbar allowance, but
   FLTK does not show the bar. The right margin is 12 px wider; nothing is
   cut.

**Pending user verification:** the same on the real GNOME/Wayland desktop.

## Affected files

- `src/ui/dialogs/settings_dialog.rs` — measured layout, scroll container,
  fixed button row (`DIALOG_WIDTH`/`DIALOG_HEIGHT` constants at `:20-21`).
- `tests/settings_dialog_layout.rs` (new) — the Settings fit test.
- `tests/support/ui_layout.rs` — outside-label check (shared helper).
- `src/ui/dialogs/settings_layout.rs` (new) — pure height/scroll/width rules.
- `src/ui/dialogs/mod.rs` — module registration.

## Notes

- Found by the T0081 layout sweep (`ferrispad-wt/uitest-fit/`), 2026-11-26.
- Stacked on `ticket/T0077`, so the user verifies it in the same final build.
  It lands last.
