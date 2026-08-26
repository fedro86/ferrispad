---
id: T0025
title: Unify the three plugin-row builders (remove the arg-swap hazard)
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`plugin_manager.rs` has three ~identical row builders (installed / available /
community, ~700 lines total), each with `#[allow(clippy::too_many_arguments)]`.
They've already drifted (title/description y-offsets differ, so the three tabs
render misaligned), and — worse — their parameter lists are the same set of
`Rc<RefCell<Vec<String>>>` handles in different orders, so swapping
`installed`/`uninstalled` at a call site compiles silently. Collapse to one
builder taking a `RowKind` enum.

## In scope

- One `PluginRow`/`create_plugin_row(kind: RowKind, ...)` builder shared by all
  three tabs; pass typed state (not three same-typed `Vec` handles in ambiguous
  order).
- Fix the vertical-alignment drift as a side effect (single layout).

## Out of scope

- Restyling the plugin manager.

## How to test

### Manual

`cargo run`, open Plugin Manager, visit all three tabs.

- Before: rows misaligned between tabs.
- After: consistent rows; behaviour unchanged (toggle/install/uninstall work).

## Acceptance criteria

- [ ] One row builder; no `too_many_arguments` allow needed.
- [ ] Same-typed handles no longer positionally ambiguous.
- [ ] Rows aligned across tabs; actions still work.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/plugin_manager.rs:924-987,1139-1197,1396-...` — three builders.

## Notes

- Origin: UI audit (SEVERE — the silent arg-swap is a latent correctness bug,
  not just dead weight).
