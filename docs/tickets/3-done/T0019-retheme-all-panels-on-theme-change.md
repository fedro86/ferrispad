---
id: T0019
title: Re-theme all panels on a settings-driven theme change
status: done
created: 2026-08-24
severity: moderate
area: ui
depends-on: [T0023]
issue: 74
---

## Goal

There is no single "re-theme everything" entry point. Theme application is
hand-enumerated at several sites that each cover a different subset of panels.
Consequently, changing the syntax theme via the Settings dialog does **not**
re-theme the split panel, terminal panel, diagnostic panel, or toast — if any
are open they keep the old colors until the user toggles dark mode. The panels
also expose different `apply_theme` signatures, so nothing can iterate them.

## In scope

- Introduce a `trait Themeable { fn apply_theme(&mut self, PanelTheme); }`
  (one uniform signature) implemented by every panel.
- Add one re-theme entry point for the `LayoutWidgets` panels and call it from
  all current theme-change sites (dark-mode toggle AND settings-driven theme
  change).

## Out of scope

- The color-math API unification (T0030) — this ticket wires the call, T0030
  fixes what colors get derived.
- The Settings dialog's live syntax-theme *preview* (`PreviewSyntaxTheme`): it
  only recolors the editor while the modal dialog is open, as before; the
  panels follow on OK through the Settings path.
- The startup menu foreground rule (see Notes).

## What was done

- `src/ui/theme.rs`: new `PanelTheme { is_dark, bg }` and `trait Themeable`
  with `apply_theme(PanelTheme)` plus `wants_retheme()` (default `true`).
- The six `LayoutWidgets` panels (`tree_panel`, `split_panel`,
  `terminal_panel`, `diagnostic_panel`, `status_bar`, `toast`) and `TabBar`
  now implement `Themeable`; their five different inherent `apply_theme`
  signatures are gone (method bodies unchanged apart from destructuring the
  `PanelTheme`). Tree, split and terminal return `wants_retheme() ==
  is_visible()` — they re-theme themselves when shown, which keeps the old
  "only if visible" rule (the tree rebuild is not free).
- `LayoutWidgets::themeable_panels()` (`src/ui/main_window.rs`) returns all six
  as `[&mut dyn Themeable; 6]`. Its destructure of `LayoutWidgets` is
  exhaustive, so a new field fails to compile until it is listed or marked as
  not themed.
- `dispatch::retheme_panels(state, lw)` is the single entry point: it iterates
  `themeable_panels()` with `state.panel_theme()` and sets the FLTK menu
  foreground. `Message::ToggleDarkMode`, `Message::OpenSettings` and startup
  (`main.rs`) all call it; the hand-enumerated per-site lists are gone.
- `AppState`: the identical owned-widget theming blocks in `toggle_dark_mode`
  and `apply_settings` (editor, window, menu, title bar, tab bar) became one
  `apply_theme_to_owned_widgets()`; new `AppState::panel_theme()`.
- Show paths (`TerminalViewShow`, `WidgetController` split/tree show) use the
  new signature.
- Side effect of the startup call: the diagnostic panel and toast are now
  themed at startup too (the toast defaulted to dark colors until the first
  dark-mode toggle).

### Red test (before the fix)

`dispatch::tests::every_theme_change_path_rethemes_every_panel` — the panels
need a live FLTK display to construct, so the test reads `dispatch.rs` itself:
for each theme-changing Message arm it collects the panels reached (direct
`lw.<panel>.apply_theme(` calls, or `retheme_panels(` → every panel listed in
`LayoutWidgets::themeable_panels`) and requires all six. Run on the unfixed
code:

```
test dispatch::tests::every_theme_change_path_rethemes_every_panel ... FAILED

---- dispatch::tests::every_theme_change_path_rethemes_every_panel stdout ----
thread 'dispatch::tests::every_theme_change_path_rethemes_every_panel' panicked at src/dispatch.rs:1451:13:
Message::OpenSettings changes the theme but does not re-theme ["split_panel", "terminal_panel", "diagnostic_panel", "toast"]
```

After the fix it passes.

## How to test

### Automated

```bash
nix develop -c cargo test --bin FerrisPad every_theme_change_path_rethemes_every_panel
nix develop -c cargo test
```

### Manual (the visual result is manual-only)

1. `nix develop -c cargo run` in light mode (View → Toggle Dark Mode off).
2. Open a terminal panel and a split/diff view from a plugin, and trigger a
   diagnostic (e.g. run a lint plugin check) so the diagnostic panel shows.
3. Open Settings, switch Theme mode to Dark (or pick a different syntax theme
   for the current mode), press OK — do **not** use the dark-mode toggle.
4. Expected: the tab bar, status bar, terminal header, split panel header /
   panes, diagnostic list background and the next toast all switch to the new
   theme immediately. Before the fix the terminal, split panel, diagnostics and
   toast kept the old colors until View → Toggle Dark Mode.
5. Toggle dark mode from the View menu: every open panel follows, as before.

## Acceptance criteria

- [x] A settings-driven theme change re-themes every open panel.
- [x] Panels share one themeable interface that can be iterated.
- [ ] Manual recipe verified.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/theme.rs` — `PanelTheme`, `Themeable`.
- `src/ui/{tree_panel,split_panel,terminal_panel,diagnostic_panel,status_bar,toast,tab_bar}.rs`
  — `impl Themeable`.
- `src/ui/main_window.rs` — `LayoutWidgets::themeable_panels`.
- `src/dispatch.rs` — `retheme_panels`, the two theme-change arms, the
  terminal show path, the regression test.
- `src/app/state.rs` — `panel_theme`, `apply_theme_to_owned_widgets`.
- `src/app/controllers/widget.rs`, `src/main.rs` — call-site updates.

## Notes

- Origin: UI audit (MODERATE). `diagnostic_panel` and `toast` were themed from
  exactly one place (the dark-mode toggle), which is why they went stale.
- Stacked on T0023 (← T0021 ← T0029 / T0030 ← T0027); line numbers in the
  original ticket predate those.
- Pre-existing, kept: at startup the FLTK menu foreground follows the theme
  background's luminance, while the toggle/Settings paths (now
  `retheme_panels`) follow the dark-mode flag. They differ only when a light
  syntax theme is chosen for dark mode (or vice versa). Unifying them is a
  behaviour change left for a follow-up.
