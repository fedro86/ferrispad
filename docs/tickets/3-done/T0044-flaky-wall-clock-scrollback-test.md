---
id: T0044
title: Make the scrollback-trim test measure complexity, not wall-clock seconds
status: done
created: 2026-09-16
severity: minor
area: process
depends-on: []
issue: 49
---

## Goal

`line_feed_flood_trims_in_bounded_time` (`services/terminal/grid.rs`, added by
T0010) asserts that 1M `scroll_up(1)` calls finish in **under 2 seconds** of
wall-clock time, in an unoptimised debug build. That is a machine-speed
assertion, not a complexity assertion: on an idle machine it runs in ~0.45 s,
but under CPU contention it fails spuriously —

```
thread '...line_feed_flood_trims_in_bounded_time' panicked at grid.rs:535:9:
scrollback trimming is not O(1): 1M line-feeds took 4.310076387s
```

Observed during the T0032 review: the same command passed 3/3 on an idle
machine and failed while a parallel `cargo build` was saturating the CPU. On a
shared CI runner this is a red build with no defect behind it, which is exactly
the kind of failure that trains people to re-run instead of read.

## In scope

- Replace the absolute-time bound with a complexity assertion that does not
  depend on machine speed or load.
- Update the test comment to say what is actually being asserted.

## Out of scope

- The ring-buffer implementation itself (`VecDeque` + `pop_front`, T0010) —
  it is correct; only its test is fragile. **No production code changed.**
- Other timing-sensitive tests, if any turn up. One ticket, one defect.

## What was done

The old test was replaced by `line_feed_flood_trims_without_moving_lines`,
which uses **no clock at all** (`std::time` is no longer imported by the
module's tests).

Deviation from the original plan: the ticket first proposed a timing *ratio*
(time N vs 2N line-feeds, plus a 60 s safety ceiling). A ratio is still a
wall-clock measurement, and two separately timed runs under fluctuating load
can still diverge. The new test instead asserts the structural property that
*makes* a trim O(1), which is deterministic:

> Once the scrollback is full, dropping the oldest line moves **none** of the
> surviving lines in memory.

How it works (`rows == 1`, so every `scroll_up(1)` goes through the trim):

1. Scroll `MAX_SCROLLBACK + 1` times, so the deque is at the cap and its buffer
   has reached its steady-state capacity.
2. Record, for each scrollback line, its identity (the line's own heap buffer
   pointer, stable while that line lives) and the address of the slot holding
   it in the deque.
3. Scroll `MAX_SCROLLBACK / 2` more times. The newer half of the history
   survives every one of these trims.
4. Assert the length is still `MAX_SCROLLBACK`, the survivors are still in
   oldest-first order, and **zero** survivors changed slot.

An O(1) ring buffer (`push_back` + `pop_front`) never moves a surviving line.
Any O(n) trim (the old `Vec::remove(0)` shift-down, or a rebuild / re-collect
of the collection) has to move all of them, so the count is exactly 0 vs. all
survivors, with no threshold to tune.

## Red-run evidence

### The flake (old test, current `master`)

Reproduced on demand by pinning the test to one CPU that four busy loops also
compete for. The box was also carrying other agents' cargo builds (load
average ~50 on 16 cores), so it failed even un-pinned:

```
$ target/debug/deps/ferris_pad-* line_feed_flood            # un-pinned, busy box
test ...line_feed_flood_trims_in_bounded_time ... FAILED     (finished in 8.26s)

$ for i in 1 2 3 4; do taskset -c 0 sh -c 'while :; do :; done' & done
$ taskset -c 0 target/debug/deps/ferris_pad-* line_feed_flood
thread '...line_feed_flood_trims_in_bounded_time' panicked at src/app/services/terminal/grid.rs:540:9:
scrollback trimming is not O(1): 1M line-feeds took 17.699384064s
test result: FAILED. 0 passed; 1 failed
```

The ring buffer had no defect; only the clock was slow.

### New test under the same load: passes

Same pinned setup with four busy loops on CPU 0, 3 runs: all `ok`
(0.83–0.98 s each).

### New test catches the regression it guards (temporarily reintroduced, then reverted)

(a) The historical O(n) trim: `scrollback` back to `Vec`, trim with
`self.scrollback.remove(0)`:

```
thread '...line_feed_flood_trims_without_moving_lines' panicked at src/app/services/terminal/grid.rs:567:9:
assertion `left == right` failed: scrollback trimming is not O(1): 5000 trims moved 5000 of 5000 surviving lines
  left: 5000
 right: 0
test result: FAILED. 0 passed; 1 failed
```

(b) Keep `VecDeque` but trim by rebuilding it,
`self.scrollback = self.scrollback.drain(1..).collect();`:

```
assertion `left == right` failed: scrollback trimming is not O(1): 5000 trims moved 5000 of 5000 surviving lines
test result: FAILED. 0 passed; 1 failed
```

(c) Unbounded scrollback (the `pop_front` trim deleted):

```
panicked at src/app/services/terminal/grid.rs:556:9:
assertion `left == right` failed
  left: 15001
```

All three edits were reverted; the production code is byte-identical to
`master`.

## How to test

### Automated

```bash
nix develop -c cargo test --lib terminal::grid::tests
```

Expected: 16 passed, including
`line_feed_flood_trims_without_moving_lines`. It takes well under a second.

Under load (the case that used to fail):

```bash
nix develop -c cargo test --lib --no-run      # note the unittests binary path
for i in 1 2 3 4; do taskset -c 0 timeout 60 sh -c 'while :; do :; done' & done
taskset -c 0 target/debug/deps/ferris_pad-<hash> line_feed_flood
kill %1 %2 %3 %4
```

Expected: `ok`. The old test failed this way with `1M line-feeds took 17.7s`.

Optional sanity check that the test still guards the ring buffer: in
`TerminalGrid::scroll_up`, temporarily replace `self.scrollback.pop_front();`
with `self.scrollback = self.scrollback.drain(1..).collect();` and re-run.
Expected: FAIL with `5000 trims moved 5000 of 5000 surviving lines`. Revert.

### Manual check in the running app

This ticket changes only a unit test, so app behaviour should be the same:

1. `nix develop -c cargo run`, open the terminal panel.
2. Run `seq 1 200000` (more lines than the 10 000-line scrollback cap).
3. Expected: the output streams without stalling. Scrolling up with the mouse
   wheel stops at about line 190 000 (only the most recent ~10 000 lines are
   kept), and the panel stays responsive.

## Acceptance criteria

- [x] The test no longer asserts a wall-clock duration at all. It uses no clock.
- [x] It fails when the trim is made O(n) per line (checked with `Vec::remove(0)`
      and with a rebuild-on-trim, both reverted afterwards).
- [x] It passes on a loaded machine, where the old test does not.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `src/app/services/terminal/grid.rs` (test module only):
  `line_feed_flood_trims_in_bounded_time` replaced by
  `line_feed_flood_trims_without_moving_lines`, unused `std::time` import
  dropped.

## Notes

- Origin: T0032 review. Pre-existing on `master`, not introduced by T0032.
- Relevant to T0036 (CI gates on push/PR): a shared GitHub runner is precisely
  the noisy environment where the old bound misfired.
- The test reads the private `scrollback` field. That works because the test
  module sits inside `grid.rs`. It relies on `VecDeque` not moving elements on
  `push_back`/`pop_front` while below capacity, which is how a ring buffer works
  (and what makes those operations O(1)).
