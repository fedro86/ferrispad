---
id: T0097
title: Make the selected row of dialog lists clearly visible in light and dark themes
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 179
---

## Goal

In dark themes the selected row of the lists in FerrisPad's dialogs is barely
visible. The lists use `DialogTheme::button_bg` as the selection color on an
`input_bg` list background, and a3 measured **1.39:1** between the two in the
theme manager. The same pair is used by the font picker, the session picker
and the themed file chooser. A row the user picked must stand out clearly in
every built-in theme, light and dark, also when the list does not have
keyboard focus, and its text must stay readable.

## In scope

- One place picks the colors: `DialogTheme` gets `list_selection_bg` and
  `list_selection_text`.
  - The background starts from `button_bg` and is pushed away from
    `input_bg` until it reaches **≥ 3:1** (WCAG, `ui::color::wcag_contrast`).
  - The text is the theme's text color if that reads at **≥ 4.5:1** on it,
    otherwise black or white, whichever reads better.
- The text FLTK draws on a selected row is `fl_contrast(textcolor,
  selection)`. At FerrisPad's level 10 that keeps light text on a mid-tone
  bar. The T0078 contrast function (`ui::contrast`) therefore also registers
  the dialog list selection color and applies its WCAG rule there, as it
  does for the editor selection, so the drawn text is exactly
  `list_selection_text`.
- One helper styles a dialog list (background, selection color, registration
  for the contrast rule). The theme manager, font picker, session picker and
  themed file chooser all use it.

## Out of scope

- The main-window panels (diagnostic panel, file tree): they are not dialog
  lists and have their own styling. Follow-up if they show the same problem.
- The editor selection (T0078) and menu highlight colors.

## How to test

### Regression test

```bash
nix develop -c cargo test dialog_list_selection
```

- Red: `dialog_list_selection_stands_out_in_every_theme` fails on the current
  colors. In the dark themes `button_bg` on `input_bg` is below 3:1 (≈1.4:1
  in the theme manager).
- Green: for every built-in theme and a sweep of synthetic backgrounds, the
  selection reaches ≥ 3:1 against the list background, and the text FLTK
  draws on it (through `ferrispad_contrast`, with the registration) reaches
  ≥ 4.5:1.

### UI (Claude, Xvfb)

Take screenshots of every dialog with a list, in a light theme and a dark
theme, with a row selected: theme manager, font picker, session picker and
the themed file chooser. Take the shot once with the list focused and once
with focus on another widget. Inspect each one edge by edge.

## Acceptance criteria

- [ ] The selected row reaches ≥ 3:1 against the list background in every
      built-in theme (light and dark), focused or not.
- [ ] The text on it reaches ≥ 4.5:1.
- [ ] All four dialog lists use the shared colors; no per-dialog color.
- [ ] Red test failed before; full gates green; Xvfb screenshots inspected.

## Affected files

- `src/ui/dialogs/mod.rs`: `DialogTheme` fields + the list-styling helper.
- `src/ui/contrast.rs`: register the list selection color for the WCAG rule.
- `src/ui/dialogs/theme_manager.rs`, `font_picker.rs`, `session_picker.rs`,
  `src/ui/file_dialogs.rs`: use the helper.

## Notes

- Origin: a3's measurement during the 0.9.10 review (theme manager list,
  1.39:1). Targets 0.9.11. It is developed and reviewed now, merged after
  0.9.10 final.

## What was done (2026-11-26)

- `dialogs::list_selection_colors(list_bg, base, text, is_dark)` is pure and
  steps `button_bg` away from `input_bg`: lighter in dark themes, darker in
  light ones, and the other way when that direction cannot reach 3:1 (a
  mid-tone list). It stops at the first color with ≥ 3:1. The text keeps
  the theme text if that reads at ≥ 4.5:1, otherwise black or white.
- `DialogTheme` gains `list_selection_bg` / `list_selection_text` (computed
  in `from_theme_bg`), `list_selection()` and `style_list(&mut list)`.
  `style_list` sets `input_bg`, the selection color and registers it with
  the contrast rule.
- `ui::contrast`: `EditorSelectionBgs` gains `list_selection`
  (`with_list_selection`). `contains` covers it, so FLTK's `fl_contrast` on
  a selected row applies the same WCAG rule as on the editor selection. A
  `LIST_SELECTION_BG` atomic is fed by `register_list_selection`.
- Theme manager, font picker and session picker call `theme.style_list`.
  The themed file chooser uses `theme.list_selection()` and registers it.
  Its existing readability test now models the registered selection.

### Red run (before)

```
test ui::dialogs::tests::dialog_list_selection_stands_out_in_every_theme ... FAILED
Base16OceanDark: row (68, 72, 80) on list (90, 93, 101) = 1.39:1, …
Base16OceanLight: row (179, 182, 188) on list (239, 241, 245) = 1.80:1, …
SolarizedDark: row (38, 68, 76) on list (63, 90, 97) = 1.41:1, …
SolarizedLight: row (215, 164, 109) on list (253, 246, 227) = 2.07:1, …
gray 15: row (48, 48, 48) on list (72, 72, 72) = 1.44:1, …
```

After the fix it passes for all 7 built-in themes and the 54-color sweep
(grays, blue-ish and warm tints). Full suite under Xvfb
(`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 582 passed, every binary green; fmt
and clippy `-D warnings` clean.

### Xvfb (Claude), before = master build, after = this branch

In each dialog a row is selected by a click (list focused), then the focus
moves to an input, or for the theme manager to an empty spot of the
dialog. The selected-row and list-background pixels are measured from the
screenshots, WCAG:

| Theme (Ocean) | Dialog | Before | After (focused / unfocused) |
|---|---|---|---|
| Light | session picker, file chooser, font picker, theme manager | 1.80:1 | **3.32:1 / 3.32:1** |
| Dark | session picker, file chooser, font picker, theme manager | 1.39:1 | **3.07:1 / 3.07:1** |

The selected row is identical with and without focus, since FLTK's browser
draws its selection independently of focus. Its text is black on the
gray bar in both modes, also in the session picker, whose rows use an
explicit `@C` text color. Screenshots were inspected edge by edge: no
layout change, nothing clipped or overlapping.

## Review follow-up (a3 nit, 2026-11-26)

The registered list selection could lag after a theme switch until the next
dialog list was styled. That was harmless, but inexact. Now
`dialogs::register_dialog_list_selection(theme_bg)` derives the dialog
theme's list selection and registers it, and `theme::apply_syntax_theme_colors`
calls it next to `register_editor_selection`. Every re-theme ends there
(startup, dark-mode toggle, Settings, live preview and its revert).

- Red: `dialog_list_selection_follows_every_retheme` failed before ("every
  re-theme must refresh the dialog list selection").
- Green: it checks the call in `apply_syntax_theme_colors` and that, for
  three backgrounds in turn, the registered color is that theme's
  `list_selection_bg`.

## Design choice: which theme color the selection starts from (user, 2026-11-26)

The user asked that the selection be **relative to the theme** (no fixed
blue, no plain gray). Three theme-derived variants were compared, all under
the same rule (≥ 3:1 against the list, text ≥ 4.5:1). Each was rendered with
a local-only experiment build (an env switch, never committed) across Base16
Ocean Dark, Base16 Mocha Dark, Solarized Dark, a strongly colored custom
theme (bg `#3b1f5c`), Base16 Ocean Light, Solarized Light and InspiredGitHub:

- **A**: the dialog button color pushed to 3:1 (this implementation).
  Neutral on the neutral themes (Ocean, GitHub), tinted on the tinted ones.
  Ratios 3.00–3.32:1.
- **B**: the dialog accent (a complementary hue) pushed to 3:1. Strongly
  colored, often clashing (gold, lime, salmon). Ratios 3.09–6.55:1.
- **C**: the syntax theme's own `.tmTheme` selection pushed to 3:1. Closest
  to the theme's native selection hue, but it needs the active theme's
  selection passed into `DialogTheme`. Ratios 3.02–3.32:1.

Montages (rows = themes; theme manager list and font picker list with a row
selected) and pixel-measured ratios:
`uitest-T0097/variants/variant-A.png`, `variant-B.png`, `variant-C.png`,
`ratios.txt` (in the `ferrispad-wt` worktree directory, not in the repo).

**The user chose A, 2026-11-26.** The experiment branch was deleted.
