---
id: T0023
title: Unify the three divider drag handlers
status: review
created: 2026-08-24
severity: minor
area: ui
depends-on: [T0018, T0021]
issue: 72
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
- Pull the drag math (pointer → new panel size, clamped) into one pure function
  that cannot panic when the clamp's minimum exceeds its maximum (the approach
  T0021 used for the split panel), and unit-test it.

## Out of scope

- Behaviour changes, apart from the degenerate tiny-window clamp below — this is
  a dedup once T0018 has corrected the terminal copy.

## Outcome (2-review)

Stacked on T0021 (which carries T0027, T0029, T0030): builds on its
`DockedHeight` / `SplitPanel::drag_docked_height` API.

New module `src/ui/divider.rs`:

- **`new_divider(sender, axis: DragAxis, on_drag: fn(i32) -> Message) -> Frame`**
  — the single divider handler. `DragAxis::X` gives `Cursor::WE` + `event_x`,
  `DragAxis::Y` gives `Cursor::NS` + `event_y`. It keeps T0018's guarantees: no
  `.unwrap()` on `window()` (all three cursor changes use `if let Some(win)`),
  and the `dragging` guard (set on `Push`, checked on `Drag` and `Leave`,
  cleared on `Released`). The three call sites pass the Message variant
  constructor directly (`Message::SplitViewResize`, `Message::TreeViewResize`,
  `Message::TerminalViewResize`).
- **`dragged_size(pointer, start, len, edge: DockEdge, min, max) -> i32`** — the
  pure drag math. `DockEdge::Start` (left/top) gives `pointer - start`;
  `DockEdge::End` (right/bottom) gives `start + len - pointer` (saturating). The
  result is clamped with `clamp(min, max.max(min))`, so when the parent is too
  small for `max >= min` the minimum wins instead of panicking.

Call sites:

- `SplitPanel::new_divider`, `TreePanel::create_divider`,
  `TerminalPanel::new_divider` are now one-line calls to `divider::new_divider`
  (their unused `Cursor` imports are dropped).
- `DockedHeight::drag_to` (T0021) delegates to `dragged_size` with
  `DockEdge::End`, `MIN_DOCKED_HEIGHT`, `col_h / 2` — the same result as before;
  T0021's four `DockedHeight` tests stay green unchanged.
- `dispatch.rs` `TreeViewResize` and `TerminalViewResize` use `dragged_size`
  with the same limits as before (tree: 100..=w/2, Start for Left / End for
  Right; terminal: 200..=4w/5, End). The tree arm's `_ => unreachable!()` goes
  away (the enclosing `matches!` already restricts it to Left/Right).

### The one behaviour change (deliberate)

The tree and terminal arms used raw `.clamp(100, w / 2)` and
`.clamp(200, w * 4 / 5)`. `i32::clamp` panics when `min > max`, i.e. a tree
drag in a content row narrower than 200 px or a terminal drag in one narrower
than 250 px — the same panic class T0021 fixed for the split panel. These now
return the minimum. Every non-degenerate drag yields exactly the old value.

Red run — `dragged_size` written first with the old raw `size.clamp(min, max)`,
then `cargo test --lib ui::divider`:

```
test ui::divider::tests::tiny_parent_where_max_is_below_min_returns_min_without_panicking ... FAILED

---- ui::divider::tests::tiny_parent_where_max_is_below_min_returns_min_without_panicking stdout ----
thread '...' panicked at src/ui/divider.rs:123:10:
min > max. min = 200, max = 160

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 387 filtered out
```

With `clamp(min, max.max(min))` all 5 tests pass.

### Why the handler itself has no unit test

The event handler needs dispatched FLTK events and a real window to observe the
cursor; neither is reachable under headless `cargo test`
(`engineering-standards.md`). It is checked manually below; the pure math it
feeds is unit-tested.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::divider        # 5 dragged_size tests
nix develop -c cargo test --lib split_panel::tests  # T0021 DockedHeight tests, unchanged
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

`ui::divider` tests: start-edge and end-edge sizes, clamping to min and max,
the tiny-parent case (`max < min`, for both tree edges, the terminal and a
zero-size parent) and extreme coordinates (no overflow).

### Manual (in the running app)

`nix develop -c cargo run`, then:

1. **Tree divider.** Open a folder so the file tree shows on the left. Hover
   the divider between tree and editor: the cursor becomes ↔. Drag it: the tree
   resizes, stopping at 100 px and at half the window width. While dragging,
   move the pointer off the thin divider: the cursor stays ↔ until release, then
   goes back to the arrow. If the tree can be put on the right (file-explorer
   setting), dragging left widens it.
2. **Terminal divider.** Show the terminal panel. Its divider behaves the same:
   ↔ cursor, drag resizes, minimum 200 px, maximum 4/5 of the row.
3. **Split divider.** Open a split/diff view in docked (non-tab) mode. Hover
   the divider above the panel: the cursor becomes ↕. Drag up/down: the panel
   resizes between 100 px and half the column. Toggle tab mode and back: the
   dragged height is kept (T0021).
4. **Tiny window.** Shrink the window as narrow (and short) as it goes, then
   drag the tree and terminal dividers: they hold their minimum width and the
   app does not panic (before, a row narrower than 200 px / 250 px panicked).
5. Close a panel while hovering its divider: no panic.

## Acceptance criteria

- [x] One divider helper; three call sites.
- [x] No behavioural change vs. post-T0018, except a tiny-window drag now holds
      the minimum instead of panicking.
- [x] Drag math in one pure, unit-tested function, including the degenerate
      tiny-window case.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/divider.rs` (new) — `new_divider`, `DragAxis`, `dragged_size`,
  `DockEdge`, tests.
- `src/ui/mod.rs` — `pub mod divider`.
- `src/ui/split_panel.rs`, `src/ui/tree_panel.rs` (divider region only, plus
  imports), `src/ui/terminal_panel.rs` — handlers collapsed to the helper.
- `src/dispatch.rs` — tree and terminal resize arms use `dragged_size`.

## Notes

- Origin: UI audit (SEVERE for the panic, which T0018 handles; this is the
  follow-up dedup).
