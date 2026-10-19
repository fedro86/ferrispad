---
id: T0048
title: Regex find respects line anchors at the cursor and steps past empty matches
status: review
created: 2026-10-19
severity: moderate
area: robustness
depends-on: [T0031]
issue: 82
---

## Goal

Regex Find gives wrong results in two cases:

- **Anchors are evaluated on a cut-off slice.** `find_in_text_regex` searches
  `&text[start..]`, so `^`, `$` and `\b` see the cursor as the start of the text.
  `^foo` searched forward from byte 1 of `"xfoo\nfoo"` returns `(1,4)`, a match
  in the middle of a line. The backward search slices `&text[..end]`, so `$`
  matches at the cut the same way.
- **Empty matches never advance.** A pattern that can match zero characters
  (`^`, `$`, `a*`, `\b`) returns an empty match at the cursor, and Find Next
  resumes at the match end, which is the same position. Repeated Find Next on
  `^` stays stuck at `(0,0)`.

Both were found while pinning behaviour for T0031 (see its ticket Notes).

## In scope

- Forward: search the whole text from the cursor with `Regex::find_at(text, start)`
  (regex-lite has it), so anchors and word boundaries see the real context.
- Backward: take the last match that ends at or before the cursor, searching the
  whole text instead of a prefix slice.
- Empty match at the resume position: step forward (or backward) by one
  character, respecting char boundaries, and search again. Apply this in
  `search_wrapping` / `Direction::resume_pos` in `src/ui/dialogs/find.rs`, the
  single shared path since T0031.

## Out of scope

- Unicode case folding in regex mode (T0049).
- Replace All semantics (`replace_all_in_text_regex`), unless the same slice bug
  is found there; if so, note it and open a ticket.

## What was done

- **`text_ops::find_in_text_regex`** now runs `Regex::find_at(text, start)` on
  the whole text instead of `find(&text[start..])`, so `^`, `$` and `\b` see the
  characters around the cursor. The start is still snapped with
  `floor_char_boundary`.
- **`text_ops::find_in_text_regex_backward`** scans the whole text with
  `find_iter` and keeps the last match that ends at or before the cursor
  (`take_while(end <= cursor).last()`). Successive matches never overlap, so
  their ends only grow and the scan stops at the first match past the cursor.
- **`find.rs::search_wrapping`** has one rule for empty matches: an empty match
  sitting exactly at the search start is where the previous search stopped, so
  the search steps one character in its direction (new `Direction::step`, which
  respects char boundaries) and searches again. The wrap-around pass is
  unchanged. If nothing else matches, the skipped empty match is returned, the
  same result as wrapping all the way round (for example `^` on a one-line
  document). `resume_pos` is unchanged: an empty match still puts the cursor
  on the match.
- The rule applies to every Find Next / Find Prev, including the first press
  after the query changes. VS Code's find widget behaves the same way when it
  moves to the next match: an empty match at the cursor never counts, so each
  press moves. For this reason `empty_regex_match_advances_on_find_next` starts
  with the cursor at the end of the text (where it sits after pasting). The
  first press then wraps to line start 0, and the next presses give 2, 4, and 0
  again.
- Backward stepping cannot lose a non-empty match that ends at the start
  point. regex-lite never reports an empty match right at the end of the
  previous match (a probe showed `a*` on `"aa"` → `[(0,2)]` and on `"baab"` →
  `[(0,0),(1,3),(4,4)]`). The test `search(Backward, "baa", "a*", 3)` →
  `(1,3)` pins this.

### Other regex helpers checked (no change needed)

- `replace_all_in_text_regex` and `replace_first_regex` run on the whole text.
  They have no cursor slice, so they do not have this bug.
- `replace_at_position_regex` already uses `captures_at(text, start)`, the
  correct full-context form.

### Tests added

The ticket's three tests, named as the ticket names them:

- `text_ops::tests::regex_caret_does_not_match_mid_line_at_cursor`
- `text_ops::tests::regex_dollar_backward_does_not_match_at_cut`
- `find::tests::empty_regex_match_advances_on_find_next`

Three more for the same two bugs:

- `text_ops::tests::regex_word_boundary_sees_text_before_cursor`: `\bo` from
  byte 1 (inside `foo`) of `"foo oo"` gives `(4,5)`, not `(1,2)`.
- `find::tests::empty_regex_match_steps_back_on_find_prev`: four Find Prev on
  `$` over `"a\nb\nc"` from the cursor at 0 give 5, 3, 1, then 5 again.
- `find::tests::empty_match_at_the_start_point_is_skipped`: the core rule in
  both directions, empty matches at the document ends that are reached by
  wrapping, and no lost non-empty match ending at the start point.

T0031's 20 pin tests in `find.rs` pass **unchanged**. None of them pinned
either bug, so none was updated.

### Red run (before the fix)

```
---- app::services::text_ops::tests::regex_dollar_backward_does_not_match_at_cut stdout ----
assertion `left == right` failed
  left: Some((1, 2))
 right: None
---- app::services::text_ops::tests::regex_caret_does_not_match_mid_line_at_cursor stdout ----
assertion `left == right` failed
  left: Some((1, 4))
 right: Some((5, 8))
---- ui::dialogs::find::tests::empty_match_at_the_start_point_is_skipped stdout ----
assertion `left == right` failed
  left: Some((2, 2))
 right: Some((4, 4))
---- ui::dialogs::find::tests::empty_regex_match_advances_on_find_next stdout ----
assertion `left == right` failed
  left: [Ok(Some((5, 5))), Ok(Some((5, 5))), Ok(Some((5, 5))), Ok(Some((5, 5)))]
 right: [Ok(Some((0, 0))), Ok(Some((2, 2))), Ok(Some((4, 4))), Ok(Some((0, 0)))]
---- ui::dialogs::find::tests::empty_regex_match_steps_back_on_find_prev stdout ----
assertion `left == right` failed
  left: [Ok(Some((0, 0))), Ok(Some((0, 0))), Ok(Some((0, 0))), Ok(Some((0, 0)))]
 right: [Ok(Some((5, 5))), Ok(Some((3, 3))), Ok(Some((1, 1))), Ok(Some((5, 5)))]
---- app::services::text_ops::tests::regex_word_boundary_sees_text_before_cursor stdout ----
assertion `left == right` failed
  left: Some((1, 2))
 right: Some((4, 5))

test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 454 filtered out
```

Before the fix, the cursor-at-end case matched `^` at byte 5 because an empty
slice counts as a line start. Both bugs show up in that one result, and it
stays stuck at 5.

## How to test

### Regression tests

```bash
nix develop -c cargo test --lib -- text_ops::tests::regex_ ui::dialogs::find
```

The ticket's original recipe filtered on `text_ops::tests::regex_anchor`, but
no test name matches that filter. `text_ops::tests::regex_` selects exactly the
new text_ops tests, because the older ones are named `test_find_regex_*`.
`ui::dialogs::find` runs all 23 find-dialog tests: T0031's 20 plus 3 new.

### Manual

1. Type `xfoo` on line 1 and `foo` on line 2. Put the cursor right after the
   `x`. Press Ctrl+F, tick "Use regex", search `^foo`, and press Find Next.
   **Expected:** the `foo` on line 2 is selected, not the `foo` after `x`.
2. Type `a`, `b`, `c` on three lines and leave the cursor at the end. Search
   regex `^` and press Find Next four times. **Expected:** the cursor goes to the
   start of line 1, then line 2, then line 3, then wraps back to line 1. Before
   the fix it never moved.
3. Same text, search regex `$`, and press Find Prev repeatedly. **Expected:**
   the cursor goes to the end of line 3, then line 2, then line 1, then wraps
   back to line 3.
4. Put `foo bar` on one line. Put the cursor right after `foo b` (inside
   `bar`), search regex `\bar`, and press Find Next. **Expected:** "Cannot find
   '\bar'", because no word starts with `ar`. Before the fix, the search treated
   the cursor as a word boundary and selected the `ar` inside `bar`.

## Acceptance criteria

- [x] All three red tests failed before the fix and pass after it (as did the
      three extra tests).
- [x] T0031's 20 pin tests still pass, all of them unchanged.
- [x] Gates green.

## Affected files

- `src/app/services/text_ops.rs`: `find_in_text_regex`, `find_in_text_regex_backward`
- `src/ui/dialogs/find.rs`: `search_wrapping` / `resume_pos`

## Notes

- Origin: T0031 pinning findings (1) and (2), 2026-10-19.
