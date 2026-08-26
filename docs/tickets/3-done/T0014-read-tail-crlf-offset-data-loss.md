---
id: T0014
title: Fix read_tail/save_partial CRLF byte-offset (silent data loss)
status: done
created: 2026-08-24
severity: severe
area: robustness
depends-on: []
---

## Goal

`read_tail` computes a byte offset as `all_lines[..start].iter().map(|l| l.len()
+ 1).sum()`, but `str::lines()` strips `\r\n` entirely, so each CRLF line is
undercounted by one byte. That `start_byte` is then handed to `save_partial`,
which `seek(start_byte)` → `write_all` → `set_len(start_byte + new_len)`. On a
large CRLF file, tail-editing writes at the wrong offset and truncates —
**silent data loss**. A second variant: `String::from_utf8_lossy` on a 1 MB
chunk boundary can change byte length when a UTF-8 sequence is split.

## In scope

- Compute the byte offset from the actual bytes consumed (track real line
  terminator lengths, or use byte-oriented splitting that preserves `\r\n`),
  not `len() + 1`.
- Handle the chunk-boundary UTF-8 case without changing byte length (don't rely
  on `from_utf8_lossy` for offset math; operate on bytes).

## Out of scope

- The read-only memory-mapped path for very large files (unaffected).

## How to test

### Regression test

`services/file_size.rs` unit test with a CRLF fixture: build a multi-line
`\r\n` document, `read_tail` from line N, edit, `save_partial`, and assert the
resulting file equals the expected content byte-for-byte (no truncation, no
off-by-N shift).

- Before the fix: the saved file is truncated / shifted by the CRLF undercount.
- After the fix: byte-exact.

## Acceptance criteria

- [x] Tail edit + save on a CRLF file preserves all bytes at the right offset.
- [x] Chunk-boundary multibyte sequences don't corrupt the offset.
- [x] Regression test added and green; it lost data before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/services/file_size.rs:83-85,114,120,201-221` — tail offset + partial
  save.

## Notes

- Origin: plugin/services audit **S7**. This is the one finding that silently
  destroys user data, so it ranks high despite needing a large file to trigger.

## Outcome (2-review)

**Root cause.** Both branches of `read_tail` (the `< 1 MiB` whole-read and the
large-file backward-chunk read) computed the tail's start offset from
`str::lines()` string lengths: `all_lines[..start].iter().map(|l| l.len() + 1)`.
`str::lines()` strips the whole `\r\n`, so every skipped CRLF line was
undercounted by exactly 1 byte (the `\r`). In the large branch the offset was
also read off the `String::from_utf8_lossy` view, so a multibyte sequence split
at the 1 MiB read boundary (→ U+FFFD, a different byte length) shifted it too.

**Fix (one rule, both branches).** Added a byte-oriented helper
`byte_offset_after_lines(bytes, skip)` that returns the offset just past the
`skip`-th raw `\n`. Both branches now derive `start_byte` from the real file
bytes (small branch: `content.as_bytes()`; large branch: `&collected_bytes`),
never from `str::lines()` lengths or the lossy string. `save_partial` is
unchanged — it was already correct once handed the right offset. `read_chunk`
was already byte-exact (it counts `read_line`'s returned `n`) and is untouched.

**Red proof.**
- `read_tail_crlf_offset_is_byte_exact_and_save_preserves_prefix` (20 CRLF
  lines): before the fix `start_byte` was `136` vs the correct `153` (17 skipped
  lines × 1 `\r`); `save_partial` then truncated into the prefix. Now byte-exact
  and the prefix survives.
- `read_tail_large_file_offset_lands_on_line_boundary` (1 MiB + 4 bytes, a 3-byte
  char split at the backward-read boundary): before the fix `original[start_byte
  - 1]` was `49` (`'1'`, mid-line) instead of `10` (`\n`). Now it lands exactly
  on a line boundary and the tail is the three clean trailing lines.

**Gates.** `cargo test` (all green, 16 `file_size` tests incl. the 2 new),
`cargo clippy --all-targets --all-features -- -D warnings` clean, `cargo fmt
--check` clean.

## How to verify (reviewer recipe)

```bash
# Both new regression tests green:
nix develop -c cargo test --lib file_size
# Full gates:
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

Optional manual repro (before-fix behaviour): open a large-ish CRLF-terminated
log in FerrisPad (tail mode kicks in), edit near the bottom, save, and confirm
with `cmp`/`xxd` that the untouched prefix is byte-identical to the original.
