---
id: T0079
title: Custom editor themes — syntect-based model, persistence and theme references
status: todo            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0078]     # other ticket IDs that must land first
issue: 150              # GitHub issue number
---

## Goal

Users want to choose their own editor colors (issue #150): background, text,
selection and line colors. The established pattern (JetBrains "Color Scheme →
Duplicate") is that built-in themes are read-only: you **clone** one, change
some colors, and keep the rest. This ticket builds the model and persistence
for such custom themes. The management window is T0080; the current-line
highlight is T0081.

**Single model, no FerrisPad-specific format.** Editor themes are already
`syntect::highlighting::Theme` values (`ThemeSet::load_defaults()` in
`src/app/services/syntax/mod.rs:93`). That type:

- already implements `Serialize`/`Deserialize`, so it is saved and loaded with
  `serde_json` without any hand-written conversion;
- already carries the standard TextMate editor colors in `theme.settings`:
  `background`, `foreground`, `caret`, `selection`, `gutter`,
  `gutter_foreground` and `line_highlight`.

`resolve_theme` (`src/app/services/syntax/highlighter.rs:30`) looks themes up
by name with a soft fallback. A custom theme is therefore just one more
`Theme` inserted into the same `ThemeSet` under its own key. No second color
model, parser or writer is written.

## In scope

- **File format and location.** One file per custom theme at
  `<config>/ferrispad/themes/<id>.json`, with the shape
  `{ "format": 1, "name": "...", "based_on": "<builtin key>" | null, "theme": <syntect Theme> }`.
  - `based_on` lets a single color be reset to the original.
  - `format` allows a future migration.
  - Themes are loaded once at startup into the shared `ThemeSet` as
    `custom:<id>`. No directory watching (0% CPU idle rule).
- **Theme references in settings.** `syntax_theme_light` /
  `syntax_theme_dark` can point to a built-in or a custom theme.
  - Old values (`"Base16OceanDark"`, …) keep loading as built-ins; custom
    themes are stored as `"custom:<id>"`.
  - A malformed value falls back to the mode default *without* rejecting the
    rest of `settings.json`.
  - A reference to a missing custom theme falls back to the mode default.
- **Effective editor colors.** One pure function
  `effective_editor_colors(theme, is_builtin) -> EditorColors` returns
  background, text, caret, selection, line-number fg/bg and current-line
  color. `apply_syntax_theme_colors` / `apply_theme` use it instead of the
  hard-coded values in `src/ui/theme.rs`.
  - **Built-in themes keep exactly today's look**: same derived text,
    line-number and caret colors, and T0078's selection.
  - Custom themes use their `theme.settings` fields; any field left unset
    uses the same derivation.
- **Theme operations as a UI-free service** (`src/app/services/custom_themes.rs`):
  `list`, `clone_from(key, name)`, `save`, `rename`, `delete`,
  `set_color(field, rgb)` and `reset_color(field)`.
  - A clone has every editor field filled with its source's **effective**
    colors, so it looks identical to what the user was seeing ("what you see
    is what you clone").
  - `save` writes atomically (temp file + rename).
- **Untrusted input.** Theme files can be shared between users and
  hand-edited, so loading must cope with bad input — see *Edge cases*.

## Out of scope

- The management window, color pickers, import UI — T0080.
- Current-line highlighting itself — T0081. This ticket only defines where
  its color comes from.
- Exporting `.tmTheme` (syntect has no plist writer).
- Editing syntax token colors (`theme.scopes`). Clones copy them untouched.
- Non-editor widgets (tab bar, menus, tree panel, dialogs).

## Edge cases (each one is covered by a test below)

1. **syntect format drift.** The serde shape of `syntect::Theme` is not a
   documented stable file format; a syntect upgrade could change it.
   - A committed fixture file must keep loading, so a breaking bump fails CI.
   - At runtime an unreadable theme is skipped and its references fall back;
     there is never a panic.
2. **Malformed, oversized or foreign files** in `themes/`: invalid JSON, wrong
   types, more than 1 MiB, non-`.json` files, directories, symlinks.
   - Each bad file is skipped, with one stderr line naming it; the others
     still load.
   - Files larger than the cap are not even read into memory.
3. **Ids are file names, so they must be safe.**
   - An id is generated once at creation from a slug of the name: `[a-z0-9-]`,
     length-capped, unique (`-2`, `-3`…), never empty.
   - The display name is free text; renaming never changes the id.
   - The id must never contain `/`, `\`, `..` or a Windows reserved name
     (`CON`, `NUL`, `COM1`…), so prefix it (e.g. `t-`).
   - `save` and `delete` canonicalize and refuse any path outside `themes/`
     (fail closed).
4. **Missing pieces in a theme.** A file without `background`/`foreground`
   (common in imported themes) falls back to the derived colors, never to
   black-on-black.
5. **Themes in use.** A light/dark slot that references a deleted or
   unloadable custom theme falls back to that mode's default built-in. The
   fallback is applied at load time; `settings.json` is not rewritten
   silently.
6. **Two instances.** Theme files are separate from `settings.json`, so
   editing a theme in one instance never reverts settings in another. For the
   same theme file, the last write wins (acceptable, documented).

## How to test

### Unit / integration tests

```bash
nix develop -c cargo test custom_theme
nix develop -c cargo test --test settings_persistence theme_ref
```

Service tests (tempdir as the themes dir; no FLTK):

- `custom_theme_roundtrip` — clone a built-in, change `selection` and
  `gutter`, save, load from disk: equal, including `theme.scopes`.
- `custom_theme_fixture_still_loads` — loads the committed
  `tests/fixtures/custom_theme_v1.json`; the asserted colors match.
- `custom_theme_bad_files_are_skipped` — the dir holds one valid theme, a
  garbage file, a wrong-typed field, a 2 MiB file, a `.txt` and a
  subdirectory: exactly the valid theme loads, with no panic.
- `custom_theme_ids_are_safe` — names `"../../x"`, `"a/b"`, `"a\\b"`,
  `"CON"`, `""`, `"   "`, a 500-char name, an emoji-only name and two equal
  names: every id matches `^t-[a-z0-9-]{1,64}$`, all ids are unique, and after
  saving them the tempdir holds only those files.
- `custom_theme_rename_keeps_id` — rename changes the name only; the file and
  the `custom:<id>` reference are unchanged.
- `custom_theme_reset_color_restores_base` — `set_color` then `reset_color`
  equals the `based_on` effective color. With `based_on: null`, reset is a
  no-op returning an error, not a panic.
- `clone_looks_identical_to_source` — for each of the 7 built-ins,
  `effective_editor_colors(clone) == effective_editor_colors(source)`.
- `builtin_effective_colors_unchanged` — for each of the 7 built-ins the
  effective text/caret/line-number colors equal today's hard-coded
  derivation (regression guard). The selection equals T0078's function.
- `effective_colors_fill_missing_fields` — a theme with no `settings` colors
  yields readable, derived colors (text vs background ≥ 4.5:1).

In `tests/settings_persistence.rs`:

- `test_theme_ref_backward_compat` — an old `settings.json` with
  `"syntax_theme_dark": "SolarizedDark"` loads as that built-in.
- `test_theme_ref_custom_roundtrip` — `custom:t-my-theme` survives a
  serialize/deserialize roundtrip.
- `test_theme_ref_malformed_keeps_other_settings` — `"syntax_theme_dark": 42`
  or `"custom:../../x"` → mode default, **and a non-default `font_size` in the
  same file is preserved**. This must fail against a naive enum change.

All pass; the full `nix develop -c cargo test` stays green.

### Manual check (record results here)

There is no UI yet (T0080), so test with hand-made files:

1. Copy `tests/fixtures/custom_theme_v1.json` to
   `~/.config/ferrispad/themes/t-test.json` and change `background` /
   `selection` / `gutter`. Set `"syntax_theme_dark": "custom:t-test"` in
   `settings.json`, then `nix develop -c cargo run` in dark mode. The editor
   uses those colors.
2. Corrupt `t-test.json` (truncate it) and run again. FerrisPad starts with
   the dark default theme, prints one stderr line, and keeps all other
   settings.
3. Delete the file and run again. Same fallback; `settings.json` is
   untouched.
4. With no custom themes, run and compare the 7 built-ins with the previous
   release. They look identical, except for the T0078 selection.

## Acceptance criteria

- [ ] Custom themes are syntect `Theme`s persisted via serde in
      `themes/<id>.json`. No second color model or parser exists.
- [ ] Built-in themes look unchanged; clones look identical to their
      source.
- [ ] Malformed, oversized or foreign theme files and bad references never
      panic and never reset unrelated settings.
- [ ] Ids are filesystem-safe on Linux, macOS and Windows; save and delete
      cannot leave `themes/`.
- [ ] All tests listed above exist and pass; the malformed-reference test
      was seen failing against a naive implementation.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.

## Affected files

- `src/app/services/custom_themes.rs` (new) — file I/O, ids, clone/rename/
  delete/set/reset, effective-color fill on clone.
- `src/app/services/syntax/mod.rs:93` — insert custom themes into the
  `ThemeSet` after `load_defaults()`.
- `src/app/domain/settings.rs:97`, `:217-222` — `SyntaxTheme` slots become
  a built-in-or-custom reference with a lenient, backward-compatible
  deserializer.
- `src/ui/theme.rs:83`, `:146` — use `effective_editor_colors` instead of the
  hard-coded line-number/text/caret values.
- `src/app/state.rs:843-863` — `apply_theme_to_owned_widgets` passes the
  resolved `Theme`.
- `tests/fixtures/custom_theme_v1.json` (new), `tests/settings_persistence.rs`.

## Notes

- Origin: user request, GitHub issue #150. The issue is split into T0078
  (high-contrast default selection), T0079 (this ticket), T0080 (theme
  manager window) and T0081 (current-line highlight).
- Reading `.tmTheme` needs no new dependency: syntect's `plist-load` feature
  is already enabled through `default-onig` (`ThemeSet::get_theme`). It is
  used by T0080's import.
- For a built-in theme, the selection is always T0078's color, even though
  syntect's built-ins define a (low-contrast) `selection`. A custom theme's
  `selection` is the user's choice and is not contrast-checked; FLTK still
  applies `fl_contrast` to selected text.
- Settings dialog's Cancel and preview logic must handle `custom:` keys too;
  this is covered by T0080's manual check.
