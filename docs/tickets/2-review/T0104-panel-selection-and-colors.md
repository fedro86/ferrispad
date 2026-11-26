---
id: T0104
title: Readable selected rows and theme-relative colors in the file tree and diagnostic panel
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0105]     # stacked on ticket/T0105 (push_to_contrast, list_selection_colors)
issue: 199
---

## Goal

The main-window panels have the same problem T0097 fixed in the dialogs:

- **File tree:** the selected row is the editor background ±10% (CIELAB
  level 10), about **1.3:1** against the tree, so it is hard to see which file
  is selected. The rows keep their git-status label colors when selected, and
  some of them become unreadable on the selection band ("modified" on Ocean
  Light: 2.12:1).
- **Diagnostic panel:** its background is a fixed gray (40 in dark mode, 250
  in light mode), not the editor background, so it looks pasted on. Its level
  colors are fixed palette colors: the warning yellow is 1.32:1 on the light
  panel, and the hint gray is 1.98:1 on the dark one. Its header bar
  ("⚠ 1 error, 1 warning, 2 info") has white text everywhere, and white on
  the orange (warning) bar is 2.70:1.

## In scope

- **Diagnostic panel background** = the editor background of the active
  theme (`diagnostic_panel_bg`).
- **Diagnostic level colors** (error, warning, info, hint) keep their hue and
  are pushed to ≥ 4.5:1 against that background
  (`diagnostic_level_colors`, using `color::push_to_contrast`).
- **Diagnostic header bar** keeps its pre-T0104 fills (red, orange, blue,
  green: `DIAGNOSTIC_HEADER_BASES`) on every theme, by the user's choice.
  Its label stays the old white wherever white reads (≥ 4.5:1) and is
  otherwise black or white, whichever reads better
  (`contrast::best_black_or_white`): the orange bar gets black text.
  A guard test keeps hand-set colors out of the panel code.
- **One panel selection rule** for the tree and the diagnostic panel:
  `theme::panel_selection_color` reuses the dialogs' list selection
  (`list_selection_colors`, T0097): the button color pushed to ≥ 3:1 against
  the panel background. `theme::style_panel_selection` sets it and registers
  it with the T0078 contrast hook, so the text on the selected row gets the
  WCAG rule (fg if ≥ 4.5:1, otherwise black or white).
- **Readability wins on the selected row:** the selected row's text is the
  contrast hook's color, not its git color. The other rows keep their git
  colors. This removes the tree's timeout that re-applied the label colors
  after a selection (and the `unsafe` it needed).

## Out of scope

- The CIELAB level-10 rule itself for unregistered backgrounds: it stays as
  it is for everything that is not a registered selection (see Notes).
- The git status colors of unselected rows.
- Dialog widgets (T0097 / T0105).

## How to test

```bash
nix develop -c cargo test diagnostic_panel_follows_the_theme   # part A
nix develop -c cargo test panel_selection_stands_out_in_every_theme   # part B
nix develop -c cargo test diagnostic_header_keeps_the_old_fills   # header bar
nix develop -c cargo test diagnostic_panel_sets_no_absolute_colors   # guard
```

Each fails on master or on the previous header attempt (output below), and
passes on this branch.

Manual (or Xvfb), for each of the 7 built-in themes: open a folder with a
modified and an untracked file, select a file in the tree, and open a file
for which a plugin reports diagnostics (e.g. python-lint on a file with an
error). Expected:

- the selected tree row is a clearly visible band, and its text is readable;
- the unselected rows keep their git colors;
- the diagnostic panel has the editor's background, and every level color is
  readable on it, with its hue kept.

## Acceptance criteria

- [ ] Diagnostic panel background = editor background on every theme.
- [ ] Every level color ≥ 4.5:1 on that background, hue kept (built-in
      themes; see Notes for mid-tone custom backgrounds).
- [ ] The header bar keeps its old fill in every state on every theme, and
      its label is ≥ 4.5:1 (white wherever white reads); no hand-set color
      left in `diagnostic_panel.rs`.
- [ ] The tree's and diagnostic panel's selected rows ≥ 3:1 against the
      panel; their text ≥ 4.5:1 on the selection.
- [ ] No re-apply timeout and no `unsafe` left in `tree_panel.rs`'s
      selection path.
- [ ] Red tests failed before; full gates green; Xvfb montage of all
      themes inspected.

## Affected files

- `src/ui/diagnostic_panel.rs`: `diagnostic_panel_bg`,
  `DIAGNOSTIC_LEVEL_BASES`, `diagnostic_level_colors`,
  `DIAGNOSTIC_HEADER_BASES`, `HeaderState`,
  `diagnostic_header_colors`, `set_panel_bg`/`paint_header` (one path for
  construction and re-theme), `color_code` (black as `FL_BLACK`), tests.
- `src/ui/color.rs`: `theme_test_backgrounds` (test-only, T0097's sweep,
  now shared).
- `src/ui/theme.rs`: `panel_selection_color`, `style_panel_selection`, test.
- `src/ui/contrast.rs`: `PANEL_SELECTION_BG`, `register_panel_selection`;
  `best_black_or_white` public; module doc (the level-10 decision, the
  inactive-tree edge).
- `src/ui/tree_panel.rs`: `git_label_rgb` made public and reused;
  `apply_theme` uses `style_panel_selection`; the colored-items list and
  the re-apply timeout removed.

## Notes

- Origin: the orchestrator after T0097's review, target 0.9.11. The user's
  decisions (2026-11-26): readability wins on the selected row (option 1),
  and the diagnostic panel background is in scope, theme-relative, in its
  own commit so it can be judged in screenshots.
- **Why level 10 stays.** The T0078 hook gives unregistered backgrounds the
  CIELAB level-10 text rule. It is kept on purpose: it is what makes FLTK's
  own widgets readable without registration. The tree's selection is now a
  registered background, so it gets the WCAG rule instead.
- Commits: part A (diagnostic panel background and level colors), part B
  (panel selection) and the header bar are separate.
- **Mid-tone custom backgrounds.** The tests run on T0097's sweep of 54
  backgrounds as well as the 7 built-in themes. On a mid-tone background
  (gray 105, `warm 135`) a level color reaches 4.5:1 only near white or
  black, where its hue fades to gray. Readability wins there; the test
  still requires that a color never turns into a *different* hue. The hue
  is checked strictly on the built-in themes.
- **Inactive tree** (a3): an inactive tree draws its selected row in
  `fl_inactive(selection_color)`, which is not registered. FerrisPad never
  deactivates the tree; the contrast.rs module doc records it.
- **`color_code` and black** (a3): `@C0` is FLTK's foreground index (the
  T0098 bug), so pure black is written as `FL_BLACK` (`@C56`).
- **Header bar: the old fills, by the user's choice.** A first version gave
  the bar the level rows' hues, stepped to ≥ 3:1 against the panel. On the
  built-in themes that meant pure, saturated red and yellow bars, and the
  user judged that "the style was much better before". The old fills are
  only 2.2–2.9:1 against the dark panels, so a 3:1 safety net would have
  lightened them (and turned their text black) on 15 of the 28
  theme × state bars. The orchestrator chose to keep the old fills
  everywhere with no 3:1 push. The bar's information is its label, now
  ≥ 4.5:1 everywhere, so the band's contrast against the panel is
  decorative, as it was before T0104.
- **Follow-up candidate** (a3, pre-existing, out of scope by option 1):
  unselected git colors are low-contrast on light themes (`ignored`
  (160,160,160) on Ocean Light ≈ 2.4:1).

## What was done (2026-11-26)

### Red runs (master code)

Part A:

```
test ui::diagnostic_panel::tests::diagnostic_panel_follows_the_theme ... FAILED
Base16OceanDark: panel bg (40, 40, 40) != editor bg (43, 48, 59)
Base16OceanDark: level 3 (85, 85, 85) on (40, 40, 40) = 1.98:1, hue kept: true
Base16OceanLight: panel bg (250, 250, 250) != editor bg (239, 241, 245)
Base16OceanLight: level 1 (255, 218, 0) on (250, 250, 250) = 1.32:1, hue kept: true
SolarizedDark: panel bg (40, 40, 40) != editor bg (0, 43, 54)
SolarizedLight: panel bg (250, 250, 250) != editor bg (253, 246, 227)
SolarizedLight: level 1 (255, 218, 0) on (250, 250, 250) = 1.32:1, hue kept: true
(… every theme fails; levels 0 and 2 at 3.4–4.1:1)
```

Part B:

```
test ui::theme::tests::panel_selection_stands_out_in_every_theme ... FAILED
Base16OceanDark: row (64, 68, 78) on (43, 48, 59) = 1.36:1
Base16OceanDark: error (255, 106, 106) on the selected row (64, 68, 78) = 3.49:1
Base16OceanLight: row (215, 216, 220) on (239, 241, 245) = 1.26:1
Base16OceanLight: modified (190, 140, 40) on the selected row (215, 216, 220) = 2.12:1
Base16EightiesDark: row (66, 66, 66) on (45, 45, 45) = 1.37:1
SolarizedLight: modified (190, 140, 40) on the selected row (227, 221, 204) = 2.22:1
InspiredGitHub: row (229, 229, 229) on (255, 255, 255) = 1.26:1
(… every theme: rows 1.26–1.37:1, colored labels 2.1–3.9:1 on the row)
```

Header bar, first version (`diagnostic_header_colors` stubbed with the
fixed colors; this test was later replaced, see Notes):

```
test ui::diagnostic_panel::tests::diagnostic_header_follows_the_theme ... FAILED
Base16OceanDark Error: bar (198, 40, 40) on (43, 48, 59) = 2.35:1, text (255, 255, 255) = 5.62:1
Base16OceanDark Warning: bar (245, 124, 0) on (43, 48, 59) = 4.89:1, text (255, 255, 255) = 2.70:1
Base16MochaDark Error: bar (198, 40, 40) on (59, 50, 40) = 2.23:1, text (255, 255, 255) = 5.62:1
Base16OceanLight Warning: bar (245, 124, 0) on (239, 241, 245) = 2.39:1, text (255, 255, 255) = 2.70:1
InspiredGitHub Warning: bar (245, 124, 0) on (255, 255, 255) = 2.70:1, text (255, 255, 255) = 2.70:1
(… 22 failures over the 7 themes)
```

Header bar, final version: `diagnostic_header_keeps_the_old_fills_with_readable_text`
on the first version's code (level hues pushed to 3:1) failed in all 222
theme × state cases, e.g.:

```
Base16OceanDark Error: bar (255, 0, 0) (old (198, 40, 40)), text (0, 0, 0) = 5.25:1
Base16OceanLight Warning: bar (162, 138, 0) (old (245, 124, 0)), text (0, 0, 0) = 6.17:1
Base16OceanDark Clean: bar (69, 139, 73) (old (46, 125, 50)), text (0, 0, 0) = 5.04:1
```

The guard (`diagnostic_panel_sets_no_absolute_colors`), on the code before
this commit, lists the 8 hand-set colors (`from_rgb(60, 60, 60)`,
`set_label_color(Color::White)`, the list's 40/80 grays and the 4 header
colors).

### After

- All the tests above pass, on the 7 built-in themes and on T0097's sweep
  of 54 backgrounds (now shared as `color::theme_test_backgrounds`), plus
  `color_code_never_writes_the_foreground_index`.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 590 passed,
  every integration binary green. fmt and clippy `-D warnings` clean.

### Xvfb (Claude)

A test plugin (Lua, `tree_view` + diagnostics of all four levels) was loaded
in a scratch config. On each of the 7 themes the campaign selected a modified
file in the tree and showed the diagnostic panel, then took a screenshot.
The montage (`ferrispad-wt/uitest-T0104/t0104-panels.png`, local) was
inspected edge by edge:

- the selected tree row is a visible band with black or white text; the
  other rows keep their git colors;
- the diagnostic panel has the editor's background; all four level colors
  are readable with their hue;
- no clipping, overflow or overlap.

Header bar: three more fixture variants (warning + info/hint, info/hint
only, no diagnostics) captured every state on every theme, before (T0105
build) and after. The "All checks passed" bar appears after Run All Checks
(Ctrl+Shift+L) and hides itself after 5 s. The header-only montage
(`ferrispad-wt/uitest-T0104/t0104-header.png`, local) shows the after bars
identical to the before ones (the same red / orange / blue / green, white
labels), except the orange warning bar, whose label is now black.
