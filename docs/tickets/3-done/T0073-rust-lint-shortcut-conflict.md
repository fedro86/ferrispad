---
id: T0073
title: Resolve the Ctrl+Shift+R clash between Rust Lint and File → Reload All
status: done            # todo | review | done
created: 2026-11-07
severity: minor
area: ui
depends-on: []
issue: 207
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

## What was done

- **Decision (the user, via the orchestrator): drop the shortcut.**
  - Rust Lint 1.5.1 declares no shortcut for "Run All Checks".
  - Core Ctrl+Shift+L (**Plugins > General > Run All Checks**) fires
    `on_highlight_request`. In rust-lint that calls the same
    `run_all_checks` as the menu action, so nothing is lost.
  - A dedicated key can be assigned in **Edit > Key Shortcuts**.
- **Plugin repo:** [fedro86/ferrispad-plugins#1](https://github.com/fedro86/ferrispad-plugins/pull/1),
  branch `fix/rust-lint-shortcut` (`adb75f5`):
  - `rust-lint/plugin.toml`: the shortcut is removed; version 1.5.0 → 1.5.1.
  - `rust-lint/README.md`: the menu action points to Ctrl+Shift+L and
    Edit > Key Shortcuts.
  - `rust-lint/CHANGELOG.md`: a `[1.5.1]` "Fixed" entry.
  - `init.lua` and `plugins.json` are untouched.
  - That repo has no tags or releases. A merge to `main` runs
    `sign-plugins.yml`, which signs the plugin and rewrites its
    `plugins.json` entry (version, checksums, signature). **That merge
    publishes the update**, so it waits for the user's explicit OK.
- **Other official manifests:** file-explorer Ctrl+Shift+E, git-diff
  Ctrl+Shift+D, python-lint Ctrl+Shift+P, yaml-json-viewer Ctrl+Shift+Y.
  None of them clashes. No current manifest declares Ctrl+Shift+I.
- **This repo, guard tests in `src/ui/menu.rs`:**
  - `plugin_shortcut_conflicts` binds every menu-item shortcut of the
    official manifests through the runtime's own `PluginShortcutBinder`
    (reserved core keys, then earlier items). It fails on a clash, naming
    the item and the holder of the key.
  - The manifests are verbatim copies in `tests/fixtures/official_plugins/`.
    Its README says which commit they come from and how to refresh them.
  - `builtin_shortcuts_are_reserved`: the binder keeps plugins off core keys
    through `RESERVED_SHORTCUTS`, a list kept by hand next to
    `BUILTIN_SHORTCUTS`. A core default missing there would let a plugin
    take it silently. This test checks every core default, in both tab
    modes, against it. It passes today.

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

- Before the fix (fixture = the rust-lint 1.5.0 manifest from
  `cce6006`): FAILS, recorded:
  ```text
  thread 'ui::menu::tests::plugin_shortcut_conflicts' panicked at src/ui/menu.rs:909:9:
  Rust Lint/Run All Checks (Ctrl+Shift+R) clashes with File/Reload All
  ```
- After (fixture = the 1.5.1 manifest from ferrispad-plugins#1): passes.
- `builtin_shortcuts_are_reserved` passes before and after; it guards
  against future drift.

### Startup check (done, Xvfb, debug build)

All five official plugins were installed into a private config dir.

| rust-lint | startup log |
|---|---|
| 1.5.0 (`main`) | `[plugins] Warning: shortcut 'Ctrl+Shift+R' for Rust Lint/Run All Checks conflicts with existing shortcut, skipping` |
| 1.5.1 (PR #1) | no shortcut warning; all five plugins load |

The screenshots with 1.5.1 show:
- **Plugins > Rust Lint**: Enable, Settings..., Run All Checks (no
  shortcut shown), Run Clippy Only, Run Build Only.
- **File > Reload All**: still Ctrl+Shift+R.

Nothing is clipped in either menu.

### After the master merge (2026-11-27)

Master (up to T0111, #213) is merged in without conflicts. Both tests are
plain unit tests: they parse TOML and bind through `PluginShortcutBinder`,
with no FLTK object, so the T0113 guard passes. The five fixtures are
still byte-identical to the plugin repo: `rust-lint` to PR #1's head
`adb75f5`, the other four to `main`. Gates: 758 passed, clippy
`-D warnings` and fmt clean.

### Manual repro

1. With Rust Lint v1.5.0 installed: `nix develop -c cargo run 2>&1 | grep shortcut`
   → the warning above is printed (before).
2. Update Rust Lint to the fixed version (Plugin Manager → Official → update).
3. Relaunch: no conflict warning; the new shortcut (if any) appears next to
   **Plugins → Rust Lint → Run All Checks** and runs the checks on a `.rs` file;
   `Ctrl+Shift+R` still does File → Reload All.

## Acceptance criteria

- [x] No startup shortcut-conflict warning with all official plugins
      installed (with Rust Lint 1.5.1 from ferrispad-plugins#1).
- [x] Rust Lint "Run All Checks" advertises no shortcut, and the README
      matches.
- [x] File → Reload All keeps `Ctrl+Shift+R`.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [x] ferrispad-plugins#1 merged (publishes 1.5.1); user's OK given.
- [x] User verification (2026-11-27).

## User verification

The user approved on 2026-11-27 ("ok pubblica rust lint 1.5.1 e poi 209"),
relayed by the orchestrator (ferrispad-03), after reading the presentation
`docs/pr/0.9.11/PR-209-rust-lint-shortcut-conflict.html`. The reviewer
(ferrispad-90) approved 81943be. Its mutation probes made both tests fail
as expected: Ctrl+Shift+R put back, and `ctrl+w` dropped from
`RESERVED_SHORTCUTS`.

Rust Lint 1.5.1 is published:
- fedro86/ferrispad-plugins#1 was merged to `main` as 017442d;
- the auto-sign bot then committed fd7a6a4 (`plugins.json` only). The
  registry lists rust-lint 1.5.1, signed.

The five fixtures were checked again against `main` at fd7a6a4: all are
byte-identical. Before the move, master was merged again (T0112 #215;
2ff533c). Gates: 786 passed, with the UI tests on a private Xvfb; clippy
`-D warnings` and fmt clean.

## Affected files

- `fedro86/ferrispad-plugins`: `rust-lint/plugin.toml`, `rust-lint/README.md`
  (+ registry entry / version bump as that repo requires)
- `src/ui/menu.rs` — the core binding is unchanged; two new tests:
  `plugin_shortcut_conflicts` and `builtin_shortcuts_are_reserved`.
- `tests/fixtures/official_plugins/` — new: the five official manifests and
  a README.

## Notes

- Origin: observed in the startup log while manually verifying T0072
  (2026-11-07). Pre-existing, unrelated to the dependency bumps.
- Ctrl+Shift+R for Reload All landed in d86870f; Rust Lint's manifest
  predates or ignored it.
- Landing order: merge ferrispad-plugins#1 first, or together with this
  ticket. The fixture already holds the 1.5.1 manifest; if that PR changes,
  update the fixture too.
