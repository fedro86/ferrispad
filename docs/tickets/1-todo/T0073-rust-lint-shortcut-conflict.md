---
id: T0073
title: Resolve the Ctrl+Shift+R clash between Rust Lint and File → Reload All
status: todo            # todo | review | done
created: 2026-11-07
severity: minor
area: ui
depends-on: []
issue: ~
---

## Goal

The official Rust Lint plugin (v1.5.0) declares `shortcut = "Ctrl+Shift+R"`
for its "Run All Checks" item, but that key is already bound to the core
**File → Reload All** (`src/ui/menu.rs:53`, added in d86870f). On every
startup FerrisPad skips the plugin binding and logs:

```
[plugins] Warning: shortcut 'Ctrl+Shift+R' for Rust Lint/Run All Checks conflicts with existing shortcut, skipping
```

So the shortcut the plugin advertises never works, and a user reading the
plugin README gets nothing on Ctrl+Shift+R except a full reload. Core's
conflict handling (core wins, plugin binding skipped) is correct; the defect
is the shipped manifest.

## In scope

- Pick a free shortcut for Rust Lint "Run All Checks" (or drop it — the core
  **Plugins → General → Run All Checks** is already on `Ctrl+Shift+L` and
  covers the same use) and update `rust-lint/plugin.toml` + README in the
  `fedro86/ferrispad-plugins` repo, bumping the plugin version.
- Check every other official plugin's declared shortcut against the core
  bindings in `src/ui/menu.rs` the same way (today: `Ctrl+Shift+Y/I/E/P/D`).
- If a check in this repo is cheap: a unit test that loads each official
  manifest shortcut against the core shortcut table and fails on a clash, so
  the next collision is caught here rather than at runtime.

## Out of scope

- Changing core's conflict policy (core keeps priority over plugins).
- Changing the core File → Reload All shortcut.
- User shortcut overrides (`shortcut_overrides` in settings) — unaffected.

## How to test

### Regression test

If the manifest-vs-core test is added:

```bash
nix develop -c cargo test plugin_shortcut_conflicts
```

- Before the fix (fixture = current rust-lint manifest, `Ctrl+Shift+R`): FAILS
  naming `Rust Lint/Run All Checks` vs `File/Reload All`.
- After: passes.

### Manual repro

1. With Rust Lint v1.5.0 installed: `nix develop -c cargo run 2>&1 | grep shortcut`
   → the warning above is printed (before).
2. Update Rust Lint to the fixed version (Plugin Manager → Official → update).
3. Relaunch: no conflict warning; the new shortcut (if any) appears next to
   **Plugins → Rust Lint → Run All Checks** and runs the checks on a `.rs` file;
   `Ctrl+Shift+R` still does File → Reload All.

## Acceptance criteria

- [ ] No startup shortcut-conflict warning with all official plugins installed.
- [ ] Rust Lint "Run All Checks" is reachable by its advertised shortcut (or
      advertises none), and the README matches.
- [ ] File → Reload All keeps `Ctrl+Shift+R`.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `fedro86/ferrispad-plugins`: `rust-lint/plugin.toml`, `rust-lint/README.md`
  (+ registry entry / version bump as that repo requires)
- `src/ui/menu.rs:53` — core binding (read-only reference; test fixture if added)

## Notes

- Origin: observed in the startup log while manually verifying T0072
  (2026-11-07). Pre-existing, unrelated to the dependency bumps.
- Ctrl+Shift+R for Reload All landed in d86870f; Rust Lint's manifest
  predates or ignored it.
