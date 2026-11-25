---
id: T0082
title: Closing the main window from the title bar must go through the unsaved-changes prompt
status: review          # todo | review | done
created: 2026-11-25
severity: severe        # severe | moderate | minor   (for audit-derived tickets)
area: robustness        # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: 157              # GitHub issue number
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
  - `Closed` → `RequestQuit`: **only** send `Message::WindowClose`. Do not
    call `program_should_quit(true)` and do not hide. The existing
    `FileQuit | WindowClose` dispatch (`src/dispatch.rs:85`) then prompts and
    either quits or (on Cancel) resets the flag, as for File → Quit.
    *(Amended: the original spec set the flag here; see Notes.)*
  - `Cancelled` (Escape) → `Ignore`, unchanged.
  - Every other reason → `Hide`, unchanged FLTK default.
- Remove the dead `Event::Close` arm from the `handle()` in `src/main.rs`,
  with a one-line comment pointing at the callback as the real close path.
- The old arm's "break out of modal loops such as an open menu" intent no
  longer applies: FLTK drops `FL_CLOSE` while a menu or modal dialog holds the
  grab (see *Edge cases* 2–3). No code is added to force it.

## Out of scope

- macOS app-menu Quit / Cmd+Q, and OS logout/shutdown notifications.
- Close behaviour of secondary dialogs (they use `run_dialog`).
- Changing the post-loop safety net or the default `session_restore`.

## Edge cases (each one is covered by a test or a manual step below)

1. **Cancel in the prompt.** The window stays open and usable,
   `program_should_quit` is reset (`dispatch.rs:89`), and a second close
   attempt prompts again.
2. **A modal dialog is open** (Settings, Find, Go to line) when the main
   window's × is clicked. × is ignored while the dialog is open: FLTK drops
   `FL_CLOSE` under a grab or for a non-modal window while another is modal
   (`Fl.cxx:1393-1394`). After the dialog is closed, × prompts normally.
3. **A menu is open** when × is clicked. Same as 2: × is ignored while the
   menu is open; after closing it, × prompts normally.
4. **Double click on ×.** Two `WindowClose` messages arrive. After a Cancel
   there is at most one extra prompt, and nothing quits without the user's
   consent.
5. **No unsaved changes.** The app closes immediately, as before. With
   session restore `Full`/`SavedFiles` the session is saved through
   `file_quit`, as with File → Quit.
6. **Escape** still never closes the main window (existing behaviour).
7. **The prompt must stay up.** `run_dialog_with` (`dialogs/mod.rs:431-437`)
   hides any dialog as soon as `should_program_quit()` is true, so the close
   path must not set that flag before the prompt is answered. Guarded by a
   source-level test.

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
- `main_window_callback_close_path_guards` (source-level, like the guards in
  `dispatch.rs`): the main window module never calls `program_should_quit(`;
  `Closed` sends `Message::WindowClose`; `main.rs` has no `Event::Close` arm.

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
4. Open Settings (modal), then click the main window's ×. × is ignored
   while the dialog is open; after closing the dialog, × prompts normally.
5. Open the File menu, then click ×. × is ignored while the menu is open;
   after closing the menu, × prompts normally.
6. Double-click × quickly, then Cancel. There is no quit without consent.
7. Open an unmodified file and click ×. It closes at once, with no prompt.
8. Press Escape in the editor: the window does not close.
9. Dirty buffer → ×: the prompt stays up until it is answered (it does not
   flash and vanish).

## Acceptance criteria

- [ ] Every window-manager close of the main window goes through
      `file_quit()`: prompt when dirty, Cancel keeps the window open.
- [x] The dead `Event::Close` arm is gone; the decision is a tested pure
      function.
- [x] The red test was seen failing on the old code, with the output
      recorded here.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [x] Manual repro recorded on Linux (Wayland or X11 stated). Windows and
      macOS are marked "pending user verification" if not run.

## Affected files

- `src/ui/main_window.rs:120-143` — decision function and window callback;
  tests at `:380-395`.
- `src/main.rs:371-380` — remove the dead `Event::Close` arm.
- `src/dispatch.rs:85-90` — no change expected; verify the Cancel path.

## What was actually done

- `src/ui/main_window.rs`: `callback_hides_main_window` → `MainWindowAction`
  + `main_window_callback_action(reason)` (`Closed` → `RequestQuit`,
  `Cancelled` → `Ignore`, else `Hide`). The main window callback sends
  `Message::WindowClose` on `RequestQuit`, without setting
  `program_should_quit` and without hiding.
- `src/main.rs`: the dead `Event::Close` arm is removed. A comment says the
  close path is the window callback.
- `src/dispatch.rs`: unchanged. `FileQuit | WindowClose` → `file_quit()`;
  Cancel resets the flag (a no-op now on this path, kept for safety).
- Tests: `main_window_callback_close_requests_quit`,
  `main_window_callback_escape_is_ignored`,
  `main_window_callback_other_reasons_hide`,
  `main_window_callback_close_path_guards`.
  `other_callback_reasons_keep_fltks_default_hide` was removed, and the red
  test was folded into `main_window_callback_close_requests_quit`.

### Red run (before any production change)

```
$ nix develop -c cargo test closing_the_main_window
test ui::main_window::tests::closing_the_main_window_never_hides_it_directly ... FAILED
thread '...closing_the_main_window_never_hides_it_directly' panicked at src/ui/main_window.rs:391:9:
assertion failed: !callback_hides_main_window(CallbackReason::Closed)
test result: FAILED. 0 passed; 1 failed
```

### Verification recipe

```bash
nix develop -c cargo test main_window_callback     # 4 tests
```

Linux X11 repro without touching the real config (needs `wmctrl` and
`xdotool`, e.g. `nix shell nixpkgs#wmctrl nixpkgs#xdotool`):

```bash
mkdir -p /tmp/fp-cfg && FLTK_BACKEND=x11 XDG_CONFIG_HOME=/tmp/fp-cfg cargo run &
sleep 4; wmctrl -a FerrisPad; xdotool type 'unsaved text'
wmctrl -c FerrisPad; sleep 2; wmctrl -l     # expect an "Unsaved Changes" window
```

### Manual results

- **Linux X11 (XWayland under GNOME), scripted.** WM close via
  `wmctrl -c FerrisPad`, which is what × / Alt+F4 send
  (`_NET_CLOSE_WINDOW` → `WM_DELETE_WINDOW` → `FL_CLOSE`). Isolated config,
  `session_restore` Off. Text was typed with `xdotool`.
  - **Before (14d4be5):** dirty buffer → close → the process exits at once
    (exit 0), with no prompt. The text is lost.
  - **After:** dirty buffer → close → an "Unsaved Changes" window appears and
    stays up (the process is still running when `timeout` ends it), so step 9
    passes. Escape (Cancel) in the prompt → the prompt closes and the main
    window stays. A second close → the prompt appears again (steps 1–2,
    partly 6).
  - **After, clean buffer:** close → exits at once, exit 0 (step 7).
- Not run here, **pending user verification**: steps 2 ("Don't Save"
  closes), 3 (native Wayland ×, Alt+F4; Windows taskbar), 4–5
  (modal/menu open), 6 (real double-click), 8 (Escape), and Windows / macOS
  in general.

## Notes

- Reported by the T0077 reviewer. T0077 (remember window geometry) depends on
  this: once title-bar close reaches `file_quit()`, T0077's geometry capture
  on a confirmed quit covers it without a second close hook.
- This branch is based on `master`, not on the T0077–T0081 ticket branch, so
  it can land independently and first.
- **Spec correction (review, before implementation).** The original spec
  had `RequestQuit` call `program_should_quit(true)` and expected an open
  dialog or menu to close and then prompt. Both were wrong:
  - with the flag set, `run_dialog_with` (`src/ui/dialogs/mod.rs:431-437`)
    hides the unsaved-changes prompt after its first `wait()`. It returns
    `None` → `false`, so × with unsaved changes would do nothing. File → Quit
    works precisely because it never sets the flag;
  - FLTK drops `FL_CLOSE` before the callback while a menu or modal dialog
    holds the grab (`Fl.cxx:1393-1394`,
    `if (grab() || (modal() && window != modal())) return 0;`).
  Edge cases 2–3 and manual steps 4–5 were rewritten accordingly. Edge case 7,
  manual step 9 and the source guard were added.
