---
id: T0106
title: Build seams so the structural theme test covers every dialog
status: todo            # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0105]     # stacked on ticket/T0105 (components, unthemed_inputs)
issue: 205
---

## Goal

T0105's structural theme test (guard (b): `unthemed_inputs` in
`tests/ui_dialogs.rs`) only reaches dialogs that have a `build_*` function
returning their window without running it: the theme manager, color picker,
name prompt, Settings and the Sessions picker. Every other dialog builds its
window and runs its event loop in one function. A themed property one of
them gets wrong (a field without the shared selection, a hand-styled
button) is caught only if someone opens it under Xvfb. The goal is for
guard (b) to walk **every** dialog.

## In scope

- Split each run-only dialog into `build_<dialog>(theme, …) -> <Dialog>Ui`
  (window and the widgets its callbacks need, built but not shown) plus the
  existing `show_*` function, which builds, wires callbacks and runs. No
  behavior change. The dialogs: find, replace, go to line, font picker, new
  session prompt, shortcut dialog, about, update available, large-file
  warning and "file too large", the read-only viewer, plugin
  config / settings / permissions / manager, and community install.
- `tests/ui_dialogs.rs` builds every one of them in dark and light, and runs
  the layout checks and `unthemed_inputs` on each.
- `unthemed_inputs` also checks buttons (frame, color and label color equal
  to a `button()`, `secondary_button()` or `warning_button()`). The lists it
  already checks.

## Out of scope

- Any visual change: proven by a pixel diff per dialog (below).
- Dialogs that FLTK builds itself (the native file chooser, `fltk::dialog`
  alerts).
- The progress window of `load_to_buffer_with_progress`, which drives its
  own loop while it reads.

## How to test

```bash
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test ui_dialogs
```

- Red: with one component mutated (e.g. `style_text_field` not setting the
  cursor color, or `button()` not setting the label color), the test fails
  and names each newly covered dialog that uses that component.
- Pixel diff (Xvfb, Claude): each dialog is opened in the app, from the
  T0105 build (before) and from this branch (after), on a dark and a light
  theme. `compare -metric AE` must report 0 differing pixels per dialog.

## Acceptance criteria

- [ ] Every listed dialog has a `build_*` seam, and the structural test
      walks it in dark and light.
- [ ] `unthemed_inputs` checks buttons, and fails on a mutated component
      for each newly covered dialog.
- [ ] Pixel diff: 0 differing pixels per dialog, before vs after.
- [ ] Full gates green.

## Affected files

- `src/ui/dialogs/*.rs`, `src/ui/file_dialogs.rs`: the `build_*` seams.
- `tests/ui_dialogs.rs`, `tests/support/ui_layout.rs`: the extended guard.

## Notes

- Origin: T0105's review (the follow-up text in T0105's Notes); assigned
  by the orchestrator on 2026-11-26, target 0.9.11.
