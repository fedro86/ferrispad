---
id: T0024
title: Route message/alert dialogs through the themed helper + one modal loop
status: done
created: 2026-08-24
severity: minor
area: ui
depends-on: [T0017, T0019, T0025, T0031, T0039]
issue: 76
---

## Goal

`show_themed_message` was introduced to replace FLTK's unthemed
`dialog::message_default`/`alert_default`, but it is called from only two sites
while ~40 sites still use the raw unthemed FLTK dialogs. The result is
inconsistent theming across the app. Likewise `run_dialog` is the canonical
modal loop but is reimplemented at several sites (T0017 fixed the two buggy
ones).

## In scope

- Migrate the ~40 raw `dialog::{alert,message,choice2,input}_default` call sites
  to the themed helpers (`show_themed_message` / a themed choice/input variant,
  adding the latter if missing).
- Ensure all modal dialogs use `run_dialog`.

## Out of scope

- The color-derivation mismatch between dialogs and the tab bar (T0030).
- The native OS file chooser (`ui/file_dialogs.rs`), which is not an FLTK popup.
- Theming the large-file *streaming progress* window (it has no buttons and its
  own polling loop; see Notes).

## Base / stacking

Stacked on `ticket/T0019` (T0027 → T0029 → T0030 → T0021 → T0023 → T0019), and
merges three in-review branches that edit the same dialog call sites, so their
conflicts are resolved here once:

- `ticket/T0025` (#63) — plugin-row builder + `PluginRowContext` in
  `plugin_manager.rs`.
- `ticket/T0031` (#66) — `find.rs` `Direction` / `search_wrapping` /
  `FindState`; the "Cannot find" / "Invalid regex" dialogs come from one place.
- `ticket/T0039` (#54) — quit check in the large-file streaming loop.

All three merged without textual conflicts; the gates were green after each.

## What was done

**One themed popup, three entry points** (`src/ui/dialogs/mod.rs`). A single
private builder `run_themed_popup` makes a modal window (dialog background,
themed text, optional themed input, a right-aligned button row) and runs it with
the canonical `run_dialog`, so the T0017 guarantees hold for every popup: one
modal loop, and every exit path — a button, Enter, Escape, the window's close
button, or an app quit — hides the window. On top of it:

- `show_themed_message(theme_bg, title, message)` — replaces
  `message_default` / `alert_default` ("Close" button).
- `show_themed_choice(theme_bg, title, message, b0, b1, b2) -> Option<usize>` —
  replaces `choice2_default` with **the same button contract**, so every call
  site keeps its `Some(0)` / `Some(1)` meaning unchanged: `b0` rightmost, `b1`
  in the middle and the Enter default (FLTK's return button), `b2` leftmost,
  empty label = no button, Escape / close button → `None`.
- `show_themed_input(theme_bg, title, prompt, default) -> Option<String>` —
  replaces `input_default` (prefilled and selected, OK = Enter, Cancel/Escape →
  `None`).

The pure parts are unit-tested: `choice_layout` (button order, default button,
empty labels, never zero buttons), `wrapped_line_count` / `popup_height` (the
popup grows with the message instead of clipping it, up to a cap) and
`popup_button_width` (the widest real button row — "Save All / Quit Without
Saving / Cancel" — fits). `DialogTheme::theme_bg()` exposes the source
background so dialogs that only hold a `DialogTheme` can open a popup.

**41 call sites migrated** — no raw FLTK popup is left:

| File | Sites | How |
|------|-------|-----|
| `app/controllers/file.rs` | 7 alerts | Controller now returns `FileAction::ShowError(msg)`; `AppState::dispatch_file_actions` shows it themed. `fltk::dialog` is gone from the controller. |
| `app/controllers/preview.rs` | 1 message, 1 alert | `preview_in_browser` returns `Result<(), PreviewNotice>` (`NotMarkdown` / `OpenFailed`); `AppState` shows it. |
| `app/state.rs` | 4 choices, 1 alert | close-tab unsaved, external modification, quit (tabs / no tabs), settings save failure. |
| `app/controllers/plugin.rs` | 1 alert | uninstall failures. |
| `dispatch.rs` | 1 input | rename tab group. |
| `ui/dialogs/large_file.rs` | 1 choice | large-file warning (now takes `theme_bg`). |
| `ui/dialogs/plugin_manager.rs` | 6 alerts, 2 choices | URL install (T0025's restructured callback), official / community install errors, uninstall and "Details" confirmations. |
| `ui/dialogs/readonly_viewer.rs` | 4 alerts, 3 messages | open/mmap errors, search not found. |
| `ui/dialogs/update.rs` | 1 message, 3 alerts | install success / failures, browser launch. |
| `ui/dialogs/goto_line.rs` | 2 messages | invalid / out-of-range line. |
| `ui/split_panel.rs` | 1 choice | confirm destructive "accept" action. |
| `ui/tree_panel.rs` | 1 input, 1 choice | plugin context-menu prompts; the panel keeps its current theme background in an `Rc<Cell<_>>` updated by `apply_theme`, so the popup matches the theme at click time. |

`find.rs` (after T0031) already went through `show_themed_message`.

### Behaviour notes (deliberate, small)

- Enter / Escape semantics are those of FLTK's dialogs: Enter presses `b1` when
  present. That means the **close-tab "unsaved changes" popup still has
  "Discard" as its Enter default, and the quit popup "Quit Without Saving"** —
  exactly as before, but now visible because the default button is drawn in the
  primary button color. Changing that default is a behaviour change left for
  the user to decide (see Notes).
- Single-button notices now also close on Enter (FLTK's alert did not).
- The existing `show_themed_message` used the raw editor background as its
  window color; the shared popup uses `DialogTheme::bg` like every other
  themed dialog, so Find / Update notices are a shade darker than before.
- File errors are now shown when the controller's actions are dispatched (right
  after the operation). For a single file that is the same moment; when opening
  several files at once or reloading all files, the errors show after the batch
  instead of between files.
- Popups are centred on screen instead of at the mouse pointer.

## How to test

### Automated

```bash
nix develop -c cargo test choice_layout
nix develop -c cargo test popup_
nix develop -c cargo test wrapped_line_count
nix develop -c cargo test preview_
nix develop -c cargo test dialog_theme_remembers
# no raw FLTK popup left (only the native file chooser import should match):
grep -rnE "fltk::dialog::|dialog::(message|alert|choice2?|input)(_default)?\(" src
# full gates
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo test
```

### Manual (UI — not unit-testable)

Run `nix develop -c cargo run`. Do each step once with a **dark** syntax theme
and once with a **light** one (Settings → theme). Every popup must use the
app's dialog background, themed text and flat themed buttons (no gray FLTK
box), and must close with **Enter**, **Escape**, the **window X** and its
**buttons**, without hanging (the app stays responsive afterwards).

1. **Error** — open a file, then make it unreadable from a shell
   (`chmod 000 file`) and use File → Reload (Ctrl+R): "Error reloading file: …" popup
   (title "Error"). Restore with `chmod 644`.
2. **Confirmation** — type in a tab, then close the tab: "Unsaved Changes" with
   Cancel / Discard / Save left to right. Escape or X keeps the tab; Enter
   discards (as before); Save saves.
3. **Info** — on a non-Markdown file, View → Preview in Browser: "Preview"
   notice. Also Edit → Go To Line... with `999999`: "Go To Line" notice.
4. **Plugin dialog** — Plugins → General → Plugin Manager... → Installed →
   Uninstall on any plugin: "Uninstall Plugin" confirm (Cancel cancels;
   Uninstall removes the row). In the Community tab, type `not-a-url` in
   "Paste GitHub URL..." → Install: "Invalid URL" error. Close the main window while one of these popups is open: the app
   exits (no hang).
5. **Find "Cannot find"** — Ctrl+F, search for `zzzz_not_there`, Find Next:
   "Cannot find 'zzzz_not_there'" popup over the Find dialog; with "Use regex"
   checked, searching for `(` shows "Invalid regex".
6. **Large-file warning** — set Settings → large-file warning to 1 MB, open a
   file of a few MB (`head -c 5000000 /dev/urandom | base64 > /tmp/big.txt`):
   "Large File Warning" with Cancel / Open. Escape, X and Cancel abort; Open
   loads the file.
7. **Input** — right-click a tab that is in a group → Rename group: themed
   "Rename Group" prompt, prefilled and selected; Enter renames, Escape cancels.

## Acceptance criteria

- [x] No remaining `*_default` FLTK dialog calls outside the themed helpers.
- [x] All modals use `run_dialog` (the large-file streaming progress loop is the
      one documented exception — it polls a reader channel, T0017/T0039).
- [ ] Manual check across several dialogs (recipe above — for the reviewer).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/mod.rs` — shared themed popup, `show_themed_choice`,
  `show_themed_input`, `DialogTheme::theme_bg()`, tests.
- `src/app/controllers/file.rs`, `src/app/controllers/preview.rs`,
  `src/app/controllers/plugin.rs`, `src/app/state.rs`, `src/dispatch.rs`.
- `src/ui/dialogs/{large_file,plugin_manager,readonly_viewer,update,goto_line}.rs`,
  `src/ui/split_panel.rs`, `src/ui/tree_panel.rs`.

## Notes

- Origin: UI audit (SEVERE for consistency).
- **Decision for the user:** the unsaved-changes popups keep FLTK's Enter
  default (`b1` = "Discard" / "Quit Without Saving"). Making "Save" or "Cancel"
  the Enter default would be safer; it is a one-line change per call site
  (reorder the labels) but changes behaviour, so it is not done here.
- The split-view confirm popup captures the theme when the action buttons are
  built (as the buttons' own colors do); it does not follow a later theme change
  until the next split-view request.
- The large-file streaming progress window stays unthemed and keeps its own
  polling loop (T0039 added its quit check); theming it is a separate ticket.
