---
id: T0033
title: Guard syntax theme lookup against a missing key
status: review
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
issue: 36
---

## Goal

Several sites index `theme_set.themes[&self.theme_name]`, which panics on a
missing key. All seven current `theme_key()` values exist in
`ThemeSet::load_defaults()`, so it's safe today — but it's a latent trap for
whoever adds theme #8 or a user-supplied theme name. Fail soft to a default
theme instead of panicking.

## In scope

- Replace the panicking index with a lookup that falls back to a known-present
  default theme (and logs) when the key is absent.

## Out of scope

- Adding new themes.

## How to test

### Regression test

```bash
nix develop -c cargo test --lib services::syntax::tests
```

Each test forces `theme_name = "no-such-theme"` on a `SyntaxHighlighter` and
drives one entry point; all must return styles covering the whole input:

- `highlight_full_with_missing_theme_falls_back`
- `highlight_incremental_with_missing_theme_falls_back`
- `chunked_highlight_with_missing_theme_falls_back` (covers `start_chunked`
  and `process_chunk`)

Plus three non-regression tests (green before and after in spirit — they
exercise the new helper and the theme list):

- `resolve_theme_prefers_the_requested_theme`
- `resolve_theme_on_empty_set_uses_builtin_default`
- `every_syntax_theme_key_is_in_the_default_theme_set` — the fallback is a
  safety net, not a way to hide a mistyped key: a new `SyntaxTheme` variant
  whose key is missing fails here.

Red run — **executed** with the three regression tests on the pre-fix code:

```
test result: FAILED. 0 passed; 3 failed
  highlight_full_...        panicked at highlighter.rs:28:34
  highlight_incremental_... panicked at highlighter.rs:72:34
  chunked_highlight_...     panicked at mod.rs:302:44
```

(`process_chunk`'s lookup at `mod.rs:330` is not reached pre-fix because
`start_chunked` panics first; post-fix the same test runs through it.)

Green: `5 + 1` syntax tests pass; full `cargo test`, `clippy -D warnings`,
`fmt --check` clean.

## What was done

- New `highlighter::resolve_theme(theme_set, name) -> &Theme`: the requested
  theme, else the `SyntaxTheme::default()` key, else any theme in the set, else
  syntect's `Theme::default()` (a static). One helper instead of four guarded
  call sites.
- The fallback logs once per process (`Once`): the lookup runs on every
  incremental highlight, i.e. every keystroke, so per-call logging would spam
  stderr.
- The four `themes[...]` index sites (`highlighter.rs` ×2, `mod.rs` ×2) now call
  `resolve_theme`.

## Acceptance criteria

- [x] A missing theme name yields a default, not a panic.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/syntax/mod.rs:302,330`,
  `src/app/services/syntax/highlighter.rs:28,72`.

## Notes

- Origin: plugin/services audit (MINOR, latent).
