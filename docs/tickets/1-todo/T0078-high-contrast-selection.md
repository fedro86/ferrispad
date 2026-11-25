---
id: T0078
title: Use a high-contrast blue text selection in the default light and dark themes
status: todo            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: 150              # GitHub issue number
---

## Goal

Text selection in the editor is hard to see. Today the editor colors come only
from the theme (light, dark or system) and from seven fixed syntax themes in
`src/app/domain/settings.rs`, and the selection color is derived as a small
lighten/darken step of the theme background, so it barely differs from it.
Users want a clearly visible blue selection on both light and dark
backgrounds, like in Windows Notepad or web browsers.

## In scope

- Replace the editor's background-derived selection color with a clearly
  visible blue in the default light theme and in the default dark theme.
- The chosen colors reach a WCAG contrast ratio of at least 3:1 between the
  selection color and the editor background in each of those themes, and
  selected text stays readable on top of the selection.
- Factor the selection-color choice into a small pure function (input: theme
  background / dark flag; output: RGB) so the contrast requirement is
  unit-tested without FLTK.
- Apply the same selection color wherever the editor is (re)themed, so it does
  not revert on theme toggle, system-theme change, or syntax-theme preview.

## Out of scope

- User-configurable colors, `settings.json` fields, and a reset button — that
  is T0079, which builds on this ticket.
- A full syntax-theme editor and importing external themes.
- Selection colors of non-editor widgets (tree panel, dialogs, menus, file
  dialogs).

## How to test

### Unit test

Add, next to the new selection-color function (e.g. in `src/ui/theme.rs`'s
`#[cfg(test)]` module):

```bash
nix develop -c cargo test editor_selection_contrast
```

- `editor_selection_contrast_light_default` and
  `editor_selection_contrast_dark_default`: compute the WCAG 2 contrast ratio
  between the selection color and the default light / dark editor background
  and assert it is `>= 3.0`.
- Before the change: the same check against the current derivation
  (`lighten(0.10)` / `darken(0.90)` of the background) is well below 3:1 —
  record the measured ratios here when implementing.
- After the change: both pass, and the full `nix develop -c cargo test` stays
  green.

No new settings fields are added, so there is no persistence test in this
ticket; the visual result is covered by the manual check below.

### Manual check (record results here)

1. `nix develop -c cargo run`, open any text file with several lines.
2. Settings → Theme → **Light**. Select a few lines with the mouse and with
   Shift+arrows. Expect: an obviously blue selection band; the selected text
   is still readable. (Before: a faint grey band close to the background.)
3. Settings → Theme → **Dark**. Repeat. Expect: an obviously blue selection on
   the dark background, selected text readable.
4. Settings → Theme → **System default**. Repeat under the OS light and dark
   appearance (where the OS supports switching). Expect: the selection matches
   step 2 / step 3 respectively.
5. Toggle between Light and Dark a few times and preview another syntax theme
   in Settings. Expect: the selection never falls back to the old faint color.

## Acceptance criteria

- [ ] In the light and dark themes, selected text is clearly distinguishable,
      with a WCAG contrast ratio of at least 3:1 between selection and
      background.
- [ ] The contrast unit tests are added and green.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features` and
      `cargo fmt --check` pass with zero warnings.
- [ ] The ticket documents the manual check with `cargo run` on the light,
      dark and system themes.

## Affected files

- `src/ui/theme.rs:166-197` — `apply_theme`: replace the `sel_color`
  derivation (`bg_rgb.lighten(0.10)` / `bg_rgb.darken(0.90)`) with the new
  high-contrast function.
- `src/ui/theme.rs:83` — `apply_syntax_theme_colors` (live syntax-theme
  preview): make sure it does not leave a stale or low-contrast selection.
- `src/ui/color.rs` — `ThemeRgb` helpers; a WCAG luminance / contrast helper
  may live here (a test-only one already exists in
  `src/ui/file_dialogs.rs:298`, which could be promoted and reused).

## Notes

- Origin: user request, GitHub issue #150.
- Issue #150 is shared with T0079 (customizable editor colors): the request
  exceeded the ~5-bullet ticket scope, so it was split into this ticket
  (high-contrast default selection) and T0079, which depends on this one.
- Open question: the editor background varies with the selected syntax theme
  (seven themes). The requirement is pinned to the default light and dark
  themes; checking the other syntax themes against the same 3:1 bar is
  welcome but not required here.
