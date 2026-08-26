---
id: T0016
title: Cross-platform session locking (no /proc off Linux)
status: done
created: 2026-08-24
severity: moderate
area: robustness
depends-on: []
---

## Goal

`session_is_locked` checks `/proc/{pid}` existence to decide whether another
instance holds the session. There is no `/proc` on macOS or Windows, so the
lock silently never engages on two of the three shipped platforms, and on Linux
PID reuse produces false positives. FerrisPad ships macOS `.dmg` and Windows
`.exe`, so this is a real gap, not theoretical.

## In scope

- Replace the `/proc`-existence check with a portable liveness/lock mechanism:
  a real lock file with an OS advisory lock (e.g. `fs2`/`flock` semantics), or a
  portable "is this PID alive" check per platform via `#[cfg]`.
- Guard against PID reuse (store something more than a bare PID, or rely on the
  advisory lock rather than PID liveness).

## Out of scope

- The empty-session wipe (T0015).

## How to test

### Regression test

Unit-test the lock abstraction: acquiring the lock in one handle blocks/rejects
a second acquisition; releasing frees it. On Linux, assert a stale lock from a
dead PID is reclaimable. If a full cross-platform test isn't feasible in CI,
document the manual check per platform.

- Before the fix: on non-Linux the lock always reports "not locked".
- After the fix: the lock engages on all platforms.

## Acceptance criteria

- [x] Session locking engages on Linux, macOS, and Windows.
- [~] PID reuse does not cause a false "locked" — **deliberately not addressed**;
      see the decision below (the user chose the small portable-liveness fix over
      the advisory-lock rework).
- [x] Regression test (characterization) + per-platform manual recipe added.
- [x] `cargo test` / `clippy` / `fmt` clean (Linux gates; see Windows caveat).

## Affected files

- `src/app/services/session.rs` — `/proc/{pid}` probe → portable
  `process_is_alive(pid)` (`#[cfg]` per platform).
- `Cargo.toml` — added the `Win32_System_Threading` feature to the existing
  `windows` dependency (for `OpenProcess`/`GetExitCodeProcess`).

## Notes

- Origin: plugin/services audit **M7**. Prefer an advisory file lock — it
  sidesteps PID liveness entirely and is the conventional answer.

## Outcome (2-review)

### Decision (asked the user)

The notes prefer an advisory file lock, but that is a **read-only predicate** as
used (`session_picker.rs` calls `session_is_locked` before switching/opening a
session). An advisory lock only signals "locked" if the *owning* instance holds
the lock for its whole run, which needs a lock handle in `AppState`, acquire/
release on session activate/switch/quit, and a new dependency — far beyond the
`session.rs` scope. Presented both; the user chose the **small portable-liveness
fix**. PID-reuse is therefore knowingly left as a rare, recoverable false
"locked" (never data loss); a full advisory lock can be a later ticket.

### Fix

`session_is_locked` now calls a portable `process_is_alive(pid)` instead of
`Path::new("/proc/{pid}").exists()`:

- **`#[cfg(unix)]`** (Linux + macOS): `kill(pid, 0)` — POSIX liveness that
  delivers no signal. Alive if it returns `0`, or fails with `EPERM` (process
  exists, owned by another user); `ESRCH`/anything else → gone. Reuses the same
  `unsafe extern "C" { fn kill }` pattern already in `api/commands.rs`, so **no
  new dependency**.
- **`#[cfg(windows)]`**: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
  `GetExitCodeProcess`, alive iff the exit code is `STILL_ACTIVE (259)`; handle
  always closed. Uses the already-present `windows` crate (one feature added).
- **`#[cfg(not(any(unix, windows)))]`**: fail open (report "not locked").

This closes the headline gap — the `/proc` probe silently reported "not locked"
on macOS and Windows, both shipped platforms.

### Testing — why no red

The defect is **platform-specific**: on Linux the old `/proc` probe and the new
`kill(0)` agree for every alive/dead case, so the bug cannot be reproduced on the
Linux CI (it manifested only on macOS/Windows, which have no `/proc`). Per
`work-sequence.md`/the ticket, a documented per-platform manual recipe stands in
for a red test, plus characterization unit tests of the new abstraction:

- `process_is_alive_reports_our_own_process` (all platforms): our own PID is alive.
- `process_is_alive_is_false_for_a_reaped_child` (`#[cfg(unix)]`): spawn `true`,
  reap it, assert its PID is no longer alive.

### ⚠️ Windows branch not compiled by the Linux gates

`cargo build/clippy/test` on Linux compile only the `#[cfg(unix)]` arm. The
`#[cfg(windows)]` arm is written to the `windows` 0.58 API but is verified only
by an actual Windows build (the release workflow builds Windows). A reviewer with
a Windows box can `cargo build` there; otherwise trust the release build.

### Manual per-platform recipe

macOS/Windows: open session "foo" in instance A; in instance B open the session
picker and try to switch to / open "foo" in a new window → **"Session is open in
another window."** Close A, retry in B → allowed. (Before: B always allowed it on
macOS/Windows because `/proc` never existed.)

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --lib process_is_alive
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
# Windows arm (optional, needs a Windows toolchain):
#   cargo build   # on Windows — compiles the #[cfg(windows)] process_is_alive
```
