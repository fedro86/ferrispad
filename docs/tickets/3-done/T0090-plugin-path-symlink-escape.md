---
id: T0090
title: One physical path rule for the plugin sandbox — no escape through in-root symlinks
status: done            # todo | review | done
created: 2026-11-26
severity: severe        # sandbox escape: arbitrary file write outside the project root
area: security
depends-on: []
issue: 222              # GitHub issue number, filled when work starts
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
| hook result `open_file` (`hook_dispatch.rs`) | opens a tab | `validate_path`, but the **raw** string was opened → now the validated path (review round 1) |

There is no copy/move API besides `rename`.

### The host opens or runs a plugin-supplied path / URL (review round 2)

| Where | Source | Verdict |
|---|---|---|
| hook `open_file` → `DeferredOpenFile` | plugin | checked by the physical rule + "existing file"; opens `root/raw` (or the absolute request) in the user's path form, never the canonical one |
| diagnostic `url` → `DiagnosticOpenDocs` → `open::that` (ShellExecute / `open` / `xdg-open`: opens **or runs** a local path) | plugin | **fixed:** dropped at parse time unless `security::is_web_url` (`http`/`https`, case-insensitive, non-empty host, no whitespace/control chars), with a `[plugin:security]` log line |
| plugin-folder `file://` diagnostic (`plugins/hook_dispatch.rs`) | built by the host | kept: host-constructed |
| Markdown preview temp file → `open::that` | host temp path | kept |
| Plugin Manager "Details" → `open::that` | registry `readme_url` / community `repo` | kept: the URL is shown in a confirmation dialog before opening; the registry is the project's own (fetched over HTTPS). Follow-up candidate: the same `is_web_url` guard |
| update dialog → `open::that(html_url)` | GitHub releases API | kept |

The official plugins (`fedro86/ferrispad-plugins`) set `url` only to
`https://` documentation links (rust-lint: clippy docs; python-lint: ruff's
rule URL), so the filter breaks nothing legitimate.

### "No project root" paths (review round 1, orchestrator decision)

| Where | Without a project root | Verdict |
|---|---|---|
| hook `open_file` | opened **any** path (comment: "allow, same as file_exists") | **fixed: refused**, logged as `[plugin:security] open_file BLOCKED` |
| every Lua fs API (`file_exists` … `setup_mcp_config`) | returns `false` / `nil` / "No project root" | already fail-closed |
| `run_command` | runs with the process cwd instead of the root | kept: it only runs a command the user approved for that plugin, and it passes no path that the sandbox resolves (the cwd isn't an access boundary) |
| terminal-view `working_dir` | plugin value, metacharacter-checked only | kept, same reason: an approved command in a terminal; no FerrisPad file op |

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

## Review round 1 (a3 + orchestrator, 2026-11-26, local only)

- **Blocking (a3):** hook `open_file` validated the path against the root but
  sent the **raw** string to `DeferredOpenFile`. `FileController::open_file`
  resolves a relative path against the process cwd, so with FerrisPad
  started from `$HOME`, `open_file = ".ssh/id_rsa"` passed the check
  (`<root>/.ssh/id_rsa` is "inside, not yet existing") and opened
  `~/.ssh/id_rsa` for the plugin to read. One-level names were already
  exploitable before T0090; making deep missing paths `Valid` widened it.
  Fix: `hook_dispatch::approve_open_file(path, root) -> Option<String>`
  returns the **validated physical path**, and only for an **existing file**.
- **Folded in (orchestrator):** with no project root, `open_file` opened any
  path; it is now refused (table above).
- **Non-blocking (a3):** `validate_path` walks up only on `NotFound` /
  `NotADirectory`; any other `lstat` error (permission, name too long) is
  `InvalidPath`. Its doc now states that `remove`/`rename` of an in-root
  symlink to an in-root target act on the target (unchanged behaviour).

Red (the helper extracted first with the old behaviour, then the tests):

```
test …hook_open_file_without_project_root_is_refused ... FAILED
  left: Some("/etc/hostname")   right: None
test …hook_open_file_refuses_paths_that_are_not_existing_files_in_root ... FAILED
  left: Some(".ssh/id_rsa")     right: None
test …hook_open_file_opens_the_validated_path_not_the_raw_one ... FAILED
  left: Some("src/main.rs")     right: Some("<canonical root>/src/main.rs")
test result: FAILED. 0 passed; 3 failed
```

After: the 3 pass.

## Review round 2 (a3 + orchestrator, 2026-11-26, local only)

- **Blocking (a3), Windows regression from round 1:** `approve_open_file`
  returned the canonical path, which on Windows is `\\?\C:\…`. A file
  opened from the file-explorer plugin would get that path, so
  `TabManager::find_by_path` (exact string compare) missed an already-open
  tab and opened a duplicate, and the verbatim path showed up in titles,
  recent files and the session. Unix had the same duplicate-tab problem for
  a project behind a symlinked directory (`/real/…` vs `/home/u/link/…`).
  Fix (orchestrator's choice): validate physically as before, but open the
  request anchored at the root as given (`root.join(raw)`, or the absolute
  request). `root` comes from the active document's path as the user opened
  it, so the form matches the user's tabs. Only TOCTOU (out of scope) could
  make the opened file differ from the checked one.
- **Folded in (a3 audit gap, orchestrator decision):** diagnostic `url`
  could be any string and reached `open::that` on a double-click, so a
  plugin could `write_file` a `.bat` / `.desktop` / `.command` in the root
  and get it run without `execute` permission. Now dropped at parse time
  unless it is an `http(s)` URL (table above).

Red runs:

```
# open_file returning the canonical path (as in 001183f), alias root:
test …hook_open_file_keeps_the_root_alias_form ... FAILED
  left:  Some("/tmp/…/real/src/main.rs")
  right: Some("/tmp/…/alias/src/main.rs")
# diagnostic url, before the filter:
test …diagnostic_url_must_be_a_web_url ... FAILED
  "C:\\x\\run.bat" must not reach open::that
```

After: both pass. The `\\?\` form itself cannot appear on Linux; the
string-form assertion in `hook_open_file_opens_the_validated_path_not_the_raw_one`
pins that the opened path is `root.join(raw)` (never canonicalized), which
is what keeps it off Windows.

## Disclosure (2026-11-27)

Made public with 0.9.10-rc.3: branch pushed, issue #222 opened. The
`app::plugins::api::tests::test_lua_create_dir_and_rename` skip is removed
from `.github/workflows/macos-gates.yml` in this branch, so the macOS gate
runs the full suite.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration
build `verify/rc3` (checklist `VERIFICA-RC3.md`), which carried this chain
(T0090 → T0092 → T0094). 2026-11-27: the session checks re-run on
`master` + this chain (`verify/rc3-final`) pass.

Still pending on the published rc.3: on Windows, opening an already-open file
from the file explorer must not open a second tab (the `open_file` path form).
