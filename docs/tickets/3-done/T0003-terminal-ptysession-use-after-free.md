---
id: T0003
title: Fix terminal PtySession use-after-free after close()
status: done
created: 2026-08-24
severity: severe
area: robustness
depends-on: []
---

## Goal

`TerminalPanel::setup_input_handler` stores `&ts.pty as *const PtySession` (a
raw pointer) inside the canvas `handle()` closure. `close()` does
`self.state.take()`, dropping the `Box<TerminalState>` that owns the
`PtySession` — but it never clears the canvas handler. A subsequent
key/paste/mousewheel event reaching the canvas dereferences freed memory
(`unsafe { (*w.pty).write(...) }`). The SAFETY comment justifying this is wrong:
it argues "the panel outlives the callbacks", which is true and irrelevant —
the *state* does not. **Verified by reading the code.**

## In scope

- Remove the dangling-pointer pattern. Preferred: make the writer own a shared
  handle to the PTY — `Arc<PtySession>` (or `Weak<PtySession>`) shared between
  `TerminalState` and the closure — so the closure can never dereference freed
  memory (it either writes or no-ops after close).
- Alternatively/additionally: clear the canvas `handle()` (install a no-op) in
  `close()` so no stale handler runs after the state is dropped.
- Join or explicitly detach the reader thread in `close()` rather than dropping
  its `JoinHandle` silently.

## Out of scope

- Reworking terminal rendering or the VTE handler.

## How to test

### Regression test

Unit-test the lifetime contract at the type level if a full FLTK event can't be
synthesised: assert that after `close()`, the writer path is a safe no-op (e.g.
the `Weak` upgrades to `None`), not a deref of freed memory. If feasible, drive
an `Event::KeyDown` into the canvas after `close()` under a sanitizer build.

### Manual repro (ASAN)

Build with `RUSTFLAGS="-Zsanitizer=address"` (nightly), open a plugin terminal
view, close it, then deliver a keystroke to the (hidden) canvas. Before: ASAN
reports heap-use-after-free at `terminal_panel.rs` write path. After: clean.

## Acceptance criteria

- [x] No raw `*const PtySession` is dereferenced after the owning state is
      dropped, under any event ordering.
- [x] Reader thread is not silently detached while holding shared channels.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/terminal_panel.rs` — `TerminalState.pty` is now `Rc<PtySession>`;
  `PtyWriteHandle` holds a `Weak<PtySession>` with a safe `write()`; the canvas
  handler and `close()` updated; all `unsafe` removed.
- `src/app/services/terminal/pty.rs` — unchanged (`write()`/`kill()`/`resize()`
  already take `&self`, so `Rc` deref works as-is).

## Notes

- Origin: UI audit (SEVERE, verified). The existing `Box<TerminalState>`
  correctly stabilises the *address* against moves — someone solved half the
  problem; the drop-on-close half remains.

## Outcome (2-review)

Fix: `TerminalState.pty` changed from an owned `PtySession` to `Rc<PtySession>`,
and the canvas input closure now holds a `PtyWriteHandle { pty: Weak<PtySession> }`
whose `write()` upgrades-or-no-ops. When `close()` drops `TerminalState` it
releases the only strong `Rc`, so the `Weak` in the (still-installed) canvas
handler upgrades to `None` and any later key/paste/wheel event is a safe no-op
instead of a dereference of freed memory. All three `unsafe` items are gone (the
raw `*const PtySession`, the two `unsafe { (*w.pty).write(...) }` derefs, and the
`unsafe impl Send/Sync` — the latter were never needed: fltk's `handle()` does
not require `Send`). `Rc` not `Arc` because the PTY never leaves the UI thread
(clippy's `arc_with_non_send_sync` confirmed this — `PtySession` is `Send` but
not `Sync`).

Reader thread: documented in `close()`. `kill()` drives the child to exit → the
PTY reaches EOF → the reader loop returns and the thread ends on its own; its
`JoinHandle` is intentionally detached (dropped) rather than joined, so `close()`
never blocks the UI thread. After close the thread's `TerminalOutput` signals are
ignored because `process_output` early-returns when `state` is `None`.

**On the "red test first" rule:** a use-after-free cannot be asserted by a clean
`cargo test` — dereferencing freed memory is UB, not a catchable failure, and the
ticket anticipates this (it puts the runtime proof under *Manual repro (ASAN)*
and asks for the lifetime contract to be tested "at the type level"). So there is
no red→green cargo test here. Instead:
- The bug is a **[V]** finding, verified by reading the code (raw pointer into a
  `Box` that `close()` drops without clearing the handler).
- `write_handle_noops_after_pty_state_dropped` (`#[cfg(all(test, unix))]`) locks
  in the post-fix contract: while the `Rc` is held the handle reaches the PTY;
  after the `Rc` is dropped the `Weak` upgrades to `None` and `write()` no-ops.

### Verification recipe

```bash
nix develop -c cargo test --lib ui::terminal_panel::tests   # contract test (unix)
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Optional runtime proof (nightly + display), per the ticket's manual repro:
build with `RUSTFLAGS="-Zsanitizer=address"`, open a plugin terminal view, close
it, deliver a keystroke to the hidden canvas — ASAN must report **no**
heap-use-after-free.
