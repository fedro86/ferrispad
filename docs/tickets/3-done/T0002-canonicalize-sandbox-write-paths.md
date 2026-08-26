---
id: T0002
title: Canonicalize sandbox write paths to close the path-traversal escape
status: done
created: 2026-08-24
severity: severe
area: security
depends-on: []
---

## Goal

A plugin can write outside its project root. For write operations to a
not-yet-existing target, `resolve_and_validate` returns the **non-canonical**
joined path — still containing `..` segments — after only checking that the
nearest *existing* ancestor is inside the root. `create_dir_all` / `fs::write`
then resolve the `..` lexically, so `create_dir("nonexistent/../../escaped")`
materialises a directory outside the sandbox, and `write_file` gets an
arbitrary out-of-root write the same way. **Verified by reading the code.**

## In scope

- In `resolve_and_validate` (the `PathValidation::NotFound` branch), do not
  return `full` with `..` intact. Normalise the path (lexically remove `.`/`..`
  without touching the filesystem, or canonicalize the parent and re-append the
  final component) and re-verify the *normalised* result `starts_with` the
  canonical project root before returning `Ok(Some(_))`.
- Reject any path that still escapes after normalisation.

## Out of scope

- The read/existing-path branch (`PathValidation::Valid`) — it already
  canonicalizes.
- Symlink-following in directory scans (T0008).

## How to test

### Regression test

Extend the existing sandbox tests in `plugins/api/mod.rs` (the traversal cases
around `:490-528`) with the `..`-through-existing-ancestor shape:

```rust
// project_root = tmp; create tmp/sub first
assert!(resolve_and_validate("sub/../../escaped", &root).unwrap().is_none());
```

- Before the fix: returns `Ok(Some("tmp/sub/../../escaped"))` — a path that
  resolves outside `tmp`.
- After the fix: returns `Ok(None)`.

### Manual repro

Plugin calls `api:create_dir("x/../../ferrispad_escape")` inside a project;
before: a dir appears one level above the project root; after: blocked with the
`[plugin:security] path blocked` message.

## Acceptance criteria

- [x] No path containing traversal that escapes the root is ever returned as
      valid, whether or not the target exists yet.
- [x] Existing in-root write paths (including legitimate nested creation) still
      succeed.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/app/plugins/api/sandbox.rs` — replaced the ancestor-walk in the
  `NotFound` branch with a lexical `normalize_lexical()` + `starts_with` on the
  canonical root.
- `src/app/plugins/api/filesystem.rs` — verified; callers pass the resolved
  path straight to `create_dir_all`/`fs::write`, so a normalised in-root path is
  fully compatible. No change.

## Notes

- Origin: plugin/services audit **S2** (verified). `create_dir_all`'s ancestor
  walk is lexical (`Path::parent()`), not kernel-resolved, which is why a
  lexical `..` survives.

## Outcome (2-review)

**The ticket's suggested repro was subtly wrong and I corrected it.** The
example `sub/../../escaped` (with `sub` created) is *already blocked* by the
current code: `validate_path` canonicalizes the parent `sub/../..` → `/tmp`,
sees it outside the root, and returns `OutsideProjectRoot`. The real escape
needs the `..` segments hidden **behind a component that does not exist**, e.g.
`missing/../../escaped`: `canonicalize` then fails on every ancestor containing
`missing`, so the old walk popped straight up to `root` (in-root) and returned
the raw joined path with `..` intact. `create_dir_all`/`fs::write` later resolve
that `..` against the kernel and land outside the sandbox.

Fix (simplify, don't layer): the whole ancestor-walk loop is gone. The
`NotFound` branch now folds `.`/`..` lexically (`normalize_lexical`, no
filesystem access) against the **canonical** root and requires the result to
`starts_with` it. Symlinks *inside* the path remain out of scope (T0008), as the
ticket says.

Regression tests in `plugins::api::tests`:
- `test_resolve_and_validate_traversal_behind_missing_dir_blocked` — the escape;
  **failed before** (returned `Some(".../missing/../../escaped")`), passes now.
- `test_resolve_and_validate_deep_new_dirs_inside_allowed` — the `create_dir_all`
  positive case (`a/b/c.txt`, no dirs exist yet) still resolves in-root.
- All four pre-existing S1 traversal tests stay green.

### Verification recipe

```bash
nix develop -c cargo test --lib plugins::api::tests::test_resolve_and_validate
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```
