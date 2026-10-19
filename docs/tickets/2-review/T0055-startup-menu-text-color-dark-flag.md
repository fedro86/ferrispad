---
id: T0055
title: Pick the menu text colour the same way at startup as on a theme change
status: review
created: 2026-10-19
severity: minor
area: ui
depends-on: [T0019, T0052]
issue: 86
---

## Goal

At startup the menu text colour (`app::foreground`) is chosen from how light
the syntax theme's background is. The dark-mode toggle and Settings choose it
from the dark-mode flag. The two disagree when a light syntax theme is used in
dark mode, or the reverse, so the menu text can change colour after the first
toggle with no visible reason. There should be one rule.

## In scope

- Move the choice into one pure function, e.g. `menu_foreground(theme_bg, is_dark)`,
  and call it at startup and from `dispatch::retheme_panels` (T0019).
- Choose the rule that guarantees contrast: base it on the actual menu
  background, because that is what the text is drawn on. Record the reasoning here.

## Out of scope

- Other startup theme steps.

## What was done

Stacked on T0052 (live Settings theme preview, draft PR #84).

- `ui::theme::menu_foreground(theme: PanelTheme) -> (u8, u8, u8)` is the one
  rule: light text `(230, 230, 230)` when the **menu background**
  (`menu_colors_from_bg(theme.bg).color`) has brightness < 128, black
  otherwise.
- `dispatch::retheme_panels` (the single re-theme entry point) sets
  `app::foreground` from `menu_foreground`. It therefore covers startup, the
  dark-mode toggle, Settings save, and the T0052 live preview
  (`Message::PreviewSyntaxTheme` → `retheme_panels`).
- `main.rs`: removed the startup brightness block. Startup already called
  `retheme_panels` just before it, and now relies on that call alone.
- `settings_dialog.rs`: after each live preview the dialog sets its own
  foreground again (`fltk::app::foreground(tr, tg, tb)`, as it already does
  when it opens). The preview now changes the menu text colour to suit the
  previewed theme, while the dialog keeps the colours of the original theme.
  Without this, previewing a light theme from a dark dialog would put black
  text in the dialog's theme dropdowns. When the dialog closes, `OpenSettings`
  calls `retheme_panels` again, which restores the menu colour.

### Why the rule ignores the dark-mode flag

The text is drawn on the menu background, and `menu_colors_from_bg` derives
that background from the syntax theme background alone. The dark-mode flag
does not affect it. If the colour followed the flag, a light syntax theme in
dark mode would get light text on a light menu, and a dark theme in light mode
would get black on a dark menu. The old startup rule used brightness, so it
read correctly until the first toggle and then flipped. Testing the actual
menu background, rather than the editor background, also keeps the rule
correct if `menu_colors_from_bg`'s blending changes. `PanelTheme` is still the
parameter type so the function has the same signature as every other theme
consumer. Its doc comment says the flag is deliberately not used.

### Red-test evidence (before the fix)

`menu_foreground` first held the old toggle rule unchanged (`theme.is_dark`)
and `retheme_panels` called it. Then:

```
$ nix develop -c cargo test --lib menu_foreground
test ui::theme::tests::menu_foreground_is_the_same_at_startup_and_on_toggle ... FAILED
Base16OceanDark (dark mode false): menu text (0, 0, 0) becomes (230, 230, 230) when the flag flips
Base16OceanDark (dark mode false): menu text (0, 0, 0) unreadable on menu background (71, 74, 83)
Base16OceanDark (dark mode true): menu text (230, 230, 230) becomes (0, 0, 0) when the flag flips
Base16OceanLight (dark mode false): menu text (0, 0, 0) becomes (230, 230, 230) when the flag flips
Base16OceanLight (dark mode true): menu text (230, 230, 230) becomes (0, 0, 0) when the flag flips
Base16OceanLight (dark mode true): menu text (230, 230, 230) unreadable on menu background (213, 214, 217)
...
SolarizedLight (dark mode true): menu text (230, 230, 230) unreadable on menu background (224, 218, 203)
InspiredGitHub (dark mode true): menu text (230, 230, 230) unreadable on menu background (226, 226, 226)
test result: FAILED. 0 passed; 1 failed
```

These are the "light theme with dark mode on" cases the ticket names. The
reverse case fails as well: dark themes in light mode get black text. The bin
test `startup_picks_menu_foreground_through_retheme_panels` also fails against
the old `main.rs`, which called `fltk::app::foreground(` directly.

## How to test

```bash
nix develop -c cargo test --lib menu_foreground
nix develop -c cargo test --bin FerrisPad startup_picks_menu_foreground_through_retheme_panels
```

- `menu_foreground_is_the_same_at_startup_and_on_toggle`: for every
  (syntax theme, dark flag) pair, the colour does not depend on the flag and
  is readable on the menu background.
- `startup_picks_menu_foreground_through_retheme_panels`: `retheme_panels`
  uses `menu_foreground`, startup calls `retheme_panels`, and `main.rs` does
  not set `app::foreground` itself.

Manual check (FLTK popups only; there is no unit test for this):

1. Settings → Theme: Dark. Set the dark-mode syntax theme to a light one
   (e.g. *Inspired GitHub*) and Save. Quit FerrisPad.
2. Start FerrisPad. Open the File menu. Expected: dark text on the light
   menu, readable.
3. Click View → Toggle Dark Mode twice. Open the
   File menu again. Expected: still dark text, the same as in step 2.
4. Reverse case: Theme: Light with a dark syntax theme (e.g. *Solarized
   Dark*). Restart and toggle twice. Expected: light text on the dark menu
   every time.
5. Live preview: in dark mode with a dark syntax theme, open Settings and
   choose a light theme in the dark-theme dropdown (the main window previews
   it). Open the dropdown in the dialog again. Expected: its items stay
   readable on the dialog's colours. Cancel. Expected: the main window's menu
   text is light again.

## Acceptance criteria

- [x] One rule, pinned by the red test; gates green.

## Affected files

- `src/ui/theme.rs` (`menu_foreground` + test)
- `src/dispatch.rs` (`retheme_panels`, source-guard test)
- `src/main.rs` (startup `app::foreground` block removed)
- `src/ui/dialogs/settings_dialog.rs` (sets its foreground again after a preview)

## Notes

- Origin: T0019 report "left out of scope", 2026-10-19.
- Stacked on T0052 (#84). Merge that first.
