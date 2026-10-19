---
id: T0026
title: Convert menu.rs closures to add_emit + a single shortcut table
status: done
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 60
---

## Goal

`menu.rs` has 41 hand-rolled `menu.add(..., move |_| s.send(Message::X))`
closures (~250 lines of `let s = *s;` boilerplate) where `add_emit` — already
used in `tab_bar.rs` — collapses each to one line. Worse, menu path strings are
duplicated between `BUILTIN_SHORTCUTS` and the `menu.add` calls and matched by
linear string scan, so a typo in either silently yields no shortcut with no
compile error and no test.

## In scope

- Replace the 41 closures with `menu.add_emit(label, shortcut, flag, sender,
  msg)`.
- Drive both the menu path and its shortcut from a single source (one table)
  so the two can't desync; ideally make a missing/typo'd entry a compile or
  test failure.

## Out of scope

- Changing the menu structure or shortcuts themselves.

## What was done

- **`builtin_menu_entries(settings, initial_dark_mode, tabs_enabled) ->
  Vec<MenuEntry>`** (new, pure data, no FLTK widget): the built-in menu in
  display order, one `MenuEntry { path, flag, msg }` per item. Small `normal` /
  `toggle` helpers replace the five hand-written `*_flag` blocks.
- **`build_menu`** is now a loop: for each entry, look up its path in the
  shortcut table, resolve it through the registry, and call
  `menu.add_emit(path, shortcut, flag, sender, msg)`. All 41 closures and the
  `rs` helper are gone (`menu.rs` build section went from ~330 to ~140 lines).
- **Shortcuts are no longer named per menu item.** `BUILTIN_SHORTCUTS` stays the
  one place a default lives; an item gets a shortcut if and only if its path has
  a row there. Before, each item had to call `rs("<path>")` *and* repeat the
  path in the table; now the path is written once in the menu and once in the
  table, and the tests below fail on any mismatch.
- **`builtin_shortcut_defaults(tabs_enabled)`** (new): the table seen through
  the tab-mode rules (tab-only commands dropped, `File/New` → Ctrl+N when tabs
  are off). These rules were copied in three places (`build_menu`,
  `apply_shortcut_overrides`, and twice in `shortcut_dialog.rs`); all now call
  this one helper.
- Behaviour check: a scratch script compared the old 41 `menu.add` calls with
  the new entries (path, flag, Message, and whether a shortcut was resolved).
  All 41 match, in the same order. The 22 items that used to call `rs(...)` are
  exactly the 22 rows of `BUILTIN_SHORTCUTS`, so looking the shortcut up by path
  gives the same result as before.
- New unit tests in `src/ui/menu.rs`:
  - `every_builtin_shortcut_names_exactly_one_menu_item` (both tab modes) — the
    orphan check this ticket asks for.
  - `every_builtin_table_row_is_reachable_with_tabs`
  - `builtin_menu_paths_are_unique`
  - `builtin_shortcut_ids_and_defaults_are_unique` — no two commands share a
    default key (normalized with the registry's `normalize_shortcut`).
  - `builtin_defaults_parse_and_are_reserved_from_plugins` — every default
    parses and is in `RESERVED_SHORTCUTS`, so plugins can't take it.
  - `tab_mode_rules_for_builtin_defaults`
  - `run_all_checks_default_matches_settings_default` — the table matches
    `AppSettings::run_all_checks_shortcut`'s default, which `AppState` mirrors.
  - `registry_override_applies_to_a_builtin_menu_id` — the registry keys
    built-ins by menu path, and an override wins over the table default.
  - `toggle_entries_reflect_settings`

Not a defect ticket, so there is no red test. This is a refactor; every
existing test stays green.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::menu
nix develop -c cargo test
```

To see the desync check work, change one path in `BUILTIN_SHORTCUTS` (e.g.
`"Edit/Find..."` → `"Edit/Find"`) and re-run the first command:
`every_builtin_shortcut_names_exactly_one_menu_item` fails and names the
orphaned id. Undo the change afterwards.

### Manual (in the running app, `nix develop -c cargo run`)

With tabs enabled (default):

1. Open the **File** menu: labels and shortcuts show as before — New `Ctrl+T`,
   Open... `Ctrl+O`, Save `Ctrl+S`, Save As... `Ctrl+Shift+S`, Close Tab
   `Ctrl+W`, Quit `Ctrl+Q`; Sessions... and Settings... have no shortcut.
2. Press `Ctrl+T` → a new tab opens. `Ctrl+Tab` / `Ctrl+Shift+Tab` → switch tabs.
   `Ctrl+W` → closes the active tab.
3. Type some text, press `Ctrl+A` then `Ctrl+X` → the text is cut; `Ctrl+Z`
   brings it back; `Ctrl+Shift+Z` redoes the cut.
4. `Ctrl+F` → Find opens; `Ctrl+H` → Replace opens; `Ctrl+G` → Go To Line opens.
5. **View** menu: the four Toggle items show check marks that match your
   settings (e.g. Line Numbers checked if they are on); clicking one toggles it.
6. **Format → Font Size**: the current size shows `← current`; picking another
   size changes the font and moves the marker.
7. **Plugins → General → Run All Checks** shows `Ctrl+Shift+L`.
8. **Edit → Key Shortcuts...**: rebind *File/Save* to `Ctrl+Shift+P`, press
   Save, then check that the File menu shows `Ctrl+Shift+P` next to Save and
   that the new key saves. Reset it afterwards.

With tabs disabled (Settings → turn tabs off, restart):

9. **File** menu: New shows `Ctrl+N`; Close Tab / Next Tab / Previous Tab are
   gone; Edit → Key Shortcuts... does not list them either.

## Acceptance criteria

- [x] Menu built with `add_emit`; boilerplate gone.
- [x] Path + shortcut share one source of truth; desync is detectable.
- [x] Test added; manual accelerator check pending user verification.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/menu.rs` — `builtin_shortcut_defaults`, `MenuEntry`,
  `builtin_menu_entries`, `build_menu`, `apply_shortcut_overrides`, tests.
- `src/ui/dialogs/shortcut_dialog.rs` — uses `builtin_shortcut_defaults`.

## Notes

- Origin: UI audit (SEVERE). `tab_bar.rs:1530-1580` already shows the `add_emit`
  pattern to copy.
- `state.rs` still passes the `"Plugins/General/Run All Checks"` /
  `"Ctrl+Shift+L"` literals to `effective_shortcut`. I left that call alone (out
  of scope); `run_all_checks_default_matches_settings_default` now catches drift
  between the table and the settings default.
