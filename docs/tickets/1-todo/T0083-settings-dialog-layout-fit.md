---
id: T0083
title: Settings dialog — labels must fit and the dialog must fit small screens
status: todo            # todo | review | done
created: 2026-11-26
severity: moderate      # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0077]     # ordering only: stacked on the T0077–T0082 verification build
issue: ~                # GitHub issue number, filled when work starts
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

- [ ] No clipped or overlapping text in the Settings dialog with Noto or
      DejaVu, at +0/+2 pt, at 100/125/150 % scaling.
- [ ] Save/Cancel are always visible; the content scrolls when it is taller
      than the work area.
- [ ] The red fit test was seen failing on the old layout, with the output
      recorded here.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass; CI is green on the PR.
- [ ] The Xvfb screenshots are listed here, with an edge-by-edge note per
      setup.

## Affected files

- `src/ui/dialogs/settings_dialog.rs` — measured layout, scroll container,
  fixed button row (`DIALOG_WIDTH`/`DIALOG_HEIGHT` constants at `:20-21`).
- `tests/ui_dialogs.rs` — the Settings fit test.

## Notes

- Found by the T0081 layout sweep (`ferrispad-wt/uitest-fit/`), 2026-11-26.
- Stacked on `ticket/T0077`, so the user verifies it in the same final build.
  It lands last.
