---
id: T0089
title: Run the FLTK widget tests on the main thread so they pass on macOS
status: done            # todo | review | done
created: 2026-11-26
severity: ~
area: process
depends-on: [T0076]     # macos-gates.yml only exists on T0076's branch; land after it
issue: 173
---

## Goal

The macOS gate (`macos-gates.yml`, added by T0076) cannot run two library
unit tests that build FLTK widgets:

- `ui::dialogs::tests::popup_is_top_level`
- `ui::scrollbar::tests::child_index_invariant_matches_bundled_fltk`

libtest runs every test on a worker thread, and on macOS FLTK's first
`Fl::lock()` opens the display through AppKit, which only allows that on the
process main thread. The test binary aborts:

```
NSInternalInconsistencyException: 'nextEventMatchingMask should only be
called from the Main Thread!'  →  SIGABRT
```

Evidence: macOS run 45857601075 (whole lib binary aborted) and the per-test
diagnostic run 45861376873 (exactly these two abort, rc 134). T0076 skips them
by exact name. This ticket makes them run on macOS, so they leave the skip
list.

## In scope

- Move both test bodies into `#[doc(hidden)] pub mod main_thread_checks` child
  modules of `ui::dialogs` and `ui::scrollbar`. As children they keep access
  to the private items under test, so no visibility changes.
- New integration test binary `tests/fltk_main_thread.rs` with
  `harness = false`. Its `main()` runs the checks sequentially on the main
  thread, prints libtest-style lines, and honours `--list`, positional filters,
  `--skip` and `--exact` (so `cargo test -- --exact --skip …` and filters keep
  working). Unknown flags are ignored.
- Display gate: on macOS and Windows a display always exists, so the checks
  run. On Linux they need `DISPLAY` / `WAYLAND_DISPLAY`. Without one they are
  skipped with a note, unless `FERRISPAD_REQUIRE_UI_TESTS=1`; then a missing
  display fails (the `ci.yml` Linux jobs set it and run under `xvfb-run`).
- Remove `ui::fltk_widget_test_lock`: its only two users are gone, and a
  single-threaded `main()` needs no lock.
- `macos-gates.yml`: drop the two FLTK skips. The third skip (plugin path
  validation on macOS, T0090) stays until T0090 lands.

## Out of scope

- The third macOS skip: `app::plugins::api::tests::test_lua_create_dir_and_rename`
  (plugin path validation on macOS; T0090).
- `tests/ui_dialogs.rs` and `tests/settings_dialog_layout.rs`: on macOS they
  skip themselves (`display_available()` looks for `DISPLAY`), so they don't
  abort. Running their layout checks on macOS too (fonts differ) is a
  follow-up candidate.

## How to test

### Red (before)

macOS gate without the two skips: the lib unit-test binary aborts (SIGABRT)
at the first FLTK widget test (run 45857601075). Per test, run 45861376873
shows `FAIL rc=134` for both.

### Green (after)

```bash
# Linux, like ci.yml:
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --locked --test fltk_main_thread
# → running 2 tests … test result: ok. 2 passed
nix develop -c cargo test --locked --test fltk_main_thread -- --list
# → popup_is_top_level: test / child_index_invariant_matches_bundled_fltk: test
env -u DISPLAY -u WAYLAND_DISPLAY nix develop -c cargo test --test fltk_main_thread
# → skipped with a note (exit 0); with FERRISPAD_REQUIRE_UI_TESTS=1 → fails
```

macOS: the `build + clippy + test (macos)` job with only the T0090 skip left
is green, and its log shows `fltk_main_thread` → `2 passed`.

## Acceptance criteria

- [x] Both checks run and pass on macOS (CI log), and on Linux in `ci.yml`
      (under Xvfb, `FERRISPAD_REQUIRE_UI_TESTS=1`).
- [x] `macos-gates.yml` skips only the T0090 test.
- [x] A failing check fails the binary (non-zero exit); filters, `--skip`,
      `--exact` and `--list` behave like libtest.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.

## Affected files

- `src/ui/dialogs/mod.rs`, `src/ui/scrollbar.rs`: tests → `main_thread_checks`.
- `src/ui/mod.rs`: `fltk_widget_test_lock` removed.
- `tests/fltk_main_thread.rs` (new), `Cargo.toml` (`[[test]] harness = false`).
- `.github/workflows/macos-gates.yml`: two skips removed.

## Notes

- Origin: T0076's first real macOS gate run (2026-11-26).
- Stacked on T0076 (#153): the branch starts from
  `ticket/T0076-weekly-security-2026-W48`, and the draft PR says so.

## What was done (2026-11-26)

- `ui::dialogs::main_thread_checks::popup_is_top_level` and
  `ui::scrollbar::main_thread_checks::child_index_invariant_matches_bundled_fltk`:
  the two test bodies, unchanged apart from dropping the test lock, now
  live in `#[doc(hidden)] pub mod main_thread_checks` children (private items
  stay private). The `orientations` helper moved with the scrollbar check.
- `ui::fltk_widget_test_lock` removed (no users left).
- `tests/fltk_main_thread.rs` + `[[test]] name = "fltk_main_thread",
  harness = false` in `Cargo.toml`: `main()` runs the checks in order on the
  main thread, catching panics per check. Output and exit code follow
  libtest. Positional filters, `--skip`, `--exact` and `--list` are honoured;
  value-taking libtest flags (`--test-threads N`, `--format F`, …) are
  skipped with their value, and other flags are ignored. Display gate:
  macOS/Windows always run; Linux needs `DISPLAY`/`WAYLAND_DISPLAY`, otherwise
  "ignored" with a note, or a failure under `FERRISPAD_REQUIRE_UI_TESTS=1`.
- `macos-gates.yml`: the two FLTK skips removed. Only the T0090 skip remains.

Local results (Linux, Nix dev shell):

| Command | Result |
|---|---|
| `xvfb-run … FERRISPAD_REQUIRE_UI_TESTS=1 cargo test --test fltk_main_thread` | `running 2 tests` … `2 passed` |
| `… -- --list` | `popup_is_top_level: test`, `child_index_invariant_matches_bundled_fltk: test` |
| `… -- --exact --skip popup_is_top_level` | 1 passed, 1 filtered out |
| `… -- child_index --test-threads 1` | 1 passed, 1 filtered out (the `1` is not read as a filter) |
| no `DISPLAY`/`WAYLAND_DISPLAY` | "skipping FLTK main-thread checks", `2 ignored`, exit 0 |
| no display + `FERRISPAD_REQUIRE_UI_TESTS=1` | panics "UI tests required but no display", exit ≠ 0 |

Full suite under Xvfb: lib 579 passed (581 − the 2 moved checks), harness 2
passed, every other binary green; fmt and clippy `-D warnings` clean.

CI on 97f5404 (PR #174), all green:

- `macos-gates.yml` run 45866050884, `build + clippy + test (macos)`:
  `fltk_main_thread` → `test popup_is_top_level ... ok`,
  `test child_index_invariant_matches_bundled_fltk ... ok`, `2 passed`. The
  lib reports `578 passed; 1 ignored; 1 filtered out` (only the T0090 skip
  is left), and every other binary passes.
- `ci.yml` run 45866050924: fmt + clippy + test (Linux, Xvfb,
  `FERRISPAD_REQUIRE_UI_TESTS=1`), test (linux arm64), build (windows x64).
- `build-deps-check.yml` run 45866051125.

## Review (a3, 2026-11-26): approved, two non-blocking points done

- `--skip=PATTERN` (the `=` form libtest's getopts accepts) is now a skip;
  before, it fell into "unknown flag" and was ignored
  (`-- --skip=popup --list` still listed `popup_is_top_level`; now only
  `child_index_invariant_matches_bundled_fltk`).
- `--ignored` now behaves like libtest: it runs only `#[ignore]`d tests,
  and none of these checks is, so nothing runs (`--list` → 0 entries);
  `--include-ignored` runs everything, as before.

## User verification

2026-11-26: a CI-only change (the macOS test harness), verified by CI. The macOS gates run both FLTK widget checks on the main thread and pass, the Linux CI runs them under Xvfb, and the user's rc.3 integration build `verify/rc3` (checklist VERIFICA-RC3.md, Fedora GNOME on Wayland) includes it.
