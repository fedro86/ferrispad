---
id: T0022
title: Fix tree_panel "none" icon drift when filtering
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: []
---

## Goal

`tree_panel.rs` has two tree builders that share an icon-match block but have
diverged: `add_tree_node` maps `"none" => ""` (no icon), while
`add_filtered_tree_node` does not. So a plugin that sets `icon: "none"` gets no
icon normally, but a 📄 emoji appears as soon as the user types in the search
box. A user-visible inconsistency caused by copy-paste drift.

## In scope

- Make `add_filtered_tree_node` handle `"none"` identically to `add_tree_node`
  (ideally by extracting the shared icon-resolution into one helper both call).

## Out of scope

- Fully unifying the two builders (larger refactor) — but a shared icon helper
  is welcome here.

## How to test

### Manual repro

Install a plugin whose tree node uses `icon: "none"`, open the tree, then type
in the filter box.

- Before the fix: the node gains a 📄 icon when filtered.
- After the fix: no icon in either view.

## Acceptance criteria

- [ ] `"none"` yields no icon in both the normal and filtered tree.
- [ ] Icon resolution shared by both builders (no second divergence possible).
- [ ] Manual recipe verified.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tree_panel.rs:400` — `add_tree_node` `"none"` handling.
- `src/ui/tree_panel.rs:954-962` — `add_filtered_tree_node` (missing case).

## Notes

- Origin: UI audit (MODERATE — user-visible drift).
