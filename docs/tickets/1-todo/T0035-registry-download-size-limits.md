---
id: T0035
title: Enforce registry download size limits before buffering and on official install
status: todo
created: 2026-08-24
severity: minor
area: robustness
depends-on: []
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

Mock an oversized response; assert the fetch aborts once the cap is exceeded
(bounded memory) for both community and official install paths.

- Before: full body buffered regardless; official path unbounded.
- After: capped mid-stream on both paths.

## Acceptance criteria

- [ ] Size limit enforced during streaming, not after buffering.
- [ ] Official install path is size-limited like the community path.
- [ ] Regression test added and green.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/plugin_registry.rs:441-453,558,668` — fetch + limit sites.

## Notes

- Origin: plugin/services audit (MINOR).
