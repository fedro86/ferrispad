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
  panel, and the hint gray is 1.98:1 on the dark one.

## In scope

- **Diagnostic panel background** = the editor background of the active
  theme (`diagnostic_panel_bg`).
- **Diagnostic level colors** (error, warning, info, hint) keep their hue and
  are pushed to ≥ 4.5:1 against that background
  (`diagnostic_level_colors`, using `color::push_to_contrast`).
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
```

Both fail on master (output below) and pass on this branch.

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
- [ ] Every level color ≥ 4.5:1 on that background, hue kept.
- [ ] The tree's and diagnostic panel's selected rows ≥ 3:1 against the
      panel; their text ≥ 4.5:1 on the selection.
- [ ] No re-apply timeout and no `unsafe` left in `tree_panel.rs`'s
      selection path.
- [ ] Red tests failed before; full gates green; Xvfb montage of all
      themes inspected.

## Affected files

- `src/ui/diagnostic_panel.rs`: `diagnostic_panel_bg`,
  `DIAGNOSTIC_LEVEL_BASES`, `diagnostic_level_colors`, `apply_theme`, test.
- `src/ui/theme.rs`: `panel_selection_color`, `style_panel_selection`, test.
- `src/ui/contrast.rs`: `PANEL_SELECTION_BG`, `register_panel_selection`;
  module doc (the level-10 decision).
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
- Commits: part A (diagnostic panel background and level colors) and part B
  (panel selection) are separate.

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

### After

- Both tests pass on all 7 themes.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 587 passed,
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
