---
id: T0031
title: Deduplicate find_next/find_prev via a Direction enum
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`find.rs::find_next` and `find_prev` are the same ~60-line function — same state
dance, same regex/plain branch, same wrap-around retry, same error dialogs —
differing only in `find_in_text` vs `find_in_text_backward` and the wrap bound
(`0` vs `text.len()`). Both carry `#[allow(clippy::too_many_arguments)]`.
Collapse into one function parameterised by a `Direction` enum.

## In scope

- Introduce `enum Direction { Forward, Backward }` and one `find(dir, ...)`
  covering both; delete the duplication.

## Out of scope

- The case-insensitive offset bug (T0013) — fix that first if landing together,
  or note the dependency.

## How to test

### Manual / regression

Find-next and find-prev (including wrap-around at start/end and regex mode)
behave exactly as before. Add a small test over the shared function for both
directions if the search core is unit-testable.

## Acceptance criteria

- [ ] One search function; no `too_many_arguments` allow.
- [ ] Forward/backward/wrap/regex behaviour unchanged.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/find.rs:34-94,96-155` — the two functions.

## Notes

- Origin: UI audit (MODERATE).
