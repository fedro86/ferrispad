---
id: T0072
title: Bump the five pending Dependabot crate updates in one change
status: review          # todo | review | done
created: 2026-11-07
severity: ~
area: process
depends-on: []
issue: 136
---

## Goal

Five Dependabot PRs (#131–#135) have been open since 2026-10-21, each green in
CI. Land them as one reviewed change instead of five separate merges, so the
dependency tree stays current and Dependabot stops re-opening them.

## In scope

- `Cargo.toml` / `Cargo.lock`:
  - `open` 5.3.3 → 5.4.4 (minor; Windows path-opening fix) — PR #135
  - `toml` 0.8.23 → 1.1.8 (**major**) — PR #134
  - `memmap2` 0.9.10 → 0.9.11 (patch) — PR #133
  - `dirs` 6.0.0 → 7.0.0 (**major**) — PR #132
  - `pulldown-cmark` 0.13.3 → 0.13.4 (patch) — PR #131
- Any source adjustment the two major bumps require (none expected: the
  Dependabot PRs already pass fmt + clippy + test on CI).
- Record the release-binary size before/after in Notes (memory frugality /
  single-binary constraint).

## Out of scope

- Other dependency upgrades not proposed by those five PRs.
- Changing the Dependabot configuration.
- A release — the bumps ship with whatever release comes next.

## How to test

### Automated

```bash
nix develop -c cargo test
nix develop -c cargo test plugin_registry      # toml round-trip of PluginSource
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

All green, zero warnings.

### Manual (major bumps)

`toml` is used to parse `plugin.toml` (`plugins/loader.rs`) and to read/write
plugin sources (`services/plugin_registry.rs`). `dirs` resolves the config,
data and home dirs (`settings.rs`, `session.rs`, `loader.rs`, `mcp/mod.rs`,
`editor_context.rs`, `start_page.rs`).

1. `nix develop -c cargo run` with your existing `~/.config/ferrispad/`.
2. Expect: settings load unchanged (theme, font, etc.), the previous session
   restores its tabs, installed plugins appear in the Plugins menu and load.
3. Open the plugin manager / registry view: plugin list and sources display
   as before.
4. Change a setting, quit, relaunch: the setting persists and the files under
   `~/.config/ferrispad/` are still in the same place (no new directory
   created elsewhere, e.g. check `ls ~/.config/ferrispad ~/.local/share/ferrispad`).

## Acceptance criteria

- [x] `Cargo.lock` resolves `open 5.4.5`, `toml 1.2.1`, `memmap2 0.9.12`,
      `dirs 7.0.0`, `pulldown-cmark 0.13.6` (latest compatible at bump time,
      at or above the Dependabot targets).
- [x] Existing settings, session and plugins load unchanged (manual recipe).
- [x] Config/data paths are unchanged on Linux.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [ ] Dependabot PRs #131–#135 closed as superseded once this lands.

## Affected files

- `Cargo.toml` — version requirements for `dirs`, `toml` (major bumps)
- `Cargo.lock` — resolved versions

## Notes

- Origin: Dependabot PRs #131, #132, #133, #134, #135 (all opened 2026-10-21,
  all mergeable and green on CI at ticket creation).

## What was done

- `Cargo.toml`: `dirs` "6.0" → "7.0", `toml` "0.8" → "1.1", `open` "5.3" →
  "5.4"; `memmap2` / `pulldown-cmark` requirements already allowed the bump.
- `Cargo.lock`: `dirs 7.0.0`, `toml 1.2.1`, `open 5.4.5`, `memmap2 0.9.12`,
  `pulldown-cmark 0.13.6`. Transitively: `toml_edit`, `toml_write`, `pathdiff`
  dropped; `toml_parser`, `toml_writer` added; `winnow` 0.7 → 1.0.
  (`toml 0.5.11` remains in the tree as a build-dependency of another crate.)
- **No source changes were needed** — the public APIs FerrisPad uses
  (`toml::from_str` / `to_string` / `Value`, `dirs::config_dir` / `data_dir` /
  `home_dir`) are unchanged. `dirs` 7's documented per-platform values for
  those three functions match 6.x.
- Release binary size (Linux x86_64, `cargo build --release`):
  13 427 456 → 13 240 496 bytes (**−186 960 B**).
- Gates green: fmt, clippy `-D warnings`, `cargo test` (all suites pass).

## Manual verification (Claude, 2026-11-07)

Release binary from this branch, run on Xvfb (X11) with an isolated `HOME`
holding a copy of the real `~/.config/ferrispad/` (6 installed plugins, the
user's `settings.json`) and `XDG_*` unset, so `dirs` resolved the default
paths itself:

- Startup: settings applied (line numbers, wrap, Solarized Light, tabs,
  font 16); all 6 plugins loaded; plugin update check (reads every `.source`
  via `toml`) reported "All plugins are up to date".
- Plugin Manager: versions/descriptions from `plugin.toml` shown; trust tier
  from `.source` correct — 5 `official` plugins "Verified", `claude-code`
  (`tier = "manual"`) "Unverified", as expected.
- Settings dialog shows the loaded values; changing Session Restore to
  "Saved Files Only" and the font size, then Save, wrote both to
  `$HOME/.config/ferrispad/settings.json`.
- Quit wrote the session to `$HOME/.local/share/ferrispad/session/default/session.json`
  (`dirs::data_dir`); relaunch with no arguments restored both tabs with the
  right active tab and the new font size.
- No files written outside `$HOME/.config/ferrispad`, `$HOME/.local/share/ferrispad`
  (plus FLTK prefs and the fontconfig cache). No errors/panics in the logs.
- Unrelated, pre-existing: Rust Lint's `Ctrl+Shift+R` "Run All Checks"
  collides with File → Reload All and is skipped (logged warning). Not caused
  by these bumps.

