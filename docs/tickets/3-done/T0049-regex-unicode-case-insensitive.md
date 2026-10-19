---
id: T0049
title: Case-insensitive regex find matches non-ASCII letters
status: done
created: 2026-10-19
severity: minor
area: robustness
depends-on: [T0048]
issue: 89
---

## Goal

With "Match case" off, a plain search for `café` finds `CAFÉ`, but a regex
search does not. `regex-lite` has no Unicode case folding, so only ASCII letters
fold. Regex search should fold case the same way plain search does, or the UI
should say clearly that it doesn't.

## In scope

- Measure the release-binary size of switching `regex-lite` to `regex` with only
  the features needed (`std`, `unicode-case`, `unicode-perl` if `\w`/`\b` should
  be Unicode-aware). Compare with `ls -l target/release/FerrisPad` before and
  after, and record both numbers here.
- Decide using that measurement. The single-binary and frugality constraints
  apply. The ticket proposes this rule, to be confirmed with the user:
  - if the binary grows by less than about 1 MB, switch to `regex`;
  - otherwise keep `regex-lite` and add a short hint next to "Use regex" in the
    Find dialog ("case folding: ASCII only").
- Either way, add a test that pins the chosen behaviour for `café` / `CAFÉ`.

## Out of scope

- Regex syntax differences between the two crates beyond case folding. If the
  switch changes accepted syntax, list the differences here.

## Size measurement

Built with `nix develop -c cargo build --release` on the branch base
(`origin/ticket/T0048`, `51eaf2c`), size via `stat -c %s target/release/FerrisPad`.
The release profile does not strip, and neither CI nor
`scripts/build-releases.sh` strips, so the unstripped size is what ships.

| Build | Unstripped (shipped) | Stripped (`strip`, for reference) |
|-------|---------------------:|----------------------------------:|
| `regex-lite = "0.1"` (base) | 12 246 848 B | 9 940 512 B |
| `regex = { version = "1", default-features = false, features = ["std", "unicode-case", "unicode-perl"] }` | 13 709 960 B | 11 017 792 B |
| **Delta** | **+1 463 112 B (≈1.46 MB / 1.40 MiB, +11.9 %)** | +1 077 280 B (≈1.08 MB / 1.03 MiB) |

The `regex` build replaced every `regex_lite` use (`text_ops.rs`, the Markdown
preview link rewriter in `controllers/preview.rs`, the plugin config validator
in `ui/dialogs/plugin_config.rs`) so `regex-lite` dropped out of the binary
entirely; `regex` pulled in `regex-automata` and `aho-corasick`
(`regex-syntax` 0.8.9 was already linked via `syntect`). `perf` was not
enabled: it would only add size, and the delta was already over the threshold
without it. `unicode-perl` cannot be dropped: without it, `\w`/`\d`/`\s`/`\b`
in Unicode mode are compile errors in `regex`, which would break existing
patterns such as `(\w+)`.

## Decision (to be confirmed by the user)

**Over the ~1 MB threshold → do NOT switch.** `regex-lite` stays. The Find and
Find & Replace dialogs now show a dimmed `(case folding: ASCII only)` hint right
of the "Use regex" checkbox, and tests pin the ASCII-only behaviour. If the user
prefers Unicode folding at +1.46 MB, the switch is a 4-line change
(`Cargo.toml` + three `use` lines) plus flipping the pinning tests below; the red
run for that path is recorded under *Red-test evidence*.

## Syntax that differs between the crates (reachable from the Find dialog)

From the `regex-lite` 0.1.9 docs ("Differences with the regex crate"), and for
`regex` with only `std` + `unicode-case` + `unicode-perl`:

- Case-insensitive matching: `regex-lite` folds ASCII only; `regex` folds
  Unicode simple case (`é`/`É`, `ß`/`ẞ`, Greek, Cyrillic...).
- `\d`, `\s`, `\w` and `\b`/`\B`: ASCII in `regex-lite` (`[0-9]`,
  `[\t\n\v\f\r ]`, `[0-9A-Za-z_]`); Unicode-aware in `regex` with
  `unicode-perl` (e.g. `\w` matches `É`, `\d` matches `٣`).
- `\p{...}` / `\P{...}`: rejected by `regex-lite`; also rejected by the
  minimal `regex` build (needs `unicode-gencat`/`unicode-script`/...), but with a
  different error message.
- Character-class set operations `&&`, `--`, `~~`: `regex` only.
- The `u` flag (`(?-u)`): accepted and ignored by `regex-lite`; in `regex` it
  switches to ASCII mode, and with a `&str` haystack patterns that could match
  invalid UTF-8 (e.g. `(?-u:\xFF)`) are rejected.

The same differences would have applied to plugin `regex:PATTERN` config
validators and the preview link rewriter.

## What was actually done

- `src/ui/dialogs/find.rs`: new `regex_case_folding_hint()` helper (dimmed,
  11 px label `"(case folding: ASCII only)"`, `theme.text_dim`), placed right
  of "Use regex" in both the Find and the Find & Replace dialog; the checkbox
  width shrank from 200 to `REGEX_CHECK_W = 100` px to make room.
- `src/app/services/text_ops.rs`: four tests pinning the chosen behaviour:
  `regex_case_insensitive_is_ascii_only` (`café` does not find `CAFÉ`, `cafÉ`
  does), `regex_case_insensitive_is_ascii_only_backward_and_replace`,
  `regex_case_sensitive_distinguishes_non_ascii`, `regex_word_class_is_ascii_only`.
- No dependency change: `Cargo.toml` / `Cargo.lock` are untouched.
- All existing regex tests (T0031 pins, T0048 anchors/empty-match) stay green.

## Red-test evidence

Before the decision, the switch-path tests were written and run against
`regex-lite` (the base). They fail for the bug's reason: no non-ASCII folding,
ASCII-only `\w`:

```
test app::services::text_ops::tests::regex_case_insensitive_folds_non_ascii ... FAILED
test app::services::text_ops::tests::regex_word_class_covers_non_ascii_letters ... FAILED
test app::services::text_ops::tests::regex_case_insensitive_folds_non_ascii_backward_and_replace ... FAILED

---- app::services::text_ops::tests::regex_case_insensitive_folds_non_ascii stdout ----
assertion `left == right` failed
  left: None
 right: Some((0, 5))

---- app::services::text_ops::tests::regex_word_class_covers_non_ascii_letters stdout ----
assertion `left == right` failed
  left: Some((0, 3))
 right: Some((0, 5))

---- app::services::text_ops::tests::regex_case_insensitive_folds_non_ascii_backward_and_replace stdout ----
assertion `left == right` failed
  left: ("tea CAFÉ", 1)
 right: ("tea tea", 2)

test result: FAILED. 4 passed; 3 failed; 0 ignored; 0 measured; 457 filtered out
```

Since the size rule says "keep `regex-lite`", these tests were turned into the
ASCII-only pins above (same inputs, asserting today's documented results).

## How to test

```bash
nix develop -c cargo test --lib text_ops::tests::regex_
```

Expect 7 passing tests, including the four T0049 pins listed above and the
three T0048 anchor tests.

Manual (UI only, no unit test drives the FLTK dialog):

1. `nix develop -c cargo run`, type `CAFÉ cafe` into a new tab.
2. Ctrl+F. Next to "Use regex" a dimmed `(case folding: ASCII only)` hint is
   visible and not clipped; "Use regex" itself is fully readable.
3. Tick "Use regex", leave "Match case" off, search `café` → "not found"
   (expected, matches the hint). Search `cafÉ` → `CAFÉ` is selected.
4. Untick "Use regex", search `café` → `CAFÉ` is selected (plain search folds
   Unicode).
5. Ctrl+H (Find & Replace): the same hint shows next to "Use regex".
6. Switch theme (light/dark): the hint stays readable (dim text color).

## Acceptance criteria

- [x] Size delta measured and recorded; decision pending user confirmation.
- [x] Behaviour pinned by a test; gates green.

## Affected files

- `src/app/services/text_ops.rs`, `src/ui/dialogs/find.rs`

## Notes

- Origin: T0031 pinning finding (3), 2026-10-19.
