---
id: T0019
title: Re-theme all panels on a settings-driven theme change
status: todo
created: 2026-08-24
severity: moderate
area: ui
depends-on: []
---

## Goal

There is no single "re-theme everything" entry point. Theme application is
hand-enumerated at four sites that each cover a different subset of panels.
Consequently, changing the syntax theme via the Settings dialog does **not**
re-theme the split panel, terminal panel, diagnostic panel, or toast — if any
are open they keep the old colors until the user toggles dark mode. The five
panels also expose five different `apply_theme` signatures, so nothing can
iterate them.

## In scope

- Introduce a `trait Themeable { fn apply_theme(&mut self, &DialogTheme); }`
  (or a uniform signature) implemented by every panel.
- Add one `retheme_all(&mut LayoutWidgets)` and call it from all current
  theme-change sites (dark-mode toggle AND settings-driven theme change).

## Out of scope

- The color-math API unification (T0030) — this ticket wires the call, T0030
  fixes what colors get derived.

## How to test

### Manual repro

`cargo run`, open the split panel + terminal + diagnostics, then change the
syntax theme in Settings (without toggling dark mode).

- Before the fix: those panels keep the old colors.
- After the fix: all panels update immediately.

## Acceptance criteria

- [ ] A settings-driven theme change re-themes every open panel.
- [ ] Panels share one themeable interface that can be iterated.
- [ ] Manual recipe verified.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/dispatch.rs:270-289,339-350,1136-1138` — the enumerated theme sites.
- `src/app/state.rs:802-821,994-1014` — the other two.
- The five panels' `apply_theme` (`split_panel`, `terminal_panel`,
  `diagnostic_panel`, `toast`, `tree_panel`, `tab_bar`).

## Notes

- Origin: UI audit (MODERATE). `diagnostic_panel` and `toast` are themed from
  exactly one place today, which is why they're the ones that go stale.
