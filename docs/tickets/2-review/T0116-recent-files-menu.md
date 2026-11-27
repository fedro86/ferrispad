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

## What was done (2026-11-27)

- **`services/recent_files.rs`** (new):
  - `push`, `remove`, `labels` and `escape_menu_label`, `start_page_entries`;
  - `load_from` / `save_to`. The file is
    `<data_dir>/ferrispad/recent_files.json` (`{"version":1,"files":[…]}`),
    written through a temp file and a rename.
  - Untrusted input:
    - a file over 64 KB is ignored;
    - JSON that is not the expected shape reads as empty;
    - entries must be absolute, not empty, without NUL and at most 4096
      bytes; duplicates are dropped and the list is capped at 10.
  - Labels:
    - the file name, plus " — <folder>" where two share a name;
    - elided in the middle at 60 characters;
    - made unique with " (2)" (FLTK replaces an item added twice under one
      label);
    - escaped for FLTK menus (`/`, `&`, `\`, a leading `_`).
- **Recording.**
  - Every open goes through `FileController::open_file`: File → Open, the
    command line, drag and drop (`DeferredOpenFile`), a plugin's
    `open_file`, MCP, the start page and the menu itself.
  - It now appends `FileAction::RememberRecent(path)` when the open shows a
    file (`opened()`: a tab switch or a title update, and no error). A
    cancelled or failed open is not remembered.
  - Save As remembers its new path.
  - `AppState::remember_recent` keeps only existing files, made absolute.
- **File → Open Recent.**
  - `build_menu` creates the submenu right under Open….
  - `rebuild_recent_menu` refills it: the entries, a divider, Clear Recent;
    or an inactive "(no recent files)".
  - `OpenRecentFile(path)` opens the file, or switches to its tab through
    the usual `open_file`.
  - A missing file shows "… no longer exists. It was removed from the
    recent files." and is dropped.
  - `ClearRecentFiles` empties the list.
- **Start page.**
  - The Recent Files card shows the first 4 entries of the same list and
    clicks through `OpenRecentFile`.
  - `sync_start_page` re-renders it when the list changes while it is
    shown.
  - A row's folder (then its name) is elided in the middle until the row
    fits the card in bold, the hover font. Before, a long folder would run
    to the card's edge.
- **First run after an upgrade.** Without a `recent_files.json`, the list is
  seeded once from the named, existing files of the saved sessions. That is
  what the card showed before, so an upgrade doesn't empty it.
- **No background work.** The list changes only on these user actions,
  with no timer and no watcher.

### Tests

- `nix develop -c cargo test recent_files`: 14 unit tests, all pass. They
  cover:
  - push moves the path to the front, the cap, remove;
  - save→load round-trips;
  - a missing, empty, malformed, wrongly-typed or oversized file reads as
    empty;
  - invalid entries are filtered;
  - labels: folder for shared names, unique, elided, escaped;
  - the first four for the start page;
  - `opened()` (cancelled, failed, tabs on, tabs off);
  - the seed keeps only existing named files;
  - the start-page row fits.
- **Main-thread FLTK check** `recent_files_open_recent_sits_under_open`
  (`tests/fltk_main_thread.rs`). Red first:
  - FLTK's `insert()` put a path item at the end of File whatever the
    index;
  - the check failed with Open Recent at position 13, after Quit, instead
    of 2;
  - green after creating the submenu in `build_menu`.
- **Gates:** fmt, clippy `-D warnings`, `cargo test` (xvfb,
  `FERRISPAD_REQUIRE_UI_TESTS=1`) all green.

### Screenshots (`new-pr` §2)

- Scenario: `docs/pr/scenarios/recent-files.sh`.
  - `notes.txt` and `todo.md` are opened via `OPEN_FILES`, with session
    restore off; then two Ctrl+W;
  - shots: the start page, File menu, File → Open Recent.
- `compare.sh` against `origin/master` c3672fe, light and dark. Montages are
  in `ferrispad-wt/uitest-T0116/compare/`.
  - Before: an empty Recent Files card and no Open Recent.
  - After: todo.md and notes.txt in the card. Open Recent sits under Open…
    with todo.md, notes.txt, a divider and Clear Recent.
- Every shot was read edge by edge. That is how the menu-position defect
  and the row that reached the card's edge were found; both are fixed.
- The host was clean after every run.
