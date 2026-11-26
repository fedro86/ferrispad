---
id: T0080
title: Theme manager window — duplicate, rename, delete, edit colors, import .tmTheme
status: done            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0079]     # other ticket IDs that must land first
issue: 159              # GitHub issue number
---

## Goal

T0079 makes custom editor themes possible: each one is a syntect `Theme`
saved in `<config>/ferrispad/themes/<id>.json`, managed by the UI-free
`custom_themes` service. Users still need a window to manage them. The
pattern follows JetBrains "Color Scheme": built-in themes are read-only, you
**Duplicate** one, then change the colors you care about.

## In scope

- A **"Manage themes…"** button in the Settings dialog, next to the light and
  dark syntax-theme choices. It opens a modal window that lists the built-in
  themes (marked read-only) and the custom ones. The window **looks like
  every other FerrisPad dialog**, built the same way as
  `src/ui/dialogs/plugin_manager.rs` (the closest sibling: a list plus action
  buttons):
  - `DialogTheme::from_theme_bg(theme_bg)` for every color: `bg`, `text`,
    `text_dim`, `input_bg`, `button_bg`, `secondary_button_bg`, `row_bg`,
    `accent`, scrollbars via `style_scroll`;
  - `theme.apply_titlebar(&dialog)` for the themed title bar on Windows and
    macOS, and the shared modal loop `run_dialog_with`;
  - secondary prompts through the existing themed helpers:
    `show_themed_choice` (delete confirmation), `show_themed_input` (name for
    Duplicate/Rename), `show_themed_message` (errors), and
    `native_open_dialog` (`src/ui/file_dialogs.rs`) for Import;
  - no hard-coded colors in the new dialog files.
- **Duplicate** (from any theme), **Rename**, **Delete** (with confirmation)
  and **Import .tmTheme…**, all through the T0079 service.
  - Import uses `ThemeSet::get_theme(path)`, which is already compiled in.
  - The imported theme becomes a custom theme with `based_on: null`.
- **Color editing** for a custom theme: background, text, caret, selection,
  line-number text, line-number background and current line. Each color has
  a swatch, a picker button and a "Reset to base" button. Reset is disabled
  when `based_on` is null.
  - **Not** FLTK's `dialog::color_chooser`: it opens a standalone, unthemed
    FLTK-grey window, and FerrisPad uses it nowhere today.
  - Instead, a small themed picker dialog that embeds the
    `fltk::group::ColorChooser` widget, adds a `#RRGGBB` hex input and
    OK/Cancel, and is styled with `DialogTheme` like the rest.
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
   - Missing colors use T0079's derived fallbacks, and a missing
     foreground/background is filled with them (T0079's
     `fill_default_colors`), so syntect never draws plain text black.
   - Duplicate names get " (2)".
   - **Scope-atom flood** (amendment after T0079's review): syntect interns
     scope atoms in a process-global repository before validating them, so
     the 1 MiB cap alone does not protect it. The file is parsed with
     `plist::Value` first (`plist` as a direct dependency at the version
     syntect already pulls in, so no new crate in the binary), every `scope`
     string goes through T0079's `ScopeAtomBudget` (the process-wide one),
     and only then is `ThemeSet::load_from_reader` called.
   - **Deeply nested / event-flood plist** (amendment after this ticket's
     review): a < 1 MiB file can nest 65 000 arrays, and dropping (or
     walking) such a `plist::Value` overflows the stack and aborts the app.
     Pipeline order: size cap → shape check (depth ≤ 32, ≤ 200 000 values,
     before any tree is built) → `plist::Value` + atom pre-scan → syntect.
5. **Picker cancelled.** Cancel or closing the picker dialog returns `None`,
   so no change is
   made.
6. **Editing the active theme vs. an inactive one.** Preview applies only
   when the edited theme is the one shown in the current mode. Editing an
   inactive theme does not touch the editor.
7. **Cancel after many edits / after Duplicate.** Cancel discards the color
   edits of this session. Theme-level actions (Duplicate, Rename, Delete,
   Import) are committed immediately and are not undone by Cancel. The
   window states this in a one-line hint.
8. **Preview leaks into dialog colors.** `apply_syntax_theme_colors` changes
   the FLTK globals `app::background` (scrollbar corner) and, through the
   main-window re-theme, `app::foreground`. Widgets in the open dialogs that
   rely on those globals (Choice popups, the color-chooser widget) would take
   on the *previewed* colors.
   - The manager window, the picker and the parent Settings dialog keep the
     colors they opened with.
   - After each preview, restore the globals the way
     `src/ui/dialogs/settings_dialog.rs:627` already does
     (`fltk::app::foreground(tr, tg, tb)`), extended to `app::background`.
   - The dialogs follow the new theme only the next time they are opened,
     consistent with Settings.
9. **Hex input is untrusted text.** Accept `#RRGGBB` / `RRGGBB` in any case,
   with surrounding whitespace trimmed. Reject everything else with an inline
   error and no change. Never slice the string by byte offsets before
   checking that it is ASCII: a multi-byte char would panic.
10. **Settings dialog Cancel.** The Settings dialog has its own preview and
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
- `theme_manager_import_missing_foreground_normalized` — an imported theme
  without `foreground` gets the effective text color as syntect's default
  foreground.
- `theme_manager_import_atom_flood_rejected` — in its own integration test
  file (`tests/theme_import_flood.rs`, own process): a `.tmTheme` with a
  66 000-atom scope (< 1 MiB) is an error, nothing is created, and
  `SyntaxSet::load_defaults_newlines()` plus a highlight still work.
- `theme_manager_hex_color_parse` — accepts `#1e1e1e`, `1E1E1E`,
  `  #AbCdEf  `; rejects `""`, `#`, `#12345`, `#1234567`, `#gggggg`,
  `rgb(1,2,3)`, `#ééé` and `#12345€` (multi-byte), all without panicking.

The window's appearance is FLTK rendering and cannot be unit-tested without
driving widgets. It is covered by manual steps 9–11 below.

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
9. **Look & feel.** In light and in dark mode, open Settings → Manage themes,
   and separately Plugins → General → Plugin Manager…. Background, text, buttons
   (primary and secondary), list rows, scrollbars and title bar match;
   nothing is FLTK grey. On Windows the title bar is themed; on macOS its
   color matches.
10. Open the color picker, the Duplicate/Rename name prompt, the delete
    confirmation, an import error and the Import file dialog. All of them are
    themed like the manager. Type `#zzz` in the hex field: an inline error and
    no change.
11. **No leak.** In dark mode, edit the background of the active custom theme
    to a very light color (live preview). The manager, the picker and the
    Settings dialog behind it keep their dark colors, including the text in
    the Choice popups. Close and reopen Manage themes: it now follows the new
    theme.

## Implementation (what was actually done)

- **`src/app/controllers/theme_manager.rs`** (new, no FLTK) —
  `ThemeManagerModel` + `ThemeManagerAction` (`Error`, `Select`,
  `ThemesChanged`, `SetSlot`, `SaveSettings`, `Retheme`).
  - Holds the `ThemeStore`, a working copy of edited themes' settings
    (`edits`), the *saved* light/dark slots and the theme the editor *shows*
    (a Settings preview may differ from the saved slot).
  - `duplicate` (keeps unsaved edits of the source), `rename`, `delete`,
    `import` are written at once; `set_color` / `reset_color` edit the working
    copy and return `ThemesChanged + Retheme` only when the edited theme is
    the shown one (edge 6); `ok` saves each edited theme atomically, `cancel`
    drops the working copy (file byte-identical).
  - Built-ins are rejected by the model (`Error`), not only greyed out.
  - Names: trimmed, not empty, ≤ 64 chars, unique case-insensitively
    (`validate_name`); imports get " (2)", " (3)"….
  - `delete`: saved slots that used the theme → mode default (`SetSlot` +
    `SaveSettings`); if the editor showed it → shows the mode default
    (`Retheme`).
  - `parse_hex_color`: trims, optional `#`, exactly 6 ASCII hex digits checked
    before any slicing (no byte-offset panic on multi-byte input).
- **Import** (`custom_themes::read_tm_theme`, spec amendment above): metadata
  size check (≤ 1 MiB) + capped read → `plist::Value` → every `scope` string
  through T0079's process-wide `ScopeAtomBudget::admit_tm_selectors`
  (syntect-mirroring selector splitter) with a bounded walk depth →
  `ThemeSet::load_from_reader` → `fill_default_colors`. `plist = "1.8"` is a
  direct dependency now; `Cargo.lock` only gains that edge (same 1.8.0 crate
  syntect already pulled in, no new crate in the binary).
- **`ThemeStore`** gained `add(name, based_on, theme)` (clone and import share
  it; it also fills a missing fg/bg), `set_settings`,
  `EditorColorField::set`, `base_colors`.
- **`src/ui/dialogs/theme_manager.rs`** (new) — the window, built like the
  Plugin Manager: `DialogTheme` colors only, `TopLevelScope`,
  `apply_titlebar`, `run_dialog_with`, themed scrollbars; list (built-ins
  marked "(built-in)", `@` formatting disabled) + Duplicate…/Rename…/Delete/
  Import .tmTheme…; seven color rows (swatch, hex, Pick…, Reset — Reset
  disabled without `based_on`); inline error line; the one-line hint about
  what Cancel undoes; OK / Cancel (Escape and the close button = Cancel).
  Prompts: `show_themed_input` (Duplicate/Rename), `show_themed_choice`
  (delete, Enter = Cancel), `show_themed_message` (import errors),
  `native_open_dialog` (Import).
- **`src/ui/dialogs/color_picker.rs`** (new) — themed picker embedding
  `fltk::group::ColorChooser`, a `#RRGGBB` field kept in sync both ways
  (invalid text → inline error, no change), OK / Cancel; `None` on Cancel,
  Escape or close (edge 5).
- **Edge 8 (no leak)**: the manager sets FLTK's foreground to its own text
  color while open, restores its foreground/background after every live
  preview, and gives the dialog behind it (Settings) its globals back when
  it closes; Settings keeps restoring its own foreground as before. If the
  manager was used and Settings is cancelled, `open_settings` re-applies the
  main window theme once so the globals match the editor again.
- **Settings dialog**: "Manage themes…" button under the dark choice. Its
  two callbacks became one `SettingsHook` (`Preview(ThemeRef)` /
  `ManageThemes`), since both need `AppState`. After the manager closes,
  both choices are rebuilt from the new custom list; a choice that used a
  deleted theme falls back to its mode default (edge 10).
- **`AppState`**: `manage_themes` loads the store, builds the model with the
  saved slots and `HighlightController::active_theme()` (what the editor
  really shows, a preview included), applies the actions (custom themes to
  the highlighter, slots + `settings.json`, re-theme through the same path as
  a Settings preview, panels included).

### Red run — import scope-atom flood

`tests/theme_import_flood.rs` (own binary) run with the plist pre-scan
disabled (temporary local change, reverted):

```
test theme_manager_import_atom_flood_rejected ... FAILED
thread 'theme_manager_import_atom_flood_rejected' panicked at tests/theme_import_flood.rs:65:9:
one.tmTheme: [ThemesChanged, Select(Custom("t-flood"))]
test result: FAILED. 0 passed; 1 failed
```

syntect itself silently drops the invalid rule and imports the theme — with
the 66 000 atoms already interned. Green with the pre-scan: both floods (one
66 000-atom scope; 66 000 one-atom scopes in one selector) are errors, nothing
is created, syntect still loads and highlights. The flood files are generated
by the test (~0.5 MB each) instead of a committed `import_atom_flood.tmTheme`
fixture.

### Review fix (PR #162) — deep nesting aborted the app

`plist::Value::from_reader` builds the tree iteratively, but dropping a
65 000-deep `Value` recurses once per level: a 975 KB `.tmTheme` of nested
`<array>`s aborted FerrisPad (stack overflow, SIGABRT), losing unsaved work.
Fix: `check_plist_shape` runs first and refuses depth > `MAX_PLIST_DEPTH` (32;
real themes need 4–6) and more than `MAX_PLIST_EVENTS` (200 000) values,
before any tree exists. It uses plist's stable serde API with a
`Deserialize` that only descends as far as the limit allows, so its own
stack use is bounded too; plist's raw event reader (`plist::stream`) is behind
an unstable feature that would require pinning `plist = "~1.8"`. XML and
binary plists alike.

Red first — `theme_manager_import_deep_nesting_rejected` (import on a 256 KiB
thread) against efa44d7:

```
running 2 tests
thread '<unknown>' has overflowed its stack
fatal runtime error: stack overflow, aborting
process didn't exit successfully: `.../theme_import_flood-...` (signal: 6, SIGABRT: process abort signal)
```

Green after the fix. Also `theme_manager_import_binary_plist_shape_rejected`
(binary plist nested 40 deep; binary flat array of 200 001 values). Both
assert the specific reason, so they cannot pass on syntect's own rejection.
The JSON theme loader (T0079) is not exposed: serde_json's recursion limit
(128) stops a deep file while parsing; pinned on ticket/T0079 by
`custom_theme_deeply_nested_json_rejected`.

### Bug found in user verification — debug-only fltk-rs assert (fixed in 2d80234)

Opening Settings → Manage themes… in a debug build (`cargo run`) panicked:
`fltk-1.5.14/src/browser.rs:120: assertion failed: c != 0 as char`. The list
called `set_format_char` with a NUL character to show names literally, and
fltk-rs `debug_assert!`s against it; release builds skip the check. No test
built the window, so gates and review missed it.

- Rows now use FLTK's documented `@.` escape ("print the rest of the line,
  no more `@` codes") via `literal_browser_label`; tabs become spaces. Rows
  are mapped to themes by index, never by their text.
- The theme manager and the color picker are built by the public
  `build_theme_manager` / `build_color_picker`, which tests call without
  showing the window. Tests: `literal_browser_label_disables_formatting`
  and the source guard `no_nul_browser_format_or_column_char_in_src` (lib),
  and `new_dialog_windows_build` in `tests/ui_dialogs.rs` (see below).
- Red: with the old call put back, the window-construction test failed with
  the same `assertion failed: c != 0 as char`, and the source guard named
  `theme_manager.rs`.
- Smoke: a debug `cargo run` with a temporary `XDG_CONFIG_HOME` (a custom
  theme in the dark slot, dark mode) started and ran 10 s without a panic.
  **Not done:** opening the new windows (Manage themes, color picker,
  prompts, delete confirmation, import dialog and its error) by hand —
  this session has no way to drive the GUI (Wayland, no input tools). The
  construction tests cover the widget setup of the manager and the picker;
  the prompts and the file dialog are the existing shared helpers. The
  interactive pass is part of the pending manual check.

### CI: the UI construction tests need a display

The first version of the window-construction tests ran in the lib unit
tests; building these windows opens the display (`center_screen`), and CI
has none: "Can't open display" made FLTK exit the whole lib test binary (PR
#162, run 45717834075, amd64 and arm64). Now:

- `tests/ui_dialogs.rs` holds them, in their own binary, as one test
  function (fltk-rs pins its "UI thread" to the first thread that creates a
  window), so a missing display can only fail that binary.
- Without `DISPLAY` / `WAYLAND_DISPLAY` the test is skipped with a note,
  unless **`FERRISPAD_REQUIRE_UI_TESTS=1`**, which turns a missing display
  into a failure ("UI tests required but no display").
- `.github/workflows/ci.yml`, both test jobs: install `xvfb`, set
  `FERRISPAD_REQUIRE_UI_TESTS=1`, run `xvfb-run -a cargo test --locked`.
  `build-deps-check.yml` only builds, so it is unchanged.
- The interim `ui::on_fltk_test_thread` helper is gone again; the existing
  `popup_is_top_level` test never opened the display and is back as it was.

Checked locally: with a display the test passes; with `DISPLAY` and
`WAYLAND_DISPLAY` unset it is skipped; unset plus
`FERRISPAD_REQUIRE_UI_TESTS=1` it fails as intended; the lib tests pass with
no display at all.

### Tests

`cargo test theme_manager`: `theme_manager_builtins_are_read_only`,
`theme_manager_delete_in_use_resets_slots`, `theme_manager_rename_validation`,
`theme_manager_cancel_discards_color_edits`,
`theme_manager_edit_inactive_theme_does_not_touch_editor`,
`theme_manager_ok_persists_color_edits`, `theme_manager_import_valid`,
`theme_manager_import_rejects_bad_files` (malformed plist, 2 MiB, binary,
a directory), `theme_manager_import_missing_colors_uses_fallback`,
`theme_manager_import_missing_foreground_normalized`,
`theme_manager_hex_color_parse`,
`theme_manager_duplicate_keeps_unsaved_edits_and_validates_name`; and
`cargo test --test theme_import_flood`. Fixtures:
`tests/fixtures/import_ok.tmTheme`, `import_malformed.tmTheme`,
`import_no_background.tmTheme`.

### Verification recipe

```bash
nix develop -c cargo test theme_manager
nix develop -c cargo test --test theme_import_flood
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Then manual steps 1–11 — results in *Manual check results* below.

## Manual check results — Xvfb/X11 (driven by Claude)

Debug build of the ticket/T0080 tip, Xvfb 1280×800, `FLTK_BACKEND=x11`,
`GDK_BACKEND=x11`, temporary `XDG_CONFIG_HOME`, driven with `xdotool`,
checked on `import` screenshots kept in `ferrispad-wt/uitest-T0080/`
(not committed). Window positions differ from a desktop (no window manager,
so no title bars).

1. **PASS** — Settings → **Manage themes…** opens (no panic in the debug
   build). The 7 built-ins are marked "(built-in)"; Rename, Delete, Pick…
   and Reset are disabled for them, with the read-only note
   (`80-01-manager.png`).
2. **PASS** — Ocean Dark → Duplicate…: themed prompt prefilled "Base16 Ocean
   Dark copy" (`80-02`); named "My Dark" → listed, selected, colors identical
   to Ocean Dark, `themes/t-my-dark.json` written (`80-03`); it appears in
   Settings' dark-theme choice (`80-04`).
3. **PASS** — "My Dark" chosen as the dark theme; in the manager the
   background set to `#f0e8d0` via the picker's hex field → the editor turns
   beige live (`80-09`). **Cancel** → previous colors back, theme file
   byte-identical (same md5) (`80-10`). Background `#401830`, selection
   `#c0a000`, current line `#503040` → **OK**, Settings **Save** → applied and
   on disk (`80-11`, `80-12`); after a restart with the same config they are
   still there (`80-13`).
4. **PASS** — Reset on Selection → `#468ce6` (Ocean Dark's T0078 selection),
   live (`80-14`).
5. **PASS** — Rename → "Night" (`80-15`, `80-16`): the file stays
   `t-my-dark.json`, its name field is "Night", the reference is unchanged.
   The unconfirmed Reset edit was not written by the rename (selection on
   disk still `#c0a000`).
6. **PASS** — Delete "Night" while in use → themed confirmation (Enter =
   Cancel) (`80-17`) → the file is gone, `settings.json` has
   `"syntax_theme_dark": "Base16OceanDark"`, the editor shows Ocean Dark at
   once (`80-18`).
7. **PASS (themed chooser)** — Import .tmTheme… with
   `tests/fixtures/import_ok.tmTheme` → "Imported OK" with the file's colors,
   Reset disabled, "no base theme" note (`80-20`); a renamed PNG → themed
   error "not a valid .tmTheme file (InvalidUtf8AsciiStream …)", nothing
   created (`80-21`). Here zenity was hidden from `PATH`, so FerrisPad's own
   themed FLTK chooser was used (`80-19`): zenity 4 (GTK4) asks the desktop
   portal and never appears inside Xvfb. **The zenity/GTK chooser on a real
   desktop is pending user verification.**
8. **PASS** — with "Imported OK" saved as the dark theme: Settings → Manage
   themes → Delete it → close the manager: the dark choice shows Ocean Dark
   (`80-23`) → **Cancel** in Settings: `settings.json` slot is
   `Base16OceanDark`, the editor shows Ocean Dark (`80-24`); restart is clean,
   no warnings (`80-25`).
9. **PASS (X11)** — dark (`80-01`) and light (`80-28`) next to the Plugin
   Manager (`80-27`): same background, text, primary/secondary buttons,
   scrollbars; nothing FLTK grey. The theme list uses `input_bg` with a
   `button_bg` selection, like the font and session pickers. Windows/macOS
   title bars: **pending user**.
10. **PASS** — color picker dark (`80-06`) and light (`80-29`), Duplicate and
    Rename prompts (`80-02`, `80-15`), delete confirmation (`80-17`), import
    chooser (`80-19`) and import error (`80-21`) are all themed. `#zzz` in the
    hex field → inline "Enter a color as #RRGGBB", no change (`80-08`).
11. **PASS** — dark mode, active custom theme's background → very light
    (`80-09`): the manager and the Settings dialog behind it keep their dark
    colors (the picker was dark too, `80-06`). Reopened afterwards, Manage
    themes follows the new theme (`80-14`). While the dialogs are open, the
    main window's menu-bar text stays the dialogs' light foreground on the
    previewed light menu — the edge-8 trade-off; it is right again once the
    dialogs close (`80-12`).

Found and fixed during this pass: the manager opened on the first list entry
instead of the theme the editor shows — it now preselects it (`80-32`;
asserted in `tests/ui_dialogs.rs`). Cosmetic, not changed: the
`ColorChooser`'s "rgb" mode selector keeps FLTK's lighter style; the
"Import .tmTheme…" label is tight in its button.

## Bug found in user verification (layout)

On native Wayland (GNOME, normal font scaling) the user saw the theme
manager's layout broken: "Import .tmTheme…" ran past its button's right
border, and "Line number bg" ran into the color swatch. All widths were
fixed pixel values tuned on Xvfb's default font; desktop fonts are wider.
The Xvfb pass had shown the Import label "tight" and it was wrongly filed as
cosmetic.

**Fix — layout from measured text.** `ui::dialogs::label_width` /
`max_label_width` measure a label with the fonts actually installed
(`fltk::draw::measure`), at the label size + `LABEL_SLACK_PT` (2 pt), plus
`LABEL_PAD` on each side. The theme manager (action buttons, list width,
field-label column, hex column, Pick…/Reset, OK/Cancel, hint) and the color
picker ("Hex:", the hex field, OK/Cancel, the dialog width) take every width
from it; the dialog width follows the content with a minimum. The Import
button keeps its full label and gained a tooltip. The Settings rows this
ticket added ("Manage themes…", the theme choices) already fit.

**Automated guard** — `tests/support/ui_layout.rs` (shared, reusable by any
UI test binary; T0083 uses it for the whole Settings dialog):
`label_overflows(window, title, bump_pt, only)` walks every visible widget of
a built window and requires each label drawn inside its widget to fit
(text + 2×4 px, plus the box of check/radio buttons; wrapped and clipped
labels skipped); `sibling_overlaps(window, title, only)` requires sibling
rectangles not to overlap; `layout_failures` runs both, the label check at
+0 and +2 pt. `tests/ui_dialogs.rs` applies it to the theme manager, the
picker (dark and light), the name prompt, and the Settings rows T0079/T0080
added (`settings_theme_rows`, asserted to select those 5 widgets). The rest
of the Settings dialog has pre-existing failures ("Show warning above (MB):"
at +0, "Change..." with DejaVu at +2) — ticket T0083. To make that test
possible, `show_settings_dialog` is split into `build_settings_dialog`
(builds without showing) + run, and `ui::dialogs::build_themed_input` builds
the name prompt.

Red — the guard against the previous layout (default Xvfb fonts):

```
theme manager/5:"Import .tmTheme…": label needs 137px at +0pt, widget is 120px
theme manager/32:"Line number bg": label needs 116px at +0pt, widget is 110px
theme manager/5:"Import .tmTheme…": label needs 155px at +2pt, widget is 120px
theme manager/27:"Line numbers": label needs 112px at +2pt, widget is 110px
theme manager/32:"Line number bg": label needs 128px at +2pt, widget is 110px
```

Green after the fix with the default fonts and with DejaVu Sans forced for
every sans family (`FONTCONFIG_FILE`), which is wider.

### Visual pass with a wide desktop font (Xvfb/X11, driven by Claude)

DejaVu Sans forced through fontconfig for sans/sans-serif/Helvetica, font
size setting 16 and 18, light and dark; screenshots in
`ferrispad-wt/uitest-T0080/` (not committed): `L-{Dark,Light}-{16,18}-{settings,manager,picker,prompt}.png`,
montages `montage-L-manager.png`, `montage-L-picker-prompt.png`,
`montage-L-settings.png`, zooms `L-zoom-buttons.png`, `L-zoom-rows.png`,
`L-zoom-bottom.png`, `L-zoom-picker.png`, `L-zoom-settings-theme-rows.png`.
Checked on each:

- **Theme manager** (768×480 with DejaVu): "Duplicate…", "Rename…",
  "Delete", "Import .tmTheme…" centred with clear margins on both sides;
  "Themes" and the list rows inside the list; field labels ("Line number
  bg" the longest) end well before the swatch; hex values inside their
  column, before "Pick…"; "Pick…" (tightest, ~4 px each side) and "Reset"
  inside their buttons; the two-line hint ends before OK; OK/Cancel inside.
- **Color picker** (300×250): "Hex:" ends before the field; the hex value
  inside the field; OK/Cancel centred with margins; FLTK's own value fields
  show the full numbers.
- **Name prompt**: prompt text, the prefilled name in the field, OK/Cancel
  inside with margins.
- **Settings** (the rows of this ticket): "Syntax Theme (Light):" /
  "(Dark):", both theme choices and "Manage themes…" inside their borders.

No text touched or crossed a border in any of the 16 screenshots. The
font-size setting changes the editor font only; dialog labels use FLTK's
label size, which the +2 pt check covers.

## Visual fixes after review — enabled buttons, swatch borders

1. **Enabled buttons looked disabled.** Cancel (manager and picker) and the
   enabled Reset buttons used the dialog's dimmed text color (`text_dim`),
   the same look as a disabled button. All dialog buttons now go through
   one helper, `ui::dialogs::style_dialog_button` (flat rounded,
   `button_bg`, label in `text` — exactly the Settings dialog's Cancel);
   FLTK dims a deactivated button's label by itself. Guard:
   `active_buttons_looking_disabled` (shared `tests/support/ui_layout.rs`),
   applied to the manager, both pickers and the name prompt. Red on the
   previous code:
   ```
   theme manager/11:"Reset": active button drawn in the dimmed text color
   theme manager/16:"Reset": active button drawn in the dimmed text color
   theme manager/21:"Reset": active button drawn in the dimmed text color
   theme manager/26:"Reset": active button drawn in the dimmed text color
   theme manager/31:"Reset": active button drawn in the dimmed text color
   theme manager/36:"Reset": active button drawn in the dimmed text color
   theme manager/41:"Reset": active button drawn in the dimmed text color
   theme manager/45:"Cancel": active button drawn in the dimmed text color
   picker dark/4:"Cancel": active button drawn in the dimmed text color
   picker light/4:"Cancel": active button drawn in the dimmed text color
   ```
2. **Swatches without a border.** A swatch close to the dialog background
   almost vanished (dark "Background" `#101820` on the dark dialog was
   1.21:1). Swatches now get a 1 px border in the dialog's `text_dim`
   (FLTK's own `BorderBox` border is black, invisible on dark themes, so the
   border is drawn by a small custom draw over a `BorderBox`); they carry the
   tooltip "Current color". Guard: `flat_widgets_with_tooltip` — all 7
   swatches must have a non-flat frame. Red with `FlatBox` put back: all 7
   reported "drawn without a border".

Screenshots (DejaVu, `ferrispad-wt/uitest-T0080/`): `montage-B-manager.png`
(dark and light), `montage-B-picker.png`, zooms `B-zoom-dark-rows.png`,
`B-zoom-light-rows.png`, and `B-dark-builtin-disabled-crop.png` (a built-in
selected: the deactivated Pick…/Reset/Rename/Delete are dimmed by FLTK,
while OK/Cancel/Duplicate/Import stay at full text color). Checked: Cancel
and Reset read like OK in both modes; every swatch shows its border, the
dark `#101820`/`#182028` ones and the white Caret one included.

## Acceptance criteria

- [x] Duplicate, rename, delete, import and per-color edit/reset work as
      described, with live preview and a real Cancel for color edits.
      (Model-tested; the window itself is manual steps 1–8.)
- [x] Built-ins cannot be modified, both in the UI and in the model.
- [x] The manager and all its sub-dialogs (picker, prompts, import) use
      `DialogTheme` and the shared helpers. They match the Plugin Manager and
      Settings in light and dark mode, contain no hard-coded colors, and are
      not repainted by the live preview. (Xvfb/X11 steps 9–11; Windows/macOS
      title bars pending user.)
- [x] Deleting a theme in use never leaves a dangling reference.
- [x] Bad import files produce an error and create nothing (scope-atom flood
      included).
- [x] All tests listed above exist and pass.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [x] The ticket records the manual check in light and dark mode (Xvfb/X11,
      driven by Claude).
- [x] System-default mode under a real GNOME appearance switch, a real
      Wayland compositor and the GTK/zenity file dialog (user, 2026-11-26).
- [x] Windows/macOS title bars (user, 2026-11-26, on 0.9.10-rc.2: Windows 11 and macOS).

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
- Consistent look is a requirement, not a preference: see the first In-scope
  bullet, edge case 8 and manual steps 9–11.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland), build `ab9ed45`
(T0077–T0083 together): every checklist step passed, including W6, W7 and P2
(manager, look after the layout fixes, portal import). Windows 11 and macOS
are optional and still to run on the 0.9.10-rc.1 release candidate.
