---
id: T0109
title: Readable git-status colors on the file tree's unselected rows
status: review           # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0104]     # stacked on ticket/T0104 (the tree's theme plumbing)
issue: 216
---

## Goal

The file tree colors each row by its git status, using fixed colors per
dark/light mode. Several of them are hard to read on the tree's background
(the editor's), measured by WCAG contrast:

- "ignored" is 2.3–3.4:1 on every theme (e.g. (160,160,160) on Base16
  Ocean Light: 2.31:1);
- on the light themes "modified" is 2.7–3.0:1 and "added" 3.7–4.2:1;
- on the dark themes "conflict" is 3.9–4.3:1, except Solarized Dark.

## In scope

- A git-status color keeps its **hue exactly** and changes only its
  lightness, just until it reaches 4.5:1 on the tree background. It uses
  the same rule (`color::push_to_contrast`) as the T0097/T0105 list
  selection and input border. A color that already reads is unchanged.
- The colors follow the theme: they are recomputed when the theme changes,
  as now.

## Out of scope

- Anything else in the tree or elsewhere: the selected row (T0104), the
  normal text color, icons, fonts. **This ticket changes no other pixel.**
- Which statuses get a color, or their hues.

## How to test

```bash
nix develop -c cargo test git_labels_read_on_the_tree
```

- Red: on T0104 the test fails, listing each status / theme below 4.5:1.
- Montage (Xvfb, Claude): the tree with every status, before / after, on
  the 7 built-in themes. **The user approves the look before review**
  (orchestrator, 2026-11-26).

## Acceptance criteria

- [ ] Every git-status color is ≥ 4.5:1 on the tree background, on the 7
      themes and T0097's 54-background sweep.
- [ ] Each keeps its hue; colors that already passed are unchanged.
- [ ] The user approved the montage; full gates green.

## Affected files

- `src/ui/tree_panel.rs`: `readable_git_label_rgb`, used by
  `resolve_label_color`; test.
- `src/ui/theme.rs`: the T0104 selected-row test uses the drawn colors.

## Notes

- Origin: a3's follow-up in T0104 review; orchestrator 2026-11-26. The user
  rejected restyles earlier the same day, so this only fixes contrast, and
  the montage comes first.

## What was done (2026-11-26)

`tree_panel::readable_git_label_rgb(name, is_dark, tree_bg)` takes the
existing `git_label_rgb` color. If it reads at 4.5:1 on the tree background
it is returned unchanged; otherwise `color::push_to_contrast` steps its
lightness until it does (darker on a light tree, lighter on a dark one).
Both directions keep the hue. `resolve_label_color` uses it with the
tree's current background, which the tree already re-resolves on every
theme change. The T0104 selected-row test now checks the colors actually
drawn.

### Red (T0104 code, today's fixed colors)

```
test ui::tree_panel::tests::git_labels_read_on_the_tree ... FAILED
Base16OceanDark: conflict (224, 108, 117) on (43, 48, 59) = 4.14:1
Base16OceanDark: ignored (120, 120, 120) on (43, 48, 59) = 2.99:1
Base16OceanLight: modified (190, 140, 40) on (239, 241, 245) = 2.66:1
Base16OceanLight: added (60, 140, 50) on (239, 241, 245) = 3.72:1
Base16OceanLight: ignored (160, 160, 160) on (239, 241, 245) = 2.31:1
Base16MochaDark: conflict (224, 108, 117) on (59, 50, 40) = 3.93:1
SolarizedLight: modified (190, 140, 40) on (253, 246, 227) = 2.79:1
InspiredGitHub: ignored (160, 160, 160) on (255, 255, 255) = 2.61:1
(… 19 status/theme pairs on the built-in themes, plus the sweep)
```

### After

- On the built-in themes every status is ≥ 4.5:1, its hue within 3°
  (integer rounding), and the colors that already passed are unchanged.
- On T0097's mid-tone sweep backgrounds, 4.5:1 can need near-white or
  near-black. There a hue may drift up to 12° or fade toward gray:
  readability wins, as for T0104.
- Full suite under Xvfb: lib 587, every binary green. fmt and clippy
  `-D warnings` clean.

### Montage (Xvfb, Claude), for the user's approval

`ferrispad-wt/uitest-T0104/t0109-tree-git-colors.png` (local) shows the
tree with no row selected, before (T0104 build) | after, on the 7 themes.

- Light themes: "modified" becomes a darker ochre, "added" a darker green,
  "ignored" a readable mid-gray.
- Dark themes: "conflict" is slightly lighter, "ignored" a lighter gray.
  "modified" and "added" are unchanged.
- The rest of the window is pixel-identical (editor area AE = 0 on all 7
  themes).
