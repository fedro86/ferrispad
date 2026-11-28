---
id: T0109
title: Readable git-status colors on the file tree's unselected rows
status: done             # todo | review | done
created: 2026-11-26
severity: minor
area: ui
release: 0.9.11         # the planned release
depends-on: []          # T0104 landed on master (f12fd3c)
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
env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  FLTK_BACKEND=x11 XDG_RUNTIME_DIR=$(mktemp -d) \
  nix shell nixpkgs#xvfb-run -c xvfb-run -a \
  nix develop -c cargo test --lib -- git_labels_read_on_the_tree panel_selection light_tree_selection
```

(The private-display environment of `scripts/gates.sh`; expected: 3 passed.)

- Red: on T0104 the test fails, listing each status / theme below 4.5:1.
- Montage (Xvfb, Claude): the tree with every status, before / after, on
  the 7 built-in themes. **The user approves the look before review**
  (orchestrator, 2026-11-26).

## Acceptance criteria

- [x] Every git-status color is ≥ 4.5:1 on the tree background, on the 7
      themes and T0097's 54-background sweep.
- [x] Each keeps its hue; colors that already passed are unchanged.
- [x] The user approved the montage; full gates green.

## Affected files

- `src/ui/tree_panel.rs`: `readable_git_label_rgb`, used by
  `resolve_label_color`; test.
- `src/ui/theme.rs`: the T0104 selected-row test uses the drawn colors.
- `docs/pr/scenarios/file-tree-{solarized,github-eighties,github-mocha}.sh`:
  `file-tree.sh` on the other built-in themes, for compare.sh.

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

## Merges and the compare.sh montage (2026-11-28, Implementer A)

- **35ee258, master 6d70289.** The branch was stacked on the in-review
  T0097, T0104 and T0105; all three landed since, T0104 in a different
  shape (the user's "Colore git, banda tenue": the selected row draws the
  git color on a soft band). Every conflict is resolved to master's version
  of the file, then T0109's own change (ae9c198) is reapplied with master's
  `fltk_rgb`. The stale `2-review/` copies of the three landed tickets are
  dropped. The diff against master is T0109's alone.
- **eae36db, master 9941c6e** (T0106) and **84a1458, master 32fe29f**
  (T0110): clean.

### Red, again, on master's colors

`readable_git_label_rgb`'s body replaced by `git_label_rgb` (a throwaway
probe, not committed): `git_labels_read_on_the_tree` FAILED with 19 pairs on
the built-in themes, from `Base16OceanLight: ignored (160, 160, 160) on
(239, 241, 245) = 2.31:1` to `Base16EightiesDark: conflict (224, 108, 117)
on (45, 45, 45) = 4.31:1`. With the fix, the test and T0104's two band
tests pass.

### Montage

compare.sh, before = master 32fe29f, after = 84a1458, on `file-tree.sh`
and the three theme variants: 16 pairs (no row selected and "modified.rs"
selected, light and dark, seven built-in themes). Outside the tree panel
the window is pixel-identical in 15 pairs. In the 16th, 8 px differ in the
editor's right border; two master runs of the same theme differ by 14 px
there, so this is run noise. Every run used its own display (:90), with no
panic or assert in the logs. The page shows the tree crops at 200%.

**The selected row (for the user's decision).** Since T0104 the selected
row draws the same git color on its soft band, so T0109 changes it too:
on the band the labels read at 3.02-5.75:1 (master: 1.84-5.75:1). That is
better everywhere, but not 4.5:1 for every status. The band is the user's
T0104 choice and stays. 4.5:1 on the band too would be a separate ticket.

## User verification

The user approved the PR as is on 2026-11-28, relayed by the orchestrator
(ferrispad-03): the look is OK, and the selected row keeps T0104's band
with no follow-up ticket. The reviewer (ferrispad-90) approved 592d415; its
one nit (the test recipe's environment) is fixed in 3e9cb40. This commit
only moves the ticket and updates the page.

