---
id: T0082
title: Closing the main window from the title bar must go through the unsaved-changes prompt
status: todo            # todo | review | done
created: 2026-11-25
severity: severe        # severe | moderate | minor   (for audit-derived tickets)
area: robustness        # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: ~                # GitHub issue number, filled when work starts
---

## Goal

Closing FerrisPad with the window's close button (title-bar ×, Alt+F4, the
taskbar's "Close window", the window manager's close) **silently loses
unsaved changes** with the default settings. The cause:

- FLTK delivers `FL_CLOSE` only to the window's **callback**
  (`Fl.cxx:1393`, `wi->do_callback(FL_REASON_CLOSED)`), never to `handle()`.
  This was confirmed with a probe on Linux Wayland and X11. The branch is the
  same C++ path on every platform.
- So the `Event::Close` arm of the main window's `handle()`
  (`src/main.rs:371-380`) is dead code: it never sends
  `Message::WindowClose`.
- The main window's callback (`src/ui/main_window.rs:139`) hides the window
  for every reason except `Cancelled`. The FLTK loop then ends with no window
  shown, and `file_quit()` (which holds the unsaved-changes prompt) never runs.
- The post-loop safety net (`src/main.rs`, `if !quit_clean { save_session… }`)
  saves nothing when `session_restore` is `Off`, and `Off` is the default
  (`settings.rs`). Unsaved edits are gone.

Only File → Quit (`Message::FileQuit`) currently goes through the prompt.

Found during the review of T0077, which needs the same close path to capture
the window geometry.

## In scope

- Replace `callback_hides_main_window(reason) -> bool` with a pure decision
  `main_window_callback_action(reason) -> MainWindowAction`:
  - `Closed` → `RequestQuit`: call `program_should_quit(true)` and send
    `Message::WindowClose`, **without hiding**. The existing
    `FileQuit | WindowClose` dispatch (`src/dispatch.rs:85`) then prompts and
    either quits or resets the flag.
  - `Cancelled` (Escape) → `Ignore`, unchanged.
  - Every other reason → `Hide`, unchanged FLTK default.
- Remove the dead `Event::Close` arm from the `handle()` in `src/main.rs`,
  with a one-line comment pointing at the callback as the real close path.
- Keep the old arm's intent: "break out of modal loops such as an open menu".
  Check that closing while a menu or a modal dialog is open still works. See
  *Edge cases*.

## Out of scope

- macOS app-menu Quit / Cmd+Q, and OS logout/shutdown notifications.
- Close behaviour of secondary dialogs (they use `run_dialog`).
- Changing the post-loop safety net or the default `session_restore`.

## Edge cases (each one is covered by a test or a manual step below)

1. **Cancel in the prompt.** The window stays open and usable,
   `program_should_quit` is reset (`dispatch.rs:89`), and a second close
   attempt prompts again.
2. **A modal dialog is open** (Settings, Find, Go to line) when the main
   window's × is clicked. `run_dialog_with` already hides dialogs when
   `should_program_quit()` is set. The `WindowClose` message is then
   processed by the main loop, and the prompt appears. There is no deadlock
   and no lost message.
3. **A menu is open** when × is clicked. Same expectation as 2.
4. **Double click on ×.** Two `WindowClose` messages arrive. After a Cancel
   there is at most one extra prompt, and nothing quits without the user's
   consent.
5. **No unsaved changes.** The app closes immediately, as before. With
   session restore `Full`/`SavedFiles` the session is saved through
   `file_quit`, as with File → Quit.
6. **Escape** still never closes the main window (existing behaviour).

## How to test

### Red test first

In `src/ui/main_window.rs`'s test module the existing test
`other_callback_reasons_keep_fltks_default_hide` **asserts the bug**: it
expects `Closed` to hide the window. Before changing any production code:

1. Add `closing_the_main_window_never_hides_it_directly`:
   `assert!(!callback_hides_main_window(CallbackReason::Closed))`.
2. Run `nix develop -c cargo test closing_the_main_window` and record the
   failing output here. It fails because the current rule hides on
   `Closed`.

Then implement, and convert both tests to the new decision function:

```bash
nix develop -c cargo test main_window_callback
```

- `main_window_callback_close_requests_quit` — `Closed` → `RequestQuit`.
- `main_window_callback_escape_is_ignored` — `Cancelled` → `Ignore`.
- `main_window_callback_other_reasons_hide` — e.g. `Unknown` → `Hide`.
- Remove `other_callback_reasons_keep_fltks_default_hide`; its `Closed` case
  is the bug.

The full `nix develop -c cargo test` stays green.

### Manual repro (before = bug, after = fixed; record results here)

Use default settings (session restore Off: move `settings.json` aside), then
`nix develop -c cargo run`.

1. Type some text in the untitled document and click the title-bar ×.
   **Before:** FerrisPad closes and the text is lost. **After:** the
   unsaved-changes prompt appears.
2. In the prompt, choose Cancel. The window stays and the text is intact.
   Close again and choose Don't Save: it closes.
3. Repeat step 1 with Alt+F4. On Windows, also use the taskbar's "Close
   window". On Linux, record whether the session is Wayland or X11.
4. Open Settings (modal), then click the main window's ×. The dialog closes,
   then the prompt appears.
5. Open the File menu, then click ×. The menu closes, then the prompt
   appears.
6. Double-click × quickly, then Cancel. There is no quit without consent.
7. Open an unmodified file and click ×. It closes at once, with no prompt.
8. Press Escape in the editor: the window does not close.

## Acceptance criteria

- [ ] Every window-manager close of the main window goes through
      `file_quit()`: prompt when dirty, Cancel keeps the window open.
- [ ] The dead `Event::Close` arm is gone; the decision is a tested pure
      function.
- [ ] The red test was seen failing on the old code, with the output
      recorded here.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [ ] Manual repro recorded on Linux (Wayland or X11 stated). Windows and
      macOS are marked "pending user verification" if not run.

## Affected files

- `src/ui/main_window.rs:120-143` — decision function and window callback;
  tests at `:380-395`.
- `src/main.rs:371-380` — remove the dead `Event::Close` arm.
- `src/dispatch.rs:85-90` — no change expected; verify the Cancel path.

## Notes

- Reported by the T0077 reviewer. T0077 (remember window geometry) depends on
  this: once title-bar close reaches `file_quit()`, T0077's geometry capture
  on a confirmed quit covers it without a second close hook.
- This branch is based on `master`, not on the T0077–T0081 ticket branch, so
  it can land independently and first.
