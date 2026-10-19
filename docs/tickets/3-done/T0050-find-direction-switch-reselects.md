---
id: T0050
title: Switching between Find Next and Find Prev moves to the adjacent match
status: done
created: 2026-10-19
severity: minor
area: ui
depends-on: [T0031, T0048]
issue: 91
---

## Goal

After Find Next selects a match, the first Find Prev selects the **same** match
again, and the same happens the other way round. The user has to press twice to
move. Every press should move to a different match, as in most editors.

## In scope

- In `src/ui/dialogs/find.rs`, when the direction changes and the current
  selection is exactly the last match, resume from the far side of that match:
  its start for Prev after Next, its end for Next after Prev.
- Update T0031's pin test `switching_direction_reselects_the_current_match` to
  the new behaviour, and say so in the ticket.

## Out of scope

- Any other change to the find logic (T0048 and T0049 cover regex).

## What was done

- **One rule instead of a direction-switch special case.** `FindState` used to
  remember a single resume *position* (`search_pos`): the match end after Next,
  the match start after Prev. That position is right only for the direction
  that wrote it, which is where the bug came from. It now remembers the last
  selected *span* (`resume_span: (start, end)`), and every search resumes from
  its own side of that span through the existing `Direction::resume_pos`. Next
  resumes from the span's end and Prev from its start, whichever direction
  selected it. Repeated presses in one direction behave exactly as before. The
  first press in the other direction now resumes from the far side of the
  current match.
- A cursor position (new query, or after Replace) is stored as the empty span
  `(p, p)`, so both directions resume from `p` as before.
- **Works with T0048.** An empty regex match (`^`, `$`) is an empty span, so it
  has no far side. Both directions resume from the same point, and T0048's
  `search_wrapping` steps one character past an empty match sitting exactly
  at the start point. Switching direction on `^` therefore moves to the
  adjacent line start. `search_wrapping`, `Direction::step` and `resume_pos`
  are unchanged.
- **Tests (`src/ui/dialogs/find.rs`):**
  - T0031's pin test `switching_direction_reselects_the_current_match` is
    **replaced** by `switching_direction_moves_to_adjacent_match`, which tests
    the new behaviour. On `foo foo foo`, Next, Next, Prev select 0, 4, 0, and
    then Prev, Prev, Next select 8, 4, 8.
  - New `switching_direction_moves_past_empty_regex_matches` (`^` on
    `a\nb\nc`: Next 0, 2, 4, then Prev 2, 0, then Next 2). It already passes on
    the T0048 code, because empty spans have no far side. It guards the stacked
    rule and is not a red test.
  - New `switching_direction_with_a_single_match_keeps_it`: with one match,
    both directions wrap back onto it.
  - The other pin tests are unchanged in behaviour. Three of them read the
    resume state directly, so their assertions now compare the span instead of
    the position. The values are the same: `1` becomes `(0, 1)`, `8` becomes
    `(8, 9)`, and `2`/`1` become `(2, 2)`/`(1, 1)`.
- The "current selection is exactly the last match" condition is the state's
  remembered span. As before, `FindState` does not read the editor selection
  while the query is unchanged, so no selection check was added.

### Red run (before the fix)

```
test ui::dialogs::find::tests::switching_direction_moves_to_adjacent_match ... FAILED
thread 'ui::dialogs::find::tests::switching_direction_moves_to_adjacent_match' panicked at src/ui/dialogs/find.rs:840:9:
assertion `left == right` failed
  left: Ok(Some((4, 7)))
 right: Ok(Some((0, 3)))
test result: FAILED. 24 passed; 1 failed; 0 ignored; 0 measured; 437 filtered out
```

That is the third press (Prev after Next, Next). It reselected `4..7` instead
of moving to `0..3`. After the fix, all 25 `ui::dialogs::find` tests pass.

## How to test

```bash
nix develop -c cargo test --lib ui::dialogs::find
```

- `switching_direction_moves_to_adjacent_match`: red before, green after.
- `switching_direction_moves_past_empty_regex_matches`,
  `switching_direction_with_a_single_match_keeps_it`: green.

Manual check in the running app (`nix develop -c cargo run`):

1. Type `foo bar Foo baz FOO` into a new tab and open Find (Ctrl+F).
2. Search for `foo` with "Match case" off and "Use regex" off.
3. Press Find Next twice: `Foo` at position 8 is selected.
4. Press Find Prev: `foo` at position 0 is selected (not `Foo` again).
5. Press Find Next: `Foo` at 8 is selected again (not `foo` at 0).
6. Regex check: type `a`, `b`, `c` on three lines, search `^` with "Use regex"
   on. Press Find Next until the cursor is at the start of line 3, then press
   Find Prev. The cursor moves to the start of line 2.

## Acceptance criteria

- [x] Red test fails before and passes after; the other pin tests are unchanged.
- [x] Gates green.

## Affected files

- `src/ui/dialogs/find.rs`

## Notes

- Origin: T0031 pinning finding (4), 2026-10-19.
- Stacked on T0048 (`ticket/T0048`, PR #88). Merge that first.
