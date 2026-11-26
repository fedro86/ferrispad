---
id: T0094
title: A non-bare plugin program must be an executable in the project or the home directory
status: review          # todo | review | done
created: 2026-11-26
severity: moderate
area: security
depends-on: [T0092]     # T0092 is what makes bare names (PATH) acceptable
issue: ~                # GitHub issue number, filled when work starts
---

Disclosure: private until the fix ships in 0.9.10-rc.3 (user decision 2026-11-26).

## Goal

`run_command` approves a program by **basename**: with `execute = ["ruff"]`,
`"/any/path/ruff"` passes, including a `ruff` dropped in a shared directory
such as `/tmp` by someone else. It also runs a relative program path whose
resolution against `current_dir` differs between platforms. The same
basename check guards the `terminal_view` request. The basename match itself
must stay: python-lint (official) runs `<file_dir>/.venv/bin/ruff` and
`pyright`.

## In scope

- `security::resolve_plugin_program(cmd, root, home, windows, pathext)`,
  for a **non-bare** program (one containing `/`, `\` or `:`):
  - a relative path is anchored at the project root (no root: refused) and
    passed to `Command` as that **absolute** path;
  - on Windows a name without an extension is validated as the `.exe` the
    OS would run;
  - its **parent directory**, physically resolved, must lie inside the
    canonical project root or the trusted home;
  - the file (followed if it is a symlink) must be an existing executable
    regular file (Unix: any x bit; Windows: a PATHEXT / `.PS1` / `.EXE`
    extension).
- `security::trusted_home(value)` (+ `trusted_home_from_env`: `USERPROFILE`
  on Windows, `HOME` elsewhere) fails closed. Unset, empty, relative or
  uncanonicalizable means no home, and so does a home that is the filesystem
  root (`HOME=/`), which would trust every directory.
- Bare names keep the PATH lookup. That is acceptable only because T0092
  stops plugins from creating or overwriting executables, even when PATH
  contains `.`, an empty entry or the project's activated venv.
- `terminal_view`: the command must be a **bare** name. A terminal request
  has no project root to anchor or confine a path to, and no official plugin
  uses `terminal_view`.
- The invariant, in a code comment next to the rule and here: **plugins
  cannot create symlinks** (rename/remove act on the resolved target, and
  there is no symlink API). A future API that creates links re-opens the
  parent-directory rule.

Why the parent directory and not the file: a venv's `bin/python` or
`bin/ruff` may be a symlink to a binary anywhere (`/usr/bin/…`, pyenv under
`$HOME`). Only the directory must be trusted. The rule exists to stop
binaries dropped in shared directories, and plugins can't make links.

## Out of scope

- Approving by basename itself: kept, the official plugins rely on it.
- `terminal_view`'s `working_dir` (metacharacter-checked only; it is not an
  execution vector by itself).
- TOCTOU between the check and `spawn`.

## How to test

```bash
nix develop -c cargo test plugin_run_                    # Lua run_command (unix)
nix develop -c cargo test terminal_request_program       # terminal_view bare names
nix develop -c cargo test trusted_home bare_program program_rule   # pure (all platforms)
```

## Acceptance criteria

- [ ] An approved name in a shared directory, or behind an in-root symlinked
      directory that points outside, does not run.
- [ ] python-lint's venv paths keep working (inside the root, or `../.venv`
      above it, which is inside the root or home), including a venv binary
      that is a symlink to a system binary.
- [ ] A non-executable file never runs; a relative program needs a root and
      runs by its anchored absolute path.
- [ ] `terminal_view` accepts bare names only.
- [ ] Red tests failed before; full gates green.

## Affected files

- `src/app/plugins/security.rs`: `trusted_home`, `trusted_home_from_env`,
  `is_bare_program`, `resolve_plugin_program` (+ tests).
- `src/app/plugins/api/commands.rs`: `run_command` uses the resolved path.
- `src/app/controllers/hook_dispatch.rs`: `validate_terminal_request`,
  bare names only.
- `src/app/plugins/api/mod.rs`: Lua-level tests.

## Notes

- Origin: a3's private follow-up after T0090; orchestrator decision of
  2026-11-26 (option 3 with the parent-directory refinement, split out of
  T0092).
- python-lint's candidates: `<file_dir>/.venv/bin/<cmd>`,
  `<file_dir>/venv/bin/<cmd>`, `<file_dir>/../.venv/bin/<cmd>` and
  `<file_dir>/../venv/bin/<cmd>`. `<file_dir>` is the open file's directory,
  inside the project root. `../` is then the root itself or a directory
  above it, which for a normal project is still inside `$HOME`, so it passes
  through the home rule. A project outside `$HOME` with its venv above the
  root falls back to python-lint's bare-name lookup.
- The tests that need a directory "outside root and home" skip with a note
  when `TMPDIR` lies inside `$HOME`.
- Process: local branch `ticket/T0094` (stacked on local `ticket/T0092`),
  with no push, issue or PR until the orchestrator / user say so.

## What was done (2026-11-26, local only)

As in *In scope*. All Windows logic (the `.exe` defaulting, the extension
check) sits in plain functions that take `windows` / `pathext` as
parameters, so it is unit-tested on every platform (`program_rule_windows_naming`).

### Red run (before, on T0092 e7b6819)

```
test …terminal_request_program_must_be_a_bare_name ... FAILED   /tmp/x/ruff must be refused: Ok(())
test …plugin_run_refuses_non_executable_program ... FAILED
test …plugin_run_refuses_program_outside_root_and_home ... FAILED
    an out-of-root, out-of-home 'ruff' ran: Ok(true)
test …plugin_run_refuses_program_behind_symlinked_dir ... FAILED
    ran through a symlinked dir: Ok(true)
test …plugin_run_relative_program_is_anchored_at_the_root ... FAILED
test …plugin_run_accepts_venv_symlink_to_system_binary ... ok   (guard: must stay accepted)
```

### After

- The 6 tests above pass, plus `trusted_home_fails_closed` (unset / empty /
  relative / nonexistent / `/` → none; a real dir → its canonical form),
  `bare_program_names` and `program_rule_windows_naming`.
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 611 passed,
  every integration binary green. fmt and clippy `-D warnings` clean.
