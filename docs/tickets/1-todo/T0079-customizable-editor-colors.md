---
id: T0079
title: Let users override editor background, text, selection and line-number colors
status: todo            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0078]     # other ticket IDs that must land first
issue: 150              # GitHub issue number
---

## Goal

Editor colors come only from the theme (light, dark or system) and from seven
fixed syntax themes in `src/app/domain/settings.rs`; users cannot choose their
own. Users want to pick the background, text, selection and line colors
themselves. This ticket adds that on top of T0078, which makes the default
selection high-contrast.

## In scope

- A colors section in the Settings dialog with pickers for editor
  **background**, **text**, **selection** and **line-number** colors.
- Store the chosen colors in `settings.json` as optional theme overrides
  (unset = use the theme value), with `#[serde(default)]` so older files load.
- Apply the overrides on top of the active theme's colors wherever the editor
  is themed, immediately when changed and again at startup.
- A **Reset** button that removes the overrides and restores the theme
  colors.

## Out of scope

- The high-contrast default selection color itself — delivered by T0078.
- A full syntax-theme editor and importing external themes.
- Colors of non-editor widgets (tab bar, menus, tree panel, dialogs).

## How to test

### Unit / integration tests

```bash
nix develop -c cargo test --test settings_persistence color_overrides
```

- `test_color_overrides_roundtrip` (in `tests/settings_persistence.rs`): set
  all four overrides on `AppSettings`, serialize, deserialize, assert they are
  identical; then set only one override and assert the other three stay
  unset after the roundtrip.
- `test_color_overrides_backward_compat` (same file): deserialize a
  `settings.json` string with none of the new fields; it loads without error
  and every override is unset.
- A unit test next to the merge logic (e.g.
  `color_overrides_take_precedence_over_theme`): given theme colors and a
  partial override set, the effective colors use the override where set and
  the theme value elsewhere; with all overrides cleared (the Reset path) the
  result equals the theme colors exactly.

All pass after the change; the full `nix develop -c cargo test` stays green.

### Manual check (record results here)

1. `nix develop -c cargo run`, open a text file with several lines.
2. Settings → colors section: set background, text, selection and
   line-number colors to distinctive values (e.g. dark green, yellow, orange,
   magenta). Expect: the editor shows all four immediately, without restart.
3. Close FerrisPad and run it again. Expect: the custom colors are still
   applied; `~/.config/ferrispad/settings.json` contains the four override
   values.
4. Switch Settings → Theme between **Light**, **Dark** and **System
   default**. Expect: the overrides stay applied on each; unset colors follow
   the theme.
5. Press **Reset**. Expect: the editor returns to the theme colors at once
   (including T0078's selection color), and after a restart the overrides are
   gone from `settings.json`.
6. Restore a `settings.json` from before this change (no override fields) and
   run. Expect: it loads with no error and the theme colors are used.

## Acceptance criteria

- [ ] Custom colors apply immediately and persist across restarts.
- [ ] Reset removes the overrides and restores the theme colors.
- [ ] A `settings.json` without the new fields loads without errors.
- [ ] A test covers saving and loading the overrides.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features` and
      `cargo fmt --check` pass with zero warnings.
- [ ] The ticket documents the manual check with `cargo run` on the light,
      dark and system themes.

## Affected files

- `src/app/domain/settings.rs:176` — `AppSettings`: optional color-override
  fields (e.g. `Option<(u8, u8, u8)>` or a defaulted nested struct).
- `src/ui/dialogs/settings_dialog.rs:32` — `show_settings_dialog`: new colors
  section with pickers and the Reset button (FLTK's color chooser dialog).
- `src/ui/theme.rs:83`, `:146` — `apply_syntax_theme_colors` / `apply_theme`:
  apply overrides after the theme colors (the editor background, text,
  selection and line-number color setters live here).
- `src/app/state.rs:843` — `apply_theme_to_owned_widgets`: pass the overrides
  through.
- `src/dispatch.rs:346` — `Message::OpenSettings`: persist the dialog's
  overrides and re-apply the theme.
- `tests/settings_persistence.rs` — roundtrip and backward-compat tests.

## Notes

- Origin: user request, GitHub issue #150.
- Issue #150 is shared with T0078 (high-contrast default selection): the
  request exceeded the ~5-bullet ticket scope, so it was split into T0078 and
  this ticket, which depends on T0078 landing first.
- Override values from `settings.json` are untrusted input: malformed or
  out-of-range values must be ignored (fall back to the theme), never panic.
- Open question for implementation: whether overrides are global, or kept
  separately per light/dark mode. The issue asks for global theme overrides;
  start there unless review decides otherwise.
