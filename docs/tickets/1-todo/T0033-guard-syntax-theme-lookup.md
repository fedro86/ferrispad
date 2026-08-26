---
id: T0033
title: Guard syntax theme lookup against a missing key
status: todo
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
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

Unit-test the highlighter constructed with a bogus theme name; assert it falls
back to the default rather than panicking.

- Before: panic on missing key.
- After: default theme used.

## Acceptance criteria

- [ ] A missing theme name yields a default, not a panic.
- [ ] Regression test added and green.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/syntax/mod.rs:302,330`,
  `src/app/services/syntax/highlighter.rs:28,72`.

## Notes

- Origin: plugin/services audit (MINOR, latent).
