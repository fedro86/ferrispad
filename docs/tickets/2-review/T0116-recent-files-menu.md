---
id: T0116
title: Keep a real recent-files list and show it in File → Open Recent
status: review          # todo | review | done
created: 2026-11-27
severity: ~
area: ui
depends-on: []
issue: 238              # tracking issue; the request is #221
---

## Goal

A user asked (#221) for a list of recently opened files in the File menu.
FerrisPad has no such list. The start page's "Recent Files" card looks like
one, but `gather_recent_files()` (`src/ui/start_page.rs`) only lists the
named files that are **currently in a saved session**. A file you opened and
closed is gone from it. With session restore off, or with only untitled tabs,
the card is empty. The goal is a real most-recently-used list that survives
closing a file, shown in the File menu and on the start page.

## In scope

- **A persisted MRU list** of named files, most recent first, at most 10
  entries, deduplicated by path. Stored as state in the data dir (next to
  the sessions, `dirs::data_dir()/ferrispad/recent_files.json`), not in
  `settings.json`. A missing or malformed file reads as an empty list and
  never fails startup (untrusted input: bounded size, bounded entry count).
- **Updated only on user actions:** opening a file (File → Open, command
  line, drag and drop, plugin `open_file`, the start page, the menu itself)
  and Save As. No watcher, no timer, no background check that the files
  still exist.
- **File → Open Recent** submenu: one item per entry (file name, with the
  parent folder where two entries share a name), then a separator and
  **Clear Recent**. Disabled (or showing a disabled "(empty)") when the list
  is empty. Rebuilt when the list changes, through the existing Message
  flow.
- **Clicking an entry** opens it like File → Open, or switches to its tab if
  it is already open. If the file no longer exists, show the usual error and
  remove the entry.
- **The start page's Recent Files card** reads the same list (first 4
  entries), so the two can't disagree.

## Out of scope

- Recent folders or projects, pinned entries, per-session lists.
- OS-level recent documents (Windows jump lists, macOS Open Recent menu,
  GNOME recent files).
- A setting for the list size, or turning the list off. Clear Recent covers
  the privacy case.
- Untitled documents (they have no path to reopen).

## How to test

### Unit tests

```bash
nix develop -c cargo test recent_files
```

- Pure list logic in a service (`services/recent_files.rs`): push moves an
  existing path to the front, caps at 10, dedups; load of a missing, empty,
  malformed or oversized file returns an empty or truncated list without
  panicking; save → load round-trips.
- The menu labels: two entries with the same file name get their parent
  folder appended.
- The start page takes its entries from the MRU list (a test on the data
  function, not on FLTK widgets; FLTK objects only in
  `tests/fltk_main_thread.rs`, see T0113).

### Manual (Claude drives it in Xvfb first, then the user)

1. `nix develop -c cargo run` with a temporary `XDG_CONFIG_HOME` and
   `XDG_DATA_HOME`, session restore **off**.
2. Open `a.txt`, then `b.txt`, close both tabs.
3. File → Open Recent shows `b.txt`, `a.txt` in that order; the start page's
   Recent Files card shows the same two (before: empty).
4. Click `a.txt`: it opens and moves to the top. Click it again: FerrisPad
   switches to the open tab instead of opening a second one.
5. Delete `b.txt` on disk, click it in Open Recent: an error, and the entry
   disappears.
6. Clear Recent: the submenu and the start-page card are empty.
7. Restart: the list (as left in step 4/5) is still there.
8. Screenshots of the File menu and the submenu in light and dark: no
   clipped or overlapping labels, long paths elided.

## Acceptance criteria

- [ ] File → Open Recent lists up to 10 recently opened or saved-as files,
      most recent first, and survives a restart.
- [ ] The start page's Recent Files card uses the same list.
- [ ] Missing files are reported and dropped on click; Clear Recent empties
      the list.
- [ ] No timer, watcher or background work: idle CPU stays at 0.
- [ ] A malformed `recent_files.json` never breaks startup.
- [ ] Unit tests added; Xvfb screenshots checked edge by edge; gates pass
      with zero warnings, including the macOS gate.

## Affected files

- `src/app/services/recent_files.rs` (new): list logic and persistence.
- `src/app/domain/messages.rs`, `src/main.rs`, `src/dispatch.rs`: messages to
  open a recent entry and to clear the list.
- `src/app/controllers/file.rs` / `src/app/state.rs`: record opens and
  Save As.
- `src/ui/menu.rs`: the Open Recent submenu and its rebuild.
- `src/ui/start_page.rs`: `gather_recent_files()` reads the MRU list.

## Notes

- Origin: GitHub issue #221 (feature request, same user as #150 and #151).
  Target: 0.9.11 (user decision, 2026-11-27).
- The user thought FerrisPad already had this; the start-page card is why.
  The CHANGELOG entry should say the start page now keeps files you closed.
- Data dir, not config dir: the list is state, like the sessions, and must
  not end up in a settings file someone shares or version-controls.
