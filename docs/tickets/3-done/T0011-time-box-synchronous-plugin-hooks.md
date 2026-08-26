---
id: T0011
title: Time-box synchronous plugin hook/command execution (UI freeze)
status: done
created: 2026-08-24
severity: moderate
area: robustness
depends-on: [T0005]
---

## Goal

Plugin hooks run synchronously on the UI thread (`file.rs` calls
`plugins.call_hook(...)` on save/open/etc.), and a plugin command can take up
to a 30 s child-process timeout. A slow linter freezes the editor on every
save. The `hooks.rs` comment conflates "0% CPU when idle" (a real philosophy
constraint) with "blocking is fine" — they are different: idle CPU stays 0% even
if hooks are time-boxed.

## In scope

- Put a wall-clock deadline on synchronous hook execution so a hung/slow hook
  can't freeze the UI indefinitely; surface a toast when a hook is aborted for
  taking too long.
- Reduce or make configurable the 30 s command timeout invoked from the UI
  thread, or move command execution off the UI thread while keeping the result
  delivery on the Message loop.

## Out of scope

- The instruction-limit bypass (T0005) — this ticket assumes that lands first
  so the deadline can't be swallowed by `pcall`.
- A general async plugin architecture (larger design; note it if raised).

## How to test

### Regression test / manual

Add a test hook that sleeps / busy-loops beyond the deadline; assert the host
returns control within the deadline + a toast, rather than blocking. Manually:
a plugin whose `on_save` sleeps 60 s must not freeze the editor on save.

## Acceptance criteria

- [x] A hook exceeding the deadline is aborted and the UI stays responsive.
- [x] The abort is surfaced to the user (toast), not silent.
- [x] Regression/manual recipe documented and verified.
- [x] `cargo test` / `clippy` / `fmt` clean; philosophy's 0%-idle rule intact.

## Affected files

- `src/app/plugins/runtime.rs` — per-hook wall-clock deadline (new
  `DEFAULT_HOOK_DEADLINE`, `deadline_ns`/`origin` fields, arm/disarm around the
  hook body and `load_script`, checked in the existing instruction hook).
- `src/app/plugins/security.rs` — `DEFAULT_COMMAND_TIMEOUT` 30 s → 15 s.
- `src/app/plugins/hooks.rs` — corrected the "blocking is fine" module comment.

## Notes

- Origin: plugin/services audit **M4**. Respect the philosophy: no background
  timer — the deadline is armed only while a hook is actually running.

## Outcome (2-review)

Two independent bounds; **no threading of the plugin VM** (that would be the
out-of-scope async architecture — mlua's `Lua` is not `Send` and the whole
plugin layer is `Rc`-based on the UI thread).

### 1. Per-hook wall-clock deadline (`runtime.rs`)

The freeze vectors the instruction limit (T0005) does *not* catch are blocking
Rust/C calls from Lua (a `run_command` child, a pathological `string.find`) —
while Lua is blocked in such a call, no bytecode advances, so the instruction
hook never fires. The deadline is armed (an `Instant`-derived nanosecond budget
in an atomic) right before the hook body runs and disarmed the moment it
returns, on every path. It is checked inside the **existing** instruction hook
(every 1000 instructions), so:

- there is **no background timer** — the check only runs while Lua executes, and
  idle CPU stays 0% (philosophy intact);
- it bounds a runaway **loop** or a **sequence** of blocking calls (the deadline
  is seen at the first instruction after each blocking call returns);
- a *single* blocking call is still bounded only by that call's own timeout —
  hence bound #2.

The abort is poison-latched exactly like the instruction limit, so a
`pcall`/`xpcall` loop cannot swallow it (reuses the T0005 latch). The resulting
`mlua::Error` ("Hook execution timed out") propagates through
`hook_dispatch::call_hook`, which **already** turns any hook error into an Error
toast (`"Plugin '<name>' failed"`) plus a diagnostic carrying the message — so
the abort is surfaced, not silent, with no new plumbing.

Default deadline **30 s** — a coarse backstop, deliberately above one command
timeout so a legitimate single-command linter is never cut off mid-run. Pure-Lua
hangs are already bounded to ~milliseconds by the 1 M instruction limit; the
deadline only bites on blocking sequences.

*Enforcement note:* the deadline lives in the instruction hook, which is only
installed when `max_instructions != 0`. Production always is (loader →
`LuaRuntime::new()` → 1 M), so this is not a gap; documented on
`with_limits_and_deadline`.

### 2. Command timeout 30 s → 15 s (`security.rs`)

`run_command` waits on the child **synchronously on the UI thread**, so its
timeout is the hard ceiling on a single command's freeze. "Move off the UI
thread" does not apply: the Lua API returns the result synchronously, so the
hook blocks on the result regardless of which thread runs the child. Reducing
the constant is the real lever. 15 s halves the previous worst case while
staying generous for heavy linters (ruff/eslint ≪1 s; mypy/tsc on one file
usually within a few s). **This is a UX default worth a reviewer's eye** — if
your project relies on a linter that legitimately needs >15 s, say so and it
becomes a follow-up "configurable timeout" ticket rather than a smaller constant.

### Red proof

- `hook_execution_is_wall_clock_deadline_bounded` (runtime): instruction limit
  set to 1e9 so the *deadline* (150 ms) is provably the stopper. Red observed by
  temporarily neutralising `arm_deadline` and lowering the test's limit to 3 M —
  the hook then aborts via **"Instruction limit exceeded"**, failing the
  `contains("timed out")` assertion in 0.01 s. Restored; now green (~150 ms).
- `command_timeout_stays_ui_appropriate` (security, deterministic): asserted
  `DEFAULT_COMMAND_TIMEOUT <= 15 s`; failed on the old 30 s value ("30s is too
  long"), passes at 15 s.
- `deadline_allows_a_fast_hook_to_complete`: guard that a normal quick hook still
  runs and returns its value.

### Manual repro

A plugin whose `on_save` busy-loops (or loops calling an approved slow command)
no longer freezes the editor: the save returns within the deadline and an Error
toast ("Plugin '<name>' failed") + a diagnostic ("Hook execution timed out")
appear. A single approved command is capped at 15 s instead of 30 s.

## How to verify (reviewer recipe)

```bash
# The three new tests:
nix develop -c cargo test --lib hook_execution_is_wall_clock_deadline_bounded
nix develop -c cargo test --lib deadline_allows_a_fast_hook_to_complete
nix develop -c cargo test --lib command_timeout_stays_ui_appropriate
# Full gates:
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

To *see the red*: in `runtime.rs` make `arm_deadline` return early and drop the
deadline test's instruction limit to a few million — the hook then aborts on the
instruction limit ("...timed out" assertion fails). For the timeout test, revert
`DEFAULT_COMMAND_TIMEOUT` to 30 s.
