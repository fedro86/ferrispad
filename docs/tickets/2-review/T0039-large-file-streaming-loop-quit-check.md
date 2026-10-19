---
id: T0039
title: Add a visible Cancel button to the large-file loading progress dialog
status: review
created: 2026-08-25
severity: minor
area: ui
depends-on: []
issue: 46
---

## Goal

When a large file streams in, `large_file.rs::load_to_buffer_with_progress`
shows a modal "Loading File" progress dialog. The only way to stop the load is
the dialog's title-bar X (or Escape), which users do not reliably find. Add a
visible **Cancel** button so stopping a long load is an obvious, explicit
action.

Rescoped on 2026-10-19 (user approved option 3 of the findings below). The
original ticket ("check `app::should_program_quit()` in the streaming loop")
turned out to fix nothing: see *Findings (2026-09-23)*.

## In scope

- A right-aligned **Cancel** button in the progress dialog.
- The button takes the **same path as the dialog's own X**: its callback only
  hides the dialog, as FLTK's default window callback does for X and Escape.
  The `while dialog.shown()` loop then exits, the `cancelled` flag is set, the
  reader thread stops, and the result is `StreamLoadResult::Cancelled`. The
  cleanup is the same as before.
- That shared post-close cleanup lives in one helper, `cancel_stream`, with a
  unit test.
- Remove the `app::should_program_quit()` check that the WIP commit added to
  the streaming loop. It cannot fire while the modal dialog is up.
- Keep the two `read_file_in_chunks` cancel unit tests from the WIP commit.

## Out of scope

- Letting the main window's X / Ctrl+Q quit the app during a load (option 2
  below). That needs the dialog to be non-modal or a window-level quit path.
- Theming the progress dialog. It was unthemed before and stays that way; the
  button uses FLTK's default look, like the rest of this dialog.
- The button-dialog loops already unified in T0017.

## What was done

- `src/ui/dialogs/large_file.rs`
  - The dialog is now 400x155 (was 400x120). A `Flex` row under the size label
    holds a spacer and a 90px **Cancel** button.
  - `cancel_btn.set_callback(move |_| dialog_cancel.hide())`. This is what
    `Fl::default_callback` does for the window's X and Escape, so all three
    leave the loop the same way.
  - New `fn cancel_stream(cancelled: &AtomicBool) -> StreamLoadResult`. It sets
    the flag and returns `Cancelled`, and it is the loop's only "closed by the
    user" exit, replacing the two inline lines that were there.
  - Removed the WIP `should_program_quit()` block and restored the original
    comment wording.
  - Removed a duplicated `/// Message sent from reader thread…` doc line (from
    the WIP commit, kept).
  - Tests (`large_file::tests`):
    - `cancel_stream_flags_reader_and_reports_cancelled` (new)
    - `reader_stops_without_reading_when_cancelled` (WIP, kept)
    - `reader_streams_whole_file_when_not_cancelled` (WIP, kept)

This is an enhancement, so there is no red-test requirement. Clicking the
button needs a live FLTK display, so the click itself is **manual-only**. The
pure part (cleanup, reader cancellation) is unit-tested.

## How to test

### Automated

```bash
nix develop -c cargo test --lib large_file::tests
```

Expect 3 passing tests: `cancel_stream_flags_reader_and_reports_cancelled`,
`reader_stops_without_reading_when_cancelled`, and
`reader_streams_whole_file_when_not_cancelled`.

### Manual (the button itself: manual-only)

The progress dialog only appears for a file in the *Large* tier. That means
larger than **Settings → "Show warning above (MB)"** (default 50) and no larger
than **"Max editable size (MB)"** (default 150). See
`services/file_size.rs::check_file_size` and the defaults in
`domain/settings.rs`. A ~140 MB file can load in under a second, so first widen
the window to make the dialog stay up long enough to click:

1. Generate a ~1.5 GB text file:
   `yes "FerrisPad T0039 large-file cancel test line" | head -c 1500M > /tmp/t0039-big.txt`
2. `nix develop -c cargo run`
3. Settings → set **Max editable size (MB)** to `1800` and Save. Files at
   1.9 GiB and above are always "too large", so stay below that.
4. File → Open `/tmp/t0039-big.txt` → click **Open** in the Large File Warning.
5. The "Loading File" dialog shows a progress bar and, bottom right, a
   **Cancel** button. Click **Cancel** while the bar is still moving.
   - Expect: the dialog closes at once, no tab opens, and the editor responds
     normally. Memory drops back instead of growing to the full file size,
     because the reader thread stopped.
6. Repeat step 4. This time close the dialog with its title-bar **X**, then
   again with **Escape**. Each one behaves exactly like step 5.
7. Repeat step 4 and let the load finish. Expect the file to open in a tab as
   before.
8. Optional: restore Max editable size to `150`, then `rm /tmp/t0039-big.txt`.

## Acceptance criteria

- [x] The progress dialog shows a visible Cancel button.
- [x] Cancel, X and Escape all end the load the same way (`cancel_stream`
      → reader stops → `StreamLoadResult::Cancelled`, no tab opened).
- [x] The unreachable `should_program_quit()` check is removed.
- [x] The `read_file_in_chunks` cancel tests are kept, and `cancel_stream` is
      unit-tested.
- [ ] Manual recipe verified by the user.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/large_file.rs`: the progress dialog layout, the Cancel
  callback, `cancel_stream`, and tests.

## Notes

- Origin: noticed while doing T0017 (UI audit, SEVERE dialog hangs).
- The file name keeps the original `…-quit-check` slug. Ticket files keep their
  name across states (`work-sequence.md`).

## Original goal (pre-rescope, kept as history)

> `load_to_buffer_with_progress` drives its own event loop that polls a
> reader-thread channel. Unlike the button dialogs fixed in T0017, it has no
> `app::should_program_quit()` check and no cancel control, so closing the main
> window (Ctrl+Q / X) during a multi-GB load supposedly leaves the app
> unresponsive to the quit until the read finishes.

## Findings (2026-09-23): blocked on a user decision

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
   explicit way out, alongside the dialog's own X. **(Chosen 2026-10-19.)**
4. Close as "not reproducible as described": the dialog's X already cancels.
