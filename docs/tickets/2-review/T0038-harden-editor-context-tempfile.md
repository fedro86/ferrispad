---
id: T0038
title: Harden the editor-context temp file (permissions + cleanup)
status: review
created: 2026-08-24
severity: minor
area: security
depends-on: []
issue: 44
---

## Goal

`editor_context.rs` writes the user's current selection to
`~/.config/ferrispad/editor-context.txt` at the default umask, and `cleanup()`
only runs on a clean exit — so selected text (possibly sensitive) can linger
world-readable after a crash. This file exists to feed external MCP agents the
editor context; it should be created with restrictive permissions and cleaned
up more robustly.

## In scope

- Create the context file with owner-only permissions (0600 on Unix).
- Make cleanup more robust (best-effort removal on more exit paths, or truncate
  on write so stale content doesn't persist).

## Out of scope

- The MCP protocol itself.

## How to test

### Regression test

```bash
nix develop -c cargo test --lib editor_context::tests
```

- `context_file_is_created_owner_only` (Unix) — a fresh context file is 0600.
- `existing_world_readable_context_file_is_tightened` (Unix) — a pre-existing
  0644 file is 0600 after the next write, with the new content.
- `dropping_the_writer_removes_the_context_file` — dropping the writer (what a
  panic unwinding out of `main` does) removes the file.

Red run — **executed**. First the write was moved, unchanged (`fs::write`),
into `write_context_file`, and a `with_path` constructor was added so tests use
a temp dir instead of the real `~/.config/ferrispad/`. On that code:

```
test result: FAILED. 0 passed; 3 failed
  context_file_is_created_owner_only                  context file mode is 644
  existing_world_readable_context_file_is_tightened   context file mode is 644
  dropping_the_writer_removes_the_context_file        context file survived the writer being dropped
```

Green after the fix: 3/3; full `cargo test`, `clippy -D warnings`,
`fmt --check` clean.

### Manual check (Linux)

1. `nix develop -c cargo run`, open a file, select some text.
2. `stat -c %a ~/.config/ferrispad/editor-context.txt` → `600`.
3. Quit FerrisPad normally → the file is gone.

## What was done

- `write_context_file`: on Unix opens with `mode(0o600)` (create + truncate)
  and then `set_permissions(0o600)`, so a file left at 0644 by an older build
  is tightened before the new selection is written into it. Non-Unix keeps
  `fs::write`.
- `impl Drop for EditorContextWriter` calls `cleanup()`: a panic that unwinds
  out of the event loop in `main` now removes the file as well. The explicit
  `cleanup()` at the end of `main` stays (removal is idempotent).
- `editor.buffer().unwrap()` → `let-else` (from the ticket's Notes).
- New private `with_path` constructor, used by `new()` and by the tests.

### Limits (documented, not fixed)

- A hard crash (SIGKILL, segfault, or a panic inside an FLTK C callback,
  which aborts instead of unwinding) still skips cleanup. The leftover file is
  then owner-only, which is what the acceptance criterion asks for.
- No startup purge of a stale file: the path is shared by every FerrisPad
  instance, so deleting it at startup would wipe a still-running instance's
  context. (Pre-existing: one instance exiting already removes the file the
  others use.)

## Acceptance criteria

- [x] Context file is owner-only on Unix.
- [x] Stale content doesn't persist world-readable after a crash (0600), and
      is removed on panic unwinding.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/editor_context.rs` — `write_context_file`, `with_path`,
  `Drop`, let-else, tests.

## Notes

- Origin: plugin/services audit (MINOR). The `unwrap` at `editor_context.rs:39`
  (`editor.buffer().unwrap()`) can be tidied to a `let-else` while here.
