---
id: T0023
title: Unify the three divider drag handlers
status: todo
created: 2026-08-24
severity: minor
area: ui
depends-on: [T0018]
---

## Goal

The split, tree, and terminal panels each carry a near-identical hidden-frame
divider handler (set resize cursor on Enter, send resize Message on Drag,
restore on Leave/Released). They differ only in cursor orientation and Message
variant, and have already drifted (T0018 fixed the terminal copy's panic).
Consolidate them into one helper so a fourth divider can't drift again.

## In scope

- Extract one `divider(orientation, on_drag_msg)` helper (in `ui/` or a small
  shared module) and use it from all three panels.

## Out of scope

- Behaviour changes — this is a pure dedup once T0018 has corrected the
  terminal copy.

## How to test

### Regression / manual

All three dividers behave exactly as before (drag to resize each of split,
tree, terminal). `cargo test` green; manual drag check on each.

## Acceptance criteria

- [ ] One divider helper; three call sites.
- [ ] No behavioural change vs. post-T0018.
- [ ] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs:588-631`, `tree_panel.rs:248-290`,
  `terminal_panel.rs:219-243` — collapse into one helper.

## Notes

- Origin: UI audit (SEVERE for the panic, which T0018 handles; this is the
  follow-up dedup).
