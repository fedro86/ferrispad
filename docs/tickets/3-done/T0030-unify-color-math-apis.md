---
id: T0030
title: Unify the three color-math APIs and fix the diverged derivation factors
status: done
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 52
---

## Goal

There were three parallel color-math APIs — `ThemeRgb::{darken,lighten,blend}`
(tab_bar), `dialogs::{darken,lighten}` (free fns), and `theme::blend_colors`
(on `Color`). A comment claimed `DialogTheme` uses "the same derivation logic as
tab_bar", but it doesn't: tab_bar uses `darken(0.65)/0.85`, `DialogTheme` uses
`darken(0.85)/0.93`. Worse, the large-file and read-only-viewer dialogs still
derived their Cancel/Close button colour from a stale copy of the old
`0.65/0.85` dialog factors (comment: "the same way DialogTheme derives dialog
bg"), so those buttons were visibly darker than intended against the real
dialog background. One API, one set of factors, correct comments.

## In scope

- Pick one color-math module and route the other two through it.
- Reconcile the derivation factors and correct the false comments.
- Fold the diff-tab tint magic numbers (`+8,+4,+15` dark / `-2,-2,+0` light),
  duplicated in three places, into one shared definition.

## Out of scope

- Wiring re-theme on settings change (T0019).
- Changing the tab-bar or dialog *background* factors themselves (see Notes).

## Decision on the factors

The dialog/tab-bar divergence is **intentional**: commit `cdf5750` ("fix:
lighten dialog backgrounds for both dark and light themes") moved the dialog
background from `0.65/0.85` to `0.85/0.93` on purpose, because dialogs looked
too dark next to the editor. So the dialog background keeps `0.85/0.93`, now
named `DIALOG_BG_DARKEN_DARK` / `DIALOG_BG_DARKEN_LIGHT` with a comment that
says why they differ from the tab bar. The actual defect was the leftover copy
of the *old* factors in `large_file.rs` and `readonly_viewer.rs`; those buttons
now derive from the real dialog background.

## What was done

- New `src/ui/color.rs` holds the single color-math type, `ThemeRgb` (moved out
  of `tab_bar.rs`), with `darken` / `lighten` / `blend` / `brightness` plus
  `from_tuple` / `to_tuple` / `from_fltk` / `to_fltk` and `WHITE`. `lighten`
  now uses `saturating_add` (same result for factors in `0..=1`, no overflow
  panic outside that range). Unit-tested.
- Removed `dialogs::{darken, lighten}` and `theme::blend_colors`; every caller
  (`dialogs/mod.rs`, `theme.rs`, `tree_panel.rs`, `split_panel.rs`,
  `large_file.rs`, `readonly_viewer.rs`, and the macOS/Windows title-bar code)
  goes through `ThemeRgb`.
- `DialogTheme` gains `secondary_button_bg` (dialog bg `lighten(0.10)` dark /
  `darken(0.90)` light). The large-file Cancel button and the read-only-viewer
  Close button use it instead of re-deriving from the stale `0.65/0.85`.
- `divider_color_from_bg` returns the tab bar's `bar_bg` directly instead of
  duplicating its `0.65/0.85` factors.
- `theme::diff_tab_tint_from_bg(theme_bg, is_dark)` with `DIFF_TAB_TINT_DARK` /
  `DIFF_TAB_TINT_LIGHT` replaces the three copies (tab bar active diff tab,
  split-panel labels row, split-panel tab-mode header/action bar).
- Corrected the false "same as tab_bar" doc comments on `DialogTheme` and the
  tree-panel header.

Behaviour is unchanged everywhere except the two secondary buttons, which are
now one step lighter in dark mode and in light mode match the dialog chrome.

## How to test

### Regression test

```bash
nix develop -c cargo test --lib secondary_button_bg_derives_from_dialog_bg
```

Red run — **executed** with the stale derivation moved verbatim into
`DialogTheme::secondary_button_bg`, before the fix:

```
test ui::dialogs::tests::secondary_button_bg_derives_from_dialog_bg ... FAILED
assertion `left == right` failed: theme_bg (40, 44, 52)
  left: (48, 50, 55)
 right: (56, 58, 65)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 369 filtered out
```

(`left` is derived from `darken(0.65)`, the pre-`cdf5750` dialog bg; `right`
is derived from the real dialog bg, `darken(0.85)`.) Green after the fix.

### Other new unit tests

```bash
nix develop -c cargo test --lib ui::color ui::theme
```

- `ui::color::tests::*` — darken/lighten/blend values, lighten saturation,
  FLTK round trip.
- `ui::theme::tests::divider_matches_tab_bar_background`,
  `ui::theme::tests::diff_tab_tint_offsets_and_saturates`.

### Manual

1. `cargo run`, choose a dark syntax theme (e.g. a One Dark-like theme).
2. Open a file above the "too large to edit" threshold (Settings, default
   150 MB; e.g. `head -c 200M /dev/urandom | base64 > /tmp/big.txt`). The
   "file too large" dialog appears: its **Cancel** button is a slightly lighter
   step off the dialog background, not a darker patch (before: visibly darker
   than the dialog).
3. In that dialog choose **View Read-Only**: the viewer's **Close** button
   looks the same way.
4. Repeat with a light theme: both buttons are a subtle darker step off the
   dialog background.
5. Everything else is unchanged: tab bar, dividers, menus, tree panel, editor
   scrollbars, and an MCP diff review shown as a tab — the active diff tab's
   tint still connects to the split panel's header and labels row.

## Acceptance criteria

- [x] One color-math API (`ui::color::ThemeRgb`); the other two removed.
- [x] Dialogs and tab bar use consistent, named factors; comments match reality.
- [x] Diff-tab tint constants shared (one definition).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/color.rs` (new), `src/ui/mod.rs`, `src/ui/tab_bar.rs`,
  `src/ui/dialogs/mod.rs`, `src/ui/dialogs/large_file.rs`,
  `src/ui/dialogs/readonly_viewer.rs`, `src/ui/theme.rs`,
  `src/ui/split_panel.rs`, `src/ui/tree_panel.rs`.

## Notes

- Origin: UI audit (MODERATE/SEVERE for the false "same logic" comment).
- If you want dialogs to match the tab bar exactly after all, that is a
  one-line change to `DIALOG_BG_DARKEN_DARK` / `_LIGHT` (it would undo
  `cdf5750`), so it was left as is.
- The macOS/Windows title-bar code was converted but only compiles on those
  targets; it was checked by reading.
