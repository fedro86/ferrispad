---
id: T0031
title: Deduplicate find_next/find_prev via a Direction enum
status: review
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 61
---

## Goal

`find.rs::find_next` and `find_prev` are the same ~60-line function — same state
dance, same regex/plain branch, same wrap-around retry, same error dialogs —
differing only in `find_in_text` vs `find_in_text_backward` and the wrap bound
(`0` vs `text.len()`). Both carry `#[allow(clippy::too_many_arguments)]`.
Collapse into one function parameterised by a `Direction` enum.

## In scope

- Introduce `enum Direction { Forward, Backward }` and one `find(dir, ...)`
  covering both; delete the duplication.

## Out of scope

- The case-insensitive offset bug (T0013): already landed on `master`, and this
  branch is based on it.
- The pre-existing regex defects listed under *Notes*. They live in
  `text_ops.rs` / the `regex-lite` dependency, not in the code this ticket
  deduplicates, and need their own tickets.

## What was done

All changes are in `src/ui/dialogs/find.rs`. No call sites outside it changed.

- `enum Direction { Forward, Backward }` carries the only three things that
  differed between the two old functions: which `text_ops` search to call
  (`search_once`), where the wrap-around retry starts (`wrap_start`: `0` if
  `from > 0` / `text.len()` if `from < text.len()`), and where the cursor and
  the next search resume after a hit (`resume_pos`: match end / match start).
- `struct SearchOptions { case_sensitive, use_regex }` bundles the two
  checkboxes.
- `search_wrapping(text, query, from, dir, opts)` is a pure function over
  `&str` that both directions share. It is one search pass plus a single
  wrap-around retry, and returns `Err` for an invalid regex.
- `FindState::locate` is the UI-free state handling: it restarts from the
  cursor when the query changed, otherwise resumes from the last match, and it
  updates the resume point only on a hit. `FindState::find` is the thin FLTK
  wrapper: it selects the match, moves the cursor, or shows the same "Cannot
  find" / "Invalid regex" dialogs as before.
- `find_next` / `find_prev` and both `#[allow(clippy::too_many_arguments)]`
  are deleted. `FindState` derives `Clone` (it holds only `Rc` cells), so each
  button closure clones the state instead of cloning two `Rc`s by hand.

### Behaviour-preservation evidence

- **Pin tests (committed, 20 tests, `ui::dialogs::find::tests`).** Both
  directions cover: a match at the cursor, skipping a match that straddles the
  cursor, wrap-around at start/end, a stale position past the end of the
  document, no match at all, case-insensitive vs case-sensitive matching,
  non-ASCII text (`İ`, which changes byte length when lowercased, and `é`),
  a cursor off a char boundary (no panic), regex with wrap, regex case option,
  and invalid regex. State tests cover repeated Next/Prev cycling and wrapping,
  same query resuming from the last match, a changed query restarting from the
  cursor, switching direction, and a miss or regex error leaving the resume
  point alone.
- **Oracle cross-check (temporary, not committed).** A verbatim copy of the old
  `find_next` / `find_prev` search blocks (FLTK calls replaced by return
  values) was compared against the new `FindState::locate`:
  - 34,880 single searches: 15 texts × 20 queries × 4 option combinations ×
    2 directions × every cursor position `0..=len+3`, including off-boundary
    and past-end positions.
  - 160,000 steps of random Next/Prev sequences with query changes and cursor
    moves.
  - Results, resume position and stored query were identical in every case.
  - The same run passed the pin tests, so they describe the old code, not
    only the new one.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::dialogs::find   # 20 pin tests
nix develop -c cargo test                            # full suite
```

### Manual (in the running app)

1. `nix develop -c cargo run`, then type or paste `foo bar Foo baz FOO` into a
   new tab and put the cursor at the very start.
2. Open Find (Ctrl+F). Enter `foo`, leave *Match case* off, and press **Find
   Next** four times. Expected: positions 0, 8 and 16 are selected in turn, and
   the fourth press wraps back to the first `foo`.
3. Press **Find Prev**. Expected: the same `foo` stays selected (direction
   switch re-selects the current match, as before). Press it again: it wraps
   to `FOO` at the end.
4. Tick *Match case* and change the query to `Foo`, then press **Find Next**.
   Expected: only `Foo` (position 8) is found, and pressing again keeps
   selecting it.
5. Enter `zzz` and press Find Next. Expected: a "Cannot find 'zzz'" dialog.
6. Tick *Use regex* and enter `ba[rz]`, then press Find Next / Find Prev.
   Expected: `bar` and `baz` cycle with wrap-around. Enter `(` and press Find
   Next. Expected: an "Invalid regex" dialog.
7. Repeat steps 2 and 6 in Find & Replace (Ctrl+H). Expected: same results.

## Acceptance criteria

- [x] One search function; no `too_many_arguments` allow.
- [x] Forward/backward/wrap/regex behaviour unchanged (pin tests + oracle
      cross-check).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/find.rs`: `Direction`, `SearchOptions`, `search_wrapping`,
  `FindState::{start_pos, locate, find}` replace `find_next` / `find_prev`;
  four button callbacks updated; new `#[cfg(test)] mod tests`.

## Notes

- Origin: UI audit (MODERATE).
- **Pre-existing behaviour found while pinning. None of it was changed; the
  oracle shows the old code does exactly the same.** Each item needs its own
  ticket:
  1. **Regex anchors are evaluated against a sliced haystack**
     (`find_in_text_regex` / `find_in_text_regex_backward` in `text_ops.rs`).
     Forward search runs the regex on `text[start..]`, so `^` matches at the
     cursor even mid-line. Example: `^foo` from byte 1 of `"xfoo\nfoo"`
     returns `(1, 4)`. Backward search runs on `text[..end]`, so `$` matches at
     the cursor. Example: `foo$` backward from 3 in `"foox\nfoo"` returns
     `(0, 3)`. `\b` has the same issue. Fix: search the full text with
     `Regex::find_at` (forward) and filter `find_iter` by `m.end() <= end`
     (backward).
  2. **Zero-width regex matches never advance.** `^`, `x*` or `\b` returns an
     empty match at the resume point, `resume_pos` equals that same point, so
     repeated Find Next stays stuck. Example: `^` in `"ab\ncd"` from 0 gives
     `(0, 0)` three times.
  3. **Regex case-insensitivity is ASCII-only.** The crate is `regex-lite`,
     which has no Unicode case folding. With *Match case* off, plain search
     matches `CAFÉ` for `café`, but regex search does not. Example: regex
     `café`, case-insensitive, from byte 4 of `"café CAFÉ café"` returns
     `(12, 17)` and skips `CAFÉ` at 6. Fixing this either means switching to
     `regex` with `unicode-case` (binary size) or documenting the limitation.
     That is a product decision.
  4. **UX quirk:** after Find Next, Find Prev selects the same match once
     (and vice versa), because the resume point is the match end / match start.
     This is pinned by `switching_direction_reselects_the_current_match`.
     Whether to change it is a product decision.
