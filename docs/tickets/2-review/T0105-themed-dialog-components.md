---
id: T0105
title: Themed dialog widgets as reusable components, so no dialog can forget a themed property
status: review          # todo | review | done
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

- Plain push buttons: `theme.button(label)` exists, but the 63 buttons
  (many with icon or custom styles) are not all converted, and the guard
  does not cover `Button`. Follow-up candidate.
- Dialogs that cannot be built without running them (find, goto, font
  picker, session picker, plugin dialogs, shortcut dialog, large file,
  viewer, update). Guard (a) covers them statically, and the Xvfb campaign
  below exercises their inputs. Extracting `build_*` functions for guard (b)
  is a follow-up.

## How to test

```bash
nix develop -c cargo test dialogs_build_widgets_only     # guard (a)
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test ui_dialogs            # guard (b)
```

## Acceptance criteria

- [ ] Every dialog input selects in the shared selection with readable text,
      and draws its cursor in the text color, in light and dark.
- [ ] Guards (a) and (b) are in place; both failed before.
- [ ] No layout change (the existing layout checks still pass).
- [ ] Full gates green; Xvfb campaign inspected.

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
