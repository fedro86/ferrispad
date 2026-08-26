---
id: T0010
title: Bound terminal escape-sequence repeats and make scrollback a real ring buffer
status: done
created: 2026-08-24
severity: moderate
area: robustness
depends-on: []
---

## Goal

The built-in terminal can be frozen by an attacker-supplied file. A single
`ESC[65535S` (scroll-up) loops 65 535 times, and at scrollback capacity each
iteration does `self.scrollback.remove(0)` — an O(n) memmove of a 10 000-entry
`Vec` — so one sequence is ~15 GB of memmove. `cat`ing a crafted file hangs the
UI. `insert_lines` / `delete_lines` / `delete_chars` share the shape. The doc
comment calls it a "ring buffer"; it's a `Vec` with `remove(0)`.

## In scope

- Clamp escape-sequence repeat counts to a sane maximum (bounded by the grid /
  scrollback size) in the VTE handler before executing the loop.
- Replace the `Vec` + `remove(0)` scrollback with an actual ring buffer
  (`VecDeque` with `pop_front`, or a fixed ring) so trimming is O(1).

## Out of scope

- Full VT100 conformance work.

## How to test

### Regression test

`services/terminal/grid.rs` unit test: feed a grid an `ESC[65535S` (or call the
scroll op with a huge count) and assert it completes in O(rows) work and leaves
scrollback ≤ capacity; feed 100 000 line-feeds and assert trimming is bounded.

- Before the fix: the test runs for seconds / effectively hangs.
- After the fix: returns promptly.

## Acceptance criteria

- [x] Repeat counts are clamped before the loop runs.
- [x] Scrollback trimming is O(1) amortised, not O(n) per line.
- [x] Regression test added and green; it was pathologically slow before.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/terminal/grid.rs` — scroll/insert/delete ops clamped;
  scrollback is now a `VecDeque` (O(1) `pop_front`).

## Notes

- Origin: plugin/services audit **M8**.

## Outcome (2-review)

The clamp lives in the **grid methods**, not in `vte_handler`, deliberately
(engineering-standards "simplify instead of layering / one coherent rule"): the
grid is where the bound (`region_height`, `rows`, `cols`) is actually known, and
clamping there protects *every* caller — including the internal
`newline()`→`scroll_up(1)` path and any future one — with one `n.min(bound)` per
method, instead of duplicating the grid's dimensions into the handler. The
handler is unchanged.

Two independent fixes, both needed:

1. **Ring buffer.** `scrollback: Vec<Vec<Cell>>` → `VecDeque<Vec<Cell>>`;
   `push`/`remove(0)` → `push_back`/`pop_front`. Trimming the oldest line is now
   O(1) instead of an O(n) memmove of the whole 10 000-entry buffer. This is what
   fixes the *flood* case (a stream of `newline()`s, each its own `scroll_up(1)`
   trim at capacity).

2. **Repeat-count clamp.** `scroll_up`/`scroll_down` clamp `n` to the region
   height; `insert_lines`/`delete_lines` to `rows`; `delete_chars`/`insert_chars`/
   `erase_chars` to `cols`. This is what fixes the *single-sequence* case
   (`ESC[65535S` looped 65 535 times).

**One intentional behaviour change:** `scroll_up` no longer keeps looping past
`region_height` to push *blank* lines into scrollback. A count larger than the
region height only ever fed blanks (the visible region is already empty after
`region_height` scrolls); the visible screen is identical, and dropping the
blank-fill is what makes the clamp deterministically testable. Real programs
never scroll a region by more than its height, so no legitimate output is
affected; scrolling by exactly the region height (the common full-screen page)
is unchanged.

`insert_lines`/`delete_lines`/`delete_chars` were not catastrophic on their own
(their per-iteration work is O(rows) / O(cols), a few ms at 65 535), but they
share the unbounded-loop shape and are clamped for the same one-line reason.

### Red proof

- `scroll_up_clamps_repeat_count_to_grid` (deterministic): on the unfixed code
  `scroll_up(30_000)` looped the raw count and filled the cap →
  `scrollback_len() == 10_000`, failing `<= rows (24)`. After the clamp: ≤ 24.
- `line_feed_flood_trims_in_bounded_time` (timing, `rows==1` to isolate the
  trim): 1 000 000 single-line scrolls took **4.37 s** in a debug build with
  `Vec::remove(0)`; with the ring buffer it is ~0.5 s, well under the 2 s bound.
  (Measured by temporarily forcing the assertion to print `elapsed`; restored.)

## How to verify (reviewer recipe)

```bash
# Both regression tests + the whole terminal module, all green:
nix develop -c cargo test --lib terminal
# The two new tests specifically:
nix develop -c cargo test --lib scroll_up_clamps_repeat_count_to_grid
nix develop -c cargo test --lib line_feed_flood_trims_in_bounded_time
# Full gates:
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

To *see the red*: `git stash` the `grid.rs` change and rerun the two tests —
`scroll_up_clamps_...` fails with "10000 lines in scrollback", and bumping the
flood test's threshold down shows the ~4 s `Vec::remove(0)` cost.
