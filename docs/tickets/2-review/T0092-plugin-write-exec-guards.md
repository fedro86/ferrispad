---
id: T0092
title: Close the plugin write→execute paths — no writes into executables or .git; web-only Details links
status: review          # todo | review | done
created: 2026-11-26
severity: severe        # plugin-controlled code execution through an approved command
area: security
depends-on: [T0090]
issue: ~                # GitHub issue number, filled when work starts
---

Disclosure: private until the fix ships in 0.9.10-rc.3 (user decision 2026-11-26).

## Goal

A plugin may run only the commands the user approved (`[permissions] execute`)
and may write only inside the project root (T0090). Combined, those two
permissions still allowed **plugin-controlled code execution**:

- **Overwrite an approved binary.** `fs::write` onto an existing file keeps
  its mode. A plugin with `execute = ["ruff"]` could rewrite the user's
  executable `<root>/.venv/bin/ruff` and then `run_command` it: it is approved
  by basename, and also reachable as a bare `ruff` when FerrisPad was started
  from an activated venv.
- **Windows.** `write_file("tools/ruff.exe", <PE bytes>)` (Lua strings are
  binary-safe), then `run_command("<root>\\tools\\ruff")`: Rust appends
  `.exe`, and the basename `ruff` is approved.
- **Through git.** git-diff has `execute = ["git"]`. A plugin that writes
  `.git/config` (`core.fsmonitor`, `core.pager`, aliases) or a hook gets code
  executed by the approved `git`.
- **Plugin Manager "Details".** It handed a registry-supplied `readme_url` /
  community `repo` to `open::that`, which opens **or runs** local paths (same
  class as T0090's diagnostic `url`).

## In scope

- (a) Plugin Manager "Details": only a web URL (`security::is_web_url`) is
  opened, after the usual confirmation. Anything else shows "This link is not
  a web address, so it was not opened" and opens nothing.
- (b1) The plugin filesystem API refuses to create or replace content in an
  executable:
  - `write_file`, `create_file`, `create_dir` and the `rename` destination
    refuse a target that is an existing executable regular file. The check
    is on the physical target, so an in-root symlink to an executable counts.
    On Unix that means any x bit.
  - On Windows they refuse an executable extension: `PATHEXT` (or its
    default) plus `.PS1` and the implicit `.EXE`, case-insensitive,
    including a rename INTO such a name.
  - On Windows a path component that Win32 rewrites on creation is refused
    outright: one containing `:` (ADS), one ending in `.` or a space, or one
    containing `~<digit>` (8.3 alias such as `GIT~1`).
  - Moving an executable away (rename source), removing one, and then writing
    a new non-executable file in its place stay allowed.
- (b3) The plugin filesystem API refuses writes, creates, renames (source or
  destination) and deletes under any `.git` component of the **physical**
  path inside the root. This covers `sub/../.git/…`, a symlink to `.git`, and
  `.git` as a file (worktrees, submodules). The match is case-insensitive on
  Windows and macOS; `.github`, `x.git` and `.gitkeep` don't match. Reads stay
  allowed.
- `setup_mcp_config`'s two files go through the same write policy.

## Out of scope

- `run_command`'s program-path rule (where a non-bare program may live, and
  that it must be an existing executable): **T0094**, which depends on this.
- TOCTOU between check and write (as in T0090).

## Residual risk (the honest boundary)

A plugin allowed to run a **build or lint tool** and to write project files
can still get code executed through that tool's own features. This can't be
closed inside the sandbox without breaking the official linters:

- `build.rs` and proc-macros (cargo / clippy);
- `conftest.py` and `setup.py` (python tooling);
- `Makefile`, `package.json` scripts, `.cargo/config.toml`;
- `.husky/*`: with `core.hooksPath` on a tracked directory, husky v9 sources
  hooks via `sh`, so no x bit is needed;
- `.gitattributes` filters (these need git config, which (b3) blocks);
- `.vscode/tasks.json`.

Each of these runs through an approved tool or through the user's own later
action. **Treat `execute` permission for such tools as trusting the plugin
with code execution in that project.** Follow-up (public after rc.3): the
permission prompt should say "can run code in this project" for build tools.

## How to test

```bash
nix develop -c cargo test plugin_write_            # b1 + b3, Lua API level (unix)
nix develop -c cargo test details_link             # (a)
nix develop -c cargo test git_component windows_   # pure name rules (all platforms)
```

## Acceptance criteria

- [ ] A plugin cannot overwrite, create or rename into an executable, and
      cannot modify anything under `.git`, through any plugin filesystem API.
- [ ] Moving or removing executables, and reading `.git`, still work.
- [ ] A non-web "Details" link is never handed to the OS opener.
- [ ] Red tests failed before; full gates green.

## Affected files

- `src/app/plugins/security.rs`: the name rules (`is_git_component`,
  `path_has_git_component`, `windows_name_is_unsafe`,
  `has_executable_extension`, `is_existing_executable`); `validate_path`
  refuses unsafe Windows names.
- `src/app/plugins/api/sandbox.rs`: `WriteKind` + `resolve_for_write`.
- `src/app/plugins/api/filesystem.rs`: `write_file`, `create_file`,
  `create_dir`, `rename` (Modify → Create), `remove` (Modify).
- `src/app/plugins/api/editor.rs`: `write_mcp_files` uses `resolve_for_write`.
- `src/ui/dialogs/plugin_manager.rs`: `details_link_opens` + the error message.
- `src/app/plugins/api/mod.rs`: tests.

## Notes

- Origin: a3's private follow-ups after T0090; the orchestrator's decision on
  2026-11-26 (options 2 + 3 + `.git`), split so that (b2) is T0094.
- Process: local branch `ticket/T0092` (stacked on local `ticket/T0090`), with
  no push, issue or PR until the orchestrator / user say so.

## What was done (2026-11-26, local only)

Implemented as in *In scope*. The pure name rules live in `security.rs`
with an explicit platform parameter, so they are tested on every OS. The
write policy is `sandbox::resolve_for_write(path, root, WriteKind)`:

- `Create` (write / create file / create dir / rename destination /
  `.mcp.json` + `.gitignore`) checks `.git`, the Windows executable
  extension and an existing executable target.
- `Modify` (rename source / remove) checks `.git` only.

### Red runs (before the fix)

Lua API level (unix), on the T0090 code:

```
test …plugin_write_refuses_dot_git_file ... FAILED            (assertion failed: !ok)
test …plugin_write_refuses_existing_executables ... FAILED    (.venv/bin/ruff content was overwritten)
test …plugin_write_refuses_paths_under_dot_git ... FAILED
    write_file[".git/config", "[core]\nfsmonitor = evil\n"] under .git must fail
test …plugin_write_allows_moving_or_removing_executables ... ok   (guard: must stay allowed)
```

(a), with `details_link_opens` extracted first with the old behaviour:

```
test …details_link_opens_only_web_urls ... FAILED
    "file:///tmp/x.desktop" must not be opened
```

### After

- All of the above pass, plus the pure name-rule tests
  (`git_component_matching`, `windows_unsafe_names`,
  `windows_executable_extensions`).
- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 599 passed,
  every integration binary green. fmt and clippy `-D warnings` clean.

### Plugin Manager "Details", driven under Xvfb (Claude)

The setup is a fresh cached official registry
(`$XDG_CONFIG_HOME/ferrispad/cache/plugins.json`) with two entries: one with
`readme_url = file:///tmp/evil.desktop` and one with
`https://example.com/good`. A logging `xdg-open` comes first on `PATH`. The
steps: Plugins → General → Plugin Manager… → Official → Details on each,
with Return on the first popup and "Open in Browser" on the second.

| Build | What reached `xdg-open` |
|---|---|
| before (T0090) | `file:///tmp/evil.desktop` (Return confirmed it) **and** `https://example.com/good` |
| after (T0092) | only `https://example.com/good` |

After the change the bad link shows "This link is not a web address, so it
was not opened:" with the URL and a Close button. Edge-by-edge check: the
text is not clipped and the button sits inside the popup. The good link
still shows the usual "This will open your browser to: … Continue?"
confirmation.

## Review round 2 (a3 + orchestrator, 2026-11-26, local only)

**Blocking (a3):** the `.git` rule looked only at the path itself, so a
recursive remove of an ANCESTOR deleted `.git`. Worse, `remove("")` and
`remove(".")` resolve to the project root and returned `(true, "")`: the
whole project, history included, was gone. A plugin **bug** was enough.

Fix, in `sandbox::resolve_for_write` (every write kind):

- **The project root itself is refused**, however it is spelled. `""`,
  `"."`, `"./"`, `"sub/.."`, the absolute root, an alias of the root and an
  in-root symlink to the root all compare equal after physical resolution.
- **Removing or moving a directory** (`Modify`, which covers `remove` and the
  `rename` source) first walks it with `dir_tree_is_git_free`:
  - `read_dir` plus `DirEntry::file_type`, which does not follow symlinks;
  - bounded at `MAX_REMOVE_DEPTH` = 32 levels and `MAX_REMOVE_ENTRIES` =
    10 000 entries;
  - it refuses on any entry named `.git` (same case rules as (b3)), on any
    read error, and past a bound (fail closed).
- A symlink inside the directory is judged by its own name only. Removing
  or moving the directory acts on the link, not its target, so a link
  pointing at a `.git` elsewhere neither blocks nor is followed. That
  target stays intact, and a test checks it.
- `rename` of an ancestor is refused by the same walk. Even if it were
  allowed it would only MOVE `.git` with its content intact. Refusing keeps
  one rule.

Red (on 354b064):

```
test …plugin_remove_refuses_the_project_root ... FAILED      remove("") must fail
test …plugin_remove_refuses_directories_containing_dot_git ... FAILED   remove(sub) must fail
test …plugin_remove_walk_is_bounded ... FAILED      deeper than the bound must fail closed
```

All three pass after the fix. The earlier T0090/T0092 tests stay green.

Non-blocking notes (a3), not changed:

- **`~<digit>` false positives on Windows:** names like Emacs numbered
  backups (`file.txt.~1~`) or `draft~2.md` are refused when a *plugin*
  creates them on Windows. This fails closed and is accepted.
- **Windows reserved device names** (`CON`, `NUL`, `COM1`, `nul.txt`, …) are
  not refused. Writing one touches a device; it is no execution vector.
  Follow-up note only.
- **Windows compile check:** all Windows rules (`windows_name_is_unsafe`,
  `has_executable_extension`, case folding via `is_git_component`) are plain
  functions compiled and unit-tested on every platform. `cfg!(windows)` only
  chooses whether to apply them, and those are ordinary expressions that
  compile everywhere. There is no Windows CI on this local branch, so the
  first pushed Windows CI run (at the rc.3 reveal) is the real compile check
  of the wiring.
