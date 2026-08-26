---
id: T0013
title: Fix case-insensitive find/replace offset mismatch (wrong replace / panic)
status: done
created: 2026-08-24
severity: severe
area: robustness
depends-on: []
---

## Goal

Case-insensitive find/replace lowercases the haystack
(`text[start..].to_lowercase()`) and then uses the match offset from the
*lowercased* string as an index into the *original*. `to_lowercase()` is not
length-preserving (`İ` U+0130 is 2 bytes → 3; `ẞ` → `ß`), so for any document
containing such characters the offset is wrong: `result[found..found+len]` and
`replace_range` land at the wrong position, or out of bounds / off a char
boundary → panic. The backward search has the same unguarded `&text[..end]`.

## In scope

- Compute match positions against the original string (e.g. case-insensitive
  search that maps back to original indices, or iterate char indices), so
  replace offsets are always valid indices into the original text.
- Guard the backward-search slice with a char boundary.

## Out of scope

- The regex variants (`:159`, `:178`) — they already use `floor_char_boundary`.

## How to test

### Regression test

```rust
// text_ops.rs tests
let text = "aİb"; // contains U+0130
let hit = find_in_text(text, "i", /*case_insensitive*/ true, 0);
// assert the returned offset indexes 'İ' in the ORIGINAL, and a replace at it
// produces the intended string without panicking.
```

Also a replace-all over a string mixing such characters.

- Before the fix: wrong replacement position, or a panic on slice.
- After the fix: correct position, no panic.

## Acceptance criteria

- [x] Case-insensitive find/replace returns offsets valid for the original text.
- [x] Backward search never slices off a char boundary.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/text_ops.rs` — new `LowerMap`/`find_ci`/`rfind_ci`;
  `find_in_text`/`find_in_text_backward` now return `Option<(usize, usize)>`
  (valid original start+end); backward slice guarded with `floor_char_boundary`;
  `replace_all_in_text` uses the real `(start, end)`.
- `src/ui/dialogs/find.rs` — the two non-regex callers use the returned
  `(start, end)` instead of `pos + query.len()`.

## Notes

- Origin: plugin/services audit **S6**.

## Outcome (2-review)

The bug had **two** faces: the forward search took a match offset from the
lowercased haystack and used it as an index into the original, and every caller
(and `replace_all`) then derived the match end as `start + search.len()`. Both
are wrong when `to_lowercase()` changes byte lengths (`İ` 2→3 bytes) → wrong
replace position or an out-of-bounds / non-boundary slice → panic.

Fix: `find_in_text`/`find_in_text_backward` now return `Option<(usize, usize)>`
— the match's real `(start, end)` in the **original**. Case-insensitive search
goes through a `LowerMap` that records, per lowered byte, the original offset it
came from and whether it starts an original character; a match is only reported
when it aligns to whole characters, so the returned offsets are always valid
`text` indices and never split a codepoint. The backward slice is floored to a
char boundary. `replace_all` and the two find-dialog callers use the returned
`end` instead of `start + search.len()`. Regex paths were already correct
(`floor_char_boundary`) and are untouched.

Signature change rippled into ~7 existing unit tests (`Some(0)` → `Some((0,
5))`, etc.) and the two `find.rs` callers — all updated.

### Red proof (temporarily restoring the buggy offset logic)

- `test_replace_ci_near_multibyte_lowercasing` — `replace_all("aİb","b","X")`:
  **panicked** (out-of-bounds slice `4..5` on a 4-byte string); now `"aİX"`.
- `test_find_ci_maps_offsets_back_to_original` — `find_in_text("aİb","b")`:
  returned `Some((4, 5))` (past the `b`) before, `Some((3, 4))` after.

Plus `test_replace_ci_replaces_full_original_char`,
`test_replace_all_ci_mixed_multibyte`, `test_find_backward_ci_multibyte`, and
the updated pre-existing find/replace tests.

### Verification recipe

```bash
nix develop -c cargo test --lib text_ops::tests
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
