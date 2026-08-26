---
id: T0005
title: Make the plugin instruction/memory limit unbypassable (coroutine + catchable error)
status: done
created: 2026-08-24
severity: severe
area: security
depends-on: []
---

## Goal

The only DoS guard on plugins — the instruction-count hook in `runtime.rs` —
has two trivial bypasses, either of which freezes the UI thread forever:

1. The hook is installed per-thread (mlua 0.10 `set_hook` = thread hook), and
   `coroutine` is **not** removed from the sandbox, so
   `coroutine.wrap(function() while true do end end)()` runs with no hook at all.
2. The hook returns an ordinary catchable Lua error, so
   `while true do pcall(function() while true do end end) end` swallows it and
   never terminates. Same for the memory-limit error.

## In scope

- Ensure the instruction/time limit applies to coroutines too — either install
  the hook such that it covers coroutine threads, or remove `coroutine` from
  the plugin environment (add it to the nil-out list in `runtime.rs:86-93`).
- Make the limit non-catchable from Lua — e.g. enforce a wall-clock deadline
  outside the VM, or use a mechanism `pcall` cannot swallow, so a plugin can't
  loop on `pcall` to defeat it.
- Apply the same non-catchable treatment to the memory-limit path.

## Out of scope

- Moving plugin execution off the UI thread (that's T0011's concern:
  time-boxing synchronous hooks).
- Per-plugin isolation (T0009).

## How to test

### Regression test

```rust
// plugins/runtime.rs tests, extending test_instruction_limit_aborts_loop
run_plugin_source("coroutine.wrap(function() while true do end end)()"); // must abort under limit
run_plugin_source("while true do pcall(function() while true do end end) end"); // must abort under limit
```

Give each a bounded timeout in the test harness.

- Before the fix: both hang past the instruction budget (test times out).
- After the fix: both are aborted by the limit within the budget.

## Acceptance criteria

- [x] A coroutine busy-loop is stopped by the limit.
- [x] A `pcall`-wrapped busy-loop is stopped by the limit.
- [x] The memory limit is likewise not defeatable via `pcall`.
- [x] Regression tests added and green.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/plugins/runtime.rs` — `setup_sandbox` nils `coroutine`; new
  `setup_pcall_guard` wraps `pcall`/`xpcall`; a `poisoned` latch (`AtomicBool`)
  is set by the instruction hook and reset in `reset_instruction_count`.

## Notes

- Origin: plugin/services audit **S4**. `Thread::set_hook` exists separately in
  mlua precisely because `Lua::set_hook` is per-thread — that's the root of
  bypass #1.

## Outcome (2-review)

**Empirical findings first (bounded throwaway experiments).** With a *bounded*
`for i=1,N do pcall(function() while true do end end) end`: (a) the stock
`Err`-returning hook is **catchable** — the loop returned `"completed"`, so the
limit was fully defeated; (b) making the hook **panic** does not help — mlua
converts the panic to an error that `pcall` also catches (it fired repeatedly).
So neither `Err` nor `panic` from the hook is uncatchable. This ruled out the
"just return a different error" approaches.

**Fix — two independent bypasses, two mechanisms:**

1. *Coroutine (bypass #1).* `Lua::set_hook` only hooks the main thread, so a
   busy-loop inside a coroutine runs unlimited. Removed `coroutine` from the
   sandbox (nil-out). Plugin hooks are synchronous and don't need it.

2. *pcall swallowing (bypass #2).* Added a `poisoned` latch set by the hook when
   the limit is first exceeded, and replaced `pcall`/`xpcall` with guards that
   **delegate to the original** but re-raise `instruction_limit_aborted()` if
   poisoned — checked both before the call and after it returns. Because every
   guard on the stack re-raises on the way up, no nesting of `pcall` can swallow
   the abort. While not poisoned they behave exactly like stock pcall/xpcall
   (verified by a no-regression test). The latch is cleared per hook call.

*Memory limit:* a one-shot `pcall(huge_alloc)` catching a `MemoryError` is not a
DoS (allocation is refused, memory stays bounded). A memory-error *loop* is an
instruction loop, so the same poison latch stops it — no separate memory
mechanism needed.

### Red proof (all bounded — no hang risk)

- `coroutine_is_removed_from_sandbox` — with the nil-out reverted, `coroutine`
  is present → fails.
- `pcall_loop_cannot_defeat_instruction_limit` — with the guard reverted, the
  bounded loop returns `Ok("completed")` → fails (`pcall loop defeated the
  instruction limit`). With the guard, `exec` returns the instruction-limit
  error.
- `coroutine_busyloop_is_blocked` and `pcall_still_catches_normal_errors` round
  out the suite. (The true-`while true` forms of both bypasses hang pre-fix and
  are therefore not run in the reverted state — the bounded forms above prove
  the same defect safely.)

### Verification recipe

```bash
nix develop -c cargo test --lib runtime::tests
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
