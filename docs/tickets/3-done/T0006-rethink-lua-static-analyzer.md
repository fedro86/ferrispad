---
id: T0006
title: Rethink the Lua static analyzer — bypasses and false positives
status: done
created: 2026-08-24
severity: moderate
area: security
depends-on: []
---

## Goal

`plugin_verify.rs::scan_lua_source` is presented as a security layer but mostly
blocks things the runtime already removes (`load`, `debug.`, `jit.`, `ffi.` —
already nil / never loaded), while missing what a plugin can actually reach,
and generating false positives that block legitimate plugins silently
(`eprintln!` only, no dialog). It should either be made sound or demoted to an
honest advisory lint so it stops giving a false sense of safety.

## In scope

- Fix the concrete bypasses if the scanner stays a gate: `_ENV` (equivalent to
  `_G` in 5.4) not blocked; string-metatable poisoning via `getmetatable('')`
  not matched; `load\n(...)` split across a newline defeats the line-oriented
  scan; `--[[ ]]` block comments scanned as live code.
- Fix the false positives: `contains("_G")` matches identifiers like
  `MY_GROUP`/`LOG_GREEN`; the `setmetatable`+`string`-on-one-line rule blocks
  normal `setmetatable(t, {__tostring=f})`.
- Decide and document the scanner's role: real gate (then it must be sound and
  surface a user-facing block reason) vs. advisory lint (then don't rely on it
  for isolation — see T0009).

## Out of scope

- The signature/checksum verification (`verify_plugin`) — that part is sound.
- Actual VM-level isolation (T0009).

## How to test

### Regression test

Add cases to `tests/plugin_security_chain.rs` / `plugin_verify` unit tests:
`_ENV` write, `getmetatable('').__index` poison, `load\n("...")`, a payload in
a block comment (all currently pass the scan → should be blocked or the scanner
demoted); and the false positives `MY_GROUP`, `setmetatable(t,{__tostring=f})`
(currently blocked → should pass).

## Acceptance criteria

- [x] Either every listed bypass is blocked, or the scanner is explicitly
      documented as advisory and isolation is delegated to T0009.
      → **advisory** chosen (user decision).
- [x] The listed false positives no longer block legitimate plugins.
- [x] A blocked plugin surfaces a user-visible reason, not just `eprintln!`.
      → no more silent hard-block; advisory notes surface in the install dialog.
- [x] Regression tests added and green.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_verify.rs:237-337` — `scan_lua_source` + helpers.
- `src/app/plugins/mod.rs:175-180` — the silent block path (add user feedback).

## Notes

- Origin: plugin/services audit **M1**. The most robust answer is per-plugin
  `_ENV` isolation (T0009), which makes most of this scanner redundant.

## Outcome (2-review)

**Decision (user): demote the scanner to an advisory lint.** A text scanner
cannot be made sound (`_ENV` via computed keys, `string.char` concatenation,
staged `getmetatable('')`, …), and everything it "blocked" is already
unreachable at runtime: `setup_sandbox` nils `os/io/debug/load/loadfile/dofile/
require/package/coroutine`, and PUC Lua 5.4 has no `ffi`/`jit`. Real isolation
is runtime primitive removal + signature/checksum verification + per-plugin
`_ENV` isolation (T0009). The scan is now documented as **not** a security
boundary and never blocks.

**Changes.**
- `plugin_verify.rs`: `LuaScanResult` loses the `Blocked` variant (now
  `Clean` | `Warnings`). `scan_lua_source` rewritten as an advisory lint over a
  *comment-stripped, string-preserving* view of the source
  (`strip_lua_comments` + `open/close_long_bracket`), with whole-identifier
  `_G`/`_ENV` matching (`contains_identifier`) and a source-wide `load(` check
  (`contains_load_call`). Module/function docs state the advisory role.
- `plugin_registry.rs`: dropped the install-time reject (advisory now); the
  community-install dialog already surfaces the notes before download.
- `plugins/mod.rs`: the silent load-time hard-block is gone; warnings are logged
  as a non-fatal advisory (the user-facing surface is the install dialog).

**False positives fixed (were `Blocked`, now `Clean`):**
- `MY_GROUP` / `LOG_GREEN` (substring `_G`) — proven red before the fix.
- `setmetatable(t, {__tostring = f})` (substring `string`) — proven red before
  the fix.

**Bypasses now caught as advisories (were missed by the line scan):** `_ENV`
write; `getmetatable('').__index` poisoning; `load` split across a newline;
`--[[ … ]]` / `--[==[ … ]==]` payloads correctly stripped (not scanned as live
code); `--` inside a string literal not treated as a comment.

**Red proof.** Two temporary tests asserted the current scanner does *not*
block `MY_GROUP`/`LOG_GREEN` and `{__tostring=f}` — both FAILED on the old code
(they were `Blocked`), confirming the false positives, then were replaced by the
permanent `*_is_clean` regressions.

**Gates.** `cargo test` green (34 `plugin_verify` tests, +8 new), `clippy
--all-targets --all-features -- -D warnings` clean, `cargo fmt --check` clean.

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --lib plugin_verify
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Follow-up: T0009 (per-plugin `_ENV` isolation) is the structural isolation this
lint no longer pretends to provide.
