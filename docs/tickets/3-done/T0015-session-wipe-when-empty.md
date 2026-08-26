---
id: T0015
title: Stop save_session from wiping another instance's session when this one is empty
status: done
created: 2026-08-24
severity: moderate
area: robustness
depends-on: []
---

## Goal

`save_session` gates its cross-instance *merge* on `!doc_sessions.is_empty()`,
but `cleanup_orphaned_temp_files` runs unconditionally afterwards. With
`SessionRestore::SavedFiles` and only unsaved untitled tabs, `doc_sessions` is
empty, so this instance writes an empty `documents` list over another instance's
list **and** deletes all its `.tmp` files. One instance with nothing open can
destroy another instance's saved session.

## In scope

- When this instance has no documents to contribute, do not overwrite the
  persisted `documents` list and do not delete another instance's temp files —
  either skip the write entirely or merge-preserve the other instance's entries.
- Make the cleanup respect the same guard the merge uses.

## Out of scope

- The cross-platform locking bug (T0016) — related but separate.

## How to test

### Regression test

Extend `tests/session_roundtrip.rs`: write a session with documents (instance
A), then run the save path for an instance B that has only untitled/empty tabs,
and assert A's documents + temp files survive.

- Before the fix: A's `documents` list is emptied and its `.tmp` files deleted.
- After the fix: A's session intact.

## Acceptance criteria

- [x] An empty instance never erases another instance's persisted documents or
      temp files.
- [x] Normal single-instance save/restore still works.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/session.rs` — extracted `merge_and_persist` (persistence
  tail of `save_session`) + the empty-overwrite guard; two unit tests.

## Notes

- Origin: plugin/services audit **M6**.

## Outcome (2-review)

**Fix.** When `doc_sessions` is empty, `merge_and_persist` now returns early
*iff* the on-disk session belongs to a different instance:

```rust
if doc_sessions.is_empty()
    && let Ok(existing) = read+parse(session.json)
    && existing.instance_id.as_deref() != Some(instance_id)
{
    return Ok(()); // leave the other instance's session + temp files untouched
}
```

This reuses the exact "different instance" test the merge already uses, so the
cleanup now respects the same guard as the merge (the ticket's ask). A session we
already own — or none at all — still falls through and is written empty, which is
the legitimate "user closed all tabs" case (covered by the second test). The
narrow behaviour change: an instance that has *never* taken ownership and does an
empty save against a previous run's session leaves that session in place instead
of wiping it — strictly safer than the old destructive write.

**Refactor to make it testable (simplify, not layer).** `save_session` takes a
`&TabManager` and writes to the global `session_dir(name)` (data-dir), so its
persistence tail couldn't be exercised in isolation. I split that tail — merge +
write + cleanup — into a private `merge_and_persist(doc_sessions, …, dir,
instance_id)` that takes an explicit `dir` and `instance_id`. `save_session` is
now: collect `doc_sessions` from the `TabManager`, then delegate. Behaviour for
the non-empty path is unchanged (verified: extraction compiled and the full
suite stayed green before the guard was added).

### Red proof

`empty_instance_does_not_wipe_another_instances_session` seeds instance A
("111")'s `session.json` (one saved doc referencing `aaaa.tmp`) plus the temp
file, then calls `merge_and_persist(vec![], …, dir, "222")` (instance B, nothing
to contribute).

- Before the guard: `merge_and_persist` skipped the merge, wrote an empty
  `documents` list, and `cleanup_orphaned_temp_files` deleted `aaaa.tmp` →
  assertion failed with **"empty instance B erased instance A's documents"**
  (0 vs 1).
- After: A's `documents` (len 1, `/home/user/a.rs`) and `aaaa.tmp` both survive.

`empty_instance_clears_its_own_session` (guard for the opposite direction) seeds
a session owned by "222", then B="222" saves empty → the session is cleared to 0
docs. Passes both before and after, proving the fix only protects *foreign*
sessions.

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --lib empty_instance_does_not_wipe_another_instances_session
nix develop -c cargo test --lib empty_instance_clears_its_own_session
# See the red: git stash the guard hunk in merge_and_persist and rerun the first
# test — it fails with "erased instance A's documents".
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
