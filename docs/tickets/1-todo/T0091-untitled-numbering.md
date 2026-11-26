---
id: T0091
title: Number new untitled tabs from the lowest free number, starting at "Untitled"
status: todo            # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: ~
---

## Goal

Start FerrisPad with tabs on (the start page shows), press Ctrl+T or File →
New: the first tab is called **"Untitled 2"**. There is no "Untitled" (1)
anywhere. Reported by the user on 0.9.10-rc.2.

Cause: `AppState::new` (`src/app/state.rs:119`) always creates an empty
untitled document, which takes `untitled_counter = 1`
(`src/app/controllers/tabs.rs:122-128`). At startup, `dispatch.rs:703-714`
removes it again so the start page shows. The counter only ever grows, so
the user's first new tab gets 2. The same thing happens wherever the default
document is created and then dropped: session restore (`session.rs:212`,
restored untitled tabs start at "Untitled 2"), a new session
(`state.rs:708`), and after closing every tab and creating a new one.

After this ticket, the first untitled tab is always "Untitled". A new
untitled tab takes the lowest number that no open untitled tab uses.

## In scope

- Replace the monotonic `untitled_counter` with one rule: a new untitled
  document gets the smallest `n ≥ 1` not used by an open untitled document
  (`n = 1` displays as "Untitled", `n ≥ 2` as "Untitled n", the same format
  as today). Store `n` on the `Document` (e.g. `untitled_number:
  Option<u32>`), not parsed back from `display_name`.
- Saving an untitled document (it gets a path) frees its number.
- Apply the rule on every path that calls `add_untitled`: File → New, the
  startup default, session restore (Full mode with temp files), new session
  window.

## Out of scope

- The tabs-disabled mode, which keeps a single "Untitled" (`file.rs:249-256`).
- Changing the "Untitled n" format or the window title format.
- Persisting untitled numbers in the session file.

## How to test

### Regression tests (red first)

In `src/app/controllers/tabs.rs` (TabManager is UI-agnostic):

```bash
nix develop -c cargo test untitled_
```

- `untitled_first_new_after_startup_is_untitled`: add the default untitled,
  remove it as startup does, `add_untitled()` → display name `"Untitled"`.
  Before the fix: `"Untitled 2"`.
- `untitled_reuses_lowest_free_number`: open three, close "Untitled 2",
  add → "Untitled 2"; close all, add → "Untitled".
- `untitled_saved_doc_frees_its_number`: give the first untitled a file path,
  add → "Untitled".
- Session restore (Full) of two untitled temp docs → "Untitled",
  "Untitled 2" (`session.rs` test with the existing fixtures).

### Manual (Xvfb by Claude, then the user)

1. Fresh config, tabs on, start → start page. Ctrl+T → tab "Untitled";
   Ctrl+T → "Untitled 2".
2. Close "Untitled", Ctrl+T → "Untitled" again.
3. Close all tabs (start page), Ctrl+T → "Untitled".

## Acceptance criteria

- [ ] The first new tab after startup is "Untitled", in every startup mode
      (no session, session restore Off / Saved Files / Full).
- [ ] New tabs take the lowest free number; saving or closing frees it.
- [ ] Regression tests added; the first one failed before the fix with
      "Untitled 2".
- [ ] Gates pass with zero warnings; UI driven in Xvfb (tab bar screenshots).

## Affected files

- `src/app/controllers/tabs.rs` — `untitled_counter` → lowest free number
- `src/app/domain/document.rs` — `new_untitled`, untitled number field
- `src/app/controllers/session.rs`, `src/app/state.rs` — callers (check only)

## Notes

- Lowest-free numbering is what VS Code and most editors do.
