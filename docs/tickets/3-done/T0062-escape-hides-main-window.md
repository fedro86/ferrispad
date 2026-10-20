---
id: T0062
title: Escape in the editor hides the main window
status: done
created: 2026-10-20
severity: high
area: ui
depends-on: []
issue: 111
---

## Goal

Pressing Escape while the editor has focus hides FerrisPad's main window.
FLTK's default for a top-level window is to run the window callback (hide) when
an Escape shortcut reaches it unhandled. `src/main.rs` installs a `handle()` on
`w.wind`, but it only catches `Event::Close`, so it does not stop this.

Observed on 2026-10-20 with the 0.9.6 release binary, on X11 (Xvfb, xdotool):
- **With unsaved text** (native binary, scratch `HOME`): after Escape the
  screen has no window, but the process keeps running. No window is left to
  quit from, and an unsaved-changes prompt, if one opened, is not visible.
- **With all changes saved** (FlatPark package): the app exits on Escape.

Escape should never close or hide the main window. Dialogs keep closing on
Escape as they do now.

## In scope

- **Red test first.** Write a test showing that Escape reaching the main window
  does not hide it or quit. It might drive the window callback with an Escape
  shortcut event, or check a pure helper that decides "close request vs.
  Escape" (the same idea as `ui::layout_shortcut`'s testable mapping). If an
  FLTK-level test is impractical, record the manual repro below and say so.
- **Fix.** Give `w.wind` a callback that ignores Escape (`app::event() ==
  Event::Shortcut && app::event_key() == Key::Escape`) and keeps the existing
  close path (WindowClose, unsaved-changes prompt) for real close requests.
- **Check the other top-level windows** for the same default: the split/diff
  windows and the terminal panel. They should behave deliberately.

## Out of scope

- Changing how dialogs react to Escape.

## What was done

- **Cause.** When an Escape press reaches the main window unhandled, FLTK
  (`Fl::handle_`, `cfltk/fltk/src/Fl.cxx`, "make Escape key close windows")
  runs the window's callback with `FL_REASON_CANCELLED`. The main window had
  no callback, so FLTK's default one ran and hid it.
- **Fix.** `src/ui/main_window.rs`: `build_main_window` gives the window a
  callback. It ignores `CallbackReason::Cancelled` and hides the window for
  every other reason, as FLTK's default does, so the close path is unchanged.
  The decision is the pure helper `callback_hides_main_window`.
- **Other windows.** No other top-level window exists. The split/diff views
  and the terminal are panels inside the main window, and dialogs keep FLTK's
  Escape-closes behaviour on purpose.
- **Unit tests.** `ui::main_window::tests`:
  - `escape_never_hides_the_main_window`;
  - `other_callback_reasons_keep_fltks_default_hide`.

### Red / green

A pure FLTK unit test cannot synthesise an Escape that reaches the window
callback with a reason, because there is no setter for `callback_reason`. The
red/green proof is therefore a scripted X11 run (Xvfb + xdotool):
1. open a file;
2. type `abc`;
3. press Escape in the editor;
4. count the visible windows.

| Binary | After Escape in editor |
|---|---|
| release 0.9.6 / master `9b4ebc9` debug build | **0 windows** (process alive with unsaved text, or exited), i.e. red |
| this branch | main window still visible, i.e. green |

Same run on this branch:
- Ctrl+F, then Escape in the Find dialog: the dialog closes and the main
  window stays.
- Ctrl+Q with unsaved text: the "You have unsaved changes" prompt appears.

## How to test

1. Run `nix develop -c cargo run`, type text in a new tab, and press Escape.
   Expect the window to stay, with the text intact.
2. Close the window with the title-bar X. Expect the unsaved-changes prompt as
   before.
3. Open the Find dialog and press Escape. Expect the dialog to close and the
   main window to stay.
4. The gates pass, and the red test fails on master for the bug's reason and
   passes with the fix.

## Acceptance criteria

- [ ] Escape never hides or quits the main window.
- [ ] The close button and File → Quit keep the unsaved-changes flow.
- [ ] A regression test, or a documented manual repro if one is impractical.

## Notes

- Found during T0047 (FlatPark package testing). It is not caused by the
  package: the native release binary does the same.
