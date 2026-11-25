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
- **Readable selected text via a custom FLTK contrast function**
  (`src/ui/contrast.rs`, installed in `main.rs` instead of
  `set_contrast_level`): on the editor's selection backgrounds (focused, and
  FLTK's unfocused blend) text keeps its color only if it is ≥ 4.5:1, else it
  becomes black or white, whichever reads better. Everywhere else it is an
  exact port of FLTK's `fl_contrast_cielab` at `FLTK_CONTRAST_LEVEL` (10), so
  every other widget draws as before.
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
3. **Selected-text readability.** FLTK draws selected text as
   `fl_contrast(fg, selection)`. At FerrisPad's `FLTK_CONTRAST_LEVEL` (10) the
   CIELAB rule keeps `fg` whenever its L* differs by ≥ 10, so on a strong blue
   most text and syntax colors would be kept at ~1.5–3:1. FerrisPad's custom
   contrast function fixes this **only on the editor selection** (focused and
   unfocused blend): a color below 4.5:1 becomes black or white. Inside the
   selection, syntax colors may therefore become plain black or white; that is
   expected. Other widgets (tree git-status labels, menus, dialogs) keep
   FLTK's rule at level 10 exactly.
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
- `editor_selection_text_readable` — for the theme foreground **and every
  syntax-scope foreground** of the syntect theme, the color FLTK actually
  draws (`ferrispad_contrast(fg, sel)`) is ≥ 4.5:1 on the focused selection
  and on the unfocused blend. (Replaces the original
  `max(B/W vs sel) >= 4.5` check, which modelled a fallback FLTK does not
  apply at level 10.)

In `src/ui/contrast.rs`:

- `contrast_matches_fltk_cielab_elsewhere` — hand-derived reference pairs
  (incl. the tree git-status amber/green/red on the light-mode selected row)
  give FLTK's level-10 CIELAB output, for the port and for
  `ferrispad_contrast` off the editor selection.
- `contrast_port_matches_fltk_itself` — the port equals FLTK's own
  `fl_contrast` (CIELAB, level 10) over a grid of colors.
- `unfocused_blend_matches_fltk` — the registered unfocused background equals
  FLTK's `fl_color_average(bg, sel, 0.4)` (truncating, not rounding).
- `readable_fg_on_selection_is_kept_unreadable_becomes_black_or_white`,
  `registry_round_trips_through_the_atomics`.
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
7. Selected text is white or black and readable in Light and in Dark mode
   (plain text and syntax-colored code). With the file tree open in light
   mode, select a row whose label has a git-status color (modified / added /
   conflict): the color still looks as before.
8. Toggle Light/Dark a few times; open Settings, preview another syntax theme,
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

### Manual check results — Xvfb/X11 (driven by Claude)

Debug build of the ticket/T0080 tip (contains this ticket), Xvfb 1280×800,
`FLTK_BACKEND=x11`, temporary `XDG_CONFIG_HOME`, a Rust sample file, driven
with `xdotool`, checked on `import` screenshots kept in
`ferrispad-wt/uitest-T0080/` (not committed).

1. **PASS** — the app starts, the sample file is highlighted (`01-start`).
2. **PASS** — Light mode, Base16 Ocean Light: mouse selection over 7 lines is
   an obvious blue band, selected text black and readable
   (`78-Light-Base16OceanLight`).
3. **PASS** — Dark mode, Base16 Ocean Dark: lighter blue band, selected text
   black and readable (`02-sel-dark-ocean`, `78-Dark-Base16OceanDark`).
4. System default under an OS light/dark switch — **pending user** (Xvfb has
   no OS appearance setting).
5. **PASS** — each of the 7 syntax themes in Light *and* Dark mode (14
   screenshots, `montage-78-Light.png`, `montage-78-Dark.png`), including
   dark syntax themes in Light mode: the selection is blue and visible on
   each; the variant follows the background (light blue on dark backgrounds,
   strong blue on light ones).
6. **PASS** — Find with the dialog focused, searching "counts", Find Next:
   the match in the unfocused editor is clearly visible — dark: the 40/60
   blend with white text (`78-find-dark`); light: lighter blue with black
   text (`78-find-light`).
7. **PASS** — selected text is black (or white on the dark unfocused blend)
   and readable in Light and Dark, plain text and syntax-colored code alike
   (all screenshots above). Tree panel (file-explorer plugin, Light mode, a
   git repo with a modified and an untracked file): selecting the rows keeps
   the amber and green git-status colors (`montage-78-tree.png`).
8. **PASS** — View → Toggle Dark Mode three times, then select: blue
   (`78-step7-after-toggles`). Settings → preview Solarized Dark (selection
   blue, `78-step7-preview-solarized`) → **Cancel**: back to Ocean Dark,
   selection blue (`78-step7-after-cancel`). The selection never fell back to
   the old faint color.

### Review finding — selected text readability (fixed)

Reviewer (PR #154): FLTK draws selected text as `fl_contrast(fg, selection)`.
At contrast level 10 that keeps `fg` whenever ΔL* ≥ 10, so with the new
colors plain text is drawn fg-on-blue: 1.55:1 on Ocean Light, 1.97:1 on Ocean
Dark (2.07–2.85 on Eighties/Mocha/InspiredGitHub; only the Solarized themes
fall back to black/white). `editor_selection_text_readable` checks a fallback
FLTK does not apply there. With fg kept, the spec's three bars are
infeasible together (`contrast(fg,bg) = contrast(fg,sel) · contrast(sel,bg)`).
Escalated; the orchestrator chose a custom FLTK contrast function (option
"d"), implemented in `src/ui/contrast.rs`:

- `fltk_cielab_contrast` — exact port of `fl_contrast_cielab`
  (`fltk-sys-1.5.14/cfltk/fltk/src/fl_contrast.cxx`), incl. FLTK's own
  `fl_lightness` (channels^2.4, CIELAB L*);
- `ferrispad_contrast(fg, bg, EditorSelectionBgs)` — on a registered editor
  selection background: `fg` if ≥ 4.5:1, else the better of black/white;
  elsewhere: the port at `FLTK_CONTRAST_LEVEL`;
- the registered backgrounds (selection, and the unfocused blend computed
  like `fl_color_average`, in `f32` and truncated) live in two `AtomicU32`:
  FLTK takes a plain function pointer with no user data, so the callback can
  only read globals; atomics need no `unsafe` and no lock that could panic.
  `apply_syntax_theme_colors` registers them wherever it sets the selection;
- `install()` sets `ContrastMode::Custom` and registers the callback (replaces
  `set_contrast_level` in `main.rs`). The callback is an `extern "C" fn` with
  FLTK's exact `Fl_Contrast_Function` signature, registered through
  `fltk_sys::fl::Fl_set_contrast_function` — not fltk-rs's
  `app::set_contrast_function`, which transmutes a Rust-ABI fn pointer (review
  finding). It runs inside C drawing code: arithmetic and atomic loads only,
  and as `extern "C"` a panic would abort instead of unwinding into FLTK.
  `contrast_callback_plumbs_raw_color_bits` covers the raw-bits plumbing.

Red first (the new `editor_selection_text_readable`, run with FLTK's current
level-10 CIELAB rule before the custom function existed; 655 failure lines
over all 7 themes — scope colors not yet de-duplicated — excerpt):

```
Base16OceanDark: text (192, 197, 206) drawn as (192, 197, 206) on focused selection (70, 140, 230): 1.97:1 (< 4.5)
Base16OceanDark: text (192, 197, 206) drawn as (192, 197, 206) on unfocused selection (59, 103, 161): 3.33:1 (< 4.5)
Base16OceanLight: text (79, 91, 102) drawn as (79, 91, 102) on focused selection (0, 120, 215): 1.55:1 (< 4.5)
Base16OceanLight: text (79, 91, 102) drawn as (79, 91, 102) on unfocused selection (95, 168, 227): 2.71:1 (< 4.5)
test result: FAILED. 0 passed; 1 failed
```

Green after installing `ferrispad_contrast` (all 7 themes, theme fg + every
scope foreground, focused + unfocused).


## Acceptance criteria

- [x] On all seven syntax themes the selection meets the focused (≥ 3:1),
      unfocused (≥ 2:1) and selected-text (≥ 4.5:1, as FLTK really draws it,
      every syntax color included) bars, enforced by tests.
- [x] Off the editor selection, FLTK's contrast behaves exactly as before
      (port tested against hand-derived pairs and FLTK itself).
- [x] The color is chosen from background luminance, not the mode flag.
- [x] The contrast tests exist; the focused one was seen failing on the old
      derivation, with the ratios recorded in this ticket.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [x] The ticket records the manual check (light, dark, all syntax themes,
      Find-dialog focus, selected-text color, tree git-status colors, Cancel)
      — Xvfb/X11, driven by Claude.
- [ ] System default under a real OS appearance switch, a real Wayland
      compositor, Windows/macOS — **pending user verification**.

## Affected files

- `src/ui/theme.rs:166-197` — `apply_theme`: replace the `sel_color`
  derivation with `editor_selection_color(theme_bg)`.
- `src/ui/theme.rs:83` — `apply_syntax_theme_colors`: confirm the selection
  is set consistently with the background it paints (it currently sets none).
- `src/ui/color.rs` — `ThemeRgb` helpers; home of the promoted
  `wcag_luminance` / `wcag_contrast`.
- `src/ui/file_dialogs.rs:298` — remove the test-only copies and use the
  promoted ones.
- `src/ui/contrast.rs` (new) — the custom contrast function, its CIELAB
  port and the registry; `src/main.rs` — installs it.
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
  `fl_contrast`. `fl_contrast.cxx` (`fl_contrast_cielab`, custom-function
  dispatch) and `fl_color.cxx:146` (`fl_color_average`) are the sources of
  the port.
- The custom contrast function is global: any widget whose background
  happens to equal the editor selection color exactly also gets the stricter
  rule. That is harmless (it only makes text more readable).
- `src/ui/split_panel.rs` editors set no selection color, so they are not
  registered and keep FLTK's default behaviour (out of scope, as above).
