---
id: T0058
title: Themed popups always open as top-level windows, never inside another window
status: done
created: 2026-10-19
severity: severe
area: ui
depends-on: [T0024]
issue: 98
---

## Goal

**Regression from T0024 (on master, not yet released).** Starting FerrisPad
with a path that doesn't exist (`FerrisPad /tmp/missing.txt`) shows the themed
error popup "Error opening file: No such file or directory" *inside* the main
window, clipped at the main window's edge, with its OK button outside the
visible area. The popup is modal, so the app is frozen and can't be used: the
user had to kill it.

Reproduced under Xvfb on 2026-10-19: the popup is a 420×106 window drawn as a
child of the main window. Screenshot: `T0058-popup-clipped.png` in the session
scratchpad.

Cause (to confirm with the red test): `src/ui/dialogs/mod.rs` builds the popup
with `Window::default()` without clearing FLTK's current group. When another
window's group is still current (here: during the deferred startup open),
FLTK adds the new window as a child of that group instead of making it a
top-level window. In 0.9.5 this path used `fltk::dialog::alert`, which FLTK
always creates top-level.

## In scope

- Before building any themed popup or dialog window, clear the current group
  (`Group::set_current(None)`), and restore the previous current group
  afterwards so callers that were mid-construction are unaffected.
- Do this once, in the shared popup builder, so every `show_themed_*` entry point
  (message, choice, input) gets it.
- Audit the other hand-built dialogs (`src/ui/dialogs/*.rs`) for the same
  pattern. Fix them through the same helper where it applies.

## Out of scope

- What opening a missing path from the CLI *should* do (T0059).

## Root cause (confirmed)

FLTK adds every new widget, windows included, to the *current group*. The
`X, Y, W, H` window constructor that fltk-rs uses (`Window::default()`,
`Window::new(..)`) does not clear it, unlike C++ `Fl_Window(W, H)`, which does.
And `Fl_Group::end()` makes the group's **parent** current, not whatever was
current before `begin()`. So after `StartPage::show` (`self.flex.begin()` …
`self.flex.end()`) the current group is the start page's parent inside the main
window, and stays so until some other code changes it. The split view's
`rebuild_action_buttons` (`action_bar.begin()` / `end()`) does the same. Any
dialog window built afterwards becomes a child window of the main window. In
0.9.5 the error path used `fltk::dialog::alert`, and FLTK's `fl_alert` clears
the current group itself.

## What was done

- `src/ui/dialogs/mod.rs`: new `TopLevelScope` guard. Creating it saves
  `Group::try_current()` and calls `Group::set_current(None)`. Dropping it
  restores the saved group, so a caller that was mid-construction is
  unaffected. This is the same save/clear/restore that `fl_alert` does.
- The themed popup is split into `build_themed_popup` (builds the window under
  a `TopLevelScope`, restored right after `dialog.end()`) and
  `run_themed_popup` (centres, shows and runs it). `center_screen()` moved to
  the run step because it queries the screen, which needs a display. That
  keeps the builder testable headless. Every `show_themed_*` entry point
  (message, choice, input) goes through the builder.
- **Audit of `src/ui/dialogs/*.rs`:** all 18 other hand-built dialog windows (15 files)
  use `Window::default()` / `Window::new(x, y, ..)`, so each one had the same
  exposure. Each now builds under a `TopLevelScope`, dropped right after its
  `end()`, before it is shown or run: `about`, `community_install`, `find`
  (find and replace), `font_picker`, `goto_line`, `large_file` (too-large
  prompt and loading progress), `plugin_config`, `plugin_manager`,
  `plugin_permissions`, `plugin_settings`, `readonly_viewer`,
  `session_picker` (picker and name prompt), `settings_dialog`,
  `shortcut_dialog`, `update`. The scope is dropped after `end()` rather than at
  function return, so it never restores a group that a modal loop may have
  deleted in the meantime.
- Not affected: the fallback `FileChooser` (`src/ui/file_dialogs.rs`). FLTK's
  `Fl_File_Chooser` builds its window with `Fl_Double_Window(W, H)`, the
  constructor that clears the current group. `NativeFileChooser` is
  GTK/zenity/OS. No other `Window` is built outside `src/ui/dialogs/` except
  the main window.
- Tests: `ui::dialogs::tests::popup_is_top_level`. `crate::ui::fltk_widget_test_lock()`
  (test-only) serializes it with the existing widget-building scrollbar test,
  because FLTK's current group is process-global.
- Out of scope, not changed: `StartPage::show` and the split view's action bar
  still leave a main-window group current after `end()`. The dialogs no longer
  depend on that.

### Red run (before the fix, with only the builder split and the test in place)

```
$ nix develop -c cargo test --lib ui::dialogs::tests::popup_is_top_level
running 1 test
test ui::dialogs::tests::popup_is_top_level ... FAILED

---- ui::dialogs::tests::popup_is_top_level stdout ----

thread 'ui::dialogs::tests::popup_is_top_level' (2790654) panicked at src/ui/dialogs/mod.rs:736:13:
the popup must be a top-level window, not a child of the current group

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 474 filtered out; finished in 0.00s
```

After the fix: `test ui::dialogs::tests::popup_is_top_level ... ok`.

### End-to-end under Xvfb (2026-10-19)

Script: `scenario.sh` in the session scratchpad (`t0058/`). It runs a release
binary under `xvfb-run` with a fresh `HOME`, takes a screenshot with
`import -window root`, lists the root window's children with `xwininfo` (that
is, the top-level windows), and sends keys with `xdotool`.

| Case | `origin/master` (before) | this branch (after) |
|------|--------------------------|---------------------|
| `FerrisPad /tmp/definitely-missing.txt`, 1280×800 | Error popup is a child of the main window, clipped at its right edge, Close button not visible | "Error" is a separate top-level window (420×106+430+347), fully visible with Close. **Return** closes it, the app stays alive and usable |
| Start page → New File, type `mz`, **Ctrl+W**, 1280×800 | "Unsaved Changes" popup is a child of the main window, clipped: Cancel and half of Discard are visible, Save is not | "Unsaved Changes" is top-level, Cancel/Discard/Save are visible. **Escape** closes it and the tab with `mz` is kept |
| Same Ctrl+W case, 1920×1080 | The popup is an unnamed **child** window at `+750+487` inside the 724×487 main window, so it is **completely invisible** while still modal. The app looks frozen | Top-level "Unsaved Changes" at `+750+487` on screen, visible. Escape closes it |

So the runtime freeze the user hit (typing `mz`, then Ctrl+W) is this bug.
`center_screen()` computes *screen* coordinates, but a child window places
them relative to its parent. On a large screen the modal popup therefore lands
outside the main window's area, where nothing of it is drawn, and every click
on the main window is swallowed by the modal. The current group is left inside
the main window by the start page (`StartPage::show`), so the bug appears at
runtime after the start page has been shown, not only at startup. Under Xvfb,
Escape still reached the invisible popup and dismissed it. A user who sees
nothing has no reason to press it.

## How to test

### Regression test

The test builds the themed popup (message and input variants) while an outer
`Window` is begun and not ended. It asserts that the popup's `parent()` is
`None`, that the outer window gained no children, and that the outer window
is still the current group afterwards. It restores FLTK's current group before
asserting.

```bash
nix develop -c cargo test --lib ui::dialogs::tests::popup_is_top_level
nix develop -c cargo test        # full suite
```

### Manual

1. `nix develop -c cargo build --release`, then
   `./target/release/FerrisPad /tmp/definitely-missing.txt`.
   Expected: the "Error" popup "Error opening file: No such file or directory"
   is its own window, centred on the screen, fully visible with its **Close**
   button. Enter, Escape or Close dismiss it, and FerrisPad is usable.
2. Start page → New File, type `mz`, press Ctrl+W. Expected: an "Unsaved
   Changes" popup in its own window (Cancel / Discard / Save). Escape keeps the
   tab, Discard closes it.
3. Smoke test: Go To Line (Ctrl+G) with an out-of-range number, Find (Ctrl+F),
   Settings, Plugin Manager uninstall confirm. Each opens as a normal
   top-level window, never drawn inside the main window.

## Acceptance criteria

- [x] Red test fails before and passes after.
- [x] No themed popup can become a child window; the audit result is recorded.
- [x] Gates green.

## Affected files

- `src/ui/dialogs/mod.rs`: `TopLevelScope`, popup builder/run split, test
- `src/ui/dialogs/{about,community_install,find,font_picker,goto_line,large_file,plugin_config,plugin_manager,plugin_permissions,plugin_settings,readonly_viewer,session_picker,settings_dialog,shortcut_dialog,update}.rs`:
  window built under `TopLevelScope`
- `src/ui/mod.rs`: test-only `fltk_widget_test_lock()`
- `src/ui/scrollbar.rs`: its widget test takes that lock

## Notes

- Origin: the user tried `FerrisPad /tmp/vuoto.txt` (file not created yet)
  while testing T0057, 2026-10-19. Blocks the 0.9.6 release.
