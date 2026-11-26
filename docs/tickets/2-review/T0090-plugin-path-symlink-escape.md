---
id: T0090
title: One physical path rule for the plugin sandbox — no escape through in-root symlinks
status: review          # todo | review | done
created: 2026-11-26
severity: severe        # sandbox escape: arbitrary file write outside the project root
area: security
depends-on: []
issue: ~                # GitHub issue number, filled when work starts
---

Disclosure: private until the fix ships in 0.9.10-rc.3 (user decision 2026-11-26).

## Goal

A sandboxed Lua plugin must never create, write, rename or delete anything
outside the project root. Today it can, through a symlink **inside** the
root. That is pre-existing in released versions; found by review (a3) and
confirmed by the orchestrator.

The plugin-path check has two rules:

- `security::validate_path` canonicalizes the target, or its parent when the
  target does not exist.
- When even the parent is missing (`NotFound`), `sandbox::resolve_and_validate`
  falls back to a **lexical** fold (`normalize_lexical`) plus `starts_with`.
  That fold never resolves a symlink that already exists in the path.

Escapes, with `root/link -> /outside` (any directory):

- `editor:write_file("link/newdir/pwned.txt", "x")` returns `(true, "")` and
  creates `/outside/newdir/pwned.txt`: `create_dir_all` and `fs::write` let the
  kernel follow `link`.
- `editor:create_dir("link/a/b")` creates `/outside/a/b`.
- Needs at least one missing intermediate directory; `link/x` alone is caught,
  because the parent exists and is canonicalized.
- A **dangling** symlink inside the root escapes with one level. With
  `root/dangle -> /outside/new.txt` (which does not exist yet),
  `write_file("dangle", …)` / `create_file("dangle")` pass the check (the
  parent `root` is fine) and the kernel creates `/outside/new.txt`.
- `setup_mcp_config` writes `<root>/.mcp.json` and `<root>/.gitignore`
  without re-checking them: a dangling `.mcp.json` symlink or a `.gitignore`
  symlink to an outside file writes outside the root.

Impact: e.g. `link -> $HOME`, then `.config/autostart/x.desktop` gives code
execution at the next login.

Same branch, opposite direction: the plugin API test
`test_lua_create_dir_and_rename` fails on macOS (the T0076 macOS gate skips
it) because the tempdir root `/var/folders/…` is an alias of
`/private/var/folders/…`. An absolute path under the alias is folded
lexically, never canonicalized, and **rejected** — a functional bug that
fails closed.

## In scope

- One resolution rule in `security::validate_path`, used by every plugin path:
  1. Build the raw path: an absolute input as-is, a relative one joined to the
     project root.
  2. Find its longest ancestor that exists **as a directory entry**
     (`symlink_metadata`, so a dangling symlink counts as existing).
  3. Canonicalize that ancestor: the kernel resolves symlinks and `..`
     physically. If that fails (e.g. a dangling link), reject.
  4. Reject (fail closed) if the not-yet-existing tail has any component other
     than a plain name (`..`, `.`, root, prefix).
  5. Join the tail and require `starts_with(canonical_root)`.

  `resolve_and_validate` becomes a thin wrapper; `normalize_lexical` and the
  lexical fallback go away.
- `setup_mcp_config` resolves `.mcp.json` and `.gitignore` through the same
  rule before touching them.
- Audit of every plugin filesystem entry point (list below): all go through
  the one rule.
- Red tests (Unix only) for each escape above, plus the macOS-style alias case.

## Out of scope

- TOCTOU: a symlink created or swapped between the check and the write (a
  plugin racing itself, or another process). Closing it needs
  `openat`/`O_NOFOLLOW`-style APIs end to end; follow-up candidate.
- `run_command` (runs with the project root as cwd, takes no path argument
  that the sandbox resolves) and the read-only `diff_text`.
- The T0089 macOS test-harness work (FLTK tests on the main thread).

## Entry points audited

| Lua API | Ops | Check before |
|---|---|---|
| `file_exists`, `read_file`, `git_status` | read / spawn `git` | `validate_path` |
| `is_file`, `list_dir`, `scan_dir` | read | `resolve_and_validate` |
| `create_file` | `OpenOptions::create_new` | `resolve_and_validate` |
| `write_file` | `create_dir_all(parent)` + `fs::write` | `resolve_and_validate` |
| `create_dir` | `create_dir_all` | `resolve_and_validate` |
| `rename` | `fs::rename`, source **and** destination | `resolve_and_validate` ×2 |
| `remove` | `remove_dir_all` / `remove_file` | `resolve_and_validate` |
| `setup_mcp_config` | `fs::write` `.mcp.json`, read+write `.gitignore` | root only → now each file too |
| hook result `open_file` (`hook_dispatch.rs`) | opens a tab | `validate_path` |

There is no copy/move API besides `rename`.

## How to test

### Regression tests (red first)

```bash
nix develop -c cargo test plugin_path_
```

All are `#[cfg(unix)]` and use a real temp tree:

- `plugin_path_write_file_through_in_root_symlink_is_rejected`: `write_file`
  through `link/newdir/…` returns false and nothing appears under the outside
  directory.
- `plugin_path_create_dir_through_in_root_symlink_is_rejected`: the same for
  `create_dir("link/a/b")`.
- `plugin_path_dangling_symlink_is_not_written_through`: `write_file` and
  `create_file` on `dangle` both return false, and the outside target is not
  created.
- `plugin_path_mcp_config_does_not_follow_symlinks`: the `.mcp.json` /
  `.gitignore` writer refuses symlinks that point outside.
- `plugin_path_absolute_path_under_root_alias_is_accepted`: root given as an
  alias symlink; `create_dir` + `rename` with absolute alias paths succeed and
  stay inside (the macOS `/var` case, reproduced on Linux).

Before the fix: each fails for the bug's reason (recorded below). After: all
green, and every pre-existing sandbox test stays green (`traversal behind a
missing dir`, `deep new dirs inside`, `absolute outside`, …).

## Acceptance criteria

- [ ] No plugin filesystem API can create, write, rename or delete outside the
      canonical project root through any in-root symlink (existing target,
      missing intermediate directories, or dangling).
- [ ] Absolute paths under a symlinked alias of the root are accepted
      (`test_lua_create_dir_and_rename` passes on macOS; verified once T0090 is
      public and the T0076 skip is dropped).
- [ ] One resolution rule; the lexical fallback is gone.
- [ ] Red tests added, failing before and green after; the full gates pass.

## Affected files

- `src/app/plugins/security.rs` — `validate_path` = the one physical rule.
- `src/app/plugins/api/sandbox.rs` — `resolve_and_validate` thin wrapper;
  `normalize_lexical` removed.
- `src/app/plugins/api/editor.rs` — `setup_mcp_config` resolves its two files.
- `src/app/plugins/api/mod.rs` — red tests.

## Notes

- Origin: review by ferrispad-a3 during T0076/T0089 triage (2026-11-26).
- Process: local branch `ticket/T0090` only — no push, no issue, no PR until
  the orchestrator / user say so. Public text elsewhere (T0089, the T0076
  `macos-gates.yml` comment) only says "plugin path validation on macOS, see
  T0090".

## What was done (2026-11-26, local branch only — not pushed)

- `security::validate_path` is now the one physical rule (see *In scope*).
  The `NotFound` variant is gone: a not-yet-existing path inside the root is
  `Valid`, anything that cannot be proven inside is `OutsideProjectRoot` /
  `InvalidPath` (fail closed).
- `sandbox::resolve_and_validate` is a thin wrapper (logs and maps to
  `Option`); `normalize_lexical` and the lexical `NotFound` fallback are
  deleted.
- `editor::setup_mcp_config`: the file writes moved into
  `write_mcp_files(root, project_root, binary)`, which resolves `.mcp.json`
  and `.gitignore` through the same rule before reading/writing them.
- `filesystem::file_exists` and the two `tests/plugin_security_chain.rs`
  matches no longer mention `NotFound`.

Every entry point in the audit table now ends in `validate_path`:
`file_exists`, `read_file`, `git_status` and hook `open_file` call it
directly; `is_file`, `list_dir`, `scan_dir`, `create_file`, `write_file`,
`create_dir`, `rename` (both paths), `remove` and `setup_mcp_config` (its
root and both files) through `resolve_and_validate`.

Behaviour change outside the escape: hook `open_file` (and `read_file` /
`file_exists`) now treat a *deep* not-yet-existing path inside the root as
valid (it was `NotFound` → blocked; a one-level missing file was already
valid). Opening it behaves like opening any missing file; reads report the
I/O error instead of "outside project root".

### Red run (before the fix, same tests)

```
test …::plugin_path_mcp_config_does_not_follow_symlinks ... FAILED
    .mcp.json written through a symlink
test …::plugin_path_write_file_through_in_root_symlink_is_rejected ... FAILED
    write_file escaped through root/link
test …::plugin_path_absolute_path_under_root_alias_is_accepted ... FAILED
    create_dir under the root alias must succeed: Path outside project root
test …::plugin_path_dangling_symlink_is_not_written_through ... FAILED
    write_file followed a dangling in-root symlink
test …::plugin_path_create_dir_through_in_root_symlink_is_rejected ... FAILED
    create_dir escaped through root/link
test result: FAILED. 0 passed; 5 failed
```

(The `write_mcp_files` extraction was done first, behaviour-preserving, so
the `.mcp.json` case could be tested without a running MCP port.)

### After the fix

- `nix develop -c cargo test plugin_path_` → 5 passed.
- All pre-existing sandbox tests green (T0002 traversal behind a missing dir,
  deep new dirs inside, absolute outside, `test_lua_create_dir_and_rename`, …).
- Gates: `cargo fmt --check` OK, `cargo clippy --all-targets --all-features
  -- -D warnings` OK, full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`):
  lib 586 passed, every integration binary green.
- macOS: once T0090 is public, drop the
  `app::plugins::api::tests::test_lua_create_dir_and_rename` skip in
  `macos-gates.yml` and check the macOS run (the alias test reproduces the
  `/var` → `/private/var` case on Linux and passes).
