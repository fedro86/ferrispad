---
id: T0109
title: Readable git-status colors on the file tree's unselected rows
status: todo            # todo | review | done
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
