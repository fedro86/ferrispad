---
id: T0038
title: Harden the editor-context temp file (permissions + cleanup)
status: todo
created: 2026-08-24
severity: minor
area: security
depends-on: []
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

### Regression test / manual

On Unix, assert the created file's mode is 0600. Manually: confirm the file is
removed (or emptied) after a normal exit, and doesn't retain the last selection
world-readable.

- Before: default-umask file, cleanup only on clean exit.
- After: 0600, robust cleanup.

## Acceptance criteria

- [ ] Context file is owner-only on Unix.
- [ ] Stale content doesn't persist world-readable after a crash.
- [ ] Regression/manual recipe verified.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/editor_context.rs:76,83`.

## Notes

- Origin: plugin/services audit (MINOR). The `unwrap` at `editor_context.rs:39`
  (`editor.buffer().unwrap()`) can be tidied to a `let-else` while here.
