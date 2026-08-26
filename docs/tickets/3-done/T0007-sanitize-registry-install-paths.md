---
id: T0007
title: Sanitize registry-controlled install paths
status: done
created: 2026-08-24
severity: moderate
area: security
depends-on: []
---

## Goal

The remote plugin registry controls the directory name a plugin is installed
into, and it is joined into the plugins dir unsanitized. `Path::join` with an
absolute component replaces the base entirely, so a registry entry with
`"path": "/etc/cron.d/x/"` or `"../../../.config/autostart/"` writes outside the
plugins directory. Unsigned/unverified entries reach this write because
`verify_plugin` returns `Unverified` (which `allows_install()`) when a
signature/checksum is absent.

## In scope

- Sanitize `dir_name` / community `name` before joining: reject absolute paths,
  reject any `..` component, restrict to a safe charset, and assert the final
  path `starts_with` the plugins dir.
- Apply the same sanitization to both the official and community install paths.

## Out of scope

- The `Unverified`-allows-install policy itself (tracked separately if the user
  wants to require signatures) — but note it in Notes.
- Download size limits (T0035).

## How to test

### Regression test

`tests/plugin_registry_fetch.rs` (or a unit test): feed a `PluginInfo` with
`path="../../evil"` and with `path="/tmp/evil"`; assert the computed install
dir stays under `get_plugin_dir()` (or the install is refused).

- Before the fix: the join escapes the plugins dir.
- After the fix: refused / clamped inside.

## Acceptance criteria

- [x] No registry-controlled path can write outside the plugins directory.
- [x] Both official and community install paths are covered.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_registry.rs:558-560` — official `dir_name` join.
- `src/app/services/plugin_registry.rs:668-678` — community `name` join.

## Notes

- Origin: plugin/services audit **M5**. Consider requiring `Verified` before
  install as a follow-up hardening ticket.

## Outcome (2-review)

**Fix (one shared, fail-closed sanitizer).** New `sanitize_install_dir(raw_name)
-> Result<(String, PathBuf), AppError>` in `plugin_registry.rs`:
- trims a trailing `/` (registry paths look like `"python-lint/"`);
- rejects empty, any name starting with `.` (kills `.`, `..`, hidden dirs), and
  anything outside the allowlist `[A-Za-z0-9._-]` — which also rejects path
  separators (`/`, `\`), absolute paths, `..`, NUL, whitespace, and shell/
  Unicode trickery (single component only);
- joins under `get_plugin_dir()` and asserts the result `starts_with` it,
  returning `Err` otherwise (belt-and-suspenders, fail-closed).

Both sinks now route through it and reuse the returned validated name:
- **official** `install_plugin` — `plugin_info.path` → `sanitize_install_dir`,
  and the same name feeds `write_plugin_source`;
- **community** `install_community_plugin` — `name` → `sanitize_install_dir`,
  same name feeds `write_plugin_source`.

This closes the escape at both the file writes *and* the `.source` write (which
also did `get_plugin_dir().join(name)` + `create_dir_all`).

**Note on `Path::starts_with`.** It is purely lexical (`base/../../evil` still
"starts_with" `base`), so it cannot detect `..` traversal on its own — the
sanitizer rejects `..` structurally via the allowlist/leading-dot rules; the
`starts_with` check is only a final guard, mainly catching the absolute-path
replacement case.

**Red proof.** A temporary test computed the install dir the old way
(`base.join("/tmp/ferrispad_evil")`) and asserted containment — it FAILED
(result `"/tmp/ferrispad_evil"` escaped), proving the vuln, then was replaced by
the permanent `sanitize_install_dir_*` tests (16 refused escaping inputs; plain
names accepted and contained; trailing slash trimmed).

**Out of scope (noted):** the `Unverified`-allows-install policy is unchanged —
a plugin without a signature still installs (into a now-safe directory). A
follow-up could require `Verified`.

**Gates.** `cargo test` green (16 `plugin_registry` tests, +3 new), `clippy
--all-targets --all-features -- -D warnings` clean, `cargo fmt --check` clean.

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --lib plugin_registry
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
