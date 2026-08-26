---
id: T0030
title: Unify the three color-math APIs and fix the diverged derivation factors
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

There are three parallel color-math APIs — `ThemeRgb::{darken,lighten,blend}`
(tab_bar), `dialogs::{darken,lighten}` (free fns), and `theme::blend_colors`
(on `Color`). A comment claims `DialogTheme` uses "the same derivation logic as
tab_bar", but it doesn't: tab_bar uses `darken(0.65)/0.85`, `DialogTheme` uses
`darken(0.85)/0.93`, so dialogs visibly don't match the tab bar. One API, one
set of factors, correct comment.

## In scope

- Pick one color-math module and route the other two through it.
- Reconcile the derivation factors so dialogs and the tab bar match (decide the
  intended look with the user if they differ on purpose), and correct the false
  comment.
- Fold the diff-tab tint magic numbers (`+8,+4,+15` dark / `-2,-2,+0` light),
  duplicated in three places, into one shared constant.

## Out of scope

- Wiring re-theme on settings change (T0019).

## How to test

### Manual

`cargo run` in dark and light mode; compare a dialog's chrome against the tab
bar — they should share the derived shades. Check the diff tab tint still
connects visually to its panel.

## Acceptance criteria

- [ ] One color-math API; the other two removed.
- [ ] Dialogs and tab bar use consistent factors; comment matches reality.
- [ ] Diff-tab tint constants shared (one definition).
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tab_bar.rs:146-164,889-894,1215-1223`, `dialogs/mod.rs:140-218`,
  `theme.rs:80-91,227`, `split_panel.rs:1086-1094,1207-1215`.

## Notes

- Origin: UI audit (MODERATE/SEVERE for the false "same logic" comment).
