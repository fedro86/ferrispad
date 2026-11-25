---
id: T0078
title: Use a high-contrast blue text selection on every editor syntax theme
status: review          # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: 150              # GitHub issue number
---

## Goal

Text selection in the editor is hard to see. The selection color is derived
as a small lighten/darken step of the syntax theme background
(`src/ui/theme.rs`, `apply_theme`), so it barely differs from it. Measured
WCAG contrast today:

- default light theme (Base16 Ocean Light): **1.26:1**;
- default dark theme (Base16 Ocean Dark): **1.36:1**.

Users want a clearly visible blue selection on both light and dark
backgrounds, like Windows Notepad or web browsers.

**Which background counts.** `apply_theme` first paints the editor `White` /
`(30,30,30)`. Right after that, `AppState::apply_theme_to_owned_widgets`
(`src/app/state.rs:863`) calls `apply_syntax_theme_colors`, which repaints the
editor with the **active syntax theme's background**
(`SyntaxTheme::background()`). That syntax-theme background is the one the
user sees, and every requirement below is measured against it. The hard-coded
`White` / `(30,30,30)` values must not be used in the tests.

## In scope

- Replace the background-derived selection color with a pure function
  `editor_selection_color(bg: (u8,u8,u8)) -> (u8,u8,u8)` that returns a
  clearly blue color. It chooses the light-background or dark-background
  variant from the background's **luminance**, not from the dark-mode flag.
  Any of the seven syntax themes can be chosen for either mode in Settings, so
  a dark syntax theme can run in light mode and vice versa.
- For **all seven** `SyntaxTheme::all()` backgrounds, the selection meets the
  three contrast bars in *How to test* (focused, unfocused, selected text).
- Promote the test-only WCAG helpers from `src/ui/file_dialogs.rs:298`
  (`wcag_luminance`, `wcag_contrast`) to `src/ui/color.rs` and reuse them.
- Apply the color wherever the main editor is (re)themed, so it never reverts
  on a theme toggle, a system-theme change, a syntax-theme preview, or a
  Cancel in Settings.

## Out of scope

- Custom themes and color editing (T0079 model, T0080 window) and the
  current-line highlight (T0081). All three build on this ticket.
- Editors inside the split / diff-review panel (`src/ui/split_panel.rs`).
  They set no selection color today and use FLTK's default. Aligning them is
  a follow-up.
- Selection colors of non-editor widgets (tree panel, terminal, dialogs,
  menus, file dialogs) and FLTK's global selection color. The latter is also
  used for checkbox and radio marks; see the comment in `apply_theme`.
- A full syntax-theme editor and importing external themes.

## Edge cases (each one is covered by a test or a manual step below)

1. **Syntax theme ≠ mode.** Light mode with Solarized Dark, or dark mode with
   InspiredGitHub. The color follows the actual background luminance.
2. **Unfocused editor.** FLTK draws the selection of an unfocused
   `Fl_Text_Display` as `fl_color_average(color(), selection_color(), 0.4)`,
   i.e. 40 % background + 60 % selection. The real case: while the Find
   dialog has focus, the found match is shown as an *unfocused* selection.
   The blended color must still be visible.
3. **Selected-text readability.** For selected text FLTK replaces the text or
   syntax color with `fl_contrast(fg, selection)`, which falls back to black or
   white. So at least one of black/white must be readable on the selection.
   Inside the selection, syntax colors may become plain black or white; that
   is expected.
4. **Theme switching / preview / Cancel.** The selection must not fall back to
   the old faint color after a light↔dark toggle, an OS appearance change
   (System default), a live syntax-theme preview, or Cancel in Settings.

## How to test

### Unit tests

In `src/ui/theme.rs`'s `#[cfg(test)]` module (or next to
`editor_selection_color`):

```bash
nix develop -c cargo test editor_selection
```

Each test loops over **every** `SyntaxTheme::all()` background, and the
failure message names the theme and the measured ratio:

- `editor_selection_contrast_focused` — `wcag_contrast(sel, bg) >= 3.0`.
- `editor_selection_contrast_unfocused` — blended = 0.4·bg + 0.6·sel per
  channel (FLTK's rule); `wcag_contrast(blended, bg) >= 2.0`.
- `editor_selection_text_readable` — `max(wcag_contrast(sel, white),
  wcag_contrast(sel, black)) >= 4.5`.
- `editor_selection_is_blue` — the blue channel is the largest and exceeds
  red by a clear margin (≥ 60), so a future tweak cannot silently drift to
  grey.
- `editor_selection_follows_luminance_not_mode` — the dark-background
  variant is returned for `SolarizedDark`'s background and the light one for
  `InspiredGitHub`'s, independent of any mode flag (the function takes none).

**Red first:** run `editor_selection_contrast_focused` against the current
derivation (`lighten(0.10)` / `darken(0.90)` of the background) and record
the failing output here. The expected values are about 1.26 on Ocean Light and
1.36 on Ocean Dark.

Feasibility was checked: `(0,120,215)` on the three light backgrounds and
`(70,140,230)` on the four dark ones give focused contrast 3.68–4.50,
unfocused 2.19–2.47, and black/white readability ≥ 4.67. Final values are the
implementer's call, as long as the tests hold.

The full `nix develop -c cargo test` stays green.

### Manual check (record results here)

1. `nix develop -c cargo run`, open a text file with several lines.
2. Settings → Theme → **Light**. Select with the mouse and with Shift+arrows.
   Expect an obviously blue band with readable selected text.
3. Settings → Theme → **Dark**. Repeat. Same expectation.
4. Settings → Theme → **System default**, under the OS light and dark
   appearance where the OS can switch. Expect it to match step 2 / step 3.
5. Pick each of the seven syntax themes (including a dark syntax theme for
   light mode). The selection is blue and visible on each.
6. Open Find (Ctrl+F), search a word that occurs several times, and step
   through the matches while the Find dialog keeps focus. The match in the
   (unfocused) editor is clearly visible.
7. Toggle Light/Dark a few times; open Settings, preview another syntax theme,
   press **Cancel**. The selection never falls back to the old faint color.

## Implementation (what was actually done)

- `src/ui/color.rs`: `wcag_luminance` / `wcag_contrast` promoted from the
  test module of `src/ui/file_dialogs.rs` (now public, with a unit test
  `wcag_contrast_extremes_and_symmetry`); `file_dialogs.rs` tests import them.
- `src/ui/theme.rs`: new pure `editor_selection_color(bg) -> (u8,u8,u8)`.
  It picks the variant by WCAG luminance (`< 0.179`, the point where black
  and white text have equal contrast) — it takes no mode flag:
  - light background → `(0, 120, 215)`;
  - dark background → `(70, 140, 230)`.
- `apply_syntax_theme_colors` now sets the editor selection right next to
  the background it paints. Every re-theme path (startup `apply_settings`,
  dark-mode toggle, Settings save, live syntax preview and the revert on
  Cancel, system-default mode) goes through
  `AppState::apply_theme_to_owned_widgets`, which calls `apply_theme` and then
  `apply_syntax_theme_colors` with the same syntax-theme background, so the
  final selection always matches the final background. `apply_theme` uses the
  same function (no more `lighten(0.10)` / `darken(0.90)`).

### Red first (old derivation, recorded before the fix)

`editor_selection_color` was first introduced with the old derivation
(`lighten(0.10)` on dark / `darken(0.90)` on light background) and the new
tests were run:

```
---- ui::theme::tests::editor_selection_contrast_focused stdout ----
Base16OceanDark: selection (64, 68, 78) on background (43, 48, 59) has contrast 1.36:1 (< 3.0)
Base16OceanLight: selection (215, 216, 220) on background (239, 241, 245) has contrast 1.26:1 (< 3.0)
Base16EightiesDark: selection (66, 66, 66) on background (45, 45, 45) has contrast 1.37:1 (< 3.0)
Base16MochaDark: selection (78, 70, 61) on background (59, 50, 40) has contrast 1.36:1 (< 3.0)
SolarizedDark: selection (25, 64, 74) on background (0, 43, 54) has contrast 1.34:1 (< 3.0)
SolarizedLight: selection (227, 221, 204) on background (253, 246, 227) has contrast 1.26:1 (< 3.0)
InspiredGitHub: selection (229, 229, 229) on background (255, 255, 255) has contrast 1.26:1 (< 3.0)

failures:
    ui::theme::tests::editor_selection_contrast_focused
    ui::theme::tests::editor_selection_contrast_unfocused
    ui::theme::tests::editor_selection_follows_luminance_not_mode
    ui::theme::tests::editor_selection_is_blue
test result: FAILED. 1 passed; 4 failed
```

(`editor_selection_text_readable` passed on the old colors too: they are
pale/dark greys, readable with black or white.) Unfocused contrast was
1.14–1.21:1 on the old derivation.

### Measured after the fix

| Syntax theme | Selection | Focused | Unfocused | Best B/W text |
|---|---|---|---|---|
| Base16OceanDark | (70, 140, 230) | 3.87 | 2.30 | 6.15 |
| Base16OceanLight | (0, 120, 215) | 3.98 | 2.26 | 4.67 |
| Base16EightiesDark | (70, 140, 230) | 4.04 | 2.34 | 6.15 |
| Base16MochaDark | (70, 140, 230) | 3.68 | 2.20 | 6.15 |
| SolarizedDark | (70, 140, 230) | 4.40 | 2.48 | 6.15 |
| SolarizedLight | (0, 120, 215) | 4.17 | 2.34 | 4.67 |
| InspiredGitHub | (0, 120, 215) | 4.50 | 2.39 | 4.67 |

### Verification recipe

```bash
nix develop -c cargo test editor_selection     # 5 tests, all green
nix develop -c cargo test                      # full suite green
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Then the manual check below.

### Manual check — pending user verification

Steps 1–7 of *Manual check* above need a display and have **not** been
performed by the implementer (headless session). Please run them and record
the result here.

### Review finding (open) — selected text readability

Reviewer (PR #154): FLTK draws selected text as `fl_contrast(fg, selection)`.
At contrast level 10 that keeps `fg` whenever ΔL* ≥ 10, so with the new
colors plain text is drawn fg-on-blue: 1.55:1 on Ocean Light, 1.97:1 on Ocean
Dark (2.07–2.85 on Eighties/Mocha/InspiredGitHub; only the Solarized themes
fall back to black/white). `editor_selection_text_readable` checks a fallback
FLTK does not apply there. With fg kept, the spec's three bars are
infeasible together (`contrast(fg,bg) = contrast(fg,sel) · contrast(sel,bg)`).
Escalated to the orchestrator for a design decision; the fix lands on this
branch.

### Note on selected-text color

FLTK draws selected text with `fl_contrast(style_color, selection)`
(`Fl_Text_Display.cxx:2404`). FerrisPad lowers FLTK's contrast level to 10
(`FLTK_CONTRAST_LEVEL`), so `fl_contrast` keeps a syntax color whenever its
CIELAB lightness differs from the selection's by ≥ 10, and only falls back to
black/white below that. So inside the selection most syntax colors stay as
they are; the `editor_selection_text_readable` test guarantees the black/white
fallback is readable. Worth a look during manual step 5.

## Acceptance criteria

- [ ] On all seven syntax themes the selection meets the focused (≥ 3:1),
      unfocused (≥ 2:1) and selected-text (≥ 4.5:1) bars, enforced by tests.
      — **open (review finding)**: focused/unfocused hold, but the
      selected-text bar as specified is vacuous (see *Review finding* below);
      awaiting a design decision.
- [x] The color is chosen from background luminance, not the mode flag.
- [x] The contrast tests exist; the focused one was seen failing on the old
      derivation, with the ratios recorded in this ticket.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [ ] The ticket records the manual check (light, dark, system, all syntax
      themes, Find-dialog focus, Cancel). — **pending user verification**

## Affected files

- `src/ui/theme.rs:166-197` — `apply_theme`: replace the `sel_color`
  derivation with `editor_selection_color(theme_bg)`.
- `src/ui/theme.rs:83` — `apply_syntax_theme_colors`: confirm the selection
  is set consistently with the background it paints (it currently sets none).
- `src/ui/color.rs` — `ThemeRgb` helpers; home of the promoted
  `wcag_luminance` / `wcag_contrast`.
- `src/ui/file_dialogs.rs:298` — remove the test-only copies and use the
  promoted ones.
- `src/app/state.rs:843-863` — `apply_theme_to_owned_widgets`: check the
  call order so the final selection matches the final background.

## Notes

- Origin: user request, GitHub issue #150.
- Issue #150 exceeded the ~5-bullet ticket scope and is split into: T0078
  (this ticket, high-contrast default selection), T0079 (custom themes: model
  and persistence), T0080 (theme manager window) and T0081 (current-line
  highlight).
- FLTK references (fltk-sys 1.5.14, `Fl_Text_Display.cxx` ~l.2380–2416):
  the unfocused blend weight is 0.4, and selected text goes through
  `fl_contrast`.
