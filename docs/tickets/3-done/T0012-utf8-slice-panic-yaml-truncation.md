---
id: T0012
title: Fix UTF-8 byte-slice panic in YAML value truncation
status: done
created: 2026-08-24
severity: severe
area: robustness
depends-on: []
---

## Goal

`yaml_parser.rs:113` truncates a long value with `&s[..47]`, slicing by **byte**
offset. A value of 17 CJK characters is 51 bytes, and byte 47 is not a char
boundary → panic. Reachable from any YAML file opened in the tree viewer, or
from a plugin's `yaml_content`. This panics on ordinary user data.

## In scope

- Truncate on a char boundary instead of a byte offset. Reuse
  `services/text_ops.rs::floor_char_boundary` (which exists for exactly this),
  or truncate by `chars().take(n)`.

## Out of scope

- The other raw-slice sites in `text_ops.rs` (T0013 covers the find/replace
  ones).

## How to test

### Regression test

```rust
// yaml_parser.rs tests
let s = "字".repeat(17); // 51 bytes
let _ = truncate_value(&s); // must not panic
```

- Before the fix: panics with "byte index 47 is not a char boundary".
- After the fix: returns a valid truncated string ending in `...`.

## Acceptance criteria

- [x] Truncation never slices mid-codepoint for any input.
- [x] Regression test added and green; it panicked before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/yaml_parser.rs` — `value_to_string` now truncates at
  `floor_char_boundary(s, 47)` instead of `&s[..47]`.
- `src/app/services/text_ops.rs` — `floor_char_boundary` made `pub(crate)` so it
  can be reused (it was private).

## Notes

- Origin: plugin/services audit **S5**. `text_ops.rs:129` already documents this
  exact hazard — the fix exists in the codebase and wasn't applied here.

## Outcome (2-review)

Reused the existing helper rather than duplicating logic: `floor_char_boundary`
(already in `text_ops.rs` for exactly this) is now `pub(crate)`, and
`value_to_string` slices `&s[..floor_char_boundary(s, 47)]`. Behaviour is
identical for ASCII (byte 47 is a boundary) and safe for multi-byte — the
byte-bounded truncation intent is preserved (snaps *down* to the nearest
boundary), rather than switching to `chars().take(n)` which would change the
display width for CJK.

Red proof: `value_to_string_truncates_multibyte_without_panic` feeds
`"字".repeat(17)` (51 bytes; byte 47 is inside a codepoint). Before the fix it
panicked with "end byte index 47 is not a char boundary; it is inside '字'";
after, it returns `"字"×15 + "..."` (47 floors to the 45-byte boundary). The
pre-existing ASCII truncation test still passes unchanged.

### Verification recipe

```bash
nix develop -c cargo test --lib yaml_parser::tests
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
