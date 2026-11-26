---
id: T0093
title: Show the unsaved "*" in the window title (and the tab dot) as soon as a document changes
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 182
---

## Goal

The window title never shows the "*" dirty marker while typing. During
0.9.10-rc.2 verification the user saw "Untitled - 🦀 FerrisPad" on a tab full
of unsaved text. The tab bar has the same gap: the "●" dot appears on an
inactive dirty tab, but not on the active tab being typed into.

## Cause

- `AppState::update_window_title` (`src/app/state.rs:238-259`) already
  prefixes "*" when `doc.is_dirty()`. `TabBar::rebuild` takes `is_dirty` from
  each document when it is called (`src/ui/tab_bar.rs:229`).
- Both are snapshots, refreshed only on tab switch, rebuild, open, or save.
  Save is covered, because `file_save` returns `UpdateWindowTitle` and
  `RebuildTabBar`.
- The flip from clean to dirty happens in the buffer modify callback
  (`register_modify_callback`, `src/app/domain/document.rs:89-130`):
  `has_unsaved_changes.set(true)`. That callback only sends
  `BufferModified`, whose handler (`dispatch.rs` `handle_highlight`)
  refreshes neither the title nor the tab bar. Nothing re-reads the dirty
  state until the next switch or rebuild.

## In scope

- The modify callback reports the **clean → dirty flip**: one
  `Message::DirtyChanged(id)`, sent only when the document was clean before
  the edit, so not on every keystroke.
- Its handler refreshes the window title and the tab bar through the
  existing Message flow. No polling and no per-iteration check.
- `update_window_title` builds the string with a pure function
  (`window_title(...)`) and calls `set_label` only when the string changes.
  The macOS custom title label (`update_macos_title_label`) is updated in
  the same place, so it follows.
- Undo back to the saved text: `has_unsaved_changes` is only ever set to
  `true` by edits, so the document stays dirty, with "*" and "●". Document
  this as the current behaviour.

- **With no document open** (the start page at startup, or after closing
  the last tab), the title is just "🦀 FerrisPad" (+ session suffix), not
  "Untitled - …". Folded in by the orchestrator from the T0091 neighbour
  check.
  - `window_title(None, …)` returns the suffix.
  - `update_window_title` now also runs where the last document goes away:
    the startup removal of the default document in `dispatch.rs`, and
    `close_tab` when no tab is left.

## Out of scope

- Tracking "dirty" against the saved content (so that undo back to the saved
  text clears "*"). That needs a saved-state marker in the undo history; it
  would be a separate ticket.

## How to test

### Regression tests

```bash
nix develop -c cargo test dirty_
```

- `domain::document::tests::dirty_flip_is_reported_once`, extended with
  coalescing: ten clean documents flipping before the handler runs give one
  `DirtyChanged`, and after the handler the next flip gives a new one.
  Red with the flag bypassed: `coalesced  left: 10  right: 1`. The test is
  the only drainer of the global channel, which its comment says.
  - A clean document; insert text into its buffer. Exactly one
    `DirtyChanged(id)` is sent for it.
  - A second edit sends none.
  - After `mark_clean()`, the next edit sends one again.
  - The test drains the global fltk channel and filters by its own
    `DocumentId`; no other test reads the channel.
  - Before the fix: no `DirtyChanged` at all.
- `app::state::tests::dirty_window_title`
  - A dirty document gives `"*name - 🦀 FerrisPad"` (`"*name - FerrisPad"`
    on Windows); a clean one has no "*"; a non-default session adds
    `[name]`. These cases pin the existing string.
  - No document gives `"🦀 FerrisPad"` (`"🦀 FerrisPad [work]"` with a
    session). Before the fix: `"Untitled - 🦀 FerrisPad"`.

Red runs, before the fix. The message variant, its handler and the
extracted `window_title` were already in place without changing behaviour;
the callback did not send the message yet:

```text
test app::domain::document::tests::dirty_flip_is_reported_once ... FAILED
assertion `left == right` failed: first edit
  left: 0
 right: 1
test app::state::tests::dirty_window_title ... FAILED
assertion `left == right` failed
  left: "Untitled - 🦀 FerrisPad"
 right: "🦀 FerrisPad"
```

After the fix both pass. Gates: 660 passed, clippy `-D warnings` and fmt
clean.

### Manual (Xvfb by Claude, then the user)

1. Tabs on, Ctrl+T, type "x". The title is "*Untitled - 🦀 FerrisPad" and the
   tab reads "● Untitled". Before: no "*", no dot on the active tab.
2. Ctrl+S, save as `/tmp/t0093.txt`. The title is
   "t0093.txt - 🦀 FerrisPad", with no dot.
3. Type "y": "*" and "●" are back. Ctrl+Z: still "*" (documented: undo
   does not clear dirty).
4. Tabs off, type: the title gets "*".

### Xvfb results (driven by Claude, debug build; "before" = `master` 7ea980d release)

Fresh config, tabs on. Ctrl+S answered by a fake `zenity` that returns a
temp path. Window title after each step:

| Step | before | after |
|---|---|---|
| start (start page) | Untitled - 🦀 FerrisPad | **🦀 FerrisPad** |
| Ctrl+T | Untitled 2 - … | Untitled 2 - … |
| type "x" | Untitled 2 - … | **\*Untitled 2 - …** |
| Ctrl+S | t0093.txt - … | t0093.txt - … |
| type "y" | t0093.txt - … | **\*t0093.txt - …** |
| Ctrl+Z | t0093.txt - … | \*t0093.txt - … (undo keeps dirty, as documented) |
| Ctrl+T, type "z" | Untitled 3 - … | **\*Untitled 3 - …** |
| close the last tab | Untitled - … | **🦀 FerrisPad** |

- Names start at "Untitled 2" because this branch starts from `master`,
  without T0091.
- Tab bar: after typing, the active tab reads "● Untitled 2"; before, the
  active tab had no dot (only inactive tabs got one, at the next rebuild).
  The screenshots were compared.
- Tabs off: typing turns "Untitled - …" into "*Untitled - …", save clears
  it, typing sets it again; before, there was never a "*".
- The saved file holds the typed text. No panic or assertion in the debug
  build's log.
- macOS custom title label: not runnable here. `update_macos_title_label`
  copies the window's title and is still called from every
  `update_window_title`, which now also runs on the flip, so it follows.

## What was done

- `register_modify_callback` remembers whether the document was clean
  before the edit, and after `BufferModified` sends one
  `Message::DirtyChanged(id)` on the clean → dirty flip. Later edits send
  nothing extra.
- **Coalesced** (review follow-up, orchestrator and ferrispad-a3). A
  tab-size change (`set_all_tab_distance`) fires the modify callback of
  every buffer, so every clean document flips and is restored with
  `mark_clean`. One message per flip meant N tab-bar rebuilds, O(tabs²).
  - A thread-local `DIRTY_REFRESH_QUEUED` flag is shared by all modify
    callbacks (they run on the UI thread).
  - The first flip sets it and sends `DirtyChanged`; later flips send
    nothing.
  - The handler clears it (`dirty_refresh_handled()`) before refreshing,
    so N flips in one step cost one title update and one tab-bar rebuild.
  - The main loop takes one message per iteration, so a per-iteration
    "pending" flag would still rebuild N times; deduplicating at send time
    avoids that, without a timer.
- `main.rs` routes `DirtyChanged` with the buffer messages;
  `dispatch::handle_highlight` calls `update_window_title()` +
  `rebuild_tab_bar()`.
- `state::window_title(active, session)` is the pure title function.
  `update_window_title` calls `set_label` only when the text changes.
- No document → app name only, with the two refresh points above.
- Undo back to the saved text keeps the document dirty: it is an
  edit-flag, not a comparison with disk. This is documented here and
  unchanged.

## Acceptance criteria

- [x] The first edit of a clean document shows "*" in the title and "●" on
      its tab, without a tab switch.
- [x] Save clears both (already the case; it stays so).
- [x] No per-keystroke refresh: exactly one message per clean → dirty flip.
- [x] With no document open the title is the app name only.
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; UI driven in Xvfb (title and tab
      screenshots).
- [ ] Pending user verification: the manual recipe on a real desktop, and
      the macOS title label.

## Affected files

- `src/app/domain/document.rs` — the modify callback reports the flip
- `src/app/domain/messages.rs` — `Message::DirtyChanged(DocumentId)`
- `src/main.rs`, `src/dispatch.rs` — route and handle it; refresh the
  title when startup removes the default document
- `src/app/state.rs` — `window_title` pure function; `set_label` only on
  change; `close_tab` refreshes the title when no tab is left

## Notes

- Reported by the user during 0.9.10-rc.2 verification; assigned by the
  orchestrator (ferrispad-5a). The tab-dot half was observed in the T0091
  Xvfb pass.
