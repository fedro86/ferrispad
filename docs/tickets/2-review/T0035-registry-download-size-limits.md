---
id: T0035
title: Enforce registry download size limits before buffering and on official install
status: review
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
issue: 40
---

## Goal

`enforce_size_limit` runs *after* the full response body is already in memory,
so the "100 KB limit" protects disk, not RAM — a hostile registry can force an
arbitrarily large allocation. And `install_plugin` (official path) uses
`fetch_file` with **no** size limit at all, while the community path enforces
one. Make the limit apply before buffering and to both paths.

## In scope

- Enforce the size limit while streaming the response (cap bytes read), before
  the whole body is buffered.
- Apply a size limit to the official `install_plugin` path too.

## Out of scope

- Requiring signatures before install (separate policy decision, see T0007
  notes).

## How to test

### Regression test

```bash
nix develop -c cargo test --lib plugin_registry::tests
```

- `oversized_download_aborts_mid_stream` — a local one-shot HTTP server
  announces 1 GiB, sends 200 KB, then stalls with the connection open. With a
  100 KB cap, `fetch_limited` must return a "size limit" error within 5 s.
- `download_within_limit_is_returned` — a 1 KB body under a 1 KB cap comes
  back intact.

Red run — **executed**. First every download was routed through one
`fetch_limited(request, url, max_bytes, name)` helper that kept the *old*
semantics (buffer the whole body with `send()`, then check its length — i.e.
exactly the pre-fix community path). On that code:

```
test result: FAILED. 17 passed; 1 failed   (finished in 5.00s)
  oversized_download_aborts_mid_stream
      panicked: download did not abort at the size cap: body is being buffered in full
```

The buffering client keeps reading the announced 1 GiB until the network
timeout; against a server that keeps sending, that is an unbounded allocation.

Green with the streaming read: all `plugin_registry::tests` pass; full
`cargo test`, `clippy -D warnings`, `fmt --check` clean.

### Real network (ignored by default)

```bash
nix develop -c cargo test --lib plugin_registry -- --ignored
nix develop -c cargo test --test plugin_registry_fetch -- --ignored
```

- `fetch_limited_reads_real_registry_and_plugin_files` downloads the real
  `plugins.json` and the first official plugin's `init.lua` over HTTPS through
  the streaming path, bypassing the registry disk cache (the existing
  `plugin_registry_fetch` tests hit the 1 h cache, so they can pass without
  touching the network). Passes.

### Manual check

`cargo run` → Plugin Manager: the official and community lists load, and
installing / updating an official plugin still works.

## What was done

- **One download path.** `fetch_file` / `fetch_file_bytes` and the two
  hand-written registry fetches are replaced by `fetch_limited` (plus a thin
  `fetch_text` for UTF-8 files). A download without a byte limit can no longer
  be written in this module.
- **Streaming cap.** `fetch_limited` uses `send_lazy()` and
  `Read::take(max + 1)`: reading stops one byte past the cap, and that byte is
  what reports "exceeds size limit".
- **Official path limited.** `install_plugin` now caps `init.lua` (100 KB) and
  `plugin.toml` (10 KB) like the community path.
- **Other downloads limited too** (same helper, same one-line change each):
  registry JSONs 1 MiB (today ~4.4 KB and ~0.7 KB), `README.md` 100 KB,
  `fetch_default_branch` repo metadata 64 KB.
- **Response head bounded.** minreq reads headers without a limit by default;
  `fetch_limited` sets a 1 KB status line / 64 KB headers cap.
- Removed `enforce_size_limit` and its two tests: after the change it had no
  callers — the check lives in `fetch_limited`.

## Acceptance criteria

- [x] Size limit enforced during streaming, not after buffering.
- [x] Official install path is size-limited like the community path.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_registry.rs` — registry fetches, `fetch_limited` /
  `fetch_text` (replacing `fetch_file`, `fetch_file_bytes`,
  `enforce_size_limit`), `fetch_default_branch`, `fetch_community_plugin_toml`,
  `install_plugin`, `install_community_plugin`, tests.

## Notes

- Origin: plugin/services audit (MINOR).
- The new caps for the registry JSON (1 MiB), README (100 KB) and repo
  metadata (64 KB) are judgement calls with wide headroom over today's sizes;
  adjust in review if needed — they are named constants at the top of the
  *File download* section.
