---
id: T0059
title: Opening a missing path from the command line starts a new file with that name
status: done
created: 2026-10-19
severity: moderate
area: ui
depends-on: [T0058]
issue: 100
---

## Goal

`FerrisPad /tmp/notes.txt`, where the file does not exist yet, shows "Error
opening file: No such file or directory". Editors like gedit, vim and VS Code
open an empty buffer bound to that path instead, and create the file on the
first save. FerrisPad should do the same, so "create a file and start typing"
works from the terminal.

## In scope

- In the CLI / deferred open path (`Message::DeferredOpenFile`, `main.rs:354`;
  `FileController` open, `file.rs:~200`): if the path does not exist, and its
  parent directory exists and is a directory, open a new empty tab whose path is
  that file. The title shows the name, it is unmodified, and Save writes there
  without asking for a name.
- If the parent directory does not exist, or the path exists but isn't a regular
  file, keep showing an error (now a proper top-level popup thanks to T0058). Do
  not create directories.
- Treat the path as untrusted input. Canonicalize the parent, keep it a regular
  file, no `..` tricks, and fail closed (engineering standards).
- File → Open (the dialog) is unchanged: it only offers existing files.

## Out of scope

- Session restore of files that were deleted since (separate behaviour).

## How to test

### Regression test (red first)

Test the controller/action logic, not FLTK:

- `open_missing_path_with_existing_parent_starts_new_file`: in a temp dir, open
  `tmp/new.txt`. The controller returns a "new file at path" action, not
  `ShowError`. It must FAIL on today's code, which returns `ShowError`.
- `open_missing_path_with_missing_parent_is_an_error`: `tmp/nodir/new.txt` gives
  `ShowError`.
- `open_directory_path_is_an_error`.

```bash
nix develop -c cargo test --lib open_missing_path
```

### Manual

1. `./target/release/FerrisPad /tmp/ferrispad-new.txt`, where the file doesn't
   exist: an empty tab named `ferrispad-new.txt` opens.
2. Type something and press Ctrl+S: the file is created at `/tmp/ferrispad-new.txt`,
   with no Save As dialog.
3. `./target/release/FerrisPad /tmp/no/such/dir/x.txt`: an error popup that you
   can close, and the app stays usable.

## Acceptance criteria

- [x] Red tests fail before and pass after; gates green.
- [x] No directory is ever created; invalid paths fail closed.

## Affected files

- `src/main.rs` (CLI deferred open), `src/app/controllers/file.rs`, possibly
  `src/app/state.rs`

## Notes

- Origin: user report, 2026-10-19 ("non riesce a crearne uno").

## What was done

- `src/app/controllers/file.rs`: new pure `resolve_cli_open_target(&str) ->
  CliOpenTarget` with three outcomes:
  - `Open(path)`: the path exists and, following symlinks, is a regular file.
    The path is passed on unchanged, so opening existing files behaves as before.
  - `NewFile(path)`: the path does not exist, it has a file name (not `..` or a
    root), and its parent canonicalizes to an existing directory. The returned
    path is `canonical_parent/file_name` (no `..`, symlinks in the parent
    resolved, `\\?\` drive prefix stripped on Windows), and it is checked again
    so that it does not exist.
  - `Invalid(message)`: everything else fails closed. That covers a directory,
    a FIFO/device, a dangling symlink, a missing or non-directory parent,
    `nodir/../x`, a path ending in `..`, any other I/O error, and a non-UTF-8
    result. Nothing is created on disk, and directories never are.
- `FileController::open_cli_path` maps those outcomes to actions: `Open` goes
  to the existing `open_file`. `NewFile` reuses the existing
  `open_file_content(path, "")`, the same path a 0-byte file takes: a tab bound
  to the path, named after the file, unmodified, with `disk_mtime = None` so
  the external-change check ignores it until the first save. `Invalid` becomes
  `ShowError`, which is a closable top-level popup since T0058.
  `file_save` already writes `doc.file_path` directly when it is `Some`, so
  Ctrl+S creates the file and skips Save As. No new document representation
  was needed.
- New `Message::DeferredOpenCliFile`, sent only by the CLI loop in `main.rs`
  (one changed line, plus one line in the deferred match arm), and handled in
  `dispatch::handle_deferred`. `Message::DeferredOpenFile` (plugin
  `open_file`, start-page recent files) is unchanged and still only opens
  existing files. A plugin therefore cannot bind a tab to a path that does not
  exist. File → Open is untouched.

### Red-test evidence

Tests were written first, against a stub that encoded today's behaviour (every
CLI path goes to `open_file`, i.e. `CliOpenTarget::Open(path)`):

```
running 3 tests
test app::controllers::file::tests::open_directory_path_is_an_error ... FAILED
test app::controllers::file::tests::open_missing_path_with_missing_parent_is_an_error ... FAILED
test app::controllers::file::tests::open_missing_path_with_existing_parent_starts_new_file ... FAILED

---- app::controllers::file::tests::open_missing_path_with_existing_parent_starts_new_file stdout ----
assertion `left == right` failed
  left: Open("/tmp/nix-shell.XHdeKC/nix-shell.TYksBk/.tmp2x2TxG/new.txt")
 right: NewFile("/tmp/nix-shell.XHdeKC/nix-shell.TYksBk/.tmp2x2TxG/new.txt")

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 475 filtered out
```

The bug's red test is `open_missing_path_with_existing_parent_starts_new_file`.
The missing path was handed to `open_file`, which answers `ShowError("Error
opening file: No such file or directory")`. The other two are fail-closed
guards: before the fix those paths also ended in `open_file`'s error. After
the fix, all of them pass, along with 6 more guards (existing file keeps its
path, `..` in the parent resolves, `nodir/../x` is refused, a path ending in
`..` is refused, a parent that is a file is refused, a dangling symlink is
refused).

### Verification done (Xvfb, scratch HOME, debug build)

- `FerrisPad /tmp/ferrispad-t0059-new.txt`: the window title is
  `ferrispad-t0059-new.txt - FerrisPad` and the tab has the same name. After
  `xdotool type hello` and `xdotool key ctrl+s`, the file exists with content
  `hello` and only the main window is open, so no Save As appeared.
- `FerrisPad /tmp/no/such/dir/x.txt`: a top-level "Error" popup appears, with
  the text "Cannot create '/tmp/no/such/dir/x.txt': the folder
  '/tmp/no/such/dir' does not exist." Return closes it, the app keeps running,
  and `/tmp/no` is not created.

## How to test (review recipe)

```bash
nix develop -c cargo test --lib open_missing_path   # the ticket's tests
nix develop -c cargo test --lib app::controllers::file::tests   # all 9
```

Manual:

1. `rm -f /tmp/ferrispad-new.txt && ./target/release/FerrisPad /tmp/ferrispad-new.txt`:
   an empty, unmodified tab named `ferrispad-new.txt` opens, and the status
   bar shows `/tmp/ferrispad-new.txt`.
2. Type `hello` and press Ctrl+S. No dialog appears, and
   `cat /tmp/ferrispad-new.txt` prints `hello`.
3. `./target/release/FerrisPad /tmp/no/such/dir/x.txt`: a popup says the
   folder `/tmp/no/such/dir` does not exist. Close it; the app stays usable and
   `/tmp/no` does not exist.
4. Optional: `FerrisPad /tmp` (a directory) is still silently skipped by the
   CLI, as before. File → Open still only lists existing files.
