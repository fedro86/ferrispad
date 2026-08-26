---
id: T0004
title: Bound recursion when parsing plugin-shaped tables/YAML (stack-overflow abort)
status: done
created: 2026-08-24
severity: severe
area: security
depends-on: []
---

## Goal

`TreeNode::from_lua_table` recurses on a table's `children` key with no depth
limit and no cycle detection. A plugin hook returning a cyclic or very deep
table triggers unbounded Rust recursion → stack overflow → **process abort**
(not a catchable panic). A two-line Lua payload does it
(`local t={label="x"}; t.children={t}`). `yaml_parser.rs::value_to_children`
has the same unbounded recursion on YAML nesting.

## In scope

- Add a `max_depth` guard (and, for the Lua table case, cycle detection or a
  depth cap that makes cycles finite) to `TreeNode::from_lua_table`.
- Add the same depth cap to `yaml_parser.rs::value_to_children`.
- On exceeding the cap: stop descending and return a truncated node / error,
  never recurse further. Consider an explicit `Drop` that is iterative if a
  deep tree can still be constructed.

## Out of scope

- The fixed-depth `split_view.rs` parser (not affected).
- Redesigning the hook result schema.

## How to test

### Regression test

```rust
// plugins/widgets/tree_view.rs tests
let deep = /* build a 100_000-deep children chain, or a self-cycle */;
assert!(TreeNode::from_lua_table(deep, /*depth*/0).is_err()); // or returns truncated
```

- Before the fix: the test overflows the stack and aborts the test process.
- After the fix: returns an error / truncated tree within the depth cap.

## Acceptance criteria

- [x] A cyclic plugin table cannot abort the process.
- [x] A pathologically deep table is truncated at a documented depth cap.
- [x] YAML nesting is bounded the same way.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/plugins/widgets/tree_view.rs` — `MAX_TREE_DEPTH` const (256);
  `from_lua_table` delegates to a depth-bounded `from_lua_table_depth`.
- `src/app/services/yaml_parser.rs` — `value_to_children` delegates to a
  depth-bounded `value_to_children_depth` using the same cap.
- `src/app/plugins/hook_result_parser.rs` — entry point; unchanged (the public
  `from_lua_table` signature is preserved, so no caller changes).

## Notes

- Origin: plugin/services audit **S1**. Reachable from any hook return value.
  A non-cyclic 50k-deep table achieves the same within the instruction/memory
  budgets, so the depth cap matters even with cycle detection.

## Outcome (2-review)

A single shared `MAX_TREE_DEPTH = 256` (`tree_view.rs`, `pub(crate)`) caps both
recursions. `from_lua_table`/`value_to_children` keep their public signatures and
delegate to private `*_depth` workers that stop descending at the cap (children
truncated to empty), so a cyclic table is made finite and a deep one is bounded.
A depth cap alone handles cycles — no table-identity tracking needed. 256 is far
above any real tree yet low enough that building **and** recursively dropping a
maxed-out tree stays well within the stack, so no custom iterative `Drop` is
needed (the ticket's "consider" item).

Chose **depth cap over cycle detection**: mlua table identity comparison is
awkward, and the cap also covers the non-cyclic deep case the Notes call out.

### Red proof

Two truncation tests build a **350-deep** chain (> cap) and assert the resulting
tree is exactly `MAX_TREE_DEPTH` deep:
- `tree_view::tests::deep_lua_table_is_truncated_at_cap`
- `yaml_parser::tests::deeply_nested_yaml_is_truncated_at_cap`

With the guards temporarily removed I ran both: they fail cleanly with
`left: 350, right: 256` (no truncation → tree as deep as input). Restored the
guards → green. A third test, `cyclic_lua_table_does_not_overflow`, drives the
`t.children = {t}` self-cycle from the ticket: it returns a cap-truncated tree
now; before the fix that path recurses forever and aborts via stack overflow
(the destructive case — not run in the reverted state).

### Verification recipe

```bash
nix develop -c cargo test --lib tree_view::tests
nix develop -c cargo test --lib yaml_parser::tests
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
