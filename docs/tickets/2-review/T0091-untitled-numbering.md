---
id: T0091
title: Number new untitled tabs from the lowest free number, starting at "Untitled"
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 178
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
- `untitled_full_restore_numbers_from_one` (`controllers/session.rs`): a
  Full restore of two unsaved untitled tabs → "Untitled", "Untitled 2".
  There were no fixtures for `SessionController::restore`, which reads the
  real data dir. The restore loop was therefore split, with no behaviour
  change, into `restore` (load the session, read temp files) and
  `restore_data(tab_manager, settings, mode, &SessionData, read_temp)`; the
  test drives `restore_data` with data built in code.

Red run, before the fix (the tests and the seam on the old numbering):

```text
test app::controllers::tabs::tests::untitled_first_new_after_startup_is_untitled ... FAILED
test app::controllers::session::tests::untitled_full_restore_numbers_from_one ... FAILED
test app::controllers::tabs::tests::untitled_reuses_lowest_free_number ... FAILED
test app::controllers::tabs::tests::untitled_saved_doc_frees_its_number ... FAILED
untitled_first_new_after_startup_is_untitled:  left: "Untitled 2"                 right: "Untitled"
untitled_full_restore_numbers_from_one:        left: ["Untitled 2", "Untitled 3"] right: ["Untitled", "Untitled 2"]
untitled_reuses_lowest_free_number:            left: "Untitled 4"                 right: "Untitled 2"
untitled_saved_doc_frees_its_number:           left: "Untitled 2"                 right: "Untitled"
test result: FAILED. 0 passed; 4 failed
```

After the fix all four pass, and the full suite is green (662 passed,
clippy `-D warnings` and fmt clean).

### Manual (Xvfb by Claude, then the user)

1. Fresh config, tabs on, start → start page. Ctrl+T → tab "Untitled";
   Ctrl+T → "Untitled 2".
2. Close "Untitled", Ctrl+T → "Untitled" again.
3. Close all tabs (start page), Ctrl+T → "Untitled".

### Xvfb results (driven by Claude, debug build; "before" = `master` 7ea980d release)

Recipe above, fresh config with `"tabs_enabled": true`. Window title after
each step:

| Step | before | after |
|---|---|---|
| start (start page) | Untitled | Untitled |
| Ctrl+T | **Untitled 2** | **Untitled** |
| Ctrl+T | Untitled 3 | Untitled 2 |
| close the first tab | Untitled 3 | Untitled 2 |
| Ctrl+T | Untitled 4 | **Untitled** (lowest free) |
| close all (start page) | Untitled | Untitled |
| Ctrl+T | **Untitled 5** | **Untitled** |

The tab-bar screenshots match the titles.

Neighbours:
- **Session restore Full.** Run 1: Ctrl+T, type, Ctrl+T, type, Ctrl+Q →
  "Quit Without Saving". Run 2 restores the two unsaved tabs as
  "Untitled 2", "Untitled 3" before the fix and as "Untitled", "Untitled 2"
  after. Config and data dirs (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`) were
  private temp dirs.
- **Tabs off.** Open a file, Ctrl+N, Ctrl+N: "Untitled" each time, before
  and after; unchanged. The single document goes through
  `Document::set_untitled(1)`.
- **New session** (`state.rs` step 5). `TabManager::clear()` never reset the
  old counter, so after a session switch the numbering kept climbing. With
  the lowest-free rule it starts from "Untitled" again. Covered by the rule;
  not driven separately.
- **Saved Files mode.** It restores only files, so it goes through the same
  rule; not driven separately.
- **Window title.** It uses `tab_label()`, so it follows the new names (table
  above).
- No panic or assertion in the debug build's log.

## Acceptance criteria

- [x] The first new tab after startup is "Untitled": no session, Off and
      Full were driven in Xvfb. Saved Files and a new session go through the
      same rule.
- [x] New tabs take the lowest free number; saving or closing frees it.
- [x] Regression tests added; the first one failed before the fix with
      "Untitled 2".
- [x] Gates pass with zero warnings; UI driven in Xvfb (tab-bar screenshots).
- [ ] Pending user verification: the manual recipe on a real desktop.

## What was done

- `Document` has a private `untitled_number: Option<u32>`, set by
  `new_untitled`.
  - `Document::untitled_number()` returns it only while the document has no
    path. Saving or opening a file frees the number, on every path that sets
    `file_path`, including the tail/chunk views that do not go through
    `update_display_name`.
  - `Document::untitled_name(n)` is the one place that formats
    "Untitled" / "Untitled n".
  - `Document::set_untitled(n)` handles the tabs-off "New".
- `TabManager::lowest_free_untitled_number()` returns the smallest `n ≥ 1`
  that no open document holds; `add_untitled` uses it. The monotonic
  `untitled_counter` field is gone, so there is one rule for every caller:
  File → New, the startup default, session restore, a new session.
- `SessionController::restore` is split into load + `restore_data` (test
  seam, no behaviour change).

## Other oddities noticed in tab naming (not fixed here; for the orchestrator)

1. **Start page title.** With no document open (the start page, or after
   closing every tab), the window title is "Untitled - 🦀 FerrisPad"
   (`state.rs` `update_window_title` fallback), although nothing called
   Untitled exists. "🦀 FerrisPad" alone would match the screen.
2. **Dirty markers do not follow typing** (T0093). Typing into a new tab
   leaves the title without "*". The tab bar shows "●" on an *inactive*
   dirty tab but not on the *active* one being typed into. After a session
   restore the active tab does show "● Untitled 2", so the active tab can
   show the dot; it is just not refreshed when the dirty state flips.
3. **A file named "Untitled" next to an untitled document.** Both have the
   display name "Untitled". `add_untitled` does not call
   `disambiguate_display_names`, so both tabs read "Untitled" until the next
   open or close. Then the untitled one becomes "Untitled ()" (empty parent
   path) and the file "Untitled (dir)".
4. **`tabs_enabled` defaults differ.** The serde default for a missing key
   is `false` (`#[serde(default)]` on a bool), while
   `AppSettings::default()` says `true`. A `settings.json` that lacks the
   key (hand-written, or from an older version) turns tabs off. Found while
   setting up the Xvfb config.

## Affected files

- `src/app/controllers/tabs.rs` — `untitled_counter` → `lowest_free_untitled_number`; T0091 tests
- `src/app/domain/document.rs` — `untitled_number` field, `untitled_number()`, `untitled_name`, `set_untitled`
- `src/app/controllers/file.rs` — tabs-off New uses `set_untitled(1)`
- `src/app/controllers/session.rs` — `restore` → load + `restore_data`; restore test

## Notes

- Lowest-free numbering is what VS Code and most editors do.
