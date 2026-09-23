---
id: T0039
title: Add a quit/cancel check to the large-file streaming progress loop
status: todo
created: 2026-08-25
severity: moderate
area: ui
depends-on: []
---

## Goal

`large_file.rs::load_to_buffer_with_progress` drives its own event loop
(`while dialog.shown() { app::wait_for(0.01); … }`) that polls a reader-thread
channel and updates a progress bar. Unlike the button dialogs fixed in T0017,
this loop has **no `app::should_program_quit()` check** and offers **no cancel
control**. Closing the main window (Ctrl+Q / X) while a large file is streaming
in leaves the app unresponsive to the quit until the read finishes — for a
multi-GB file that is many seconds of an apparently frozen app. It is not an
infinite hang (the loop exits when the reader thread sends `Done`/`Error`), which
is why it was out of scope for T0017, but it is the same class of defect.

## In scope

- In the streaming loop, check `app::should_program_quit()` each iteration;
  when set, signal the reader thread to stop (`cancelled.store(true, …)`), hide
  the dialog, and return `StreamLoadResult::Cancelled`.
- Confirm the reader thread (`read_file_in_chunks`) observes the `cancelled`
  flag promptly and terminates without finishing the file.

## Out of scope

- Adding a user-facing Cancel *button* to the progress dialog (nice-to-have; a
  separate UI ticket if wanted). This ticket only handles the app-quit path.
- The button-dialog loops already unified in T0017.

## How to test

### UI-only — no unit test is feasible

Same constraint as T0017: the loop needs a live FLTK display, a real streaming
load, and FLTK's global quit flag, so it cannot run under headless `cargo test`
(`engineering-standards.md`: UI-only bugs document a manual repro). The reader
thread's response to `cancelled` *could* be unit-tested in isolation if
`read_file_in_chunks` is callable with a pre-set `AtomicBool` — add that test if
it is practical; otherwise rely on the manual repro.

### Manual repro

1. `nix develop -c cargo run`
2. Open a very large file (multi-GB) so the streaming progress dialog appears
   and stays up for several seconds.
3. While it is loading, close the main window (Ctrl+Q or the window's X).
4. Expect: the app exits promptly (load is cancelled).
   - Before the fix: the app stays up, unresponsive to the quit, until the whole
     file finishes loading.

## Acceptance criteria

- [ ] Quitting during a streaming load cancels it and the app exits promptly.
- [ ] The reader thread stops on the `cancelled` flag (no wasted full read).
- [ ] Any feasible test added; manual recipe verified.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/large_file.rs` — the streaming loop (`while dialog.shown()`,
  ~line 324) and, if needed, `read_file_in_chunks`'s cancel responsiveness.

## Notes

- Origin: noticed while doing T0017 (UI audit, SEVERE dialog hangs). This is the
  distinct, self-terminating sibling of that bug. `StreamLoadResult::Cancelled`
  and the `cancelled: Arc<AtomicBool>` plumbing already exist — the fix is to
  wire the quit path into them.

## Findings (2026-09-23) — blocked on a user decision

Work started (issue #46, branch `ticket/T0039`), then paused: the ticket's
premise does not hold as written.

- **The main-window close never reaches FerrisPad while this dialog is up.**
  The progress dialog is `make_modal(true)`, and FLTK drops `FL_CLOSE` for any
  window other than the modal one (`fltk-sys` `Fl.cxx:1394`:
  `case FL_CLOSE: if (grab() || (modal() && window != modal())) return 0;`).
  So the main window's `Event::Close` handler (`main.rs:375-385`, the only place
  that calls `program_should_quit(true)`) does not run, and
  `app::should_program_quit()` stays false for the whole load. Ctrl+Q is a
  keyboard event and goes to the modal dialog, not the menu.
- **Closing the progress dialog itself already cancels.** Its X is allowed
  (it *is* the modal window); the default callback hides it, `while
  dialog.shown()` exits, `cancelled` is set and the reader thread stops.
- The branch has the `should_program_quit()` check (harmless, consistent with
  `run_dialog`, but effectively unreachable today) and two unit tests on
  `read_file_in_chunks` (cancel flag stops the reader before any read; without
  it the whole file streams and `Done` arrives). Both green.

Options:

1. Ship the branch as a defensive change + tests (no user-visible effect).
2. Let quit work during a load: e.g. make the dialog non-modal or add a
   window-level quit path, so X / Ctrl+Q on the main window cancel the load.
   A real UI change — needs manual verification on a display.
3. Add a visible **Cancel** button (the ticket's out-of-scope item) — the
   explicit way out, alongside the dialog's own X.
4. Close as "not reproducible as described": the dialog's X already cancels.
