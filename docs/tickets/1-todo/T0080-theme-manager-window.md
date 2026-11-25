---
id: T0080
title: Theme manager window — duplicate, rename, delete, edit colors, import .tmTheme
status: todo            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0079]     # other ticket IDs that must land first
issue: ~                # GitHub issue number
---

## Goal

T0079 makes custom editor themes possible: each one is a syntect `Theme`
saved in `<config>/ferrispad/themes/<id>.json`, managed by the UI-free
`custom_themes` service. Users still need a window to manage them. The
pattern follows JetBrains "Color Scheme": built-in themes are read-only, you
**Duplicate** one, then change the colors you care about.

## In scope

- A **"Manage themes…"** button in the Settings dialog, next to the light and
  dark syntax-theme choices. It opens a modal, themed window that lists the
  built-in themes (marked read-only) and the custom ones.
- **Duplicate** (from any theme), **Rename**, **Delete** (with confirmation)
  and **Import .tmTheme…**, all through the T0079 service.
  - Import uses `ThemeSet::get_theme(path)`, which is already compiled in.
  - The imported theme becomes a custom theme with `based_on: null`.
- **Color editing** for a custom theme: background, text, caret, selection,
  line-number text, line-number background and current line. Each color has
  a swatch, a picker button (FLTK `dialog::color_chooser`) and a
  "Reset to base" button. Reset is disabled when `based_on` is null.
- **Live preview** in the main editor while editing. **OK** saves the changes
  atomically; **Cancel** (or closing the window) discards them and restores
  the previous look.
- The editing logic lives in a UI-free `ThemeManagerModel` (working copy +
  committed state) returning action enums, as the controllers do, so it is
  unit-testable without FLTK.

## Out of scope

- The theme model and file format — T0079.
- Current-line highlight rendering — T0081. This window only edits its
  color.
- Editing syntax token colors (`theme.scopes`) and exporting `.tmTheme`.

## Edge cases (each one is covered by a test or a manual step below)

1. **Built-ins are read-only.** Rename, Delete and the color controls are
   disabled for them; the model rejects those actions as well, not only the
   UI.
2. **Deleting a theme in use.** If the deleted theme sits in the light and/or
   dark slot, those slots fall back to the mode default built-in, the change
   is saved in `settings.json`, and the editor is re-themed at once.
3. **Rename keeps the id.** The file name and the `custom:<id>` references
   are unchanged.
   - Name validation: not empty or whitespace-only, at most 64 chars, unique
     among custom themes (case-insensitive). An invalid name shows an inline
     error and nothing is written.
4. **Import of untrusted files.** A file over 1 MiB, malformed plist, a
   binary file, a `.tmTheme` without `background`/`foreground`, or a
   duplicate name.
   - A clear error dialog, and **nothing is created** on failure.
   - Missing colors use T0079's derived fallbacks.
   - Duplicate names get " (2)".
5. **Picker cancelled.** `color_chooser` returns `None`, so no change is
   made.
6. **Editing the active theme vs. an inactive one.** Preview applies only
   when the edited theme is the one shown in the current mode. Editing an
   inactive theme does not touch the editor.
7. **Cancel after many edits / after Duplicate.** Cancel discards the color
   edits of this session. Theme-level actions (Duplicate, Rename, Delete,
   Import) are committed immediately and are not undone by Cancel. The
   window states this in a one-line hint.
8. **Settings dialog Cancel.** The Settings dialog has its own preview and
   revert path (`state.open_settings`). Opening the manager from it, then
   cancelling Settings, must not resurrect a deleted theme or point a slot at
   a missing id.

## How to test

### Unit tests

Next to `ThemeManagerModel` (tempdir themes dir, no FLTK):

```bash
nix develop -c cargo test theme_manager
```

- `theme_manager_builtins_are_read_only` — rename / delete / set_color on a
  built-in return an error action and change nothing on disk.
- `theme_manager_delete_in_use_resets_slots` — delete a theme referenced by
  both slots: the returned actions set both slots to the mode defaults and
  request a settings save and a re-theme.
- `theme_manager_rename_validation` — `""`, `"   "`, a 65-char name and a
  case-insensitive duplicate are rejected with no file change; a valid
  rename keeps the id.
- `theme_manager_cancel_discards_color_edits` — set three colors, cancel:
  the file on disk is byte-identical to before, and the returned actions
  restore the previous editor colors.
- `theme_manager_ok_persists_color_edits` — set colors, OK: reloading from
  disk shows them.
- `theme_manager_import_valid` — importing `tests/fixtures/import_ok.tmTheme`
  creates one custom theme with `based_on: null` and the file's colors.
- `theme_manager_import_rejects_bad_files` — `import_malformed.tmTheme`, a
  2 MiB file and a binary file each return an error, and the themes dir is
  unchanged.
- `theme_manager_import_missing_colors_uses_fallback` —
  `import_no_background.tmTheme` imports, and its effective colors are the
  derived fallbacks (text vs background ≥ 4.5:1).

All pass; the full `nix develop -c cargo test` stays green.

### Manual check (record results here)

1. `nix develop -c cargo run` → Settings → **Manage themes…**. The 7
   built-ins are listed as read-only, and their Rename/Delete/colors are
   disabled.
2. Select "Base16 Ocean Dark" → **Duplicate**, name it "My Dark". It appears
   in the list and in the dark-theme choice in Settings.
3. In dark mode, assign "My Dark" to the dark slot. In the manager, change
   the background, selection and current-line colors: the editor updates
   live. **Cancel** → the previous colors return. Repeat and press **OK** →
   the colors persist after a restart.
4. "Reset to base" on the selection → it returns to the Ocean Dark (T0078)
   selection.
5. **Rename** "My Dark" → "Night". The editor keeps using it, and
   `settings.json` still references the same `custom:<id>`.
6. **Delete** "Night" while it is in use → confirmation, then the editor
   switches to the dark default at once; after a restart it is still the
   default.
7. **Import .tmTheme…** with a theme downloaded from the web: it appears and
   can be assigned. Import a renamed `.png`: you get an error and nothing is
   added.
8. Open Settings → Manage themes → delete a theme in use → close the manager
   → **Cancel** in Settings. No slot points at the deleted theme, and
   FerrisPad does not crash on restart.

## Acceptance criteria

- [ ] Duplicate, rename, delete, import and per-color edit/reset work as
      described, with live preview and a real Cancel for color edits.
- [ ] Built-ins cannot be modified, both in the UI and in the model.
- [ ] Deleting a theme in use never leaves a dangling reference.
- [ ] Bad import files produce an error and create nothing.
- [ ] All tests listed above exist and pass.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [ ] The ticket records the manual check in light, dark and system theme
      modes.

## Affected files

- `src/ui/dialogs/theme_manager.rs` (new) — the window.
- `src/app/controllers/theme_manager.rs` (new) — `ThemeManagerModel` and
  its action enum.
- `src/ui/dialogs/settings_dialog.rs:117-140` — the "Manage themes…" button,
  and refreshing the light/dark choices afterwards (they list
  `SyntaxTheme::all()` today and must also list the custom themes).
- `src/app/state.rs` (`open_settings`) / `src/dispatch.rs:346` — re-theme
  and settings save on the returned actions.
- `tests/fixtures/import_ok.tmTheme`, `import_malformed.tmTheme`,
  `import_no_background.tmTheme` (new).

## Notes

- Origin: user request, GitHub issue #150 (see T0079 for the full split).
  Open a tracking issue when implementation starts.
- The window is modal and blocks the main loop, like the Settings dialog, so
  live previews are applied directly from inside it, the way
  `handle_settings` already does.
- Use the existing themed dialog helpers (`src/ui/dialogs/`), so the window
  follows the light/dark theme.
