---
id: T0104
title: Readable selected row in the file tree
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0105]     # stacked on ticket/T0105 (push_to_contrast, list_selection_colors)
issue: 199
---

## Goal

The file tree has the same problem T0097 fixed in the dialog lists. Its
selected row is the editor background ±10% (CIELAB level 10), about
**1.3:1** against the tree, so it is hard to see which file is selected.
The rows keep their git-status label colors when selected, and some of them
become unreadable on the selection band ("modified" on Ocean Light: 2.12:1).

## In scope

- **The tree's selected row** uses the dialogs' list selection rule
  (`list_selection_colors`, T0097): the button color pushed to ≥ 3:1
  against the tree background (`theme::panel_selection_color`).
  `theme::style_panel_selection` sets it and registers it with the T0078
  contrast hook (`contrast::register_panel_selection`). The text on the
  selected row then gets the WCAG rule: the label color if ≥ 4.5:1,
  otherwise black or white.
- **Readability wins on the selected row:** the selected row's text is the
  contrast hook's color, not its git color. The other rows keep their git
  colors. This removes the tree's timeout that re-applied the label colors
  after a selection, and the `unsafe` it needed.

## Out of scope

- **The diagnostic panel: unchanged, exactly as before** (user decision,
  2026-11-26, see Notes).
- The git-status colors of unselected rows (follow-up T0109).
- The CIELAB level-10 rule for unregistered backgrounds: it stays as it
  is for everything that is not a registered selection (see Notes).
- Dialog widgets (T0097 / T0105).

## How to test

```bash
nix develop -c cargo test panel_selection_stands_out_in_every_theme
```

It fails on master (output below) and passes on this branch.

Manual (or Xvfb), for each of the 7 built-in themes: open a folder with a
modified and an untracked file, and select a file in the tree. Expected:

- the selected row is a clearly visible band, and its text is readable;
- the unselected rows keep their git colors;
- the diagnostic panel looks exactly as on master.

## Acceptance criteria

- [ ] The tree's selected row is ≥ 3:1 against the tree, and every label
      on it (theme text, git colors) is ≥ 4.5:1, on the 7 built-in themes
      and T0097's 54-background sweep.
- [ ] No re-apply timeout and no `unsafe` left in `tree_panel.rs`'s
      selection path.
- [ ] The diagnostic panel is pixel-identical to master's (Xvfb, 7 themes).
- [ ] Red test failed before; full gates green; Xvfb montage inspected.

## Affected files

- `src/ui/theme.rs`: `panel_selection_color`, `style_panel_selection`, test.
- `src/ui/contrast.rs`: `PANEL_SELECTION_BG`, `register_panel_selection`;
  module doc (the level-10 decision, the inactive-tree edge).
- `src/ui/tree_panel.rs`: `git_label_rgb` made public and reused;
  `apply_theme` uses `style_panel_selection`; the colored-items list and
  the re-apply timeout removed.
- `src/ui/color.rs`: `theme_test_backgrounds` (test-only, T0097's
  background sweep, now shared); `src/ui/dialogs/mod.rs` uses it.

## Notes

- Origin: the orchestrator after T0097's review, target 0.9.11. The user's
  decision (option 1): readability wins on the selected row.
- **The diagnostic panel stays as it was (user decision, 2026-11-26).**
  T0104 first also made the diagnostic panel theme-relative: the editor
  background, readable level colors, the shared selected row, and a
  theme-derived header bar. After seeing before/after previews the user
  decided: "the tree can stay, but put the diagnostics back as before".
  Before that the user had already rejected the restyled header twice
  ("the style was much better before"; "no black text"). That work was
  reverted: `diagnostic_panel.rs` is master's again, its fixed colors
  included, with no tests or guard of its own. Xvfb proves it
  pixel-identical to master on the 7 themes.
- **Why level 10 stays.** The T0078 hook gives unregistered backgrounds
  the CIELAB level-10 text rule. It is kept on purpose: it is what makes
  FLTK's own widgets readable without registration. The tree's selection
  is now a registered background, so it gets the WCAG rule instead.
- **Inactive tree** (a3): an inactive tree draws its selected row in
  `fl_inactive(selection_color)`, which is not registered. FerrisPad never
  deactivates the tree; the contrast.rs module doc records it.
- **How the tree draws a selected label** (a3, verified): FLTK 1.5.24's
  `Fl_Tree_Item::drawfgcolor()` computes it at draw time as
  `fl_contrast(labelfgcolor, selection_color)`. The item's color is never
  changed, so there was nothing to re-apply.
- **Follow-up T0109** (a3, pre-existing): unselected git colors are
  low-contrast on light themes (`ignored` (160,160,160) on Ocean Light ≈
  2.4:1). Any change there needs the user's OK on a montage first.

## What was done (2026-11-26)

### Red run (master code)

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

(The `error` lines in that run were diagnostic level colors, which the test
also checked while the diagnostic panel was in scope.)

### After

- The test passes on the 7 built-in themes and on T0097's 54-background
  sweep.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 586, every binary
  green. fmt and clippy `-D warnings` clean.

### Xvfb (Claude)

A test plugin (Lua, `tree_view` + diagnostics of all four levels) was
loaded in a scratch config. On each of the 7 themes the campaign selected a
modified file in the tree and a warning row in the diagnostic panel, then
took a screenshot. This ran with the T0105 build (before) and this branch
(after). The montage (`ferrispad-wt/uitest-T0104/t0104-panels.png`, local)
was inspected edge by edge:

- the selected tree row is a visible band with black or white text; the
  other rows keep their git colors;
- the diagnostic panel crops are identical before and after;
- no clipping, overflow or overlap.
