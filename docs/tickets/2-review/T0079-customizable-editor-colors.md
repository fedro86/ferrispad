---
id: T0079
title: Custom editor themes — syntect-based model, persistence and theme references
status: review          # todo | review | done
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

## Implementation (what was actually done)

- **`src/app/domain/settings.rs`** — `ThemeRef { Builtin(SyntaxTheme), Custom(id) }`
  for `syntax_theme_light` / `syntax_theme_dark`.
  - Serialized as the built-in's variant name (unchanged on-disk form, via
    `SyntaxTheme::setting_name`) or `"custom:<id>"`.
  - Read per field by `lenient_theme_ref` (`deserialize_with`): an untagged
    `String | IgnoredAny` buffer, so a wrong type, an unknown name or a
    malformed id (`is_valid_custom_theme_id`: `^t-[a-z0-9-]{1,64}$`) falls
    back to that slot's default with one stderr line, and the rest of the
    file is kept. `AppSettings::load` now goes through
    `AppSettings::from_json_or_default` (testable, same behaviour).
  - `default_syntax_theme(is_dark)` is the single mode-default rule.
- **`src/ui/theme.rs`** — `EditorColors` (background, text, caret, selection,
  line-number fg/bg, current line), `derived_editor_colors(bg, fg)` (the
  pre-T0079 derivation verbatim + T0078's selection + a derived current-line
  tint), and `effective_editor_colors(theme, is_builtin)`:
  - built-in → always the derivation (syntect's own faint `selection` is
    ignored);
  - custom → each `theme.settings` field it sets (translucent TextMate
    colors composited over the background), the derivation for the rest; no
    background → white, or (30,30,30) when the foreground is light, never
    black-on-black.
  - `apply_syntax_theme_colors(editor, &EditorColors)` applies them (and
    registers the selection with T0078's contrast function); `apply_theme`
    no longer touches the editor (its White/(30,30,30) values were always
    overwritten).
- **`src/app/services/custom_themes.rs`** (new, no FLTK) — `ThemeStore`:
  `load`, `list`, `get`, `clone_from(ThemeRef, name)`, `save`, `rename`,
  `delete`, `set_color(field, rgb)`, `reset_color(field)`; `EditorColorField`
  maps each editor color to its TextMate field (`background`, `foreground`,
  `caret`, `selection`, `gutter_foreground`, `gutter`, `line_highlight`).
  - Ids: `t-` + slug (`[a-z0-9-]`, ≤ 56 chars, `theme` when empty) + `-2`,
    `-3`… for uniqueness (in memory and on disk). Names: trimmed, ≤ 64 chars,
    `Custom theme` when empty.
  - `save`: atomic (`<id>.json.tmp` created with `create_new`, `sync_all`,
    `rename`). `save`/`delete` re-validate the id, canonicalize the dir and
    refuse any path not directly inside it; `delete` refuses non-regular
    files. Fail closed.
  - `load`: only `*.json`; skips (one warning each) invalid file-name ids,
    non-regular files (dirs, symlinks), files over 1 MiB (checked from
    metadata before reading, and the read is capped), bad JSON / wrong types,
    and `format != 1`.
  - `set_color` / `reset_color` work in memory (T0080's working copy);
    `clone_from`, `rename`, `delete` persist immediately.
- **`src/app/controllers/highlight.rs`** — holds the loaded custom themes and
  the active `EditorColors`; `set_theme(&ThemeRef, is_dark)` resolves a
  missing custom theme to the mode default at apply time (settings.json is
  not rewritten).
- **`src/app/services/syntax/mod.rs`** — custom themes are inserted into the
  `ThemeSet` as `custom:<id>` (on lazy load, or at once if already loaded);
  the highlighter now stores the active key + bg/fg instead of a
  `SyntaxTheme`.
- **`src/app/state.rs`** — loads `themes/` once in `AppState::new`, prints
  skip warnings, passes `&ThemeRef` everywhere a theme is set.
- **Settings dialog** — the light/dark choices are built from `ThemeRef`s:
  built-ins, then loaded custom themes; a slot pointing at a missing custom
  theme gets an extra `<id> (missing)` entry so Save never silently rewrites
  it. Preview/Cancel pass `ThemeRef`. Names are escaped for FLTK menus
  (`/`, `\`, `&`, `|`). (T0080 adds the "Manage themes…" flow.)
- `tests/fixtures/custom_theme_v1.json` — generated from a real clone of
  Base16 Ocean Dark (3 scopes kept, distinctive background/selection/gutter).

### Red first — `test_theme_ref_malformed_keeps_other_settings`

Run against the naive version (a strict `Deserialize` on `ThemeRef` that
errors on an invalid value, before `lenient_theme_ref` existed):

```
test test_theme_ref_malformed_keeps_other_settings ... FAILED
test test_theme_ref_backward_compat ... ok
test test_theme_ref_custom_roundtrip ... ok
thread 'test_theme_ref_malformed_keeps_other_settings' panicked at tests/settings_persistence.rs:210:9:
assertion `left == right` failed: font_size lost for slot value 42
  left: 16
 right: 23
test result: FAILED. 2 passed; 1 failed
```

(The whole file was rejected, so `font_size` reverted to the default 16.)

### Tests added

- `cargo test custom_theme` — `custom_theme_roundtrip`,
  `custom_theme_fixture_still_loads`, `custom_theme_bad_files_are_skipped`
  (garbage, wrong type, future format, 2 MiB, `.txt`, `.json.tmp`, a
  directory, a bad file name, a symlink on unix), `custom_theme_ids_are_safe`,
  `custom_theme_save_and_delete_refuse_unsafe_ids`,
  `custom_theme_rename_keeps_id`, `custom_theme_reset_color_restores_base`,
  `clone_looks_identical_to_source` (+ clone of a clone),
  `builtin_effective_colors_unchanged` (against a verbatim copy of the old
  derivation, and checks the hard-coded bg/fg equal syntect's),
  `effective_colors_fill_missing_fields`,
  `translucent_theme_colors_are_composited_over_the_background`, and in the
  highlight controller `custom_theme_missing_reference_falls_back_to_mode_default`,
  `custom_theme_loaded_reference_is_applied`.
- `cargo test --test settings_persistence theme_ref` —
  `test_theme_ref_backward_compat`, `test_theme_ref_custom_roundtrip`,
  `test_theme_ref_malformed_keeps_other_settings` (`42`, `"custom:../../x"`,
  `"custom:"`, `"custom:T-UPPER"`, unknown name, `null`, an object).

### Verification recipe

```bash
nix develop -c cargo test custom_theme
nix develop -c cargo test --test settings_persistence theme_ref
nix develop -c cargo test --test theme_scope_flood
nix develop -c cargo test --test scope_atom_mirror
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Then manual steps 1–4 below (**pending user verification** — not run by
the implementer: no display in this session).

### Review fixes (PR #158, first review)

1. **Blocking — scope-atom flood.** syntect interns every scope atom in a
   process-global repository (max 65 533 atoms, never freed) *before* it
   rejects a bad scope. A < 1 MiB theme file with one 66 000-atom scope was
   "skipped", but had already filled the repository, so syntect's own
   syntaxes panicked on the first highlight, on every launch. Fix: theme
   files are parsed as plain `serde_json::Value` first, and every string
   under `theme.scopes` goes through `ScopeAtomBudget` (own tokenizer, never
   touches syntect) before `serde_json::from_value::<ThemeFile>`:
   - ≤ 8 atoms per scope (syntect's limit, trailing dot ignored like syntect);
   - ≤ 2 048 new distinct atoms per theme;
   - ≤ 8 192 distinct atoms for all custom themes of the process
     (`with_process_atom_budget`, a process-wide budget like the repository it
     protects; T0080's import uses the same one). The built-in syntaxes and
     themes need a few thousand of the 65 533 atoms.
   Red first, `tests/theme_scope_flood.rs` (its own test binary, because the
   repository is process-global) against the previous loader:
   ```
   test custom_theme_atom_flood_rejected ... FAILED
   thread 'custom_theme_atom_flood_rejected' panicked at .../syntect-5.3.0/src/dumps.rs:202:91:
   called `Result::unwrap()` on an `Err` value: Custom("Invalid scope: TooManyAtoms")
   test result: FAILED. 0 passed; 1 failed
   ```
   Green after the fix. Unit tests: `custom_theme_scope_with_too_many_atoms_rejected`,
   `custom_theme_atom_budget_is_cumulative`, `scope_atom_budget_verdicts`.
2. **Missing foreground/background drawn black.** syntect paints unscoped
   text with `settings.foreground`, or black when absent. `fill_default_colors`
   now sets a missing foreground/background to the effective text/background
   at load (clones already fill every field). Test:
   `custom_theme_missing_foreground_normalized` (syntect's default style
   foreground == effective text, ≥ 4.5:1 on a dark background).
3. Nit — the temp file is `<id>.json.<pid>.tmp`, so two instances saving the
   same theme never share (or delete) each other's temp file.
4. Nit — a clone of a built-in takes the same colors "reset to base"
   restores (`derived_editor_colors` of the built-in), so reset == clone by
   construction.

### Review fixes (PR #158, second review)

1. **Blocking — the atom count under-counted.** The first budget split
   selector-style on whitespace, `,`, `|`… But in a JSON theme every string
   is ONE syntect `Scope` (`Scope::new` → `ScopeRepository::build`: trim,
   `trim_end_matches('.')`, `split('.')`; nothing else splits), so an atom
   like `"c a j b e"` was counted as letters, and 66 000 such atoms from a
   10-letter alphabet passed as 10. Fix: new `src/app/services/scope_atoms.rs`
   with two pure splitters, each citing the syntect 5.3.0 code it mirrors:
   - `scope_atoms(s)` — one scope string (JSON themes);
   - `tm_selector_scopes(s)` — a `.tmTheme` selector text (`,`/`|`, then
     `" -"`, then whitespace), each piece then through `scope_atoms` (used by
     T0080's import).
   The budget (`ScopeAtomBudget::admit_scopes` / `admit_tm_selectors`, the
   process-wide `with_process_atom_budget`) moved there too.
   Red first — the combinatorial flood added to `tests/theme_scope_flood.rs`,
   against 52bee3f:
   ```
   test custom_theme_atom_flood_rejected ... FAILED
   thread 'custom_theme_atom_flood_rejected' panicked at .../syntect-5.3.0/src/dumps.rs:202:91:
   called `Result::unwrap()` on an `Err` value: Custom("Invalid scope: TooManyAtoms")
   test result: FAILED. 0 passed; 1 failed
   ```
   Green after the fix (both floods rejected, syntect loads and highlights).
2. **Differential guard** — `tests/scope_atom_mirror.rs` (own binary): for
   tricky benign inputs (spaces inside atoms, surrounding whitespace,
   trailing dots, empty strings, commas, `|`, `&`, parentheses, `" -"`
   excludes, tabs/newlines) the atoms each splitter predicts equal the atoms
   syntect really interns. syntect exposes no atom count, so the test interns
   a fresh probe atom before and after each parse (`Scope::new(probe)
   .atom_at(0)`, atoms are numbered in insertion order) and compares the
   difference. A syntect upgrade that changes the splitting fails here.

### Deep-nesting guard (after T0080's review)

T0080's review found that dropping a deeply nested `plist::Value` overflows
the stack. The JSON theme loader is not exposed: it keeps serde_json's default
recursion limit (128; no `disable_recursion_limit`, no `serde_stacker`), so a
deeply nested file fails while parsing and no deep tree is ever built, walked
(`json_strings`) or dropped. Guarded by
`custom_theme_deeply_nested_json_rejected` (100 000 nested arrays, loaded on a
256 KiB thread: skipped with a "recursion limit" warning). No code change was
needed, so there is no red run.

### Notes from implementation

- `current_line` is defined here (theme `line_highlight`, else a derived
  tint: `lighten(0.07)` on dark, `darken(0.95)` on light backgrounds) but not
  rendered; T0081 owns rendering and may tune the tint. For built-ins the
  derived tint is used (built-ins ignore `theme.settings`, like the
  selection); T0081 should read `HighlightController::editor_colors()`.
- A custom theme's selection is the user's choice; selected text on it still
  goes through T0078's contrast function (registered in
  `apply_syntax_theme_colors`), so it stays readable.

## Acceptance criteria

- [x] Custom themes are syntect `Theme`s persisted via serde in
      `themes/<id>.json`. No second color model or parser exists.
- [x] Built-in themes look unchanged; clones look identical to their
      source. (Enforced by tests; visual confirmation is manual step 4.)
- [x] Malformed, oversized or foreign theme files and bad references never
      panic and never reset unrelated settings.
- [x] Ids are filesystem-safe on Linux, macOS and Windows; save and delete
      cannot leave `themes/`.
- [x] All tests listed above exist and pass; the malformed-reference test
      was seen failing against a naive implementation.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [ ] Manual check 1–4 — **pending user verification**.

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
