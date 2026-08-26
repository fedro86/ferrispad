---
id: T0009
title: Isolate plugins per Lua environment (shared state / shared budget)
status: done
created: 2026-08-24
severity: moderate
area: security
depends-on: []
---

## Goal

All plugins load into a single shared `LuaRuntime`/Lua state with no per-chunk
`_ENV`. Consequences: plugin A's globals leak to plugin B; plugin A can
monkey-patch `string`/`table`/`math`/`pcall` for every other plugin (subverting
a privileged plugin); and the 16 MB memory budget is a shared pool one greedy
plugin can exhaust. Per-plugin permissions are therefore not VM-enforced. This
is the structural fix that makes much of the static scanner (T0006) redundant.

## In scope

- Give each plugin its own sandboxed environment: a fresh `_ENV` table per
  plugin chunk (or a separate Lua state per plugin), so globals and
  monkey-patches don't cross plugins.
- Decide the memory-budget model: per-plugin budget rather than one shared
  pool (or document explicitly that it's shared and why).

## Out of scope

- The instruction-limit bypass (T0005) — orthogonal.
- Rewriting the plugin API surface.

## How to test

### Regression test

`tests/lua_sandbox_integration.rs`: load two plugins; plugin A sets a global
and patches `string.rep`; assert plugin B sees neither (its `string.rep` is
intact, A's global is `nil` in B's env).

- Before the fix: B sees A's global and patched `string.rep`.
- After the fix: B is isolated.

## Acceptance criteria

- [x] One plugin's globals/metatable edits are invisible to another.
- [x] Memory budgeting model is per-plugin or explicitly documented as shared.
      → documented as **shared** (single Lua state); rationale below.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/plugins/mod.rs:96,145-164` — single runtime / load loop.
- `src/app/plugins/runtime.rs:185` — `self.lua.load(&content)` (needs per-chunk env).

## Notes

- Origin: plugin/services audit **M2**. mlua already sets `__metatable` on
  userdata so the `EditorApi` method table isn't reachable — real isolation,
  but from mlua, not this code. This ticket closes the cross-plugin gap.

## Outcome (2-review)

**Approach: per-plugin `_ENV` (not separate Lua states).** The `EditorApi` and
other APIs reach a plugin as **hook arguments**, not as globals, so a plugin's
environment need not carry them — which makes a fresh per-chunk `_ENV` in the
shared state enough for the isolation this ticket targets, without the larger
per-state refactor of `PluginManager`/`hook_dispatch`.

**Change (contained to `runtime.rs`).**
- `LuaRuntime::build_plugin_env` builds a fresh env per plugin: base functions
  (`print`, `type`, the **guarded** `pcall`/`xpcall`, …) are shared references
  (immutable — sharing the guarded pcall keeps the T0005 protection), while the
  mutable stdlib tables `string`/`table`/`math`/`utf8` are **shallow-copied**
  (`shallow_copy_table`) so a patch to one plugin's `string.rep` can't touch
  another's. `_G` points at the env itself, so plugin globals stay plugin-local.
- `load_script` now runs the chunk with `.set_environment(env)`; the returned
  hook functions capture that `_ENV`, so later hook calls stay isolated too.
- `mod.rs` needs no structural change — still one runtime.

**Memory model: shared (documented).** All plugins share the single Lua state's
16 MB pool. Separate states would give per-plugin budgets but at a much larger
structural cost; the shared pool is acceptable because hooks are synchronous
(one plugin runs at a time) and the instruction + memory limits already abort a
runaway plugin, and plugins hold little between hooks (GC reclaims). Recorded
here rather than changed.

**Known residual (out of this ticket).** The process-wide **string value**
metatable reachable via `getmetatable("")` is still state-global; only separate
Lua states would isolate it. It is covered by the T0006 advisory lint and noted
in `build_plugin_env`'s docs.

**Red proof.** `tests/lua_sandbox_integration.rs::plugins_are_isolated_from_each_other`
loads two plugins in one runtime; A leaks `LEAKED`/`_G.LEAKED_G` and patches
`string.rep`. Before the fix B saw `LEAKED == "from A"` (test FAILED at that
assertion); after, B sees `nil`, `nil`, and `"xxx"`, while A still sees its own
`"HACKED"` — proving isolation is real, not blanket suppression.

**Gates.** `cargo test` green (6 `lua_sandbox_integration` tests incl. the new
one; runtime unit tests unchanged), `clippy --all-targets --all-features -D
warnings` clean, `cargo fmt --check` clean.

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --test lua_sandbox_integration
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
