---
id: T0101
title: The session never drops a doc another live instance has open, and a closed doc stays closed
status: review          # todo | review | done
created: 2026-11-26
severity: moderate
area: robustness
depends-on: [T0099]
issue: 202
---

## Goal

With session restore on and several FerrisPad instances on one session, the
session merge can lose unsaved text, and it can bring back documents that
were closed. A document must leave the session only when an instance closed
it.

## What was reported vs. what is real

The ticket started from two "a closed document comes back" cases. Driven
on Xvfb with two real instances (T0099 tip), **neither reproduces through
normal flows**:
- Close a restored doc before the first auto-save: it stays closed, because
  `restore_session` re-saves right after restoring, so the next save sees
  its own file.
- B closes its doc and quits, A quits last: only A's doc remains.

The first script's fixed-position clicks missed the dialogs; that apparent
"reproduction" was discarded.

The merge code still allows the following:
- **(b) Data loss, pre-existing, shown on Xvfb.**
  1. A and B are alive. B saves its unsaved `b1`. A's save merges it, so the
     file holds a1 + b1, written by A.
  2. A's next save finds its **own** file and skips the merge (the old
     rule). It writes only `a1`, and the orphan cleanup **deletes `b1`'s
     temp file** while B is running.
  3. If B is killed before it saves again, `b1` is gone.
- **(a) Resurrection, in an interleaving.** A merged B's doc. B closes it
  and saves with an empty list, which hits the "never clobber a foreign
  session" guard, so B writes nothing. Any instance started before A's next
  save restores the closed doc.
- **Shared temp file.** B starts while A runs and restores A's unsaved
  `a1`. Since T0099 it reuses A's temp file, so two live instances write one
  file and the last writer wins.

## Design: owner-scoped closes

A doc leaves the session only when the instance that had it closed it.
There is no liveness test in the merge: "is the other instance alive?"
cannot tell "someone took over its docs" from "nobody has them", so a
crashed instance's unsaved text would be dropped.
- **Owner on every entry.** `DocumentSession.owner` is the instance that
  wrote the entry as its own (`#[serde(default)]`; for older files, the
  file's `instance_id` stands in).
- **Each instance remembers what it had** (`session::Ownership`, in
  memory):
  - the identities it wrote in its last save;
  - the copies (identity, owner) it took over at restore.
  - A copy it had and no longer has was closed, or renamed by Save As,
    **by it**. Having the identity again (a reopen in the same instance)
    cancels that.
- **Merge (always, also with the own file):**
  - our current docs replace every entry we own;
  - another instance's entry stays unless we closed **that copy**, so a
    reopen in another instance is a different copy and survives;
  - unsaved text (a temp file) of another instance is kept in any mode,
    even for a file we also have open, unless it is our own temp file; a
    clean file is listed once.
  - That also keeps an instance with nothing open from clobbering a
    foreign session (T0015) without a special case, and closing your last
    tab still removes your docs.
- **Adoption at restore:** take over an entry's temp file only from an
  instance that is gone. Otherwise (alive, or owner unknown) the restored
  copy gets its own temp file (a fork), so no temp file ever has two
  writers. A duplicate tab on a later restore is acceptable; a lost text is
  not.
- **No persisted tombstones are needed.** A close lost to a crash before
  the save leaves the doc in the session, which is the fail-safe direction.
  An ordered-tombstone variant was considered and replaced by this
  owner-scoped rule, which needs no clock.

## Out of scope

- Live sync between instances.
- Merging two instances' edits of the same file. Both are kept as separate
  entries; a duplicate tab on restore is accepted.

## How to test

### Regression tests

```bash
nix develop -c cargo test session_closed
```

Red run on the scaffold (new API, old merge semantics):

```text
session_closed_live_owner_entry_survives_own_save ... FAILED   B's document was dropped  left: ["ta.tmp"]
session_closed_in_other_instance_stays_closed     ... FAILED   the closed b1 came back   left: ["ta.tmp", "tb.tmp"]
session_closed_shared_dirty_file_keeps_both_edits ... FAILED   A's unsaved edits dropped: ["tb.tmp"]
session_closed_old_entries_without_owner          ... FAILED   left: ["tx.tmp"]
session_closed_adoption_forks_when_owner_alive    ... FAILED   a live instance: fork
session_closed_ownership_close_and_reopen         ... FAILED   (my first Ownership draft dropped adopted copies too early; fixed)
session_closed_reopen_in_other_instance_survives  ... ok       (guard)
session_closed_crashed_instance_docs_are_kept     ... ok       (guard)
```

After the fix all 8 pass, as do the T0015 and T0099 session tests. Gates:
673 passed, clippy `-D warnings` and fmt clean.

### Xvfb (debug builds; "before" = `ticket/T0099` tip)

Two real instances, private config and data dirs, Full restore; saves via
auto-save (an event 31 s after a change).

**(b) Data loss**: A and B type; B saves; A saves (merge); A types and saves
again; `kill -9 B`.

| step | before | after |
|---|---|---|
| B saved, then A merged | a1 + b1 | a1 + b1 (owners A, B) |
| A saved again (own file) | **a1 only, b1's temp file deleted** | a1 + b1 |
| B killed (-9) | **"from B" lost** | "from B" kept |

**Adoption while the owner is alive**: A saves its unsaved `a1`; B starts and
restores it; both edit it; both save.

| | before | after |
|---|---|---|
| temp files | one, shared: "from A + A edits" (**B's edits lost**) | two: A's "from A + A edits", B's fork "from A + B edits" |

### Manual (the user)

The same two scenarios on a desktop. Also close a tab in one instance and
check that it does not come back after the other instance saves and both
restart.

## Acceptance criteria

- [x] No save drops another live instance's document or its temp file.
- [x] A copy closed by an instance does not come back; a reopen elsewhere
      survives; close and reopen in one instance works.
- [x] No temp file has two writers (fork on adoption from a live or unknown
      owner).
- [x] Old session files (no `owner`) restore and merge.
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; Xvfb before/after for (b) and adoption.
- [ ] Pending user verification.

## Affected files

- `src/app/services/session.rs`:
  - `DocumentSession.owner`, `session_identity`, `entry_owner`;
  - `reuse_restored_temp` (fork unless the owner is gone);
  - `Ownership`;
  - `Saver`;
  - the merge rule (`mode` is no longer a parameter);
  - `save_session` updates `Ownership`.
- `src/app/controllers/session.rs`:
  - `SessionController.ownership`, `save_now` (every save goes through it),
    `adopt`;
  - `RestoreResult.adopted`;
  - restore decides reuse or fork per entry;
  - a session switch resets `ownership`.
- `src/app/state.rs`, `src/main.rs` — saves go through `save_now`; restore
  adopts.

## Notes

- Depends on T0099 (temp-file identity). Assigned by the orchestrator
  (ferrispad-5a), target 0.9.11. Case (a) comes from ferrispad-a3's T0099
  review; the adoption fork rule is from the orchestrator.
