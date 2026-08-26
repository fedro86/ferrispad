---
id: T0008
title: Bound Rust-side plugin work (diff size cap + symlink-safe dir scan)
status: done
created: 2026-08-24
severity: moderate
area: security
depends-on: []
---

## Goal

The plugin instruction/memory limits govern only the Lua VM; Rust-side work
reachable from the plugin API is unbounded. Two concrete DoS vectors:

1. `api:diff_text` → `plugins/diff.rs::TextDiff::from_chars` runs a
   character-level Myers diff on two plugin-supplied strings with no size cap;
   two 1 MB single-line strings hang the UI thread and allocate outside the
   16 MB budget.
2. `api/sandbox.rs::scan_dir_recursive` follows symlinks (`path.is_dir()`) with
   no visited-set and no result cap; a directory of 10 self-symlinks explodes
   to 10^10 entries at the depth-10 cap.

## In scope

- Cap the input size (and/or line length) accepted by `diff_text`; return an
  error above the cap instead of diffing.
- Make `scan_dir_recursive` symlink-safe: don't traverse into symlinked
  directories (or track visited canonical paths), and cap the total number of
  returned entries.

## Out of scope

- Moving diff off the UI thread.
- The general "plugins block the UI thread" concern (T0011).

## How to test

### Regression test

`plugins/diff.rs` unit test: `diff_text` with two 2 MB strings returns an error
(or completes under a small time bound), not an unbounded run.
`api/sandbox.rs` test: a temp dir containing a symlink to `.` yields a bounded,
terminating scan.

- Before the fix: the diff test hangs / OOMs; the scan test does not terminate.
- After the fix: both return promptly within caps.

## Acceptance criteria

- [x] `diff_text` rejects oversized inputs.
- [x] `scan_dir_recursive` terminates on symlink cycles and caps its output.
- [x] Regression tests added and green.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/plugins/api/filesystem.rs:355` — `diff_text` entry.
- `src/app/plugins/diff.rs:48` — `from_chars` call site (add the guard here).
- `src/app/plugins/api/sandbox.rs:85` — `scan_dir_recursive`.
- `src/app/plugins/api/filesystem.rs:134` — the depth cap that bounds it today.

## Notes

- Origin: plugin/services audit **M3**.

## Outcome (2-review)

**Vector 1 — diff size (`diff.rs` + `filesystem.rs`).**
- `compute_intraline_spans` now skips the O(n·m) `TextDiff::from_chars` when
  either line exceeds `MAX_INTRALINE_BYTES` (4096): the pair still shows as
  changed, just without intraline emphasis. This is the core fix and also
  protects the MCP diff path (`mcp/tools.rs`), which shares this code.
- `diff_text` rejects input where either side exceeds `MAX_DIFF_INPUT_BYTES`
  (1 MiB), returning a Lua error instead of diffing (`diff_input_too_large`).

**Vector 2 — directory scan (`sandbox.rs`).**
- `scan_dir_recursive` no longer follows symlinked directories: it reads the
  `DirEntry` file type (which does not follow the link) and recurses only when
  `is_dir && !is_symlink`. A symlink is still *listed*, never *descended*, so a
  cycle (e.g. a link back to `.`) can't diverge.
- Total output is capped at `MAX_SCAN_ENTRIES` (10 000), checked at entry and
  in the loop, bounding work even on a huge real tree.

**Red proofs (both temporarily reverted, run, restored — no residue).**
- Intraline: with the guard disabled, 5000-byte replacement lines produced
  non-empty spans (test asserts empty) → FAILED. `huge_single_line_diff_
  terminates` (two 2 MiB lines) would hang pre-fix; post-fix it returns at once.
- Symlink: with the `!is_symlink` gate removed, the scan descended
  `loop/loop/loop/…` to the depth-10 cap (36 entries under the symlink) → the
  `!rel_path.contains("loop/")` assertion FAILED. Post-fix the symlink is listed
  but never entered.

**Out of scope (unchanged):** moving the diff off the UI thread, and the general
"plugins block the UI thread" concern (T0011).

**Gates.** `cargo test` green (diff: 13 tests incl. 4 new; sandbox: 2 new),
`clippy --all-targets --all-features -- -D warnings` clean, `cargo fmt --check`
clean.

## How to verify (reviewer recipe)

```bash
nix develop -c cargo test --lib app::plugins::diff
nix develop -c cargo test --lib app::plugins::api::sandbox
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
