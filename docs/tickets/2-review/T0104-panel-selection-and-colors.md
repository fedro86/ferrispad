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
selected row is the editor background ±10%, about **1.3:1** against the
tree, so it is hard to see which file is selected. The rows keep their
git-status label colors when selected (FLTK's contrast rule at level 10).

## In scope

- **The tree's selected row is a soft band** (user decision 2026-11-27,
  "Colore git, banda tenue"; see Notes): the tree background lightened
  (dark theme) or darkened (light theme) by the least amount that reaches
  **1.5:1** against the tree (`theme::panel_selection_color`, applied by
  `style_panel_selection`). Master's band is 1.26–1.37:1.
- **The selected file's name keeps its git color** (the theme text for a
  clean file). The band is *not* registered with the T0078 contrast hook, so
  FLTK's level-10 rule draws the labels, and at 1.5:1 it keeps every label
  color of the built-in themes. No black/white override.
- The tree's timeout that re-applied the label colors after a selection is
  removed, with the `unsafe` it needed (FLTK never changes the item color).

## Out of scope

- **The diagnostic panel: unchanged, exactly as before** (user decision,
  2026-11-26, see Notes).
- The git-status colors of unselected rows (follow-up T0109).
- The dialog lists' selected row (T0097, ≥ 3:1): unchanged.
- The CIELAB level-10 rule for unregistered backgrounds: it stays as it
  is for everything that is not a registered selection (see Notes).
- Dialog widgets (T0097 / T0105).

## How to test

```bash
nix develop -c cargo test panel_selection_band_keeps_the_label_colors
```

It fails on master's band (rows 1.26–1.37:1) and on a 3:1 band (labels
drawn black or white); output below. It passes on this branch.

```bash
.claude/skills/new-pr/compare.sh docs/pr/scenarios/file-tree.sh ~/Code/personal/ferrispad-wt/uitest-T0104/soft
```

`tree-selected-{light,dark}-compare.png`: the selected `modified.rs` keeps
its orange (light) / yellow (dark) name on a visibly stronger gray band.

Manual: open a folder with a modified and an untracked file in the file
explorer, and select a file, in a light and a dark theme. Expected:

- the selected row is a visible band, stronger than before;
- the selected name keeps its git color; the other rows keep theirs;
- the diagnostic panel looks exactly as on master.

## Acceptance criteria

- [x] The tree's selected row is in [1.5, 1.6):1 against the tree on the 7
      built-in themes and T0097's 54-background sweep.
- [x] On the 7 built-in themes every label on the selected row (theme
      text, modified, added, conflict, ignored) is drawn in its own color.
- [x] No re-apply timeout and no `unsafe` left in `tree_panel.rs`'s
      selection path.
- [x] The diagnostic panel and everything outside the tree are
      pixel-identical to master's (Xvfb, light and dark).
- [x] Test failed on the old rules; full gates green; montage inspected.

## Affected files

- `src/ui/theme.rs`: `PANEL_SELECTION_CONTRAST`, `panel_selection_color`,
  `style_panel_selection`, test `panel_selection_band_keeps_the_label_colors`.
- `src/ui/color.rs`: `shift_to_contrast` (bisection, with a test);
  `theme_test_backgrounds` (test-only, T0097's background sweep, now
  shared); `src/ui/dialogs/mod.rs` uses it.
- `src/ui/tree_panel.rs`: `git_label_rgb` made public and reused;
  `apply_theme` uses `style_panel_selection`; the colored-items list and
  the re-apply timeout removed.
- `src/ui/contrast.rs`: unchanged from master (the band is not registered).

## Notes

- Origin: the orchestrator after T0097's review, target 0.9.11.
- **The user's decision (2026-11-27): "Colore git, banda tenue"**, both
  themes. The first version of this PR used the dialogs' rule (a 3:1 band,
  registered with the contrast hook), which drew the selected name in black
  on both themes of the screenshots. After the before/after the user chose:
  the name keeps its git color, on a band softer than 3:1 but clearly more
  visible than master's ≈1.3:1, theme-derived, chosen to maximise the worst
  git color's contrast while staying visibly distinct.
- **Why 1.5:1 (implementer's choice, A).** On light themes every git color
  is darker than the tree, so a stronger band always costs them contrast;
  the softest visible band is the best for them. At 1.5:1 no built-in label
  is swapped; at 1.8:1 Ocean Light's and Solarized Light's `ignored` would
  turn black, and at 2:1 Ocean Light's `modified` too. The worst label on
  the band is `ignored` on Ocean Light, 1.53:1 (2.31:1 on the tree itself,
  1.84:1 on master's band). The git colors' own low contrast on light
  themes is T0109.
- **Custom themes.** On mid-gray backgrounds (the sweep's 75–255 grays and
  tints) some git colors are within 10 L* of the band and FLTK draws them
  black or white: 50 label/background pairs at 1.5:1, 43 with master's
  band. Those colors are already barely distinguishable on such a tree.
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
  the CIELAB level-10 text rule, which is what keeps the git colors on the
  selected row (it was lowered to 10 for exactly that). The tree's band is
  unregistered again, so `contrast.rs` is master's.
- **How the tree draws a selected label** (a3, verified): FLTK 1.5.24's
  `Fl_Tree_Item::drawfgcolor()` computes it at draw time as
  `fl_contrast(labelfgcolor, selection_color)`. The item's color is never
  changed, so there was nothing to re-apply.
- **Follow-up T0109** (a3, pre-existing): unselected git colors are
  low-contrast on light themes (`ignored` (160,160,160) on Ocean Light ≈
  2.4:1). Any change there needs the user's OK on a montage first.

## What was done

### First version (2026-11-26, superseded)

The 3:1 registered band; the user replaced it on 2026-11-27 (Notes).

#### Red run (master code)

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

#### After

- The test passes on the 7 built-in themes and on T0097's 54-background
  sweep.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 586, every binary
  green. fmt and clippy `-D warnings` clean.

#### Xvfb (Claude)

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

### The soft band (2026-11-27, the user's decision)

`panel_selection_band_keeps_the_label_colors` replaces
`panel_selection_stands_out_in_every_theme`. Its threshold: the band is in
[1.5, 1.6):1 against the tree on all 61 backgrounds, and on the 7 built-in
themes every label is drawn in its own color through `ferrispad_contrast`
with nothing registered (FLTK's level-10 rule).

Failing runs (the band rule swapped back temporarily):

```
master's band (±10 %):
Base16OceanDark: row (64, 68, 78) on (43, 48, 59) = 1.36:1
Base16OceanLight: row (215, 216, 220) on (239, 241, 245) = 1.26:1
InspiredGitHub: row (229, 229, 229) on (255, 255, 255) = 1.26:1
(… all 7 built-in themes 1.26–1.37:1)

a 3:1 band, unregistered:
Base16OceanLight: row (138, 139, 142) on (239, 241, 245) = 3.01:1
Base16OceanLight: modified (190, 140, 40) drawn as (0, 0, 0) on the selected row (138, 139, 142)
Base16OceanDark: ignored (120, 120, 120) drawn as (255, 255, 255) on the selected row (117, 121, 128)
Base16MochaDark: conflict (224, 108, 117) drawn as (255, 255, 255) on the selected row (129, 123, 116)
(… every built-in theme has a swapped label)
```

WCAG contrast on the 7 built-in themes (tree → master's band → this band):

| Theme | Band vs tree | text | modified | added | conflict | ignored |
|---|---|---|---|---|---|---|
| Base16OceanDark | 1.36 → **1.51** | 7.81 → 7.03 | 5.64 → 5.08 | 4.83 → 4.35 | 3.05 → 2.74 | 2.21 → 1.99 |
| Base16EightiesDark | 1.37 → **1.51** | 8.05 → 7.33 | 5.82 → 5.29 | 4.99 → 4.54 | 3.14 → 2.86 | 2.28 → 2.07 |
| Base16MochaDark | 1.36 → **1.51** | 7.42 → 6.67 | 5.36 → 4.82 | 4.59 → 4.13 | 2.90 → 2.61 | 2.10 → 1.89 |
| SolarizedDark | 1.34 → **1.51** | 8.98 → 7.96 | 6.49 → 5.75 | 5.56 → 4.93 | 3.51 → 3.11 | 2.54 → 2.25 |
| Base16OceanLight | 1.26 → **1.51** | 14.75 → 12.30 | 2.12 → 1.77 | 2.96 → 2.47 | 3.99 → 3.33 | 1.84 → 1.53 |
| SolarizedLight | 1.26 → **1.50** | 15.48 → 12.97 | 2.22 → 1.86 | 3.10 → 2.60 | 4.19 → 3.51 | 1.93 → 1.62 |
| InspiredGitHub | 1.26 → **1.51** | 16.67 → 13.89 | 2.39 → 1.99 | 3.34 → 2.78 | 4.52 → 3.76 | 2.08 → 1.73 |

Gates: lib 663 passed, every test binary green; clippy `-D warnings` and
`cargo fmt --check` clean.

Xvfb (`compare.sh … file-tree.sh …/uitest-T0104/soft`, master c3672fe vs
1de59d1): the band measured on the shots is (197, 198, 202) on Ocean Light
and (70, 75, 84) on Ocean Dark, as computed; `modified.rs` keeps its
orange / yellow. The unselected shots are pixel-identical to master's
(AE 0), and so is everything right of the tree in the selected shots
(diagnostic panel included). No clipping or overlap; no panic or assert
in the logs; host clean afterwards.
