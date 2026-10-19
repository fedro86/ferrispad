---
id: T0057
title: Match Ctrl+letter shortcuts by the typed letter, not the first layout's key
status: done
created: 2026-10-19
severity: severe
area: ui
depends-on: []
issue: 101
---

## Goal

On the user's GNOME Wayland session, Ctrl+M does not trigger View → Preview in
Browser on a Markdown file. Clicking the menu item works, and the Key Shortcuts
dialog shows the binding as `Ctrl+M`.

The same release binary, run under Xvfb (X11), opens the preview on Ctrl+M:
checked on 2026-10-19 with `xdotool key ctrl+m` and a fake `xdg-open` that logged
the call. The user's config has no override for this shortcut, and no plugin
claims Ctrl+M (it is in `RESERVED_SHORTCUTS`). The binding is defined the same
way as in 0.9.5, so this is not a regression from T0026.

Findings (2026-10-19):

- On the user's session Ctrl+M fails on native Wayland, with `FLTK_BACKEND=x11`
  (XWayland) and in the installed 0.9.5 Flatpak. So it is NOT Wayland-specific
  and NOT a regression.
- Under Xvfb (no input method: `XOpenIM() failed`) Ctrl+M works, both with an
  empty config and with a COPY of the user's full `~/.config/ferrispad` (all 6
  plugins loaded). So FerrisPad's config and plugins are ruled out.
- No GNOME keybinding or custom keybinding uses `<Control>m`, and no key
  remapper is running. IBus IS running (`XMODIFIERS=@im=ibus`).

Working hypothesis: the IBus input method on the user's session intercepts or
translates Ctrl+M (CR, 0x0D), e.g. into Return, before FLTK's menu shortcut
matching sees it. Next check: `FLTK_BACKEND=x11 XMODIFIERS= ./target/release/FerrisPad README.md`
(XIM disabled), and whether Ctrl+M inserts a newline in the editor.

## ROOT CAUSE (confirmed 2026-10-19)

The user has a physical AZERTY keyboard and types in a US layout. System xkb
layouts are `fr,it,us`, and GNOME input sources are empty, so the defaults are used.
`wev` (a native Wayland client) receives `sym: m` for the key, so the compositor
is right. A diagnostic FerrisPad build (temporary `app::add_handler` logging,
never committed) logged, for Ctrl+M:

```
Event::Shortcut key=0x2c(',') original=0x2c(',') text="\r" bytes=[13] ctrl=true
```

FLTK's `event_key()` is the AZERTY keysym (`,` sits where QWERTY `m` is), but
`event_text()` is correct: CR is Ctrl+M in the active layout. FLTK menu shortcut
matching uses `event_key()`. `Fl::test_shortcut`'s utf8 fallback only covers
the 0x3f–0x5f "^_" range, not lowercase letters, so:

- **Ctrl+Z (undo) runs Ctrl+W, which closes the tab.** Data-loss risk.
- **Ctrl+A (select all) runs Ctrl+Q, which quits.**
- Ctrl+M (preview) runs nothing (Ctrl+,).

This happens on native Wayland, with `FLTK_BACKEND=x11`, and in 0.9.5. It is not
a regression, and it is not caused by FerrisPad config or plugins: Xvfb with a copy
of the user's config works because it has a single US layout.

## Fix design

When Ctrl (with or without Shift or Alt) is held and `event_text()` is a single
control character 0x01–0x1A, the intended letter is `'a' + (c - 1)`. If that
letter differs from `event_key()`, the layouts disagree. Trust the text: run the
shortcut for Ctrl[+mods]+letter and consume the event, so the menu's wrong match
(e.g. Ctrl+W) never fires.

- Intercept BEFORE menu matching: an `app::event_dispatch` hook, or the main
  window's `handle` for `Event::Shortcut`. `add_handler` is too late, because it
  only sees events nobody handled.
- Resolve the command from the same shortcut table / registry the menu uses
  (T0026's `BUILTIN_SHORTCUTS` + user overrides + plugin shortcuts), and
  trigger that menu item. Don't hard-code letters.
- No change when `event_key()` already matches the text, as for genuine AZERTY
  typists. Keys with no control character (digits, punctuation) are unchanged.
- The pure part, `(event_key, event_text, state) -> Option<intended shortcut>`, is
  unit-tested: Ctrl+M (`,` + "\r") gives Ctrl+M; Ctrl+Z (`w` + 0x1A) gives Ctrl+Z;
  Ctrl+Shift+Z; a matching key gives None; no control char gives None.

## In scope

- **Diagnose first, on the user's session.**
  - Run with `FLTK_BACKEND=x11` (XWayland): does Ctrl+M work there?
  - Does Ctrl+M fail in the installed 0.9.5 Flatpak too, which also runs on Wayland?
  - If needed, log `app::event_key()`, `event_text()` and `event_state()` for the
    key event (temporary debug code, not committed) to see what FLTK actually
    delivers.
  - Check the bundled FLTK source (`fltk-sys-*/cfltk/fltk/src/drivers/Wayland/`)
    for how it builds key events with Ctrl.
- **Fix at the right layer** once the cause is known. For example, a window-level
  handler that maps the delivered event back to the `Ctrl+M` command, or a
  documented change of the default binding if Ctrl+M can't be made reliable on
  Wayland.
- Check other Ctrl+letter defaults that map to control characters with special
  meaning (Ctrl+H = BS, Ctrl+I = Tab, Ctrl+J = LF) for the same problem on Wayland:
  Ctrl+H is Replace.

## Out of scope

- Changing other shortcuts unless the diagnosis shows the same root cause.

## What was done

- **Pure mapping** `ui::layout_shortcut::intended_shortcut(event_key, event_text,
  ctrl, shift, alt) -> Option<String>`: with Ctrl held and `event_text` a single
  control character 0x01–0x1A, the intended letter is `'a' + (c - 1)`. If
  `event_key` is a character key that is a different letter, it returns that
  shortcut in `normalize_shortcut` form (`"ctrl+z"`, `"ctrl+shift+z"`,
  `"alt+ctrl+z"`). It returns `None` when the key already matches, when no
  control character is present, without Ctrl, and for named keys
  (0xef00..=0xffff: Ctrl+Enter, Ctrl+Tab and Ctrl+BackSpace carry CR / TAB / BS
  but are not Ctrl+M / I / H).
- **Resolution through the menu's own rules**: `ui::menu::command_for_shortcut`
  returns the menu path bound to a normalized shortcut: first
  `builtin_shortcut_defaults(tabs_enabled)` with the `ShortcutRegistry`
  overrides, then the plugin menu items. The plugin binding rule (override,
  else manifest default; never a reserved or already-taken shortcut) moved into
  a `PluginShortcutBinder` that `rebuild_plugins_menu_with_orphans` now uses
  too, so both always agree. No letter→message list is hard-coded.
- **Hook before FLTK's matching**: `ui::layout_shortcut::install` registers an
  `Fl::event_dispatch` hook (C ABI, through `fltk-sys`, which fltk already
  builds on; `fltk::app::event_dispatch` transmutes a Rust-ABI `fn` to a C
  callback, so it is not used). Per the bundled FLTK source (`Fl.cxx`),
  `Fl::handle` calls the dispatch hook for every event, including the
  `FL_SHORTCUT` that `Fl::handle_` raises when the focus widget declines an
  `FL_KEYBOARD`. A window `handle` callback is too late, because a group that
  holds the menu bar can match first, and `add_handler` only sees unused events.
  The hook acts only on the main window, with no modal dialog or menu grab:
  - `FL_SHORTCUT` with a mismatch: consumed, `Message::RunShortcut(shortcut)`
    is sent, so the wrong menu item (Ctrl+W) never fires.
  - `FL_KEYBOARD` with a mismatch, only when the main editor has focus:
    `Fl_Text_Editor` matches its own Ctrl+Z / Ctrl+A bindings by `event_key()`
    too, so without this, Ctrl+W (key reported as `z`) would undo. Other
    widgets keep the key; the terminal reads `event_text()`, which is right.
- `AppState::run_shortcut` resolves the path and runs that menu item's callback
  (only if the item is active), exactly what clicking it does.
- Ctrl+H / Ctrl+I / Ctrl+J: `h`, `i` and `j` sit on the same keys in AZERTY and
  QWERTY, so they never mismatch in the user's setup. Ctrl+H opens Replace under
  the two-layout Xvfb setup below. BackSpace, Tab and Enter are named keys and
  are never remapped.

Known limits: other editor widgets (split/diff panels) and dialog inputs still
get the raw key first. If the user unbinds a built-in, a mismatched key bound
to nothing does nothing. Before, it could run whatever `event_key()` matched.

### Red run (stubs returning `None`, before the fix)

```
test ui::layout_shortcut::tests::ctrl_shift_z_keeps_shift ... FAILED
test ui::layout_shortcut::tests::ctrl_w_reported_as_z_is_ctrl_w ... FAILED
test ui::layout_shortcut::tests::ctrl_z_reported_as_w_is_ctrl_z ... FAILED
test ui::layout_shortcut::tests::matching_key_is_left_alone ... ok
test ui::layout_shortcut::tests::named_keys_are_left_alone ... ok
test ui::layout_shortcut::tests::no_control_character_is_left_alone ... ok
test ui::layout_shortcut::tests::without_ctrl_is_left_alone ... ok
test ui::menu::tests::command_for_shortcut_finds_builtin_defaults ... FAILED
test ui::layout_shortcut::tests::ctrl_a_reported_as_q_is_ctrl_a ... FAILED
test ui::menu::tests::command_for_shortcut_finds_plugin_menu_items ... FAILED
test ui::menu::tests::command_for_shortcut_follows_tab_mode ... FAILED
test ui::menu::tests::command_for_shortcut_follows_user_overrides ... FAILED
test ui::layout_shortcut::tests::ctrl_m_on_azerty_first_layout_is_ctrl_m ... FAILED
test ui::layout_shortcut::tests::ctrl_alt_keeps_alt ... FAILED
test result: FAILED. 4 passed; 10 failed; ...
  left: None
 right: Some("ctrl+m")
```

All 14 pass after the fix. The 4 that passed on the stub are the "leave it
alone" cases.

### End-to-end reproduction under Xvfb (X11, two layouts)

Xvfb started with `-noreset` (otherwise the server resets the keymap when the
last client disconnects), `setxkbmap -layout fr,us -option grp:alt_shift_toggle`,
group 2 (us) locked with Alt+Shift, physical keys sent as raw keycodes through
XTest (python-xlib). `xev` confirms the setup: keycode 58 with Ctrl gives
`keysym m`, text `0d` in group 2, and FLTK reads group 0, giving `,`. Scratch HOME,
and a fake `xdg-open` on PATH that logs its args.

| Keys (us group) | Before (master) | After |
|---|---|---|
| Ctrl+M on `test.md` | no `xdg-open` call | `xdg-open .../preview_*.html` |
| type `abc`, Ctrl+Z | "test.md has unsaved changes" close-tab prompt | `abc` undone |
| Ctrl+A | FerrisPad EXITED (quit) | 4 lines selected |
| `abc`, Ctrl+Z, Ctrl+Shift+Z, Ctrl+S | – | file saved as `abc# Title` |
| Ctrl+W / Ctrl+T / Ctrl+F / Ctrl+H / Ctrl+Q | – | close tab / new tab / Find / Replace / quit |
| group 1 (genuine AZERTY): Ctrl+M, Ctrl+Z | works | still works (no remap) |

## How to test

(a) Unit tests:

```bash
nix develop -c cargo test --lib -- layout_shortcut command_for_shortcut
```

(b) Manual, on GNOME Wayland with the physical AZERTY keyboard typing US (the
user's setup: xkb layouts `fr,it,us`, US active):

1. `./target/release/FerrisPad README.md` (any `.md` file), press Ctrl+M.
   Expected: the browser preview opens.
2. Type a few characters, press Ctrl+Z. Expected: they are undone and the tab
   stays open (before: the "unsaved changes" close prompt).
3. Press Ctrl+A. Expected: everything is selected; FerrisPad does not quit.
4. Ctrl+Z, then Ctrl+Shift+Z. Expected: undo, then redo.
5. Ctrl+S saves, Ctrl+F opens Find, Ctrl+T opens a new tab, Ctrl+W closes it.
6. Optional: repeat with `FLTK_BACKEND=x11`. Expected: same results.

## Acceptance criteria

- [x] Root cause recorded with evidence.
- [x] Ctrl+M works with the layouts disagreeing on X11 (Xvfb, two layouts);
      Ctrl+H checked. Native Wayland: user's manual check.
- [x] Gates green.

## Affected files

- `src/ui/layout_shortcut.rs` (new): pure mapping, dispatch hook, tests.
- `src/ui/menu.rs`: `command_for_shortcut`, `PluginShortcutBinder` (shared
  with the plugins menu build), tests.
- `src/app/state.rs`: `AppState::run_shortcut`.
- `src/app/domain/messages.rs`, `src/main.rs`, `src/dispatch.rs`:
  `Message::RunShortcut` and routing; hook installed after `wind.show()`.
- `src/ui/mod.rs`, `Cargo.toml`, `Cargo.lock`: module, direct `fltk-sys` dependency
  (already in the build through fltk).

## Notes

- Origin: user report during T0046/T0056 verification, 2026-10-19.
- The clipped close/Find popups in the Xvfb screenshots are T0058, not this
  ticket.
