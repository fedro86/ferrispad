---
id: T0103
title: Refresh the file-type menus whenever the active document is rebound (Preview after a restored .md)
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 190
---

## Goal

FerrisPad starts with a restored `.md` file as the active tab, yet Ctrl+M /
View → Preview in Browser is disabled. Switching to another tab and back
enables it. The user reported it on macOS; it does not depend on the
platform.

## Cause

- `update_menus_for_file_type()` enables "View/Preview in Browser" for
  `.md` / `.markdown`.
- `main.rs:345` calls it once at startup, *before* the deferred session
  restore, so it sees the pre-restore default untitled document.
- `AppState::restore_session` (`state.rs:613`) and every other path that
  binds the active document through `bind_active_buffer()` never refresh
  the menus: new session, session switch, settings/theme rebinds.
- Only `switch_to_document()` does, which is why switching tabs "fixes"
  it.
- Closing the last tab leaves the menu as it was for the closed document.

## In scope

- One choke point: `bind_active_buffer()`, the binding of the active
  document to the editor, also refreshes the file-type menus. Callers no
  longer have to remember it.
- Closing the last document (start page) refreshes them too, to disabled.
- Check the neighbours: CLI open / `DeferredOpenFile`, a new session or
  session switch, closing tabs.

## Out of scope

- Which items depend on the file type, beyond Preview (unchanged).
- Plugin menu items (`rebuild_plugins_menu`), which have their own refresh.

## How to test

### Regression test (red first)

```bash
xvfb-run -a nix develop -c cargo test --test active_doc_menus
```

`tests/active_doc_menus.rs` is a UI test binary: it needs a display, like
`tests/ui_dialogs.rs`, and is skipped without one unless
`FERRISPAD_REQUIRE_UI_TESTS=1`.
- Config and data dirs point at temp dirs.
- It builds the main window and `AppState` as `main.rs` does.
- It writes a session whose active document is a `.md` file and calls
  `restore_session()`.
- Expected: "View/Preview in Browser" is active. Before the fix: inactive.

### Manual (Xvfb by Claude, then the user)

1. Session restore on; open `notes.md` and `other.txt`, `.md` active; quit.
2. Start FerrisPad; open the View menu. "Preview in Browser" is enabled
   (before: greyed out). Screenshot.
3. Close all tabs; View → Preview is disabled.

Red run, before the fix (`DISPLAY=:99`):

```text
test file_type_menus_follow_the_active_document ... FAILED
panicked at tests/active_doc_menus.rs:123:5:
Preview in Browser must be enabled for the restored active .md
```

After the fix it passes, including the last step (close every tab →
Preview disabled). The full suite with a display: 662 passed. Clippy
`-D warnings` and fmt clean.

The test sets `XDG_CONFIG_HOME` / `XDG_DATA_HOME` to temp dirs before
anything else (one test function per binary). `AppState` (themes, plugins,
session) therefore never touches the user's real directories; the test
asserts that the session dir is under its temp root.

### Xvfb results (driven by Claude, debug build; "before" = `master` 7ea980d release)

Private dirs, `session_restore: SavedFiles`, a session with `other.txt` and
`notes.md`, the `.md` active. Start, open the View menu:

| | View → Preview in Browser |
|---|---|
| before | greyed out, with `notes.md` active |
| after | **enabled** |
| close every tab (both) | greyed out |

## What was done

- `AppState::bind_active_buffer()` ends with `update_menus_for_file_type()`.
  That is the choke point for every path that binds the active document
  without a tab switch: session restore, a new session or session switch,
  rebinds after settings and theme changes (11 callers).
  `switch_to_document()`, used by tab switches, CLI open, `DeferredOpenFile`
  and closing a tab, already refreshed the menus.
- `close_tab()` refreshes them when the last document goes away (start
  page).
- Neighbours checked:
  - CLI open and `DeferredOpenFile` go through
    `FileAction::SwitchToDocument` → `switch_to_document`;
  - tabs-off open and Save As return `UpdateMenusForFileType`;
  - closing a non-last tab switches to the neighbour.
  All were already covered.

## Acceptance criteria

- [x] After a restore whose active document is a `.md`, Preview is enabled
      without switching tabs.
- [x] Every `bind_active_buffer()` caller gets the refresh; no caller calls
      it separately.
- [x] Regression test added; it failed before the fix.
- [x] Gates pass with zero warnings; Xvfb screenshot of the View menu.
- [ ] Pending user verification: on macOS, the user's original report.

## Affected files

- `src/app/state.rs` — `bind_active_buffer` refreshes the menus;
  `close_tab` with no tab left
- `tests/active_doc_menus.rs` — new UI test binary

## Notes

- User report (macOS), assigned by the orchestrator (ferrispad-5a); an
  rc.3 candidate.
