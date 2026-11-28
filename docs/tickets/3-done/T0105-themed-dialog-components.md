---
id: T0105
title: Themed dialog widgets as reusable components, so no dialog can forget a themed property
status: done            # todo | review | done
created: 2026-11-26
severity: moderate
area: ui
depends-on: [T0097]     # the shared selection color; land together with T0097
issue: 191
---

## Goal

In Settings, selecting text in Font / Font size uses the theme's selection,
but the two **Large Files** inputs select in FLTK's default **navy**
`(0,0,128)` with poor text contrast. Several dialog inputs also draw a
**black cursor on a dark field**.

The root cause is not those two fields: there was no component. Every dialog
built its inputs, choices, check buttons and lists by hand (24 hand-built
inputs, ~46 scattered `set_selection_color` calls) and repeated the recipe,
forgetting parts of it. T0098's black swatch was the same class of mistake.
The goal is to make the mistake impossible.

## In scope

- `ui::dialogs::widgets`: one constructor per widget kind on `DialogTheme`,
  the only way dialogs create them. Each sets **every** themed property:
  - `input()` and `int_input()`, plus `style_foreign_input` for inputs FLTK
    builds itself: input background, text, **cursor = text**, frame, and the
    **shared selection** `list_selection_bg` (T0097, variant A), registered
    with the contrast rule. Selected text in any input then reads like a
    selected list row.
  - `text_display()` and `text_editor()`: the same colors as an input.
  - `list()` / `style_list` (moved here from T0097): the shared selection.
  - `choice()` and `style_foreign_menu`: input colors, menu highlight in the
    button color.
  - `check_button()` and `radio()`: dialog background and text label. The
    mark keeps FLTK's own color: it is drawn on FLTK's white `BACKGROUND2`,
    where a theme color can read poorly (the button color did in light
    themes).
  - `button(label)`, `progress()`, `style_tabs`, `style_color_chooser`.
- Every dialog uses them: settings, find/replace, goto line, font picker,
  session picker, plugin config/settings/permissions/manager, shortcut
  dialog, theme manager, color picker, large file, read-only viewer, update,
  themed popups, and the themed file chooser. The change is mechanical, with
  no layout change. Calls keep `.with_pos(..).with_size(..)` chains.
- Two guards:
  - (a) **source guard** `dialogs_build_widgets_only_through_the_components`:
    outside `widgets.rs`, no dialog source (nor `file_dialogs.rs`) constructs
    an input, int input, text view/editor, list, choice, check/radio button
    or progress bar raw, nor calls `set_selection_color` /
    `set_cursor_color`;
  - (b) **structural UI test** in `tests/ui_dialogs.rs` (under Xvfb in CI):
    it builds the theme manager, both color pickers, the name prompt and
    Settings in dark **and** light, walks every widget, and requires every
    text input's background, text, cursor and selection to equal the
    theme's (`support::ui_layout::unthemed_inputs`).

## Out of scope

- Guard (b) for the dialogs that cannot be built without running them
  (find, goto, font picker, session picker, plugin dialogs, shortcut dialog,
  large file, viewer, update). Guard (a) covers them statically, and the
  Xvfb campaigns exercise them. **T0106** (text below) adds the `build_*`
  seams.

## How to test

```bash
nix develop -c cargo test dialogs_build_widgets_only     # guard (a)
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test ui_dialogs            # guard (b)
```

## Acceptance criteria

- [x] Every dialog input selects in the shared selection with readable text,
      and draws its cursor in the text color, in light and dark.
- [x] Guards (a) and (b) are in place; both failed before.
- [x] No layout change (the existing layout checks still pass).
- [x] Full gates green; Xvfb campaign inspected.

## Affected files

- `src/ui/dialogs/widgets.rs` (new): components + guard (a).
- `src/ui/dialogs/*.rs`, `src/ui/file_dialogs.rs`: use the components.
- `tests/support/ui_layout.rs`, `tests/ui_dialogs.rs`: guard (b).

## Notes

- Origin: the user hit the navy selection in Settings during rc.2
  verification and asked for reusable components "that don't keep making
  the same mistakes".
- A latent T0097 issue fixed here: the themed file chooser set its whole
  list's text color to `list_selection_text`. That is black in dark themes,
  and it only looked right because `Color::from_rgb(0,0,0)` encodes FLTK's
  foreground (T0098's bug). Once T0098 lands, the chooser's rows would turn
  black on dark. The list's text is now `theme.text`; the selected row's
  text comes from the contrast rule.

## What was done (2026-11-26)

### Red runs

- Guard (b) on the T0097 base:

  ```
  settings dark/…/31, …/33: input selection (0, 0, 128) != theme (176, 177, 180)   ← Large Files
  settings dark/…/8, /11, /31, /33: input cursor (0, 0, 0) != theme (230, 230, 230)
  picker dark/2: input cursor (0, 0, 0); selection (68, 72, 80) != (176, 177, 180)
  settings light/…: selection (179, 182, 188) / (0, 0, 128) != theme (129, 132, 136)
  name prompt: selection (68, 72, 80) != (176, 177, 180)
  ```

- Guard (a): on the T0097 base the patterns match **98** lines in 17 files
  (settings 28, find 14, font picker 8, …).

### After

- Both guards pass. Full suite under Xvfb: lib 584 passed, every binary
  green (the existing layout checks unchanged). fmt and clippy `-D warnings`
  clean.
- The transform (constructor swap + removal of the now-redundant property
  lines) removed 158 lines, all listed in the PR for review; site-specific
  deviations (a dimmed placeholder color, text sizes, a disabled plugin's
  dimmed label) were kept.

### Xvfb campaign (Claude): text selected in every dialog input, dark and light

Montage: `ferrispad-wt/uitest-T0105/montage/t0105-inputs.png` (one row per
input, dark | light; crops and full screenshots in `uitest-T0105/run/`,
driver `campaign.sh` / `viewer.sh` / `picker.sh`). Each input was focused,
filled when empty, and selected with End, Shift+Home (the shortcut field,
which captures keys, with a triple click):

- Settings: Font, Font size, Large Files warning and max. **Before**
  (T0097 base): the Large Files fields select in FLTK's **navy** in both
  modes, and Font / Font size use the old low-contrast `button_bg`.
  **After**: all four use the shared selection.
- Find, Replace (both fields), Go to Line, New Session Window name, Sessions
  "New:", font picker search and size, key-shortcut field, Duplicate name
  prompt, color picker hex **and** its value fields, Plugin Manager search,
  large-file "Open Lines" start / end, and the read-only viewer's page /
  line / search fields plus a text selection in its view.

After the fix every field selects in the shared bar (light gray in dark,
mid-gray in light) with readable dark text, and draws its cursor in the
text color: no more black caret on a dark field. Found and fixed during
the campaign: the color chooser's FLTK value fields still selected in
`button_bg`; `style_color_chooser` now gives them the shared selection
(its mode menu keeps the button color). Inspected edge by edge: no layout
change, nothing clipped.

## Round 2 (orchestrator + a3, 2026-11-26): buttons and a broader guard

- **Buttons are components too.** All 62 plain dialog buttons go through
  `theme.button()` (flat, button color), `theme.secondary_button()`
  (Cancel/Close, secondary color, **normal** text) or
  `theme.warning_button()` ("Install Anyway", `warning_text()` label). The
  popups' non-default buttons use `secondary_button()`. The Plugin Manager
  row buttons are `button()` plus their 11 pt label. `style_dialog_button`
  and `style_row_button` are gone. Shortcut-dialog section bars use
  `theme.header_bar()`; the file chooser's own menu button uses
  `style_foreign_menu_button`.
- **Guard (a) is a regex now** (a3): any
  `\w*(Input|Output|Browser|Choice|Button|Display|Editor|Progress|Spinner)::(new|default)(`
  outside `widgets.rs`. That also covers `Output`, `MultilineOutput`,
  `Select/Multi/Browser`, `MenuButton`, `Spinner`, `ValueInput`,
  `FileInput`, `ReturnButton`. Hand styling is also refused:
  `.set_selection_color(`, `.set_cursor_color(`, `button_bg)`,
  `set_frame(FrameType::RFlatBox)`. Comments and `#[cfg(test)]` code are
  skipped. Mutation check: an `Output::default()` and a `Button::new(..)`
  added to `goto_line.rs` were both reported, then reverted.

### Intended visible changes (for the screenshot reviewer)

1. Inputs that had FLTK's default `DOWN_BOX` (find/replace, shortcut
   fields, …) are flat now (`FlatBox`): no 3D border, text ~2 px left.
2. Buttons that had the default raised frame (Find, Replace, Go to Line,
   Sessions, the font/size "Change…", plugin dialogs, update) are flat like
   the rest (Settings, About, Plugin Manager already were).
3. Secondary buttons (large-file and viewer Cancel/Close) draw their label
   in the normal text color, no longer dimmed (an active button must not
   look disabled; `active_buttons_looking_disabled` enforces it).
4. Preselected or selected input text uses the shared selection (the file
   chooser's Filename, the Duplicate name prompt).
5. The Find check buttons keep FLTK's mark color (drawn on FLTK's white
   box) instead of the button color.
6. Input labels use the theme text color.

### Screenshot diff, every converted dialog (Claude, Xvfb)

Before = T0097 base, after = this branch, dark and light, full screen,
`compare -metric AE -fuzz 2%`. Shots, diff images and before | after | diff
triptychs are in `ferrispad-wt/uitest-T0105/diff/` (`trip/*.png`,
`all-dark.png`, `all-light.png`, `ae.txt`):

| Dialog | AE dark / light | Explained by |
|---|---|---|
| theme manager, plugin manager | 0 / 0 | identical |
| read-only viewer | 36 / 32 | 3 (Close label) |
| large-file dialog | 44 / 39 | 3 (Cancel label) |
| Settings | 99 / 106 | 2 ("Change…" flat) |
| color picker | 188 / 318 | 1, 2 |
| font picker | 232 / 249 | 1, 2 |
| goto line, session name | 262–419 | 1, 2 |
| find, sessions | 460–771 | 1, 2, 5 |
| name prompt | 1380 / 567 | 4 (preselected name) |
| replace | 839 / 1283 | 1, 2 |
| file chooser | 2973 / 1200 | 4 (preselected Filename) |
| shortcuts | 2039 / 4430 | 1 (every shortcut field) |

Each diff image was inspected: the changed pixels are only on input
borders, button frames, the dimmed labels and the preselected text. No
geometry moved, and the existing layout checks pass.

Gates: fmt and clippy `-D warnings` clean. Full suite under Xvfb: lib 584,
every binary green.

## T0106 (follow-up ticket text, ID reserved, not implemented here)

> **T0106: `build_*` seams so the structural theme test covers every dialog.**
> Goal: guard (b) of T0105 (`unthemed_inputs` in `tests/ui_dialogs.rs`)
> only reaches dialogs that have a pure `build_*` function (theme manager,
> color picker, name prompt, Settings). Find/replace, goto line, font
> picker, session picker and name prompt, shortcut dialog, plugin
> config/settings/permissions/manager, community install, large-file,
> read-only viewer and update build and run in one function, so a themed
> property they get wrong would only show up in Xvfb.
> In scope: split each into `build_<dialog>(theme_bg, …) -> (Window, handles)`
> plus the existing run loop, with no behavior change. Call every builder in
> `tests/ui_dialogs.rs` in dark and light, with `unthemed_inputs` and the
> layout checks. Extend `unthemed_inputs` to buttons (frame, color, label
> color matching a `button()` / `secondary_button()` / `warning_button()`)
> and lists.
> Out of scope: visual changes.
> Test: the extended `ui_dialogs` test fails on a mutated component (e.g.
> no cursor color) for each newly covered dialog.

## Round 3 (user + orchestrator, 2026-11-26): a thin theme-derived input border, Sessions overflow

- **Input border (user's choice).** In light themes an empty Find/Replace
  field was just a slightly lighter rectangle on the light-gray dialog,
  hard to locate. Text fields, text views and lists now draw a **1 px
  border** in `DialogTheme::input_border`: the button color pushed to
  **≥ 3:1 against the dialog background** (WCAG 1.4.11), by the same
  `color::push_to_contrast` rule that now also computes the list selection.
  FLTK's `BORDER_BOX` draws its border black and passes a box-drawing
  function only the fill color. The border is therefore a custom box type,
  `widgets::INPUT_FRAME` (`FreeBoxType`, insets 1/1/2/2 like `BORDER_BOX`),
  drawn by one `extern "C"` callback. That callback looks the border up
  from the fill in a small lock-free registry
  (`register_input_border` / `input_border_for`). An unregistered fill
  (FLTK's inactive variant) gets a border derived from the fill itself. The
  callback is registered once, directly via `fltk_sys`, the same pattern
  and justification as `ui::contrast::install` (fltk-rs's wrapper
  transmutes a Rust-ABI fn pointer).
- **Read-only field**: `theme.readonly_input()` (the Settings Font field)
  draws no caret. FLTK's read-only `^` caret became visible once cursors
  were drawn in the text color.
- **Guard (b)** also checks every input's frame (`INPUT_FRAME`), its
  registered border (`== theme.input_border`), the read-only cursor, and
  that every list uses `INPUT_FRAME` and the shared selection. Mutation:
  with `set_input_frame` replaced by a flat frame, the test reported "input
  frame FlatBox != INPUT_FRAME" / "input border None != theme" for the
  picker, prompt and Settings inputs; then reverted.
- **Guard (a)**: cut only at `#[cfg(test)]\nmod tests` (a3's nit, done
  in 2094508).
- **Sessions picker overflow (a3, pre-existing)**: the button row ended
  5 px past the list (Cancel at x 345, list at 340). The dialog is now
  measured with `label_width` and has a `build_session_picker` seam:
  - list, New row and button row share one width, wide enough for every
    label at +2 pt;
  - spare width is spread over the buttons, and the row ends exactly on
    the list's right edge;
  - Cancel is `secondary_button()`.

  `tests/ui_dialogs.rs` now builds it in dark and light, with
  `layout_failures`, `unthemed_inputs`, and the row/list alignment. **Red**
  with the old geometry: "button "Cancel" ends at 345 past the list edge
  340", "button row not aligned", plus labels that did not fit ("New:"
  needs 46 px at +2 pt in 40 px, "New Window" 107 in 100, "Delete" 57 in
  55, "Cancel" 58 in 55). Green after.

### Xvfb: input border before | after (user review)

Montage: `ferrispad-wt/uitest-T0105/border/t0105-input-border.png` (rows in
`border/rows/`). Before = the approved flat state (2094508), after = this
round, 100 % crops, dark and light: Find, Replace, Go to Line, Sessions,
Settings, font picker, color picker, theme manager.

- Light: every field and list now has a visible 1 px border; empty fields
  are easy to locate.
- Dark: the border is subtle; the fields already stood out by their
  lighter fill, and the border still meets 3:1 against the dialog.

Lists got the border too, to stay consistent with the inputs (the
orchestrator asked to show it).

Gates: fmt and clippy `-D warnings` clean; full suite under Xvfb: lib 584,
every binary green.

Landing note: `widgets.rs` calls `Color::from_rgb` for the registry. When
this meets T0098 (whose guard forbids direct `Color::from_rgb`), switch
those calls to `fltk_rgb` in the merge.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration build `verify/rc3` (checklist VERIFICA-RC3.md, section 2): every field has the thin border, selected text looks the same in every dialog (Settings, Large Files, Find/Replace), buttons are flat, the Sessions button row stays inside, in light and dark.
