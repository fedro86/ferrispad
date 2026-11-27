---
id: T0099
title: Keep every instance's unsaved untitled text when sessions merge (identity = temp file, not name)
status: done            # todo | review | done
created: 2026-11-26
severity: moderate
area: robustness
depends-on: []
issue: 186
---

## Goal

With session restore set to Full, two FerrisPad instances that each hold an
unsaved untitled document with *different* text can lose one instance's
text. Whichever instance saves its session last drops the other's document
from `session.json`, and the orphan cleanup then deletes its temp file. This
is data loss of unsaved work. Found by ferrispad-a3 while reviewing T0091;
pre-existing.

## Cause

There are two identity mistakes in `services/session.rs`.

1. **Merge by name.** `merge_and_persist` keeps a foreign instance's
   untitled document only if `!temp_dup && !name_dup`, where `name_dup` is
   `display_name` equality with one of our untitled documents
   (`session.rs:386-409`). Two different documents both called "Untitled"
   (or both "Untitled 2" before T0091) count as duplicates.
2. **Temp file names collide.** The temp file of an unsaved document is
   `hash(file_path or display_name, doc.id)`. The id is per-process and
   starts at 1 in every instance, so instance A's "Untitled" #2 and instance
   B's "Untitled" #2 get the **same** temp file name. They overwrite each
   other's content even without the name check.

`name_dup` exists for one real case. An instance that *restored* a document
from the session saves it again under a new hash (new process, new id),
while the session file still lists the old temp file; without a check the
document would be duplicated on the next restore.

## In scope

One rule: **the identity of an unsaved document in the session is its temp
file.**

- A document restored from a temp file keeps writing to that same file
  (`Document::restored_temp_file`, set by session restore). The restored
  document is then recognised by `temp_dup`, and `name_dup` is no longer
  needed.
- A new document's temp file name also hashes the instance id, so two
  instances never produce the same name. This is one pure
  `temp_file_name(...)` function.
- `merge_and_persist` treats a foreign untitled document as a duplicate only
  when its temp file is one of ours; `name_dup` is removed.

## Out of scope

- Two instances that restored the *same* session document both write its
  one temp file, and the last write wins. That is the same document edited
  in two places; this ticket keeps it a single document, as before.
- Dirty file-backed documents are still merged by path (one file, one
  entry).
- Live sync between instances.
- **Known pre-existing issue, tracked in T0101.** A document closed by a
  new run *before its first auto-save* comes back on the next restart.
  The merge treats the previous run's session as a foreign instance's (its
  pid differs) and keeps the closed document. After the first auto-save
  the run owns the session and closes stick. T0101 will merge only with
  instances that are still alive, failing safe.
- **Known, also for T0101 (cross-instance closes).** With `name_dup` gone,
  an untitled document that instance B closed can be merged back by
  instance A's next save: A keeps every foreign temp file not in its own
  list. Before, `name_dup` sometimes hid this by accident, when A had an
  "Untitled" of its own. Resurrection is preferred to data loss. T0101's
  fix has to account for closes in other instances, e.g. with a
  per-instance list of closed temp files or an ownership tag on
  `DocumentSession` (review note, ferrispad-a3).

## How to test

### Regression tests

```bash
nix develop -c cargo test session_identity
```

- `services::session::tests::session_identity_foreign_untitled_survives_merge`
  - Instance "111" has persisted an untitled "Untitled" (temp
    `aaaa.tmp`, text A).
  - Instance "222" saves its own untitled "Untitled" (temp `bbbb.tmp`,
    text B).
  - Expected: `session.json` lists both, and both temp files still exist.
  - Before the fix: A's document is dropped (`name_dup`) and `aaaa.tmp` is
    deleted.
- `…::session_identity_temp_names_differ_across_instances`
  - The same display name and document id in two instances give two
    different temp file names.
  - Before the fix: the names are equal.
- `…::session_identity_restored_doc_keeps_its_temp_file`
  - A document restored from `xyz.tmp` saves to `xyz.tmp`, and the merge
    does not duplicate it.

Red run, before the fix (`temp_file_name` already extracted with the old
behaviour):

```text
test app::services::session::tests::session_identity_temp_names_differ_across_instances ... FAILED
  assertion `left != right` failed   left: "6cbb5b99cd7f50b2.tmp"  right: "6cbb5b99cd7f50b2.tmp"
test app::services::session::tests::session_identity_restored_doc_keeps_its_temp_file ... FAILED
  assertion `left == right` failed   left: "cbc9888b65d515ea.tmp"  right: "xyz.tmp"
test app::services::session::tests::session_identity_foreign_untitled_survives_merge ... FAILED
  instance 111's unsaved text was dropped from the session: ["bbbb.tmp"]
test result: FAILED. 0 passed; 3 failed
```

Read side, folded in during review (ferrispad-a3, orchestrator):
`read_temp_file` now applies the same `is_plain_temp_name` rule as the
write side. A `session.json` entry whose temp file is an absolute path or
contains `..` is skipped with a log line, and the restore goes on with the
other documents: the restore loop's `let`-chain already skips an entry whose
read returns `None`. Test
`…::session_identity_read_only_plain_temp_names`: `good.tmp` is read;
`../x.tmp`, an absolute path to a file outside the session dir, and
`/etc/hostname` are not. Red before the rule:
`../x.tmp must not be read  left: Some("outside the session dir")  right: None`.
Xvfb, with a hand-made `session.json` of three untitled entries whose temp
files are `../x.tmp` (a file one level above, holding "OUTSIDE"),
`good.tmp` and `/etc/hostname` (absent on the test host):
- `master` restores two tabs, one of them the outside file's text;
- the fix restores only the `good.tmp` tab ("kept text"), and logs
  `[session] Skipping temp file "../x.tmp": not a plain .tmp name` and the
  same for `/etc/hostname`.

Guard tests, added at the orchestrator's request. They pass on the old code
too; they pin that the new naming does not cost disk or compatibility:

- `…::session_identity_restored_name_must_be_plain`
  - A restored name from `session.json` is reused only when it is a bare
    `*.tmp` file name.
  - `../evil.tmp`, `/tmp/x.tmp`, `sub/x.tmp`, `..`, `.tmp` and `x.txt` all
    get a fresh name. A path is never written to: the session file counts
    as untrusted input.
- `…::session_identity_no_temp_file_leak_over_runs` (no disk leak)
  - Six simulated runs: new pids and ids on each restart, documents
    restored, added, and one closed after the run owns the session.
  - The temp dir always holds exactly as many `.tmp` files as the session
    lists (2 → 2 → 2 → 2 → 3 → 2).
  - Restored documents create no new files; a closed one's file is removed
    by the existing orphan cleanup.
- `…::session_identity_old_session_upgrades_cleanly` (compatibility)
  - A `session.json` in the 0.9.9 / 0.9.10-rc.2 format: old-style names
    `6cbb5b99cd7f50b2.tmp` / `620ea35db537a593.tmp`, "Untitled 2/3",
    instance 4242.
  - It parses, and its texts read back.
  - The first save by the new version (new pid, new ids, new names) writes
    back to exactly those two files: same order, same texts, no loss, no
    duplicate.

### Manual (Xvfb by Claude, then the user)

1. Use a private data dir, session restore Full, tabs on.
2. Instance A: Ctrl+T, type "from A". Instance B (second process, same
   dirs): Ctrl+T, type "from B".
3. Quit A, then quit B ("Quit Without Saving"; the session keeps the
   unsaved text in temp files).
4. Start one instance. Expect two restored untitled tabs, "from A" and
   "from B". Before: only one survives.

### Xvfb results (driven by Claude, debug build; "before" = `master` 7ea980d release)

Private config and data dirs, Full restore, tabs on. Instance A types "from
A" in a new tab and instance B (a second process, same dirs) types "from B";
quit A, then B ("Quit Without Saving"); start a third instance.

| | session.json after both quit | temp files | third instance restores |
|---|---|---|---|
| before | 1 doc ("Untitled 2") | 1: "from B" | one tab, "from B" (A's text **lost**) |
| after | 2 docs (both "Untitled 2") | 2: "from A", "from B" | two tabs, "from B" and "from A" |

No panic in the debug log.

**Harness correction (2026-11-27).** The scripted two-instance runs above
looked up each instance's window with `xdotool search --pid P --name
FerrisPad`. xdotool ORs those criteria, so both "instances" could resolve to
one window. The same cell was rerun with the fixed lookup (`--all`,
`uitest-T0087/uc.sh`): one unsaved untitled tab per instance, close one
window and then the other with "Quit Without Saving", restart one instance.
- `master` 4cd7cf5 keeps only the last writer's text ("da B" when A closes
  first, "da A" when B closes first), because both instances write one
  shared temp file.
- `ticket/T0099` keeps both texts in either close order, also with an
  auto-save before the closes.

The conclusion above stands.

## What was done

- `services::session::temp_file_name(hash_key, doc_id, instance_id,
  restored_temp_file)` is the one place that names a temp file.
  - It returns the restored name when that is a plain `*.tmp` file name.
  - Otherwise it returns `hash(hash_key, doc_id, instance_id)`, which
    replaces `make_hash(name, id)`.
- `Document::restored_temp_file` is set by `SessionController::restore`
  for both restore paths that read a temp file (file-backed with unsaved
  edits, and untitled).
- `merge_and_persist`: a foreign untitled document is a duplicate only if
  its temp file is one of ours; `name_dup` is removed.
- `save_session` computes the instance id once, before naming files.

## Acceptance criteria

- [x] Unsaved untitled text from two instances survives a merge (test and
      Xvfb).
- [x] A restored document is not duplicated on the next save/restore (test;
      the old-format upgrade test too).
- [x] No temp-file accumulation over runs; old sessions upgrade without
      loss.
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings (664 passed ×2, clippy `-D warnings`,
      fmt).
- [x] User verification: two real instances on a desktop.

## Affected files

- `src/app/services/session.rs` — `temp_file_name`, `save_session`,
  `merge_and_persist`
- `src/app/domain/document.rs` — `restored_temp_file`
- `src/app/controllers/session.rs` — restore records the temp file

## Notes

- Found by ferrispad-a3 (T0091 review); assigned by the orchestrator as
  T0099, rc.3 if ready in time.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration
build `verify/rc3` (checklist `VERIFICA-RC3.md`).

Section 5 of the checklist (two instances, Full restore, unsaved untitled
tabs) is re-run on `master`, with an isolated `XDG_DATA_HOME`, before the
0.9.10-rc.3 tag.
