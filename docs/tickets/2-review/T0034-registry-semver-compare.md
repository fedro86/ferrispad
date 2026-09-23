---
id: T0034
title: Use the semver crate for registry version comparison
status: review
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
issue: 38
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

```bash
nix develop -c cargo test --lib plugin_registry::tests
```

- `release_is_an_update_over_its_prerelease` — `1.0.0-rc1 → 1.0.0`,
  `1.0.0-rc1 → 1.0.0-rc2`, `1.0-beta → 1.0.0` are updates.
- `prerelease_is_not_an_update_over_its_release` — `1.0.0 → 1.0.0-rc1`,
  `rc2 → rc1`, `rc1 → rc1` are not.
- `unparseable_version_is_never_an_update` — `"latest"`, `"garbage"`, `""`
  fail closed.
- Pre-existing `test_is_update_available` and
  `test_is_update_available_partial_versions` (`"1"`, `"1.0"`) stay green.

Red run — **executed** on the pre-fix comparator:

```
test result: FAILED. 17 passed; 2 failed
  release_is_an_update_over_its_prerelease
      assertion failed: is_update_available("1.0.0-rc1", "1.0.0")
  unparseable_version_is_never_an_update
      assertion failed: !is_update_available("garbage", "1.0.0")
```

(`prerelease_is_not_an_update_over_its_release` was already green: dropping the
prerelease made those pairs compare *equal*, which happens to answer "no".)

Green: 19/19 in `plugin_registry::tests`; full `cargo test`,
`clippy -D warnings`, `fmt --check` clean.

## What was done

- `is_update_available` now parses both sides with `semver::Version` and
  compares them, like `updater::is_newer_version`.
- New private `parse_plugin_version`: strict semver first; otherwise pads a
  1- or 2-component core with `.0` (keeping any `-pre`/`+build` suffix) so the
  short forms the old comparator accepted (`"1"`, `"1.0"`) still work.
- **Behaviour change (intended): fails closed.** A version that is not semver
  even after padding (`"latest"`, `"v1.0"`, `""`) now never produces an update
  offer. Before, such strings were compared on whatever numeric pieces
  survived (`"garbage"` → `[]`, so *anything* looked newer). All versions in
  the repo fixtures and in the locally installed plugins are full `X.Y.Z`.

## Acceptance criteria

- [x] Version comparison uses `semver` and handles prerelease correctly.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_registry.rs:726-745` — `is_update_available`.

## Notes

- Origin: plugin/services audit (MINOR, accretion — two version comparators in
  one codebase).
