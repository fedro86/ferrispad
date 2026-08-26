---
id: T0034
title: Use the semver crate for registry version comparison
status: todo
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
---

## Goal

`plugin_registry.rs::is_update_available` hand-rolls version comparison with
`split('.').filter_map(parse)`, which silently drops prerelease components — so
`1.0.0-rc1` compares equal to `1.0`, and a prerelease can be offered as an
update to itself or mask a real one. The `semver` crate is already a dependency
(used by `updater.rs`); use it here too.

## In scope

- Replace the hand-rolled comparison with `semver::Version` parsing/compare,
  matching how `updater.rs:59-60` does it.

## Out of scope

- The registry install-path and size-limit issues (T0007, T0035).

## How to test

### Regression test

`tests/plugin_registry_fetch.rs` (or unit): assert `1.0.0-rc1 < 1.0.0`,
`1.0.1 > 1.0.0`, and that a prerelease isn't treated as newer than its release.

- Before: prerelease dropped → wrong comparisons.
- After: semver-correct ordering.

## Acceptance criteria

- [ ] Version comparison uses `semver` and handles prerelease correctly.
- [ ] Regression test added and green.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_registry.rs:726-745` — `is_update_available`.

## Notes

- Origin: plugin/services audit (MINOR, accretion — two version comparators in
  one codebase).
