---
id: T0020
title: Fix panic-by-indexing on stale tab indices during drag/release
status: done
created: 2026-08-24
severity: moderate
area: ui
depends-on: []
---

## Goal

Tab drag stores `DragSource::Tab(usize)` at `Push` and indexes the tab vector
directly at `Released` (`st.tabs[from].id`, `&st.tabs[from]`, etc.). If the
document list shrinks between press and release — a plugin closing a tab, a
reload event — this panics with an out-of-bounds index. The same function uses
safe `.get()` a few lines away, so the inconsistency is accidental, not
intentional.

## In scope

- Replace the direct `st.tabs[idx]` indexing on stored drag indices with
  bounds-checked `.get()` / `.get_mut()`, bailing out of the drag gracefully if
  the index is stale.

## Out of scope

- The hover-field refactor (T0028) — separate cleanup.

## How to test

### Regression test / manual

If the tab vector can be mutated mid-drag in a unit harness, assert a
release with a now-out-of-range stored index is a no-op, not a panic.
Manually: start dragging a tab and have a plugin/reload close a tab before you
release.

- Before the fix: index-out-of-bounds panic.
- After the fix: the drag is abandoned safely.

## Acceptance criteria

- [x] No direct indexing on stored drag indices; all bounds-checked.
- [x] A stale index at release is handled without panic.
- [x] Regression/manual recipe verified (by inspection — UI-only, see below).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tab_bar.rs` — `build_context_menu` (1526), the `HitResult::Tab` push
  arm (1605/1611), and the three `DragSource::Tab` release arms (1820/1821,
  1828, 1891) now resolve indices with `.get()`.

## Notes

- Origin: UI audit (MODERATE). Match the safe `.get()` usage already present at
  `tab_bar.rs:1832-1846`.

## Outcome (2-review)

Every place that indexed `st.tabs[..]` with a **stored or hit-derived** index now
resolves it with `.get()` and bails out gracefully, matching the safe `.get()`
already used a few lines away for the insertion point:

- **`build_context_menu` (1526):** `if let Some(idx) = tab_index { &st.tabs[idx] }`
  → `if let Some(tab) = tab_index.and_then(|idx| st.tabs.get(idx))`. A stale
  `tab_index` now falls through to the group-only menu branch instead of
  panicking.
- **`HitResult::Tab` push arm (1605 + 1611):** the two `st.tabs[index]` reads
  (`.id`, `.group_id`) become one `let Some((tab_id, group_id)) =
  st.tabs.get(index).map(|t| (t.id, t.group_id)) else { return true; }`. A stale
  layout index consumes the click and does nothing.
- **Release `OnTab` arm (1820/1821):** both `from` and `target_idx` guarded via
  `if let (Some(source), Some(target)) = (st.tabs.get(from), st.tabs.get(target_idx))`.
- **Release `InsertAt` (1828) and `OnCollapsedGroup` (1891) arms:**
  `&st.tabs[from]` → `let Some(source_tab) = st.tabs.get(from) else { drop(st);
  return false; }`, abandoning the drag on a stale source.

No behaviour change on the happy path — the indices are the same, only the
out-of-range case changed from panic to a graceful no-op.

### Why no unit test

UI-only defect: the panic needs a live FLTK `Widget`, dispatched `Push`/`Released`
events, and the tab vector mutated between them, none reachable under headless
`cargo test` (`engineering-standards.md`). Verified structurally instead: a
`grep` confirms no `st.tabs[...]` on a stored/hit index remains (the surviving
`st.tabs[i]` sites at 335–529 use a `0..len` loop counter, and 1009 is the draw
path — see the related finding).

### Manual repro

`cargo run`, start dragging a tab, and — before releasing — have a plugin or a
reload close a tab (shrinking the list), then release over another tab.

- Before: index-out-of-bounds panic at the release site.
- After: the drag is abandoned safely; no panic.

### Related finding (out of scope, flagged)

`tab_bar.rs:1009` (`let tab = &st.tabs[*index];`) is the **same** layout-index →
tabs-index pattern but in the **draw** path, not drag/release, so it was not
among the ticket's enumerated sites. It would panic in the same "tab closed with
a stale layout" race on the next redraw. Lower-risk (draw normally runs with a
freshly rebuilt layout) and a trivial `.get()`-and-skip fix — recommend folding
it into T0028 (the hover/layout cleanup) or a one-line follow-up.

## How to verify (reviewer recipe)

```bash
# No stored/hit index is indexed directly anymore (only 0..len loop counters and
# the draw-path 1009 remain):
grep -nE "st\.tabs\[" src/ui/tab_bar.rs
# Gates:
nix develop -c cargo build
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
nix develop -c cargo test
```

Manual: `cargo run`, drag a tab, close a tab via plugin/reload mid-drag, release
over another tab → no panic (before: index-out-of-bounds).
