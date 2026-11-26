---
id: T0098
title: Draw pure black (#000000) as black, not as FLTK's foreground color
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 183
---

## Goal

In dark mode the theme manager shows the "Caret #000000" swatch of Base16
Ocean Light as **light gray**, rgb(230,230,230); in light mode it is black.
The bug shipped in rc.1/rc.2 with the theme manager (T0080).

Cause: FLTK packs an RGB color as `0xRRGGBB00` and reads `0x00000000` as
palette **index 0**, `FL_FOREGROUND_COLOR`. fltk-rs's
`Color::from_rgb(0, 0, 0)` yields exactly that value, and FerrisPad sets the
foreground light in dark mode. Any pure-black RGB is therefore drawn in the
foreground color.

## In scope

- One helper, `ui::color::fltk_rgb(r, g, b)` (`const fn`), returns
  `Color::Black` (`FL_BLACK`, index 56) for `(0, 0, 0)` and the plain RGB
  encoding for everything else. `ThemeRgb::to_fltk` uses it.
- **Every** `Color::from_rgb(` in `src/` (162 calls) goes through it. That
  covers swatches, the color picker, editor and style-table colors, the tab
  bar, split view, tree, plugin manager, terminal palette, dialogs and the
  T0078 contrast tests.
- A guard test fails on any new direct `Color::from_rgb(` outside
  `ui/color.rs`.

## Out of scope

- `Color::from_rgbi` with raw bits coming from FLTK (the contrast callback):
  those are already FLTK colors.

## How to test

```bash
nix develop -c cargo test fltk_rgb_black no_direct_color
```

Xvfb steps: dark app mode → Settings → Manage themes… → select Base16 Ocean
Light; the Caret swatch must be black.

## Acceptance criteria

- [ ] `fltk_rgb(0,0,0)` is not palette index 0, and every other RGB keeps
      its encoding.
- [ ] No direct `Color::from_rgb(` is left in `src/` (guard test).
- [ ] The swatch is black in dark mode (Xvfb).
- [ ] Red tests failed before; full gates green.

## Affected files

- `src/ui/color.rs`: `fltk_rgb` + tests.
- 22 other files under `src/`: `Color::from_rgb(` replaced by `fltk_rgb(`,
  with an import; 5 now-unused `Color` imports removed.

## Notes

- Origin: the orchestrator spotted it in T0097's Xvfb screenshots
  (`before-*` too, so it predates T0097).
- Why the T0078 "black text on the selection" already worked: it returns
  `Color::Black` (`FL_BLACK`) from `best_black_or_white`, never an RGB
  black. This ticket uses the same encoding.
- **Was the editor affected?** Not in the configuration tried. With a dark
  app mode and **Base16 Ocean Light as the dark syntax theme** (caret
  `#000000`), the editor caret was already black before the fix: the
  foreground follows the *active* syntax theme (its menu foreground), so on
  a light editor theme index 0 happens to be dark. The bug shows wherever
  black is drawn against a theme other than the active one, or on a dark UI:
  - the theme manager's swatches of other themes (the reported case);
  - the built-in terminal's ANSI / 256-color black (`(0,0,0)`) on a dark
    theme;
  - plugin-supplied RGB highlight colors.

  The helper covers all of them.

## What was done (2026-11-26)

### Red run (helper still `Color::from_rgb`, no call sites changed)

```
test ui::color::tests::fltk_rgb_black_is_not_the_foreground_index ... FAILED
    assertion `left != right` failed: black packed as palette index 0 (foreground)
test ui::color::tests::no_direct_color_from_rgb_outside_fltk_rgb ... FAILED
    use ui::color::fltk_rgb: (162 call sites listed)
```

### After

- Both pass. The two files that use it only in tests (`contrast.rs`,
  `file_dialogs.rs`) call it by full path.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 583 passed,
  every binary green. fmt and clippy `-D warnings` clean.

### Xvfb (Claude), before = master build, after = this branch

| Check | Before | After |
|---|---|---|
| Theme manager, dark app, Ocean Light selected: Caret `#000000` swatch pixel | (230,230,230) | **(0,0,0)** |
| … Text `#4f5b66` swatch (control) | (79,91,102) | (79,91,102) |
| Editor, dark app, Ocean Light as dark syntax theme: darkest pixel in the caret cell | 0 (black) | 0 (black) |

The screenshot was inspected edge by edge: only the swatch changed.
