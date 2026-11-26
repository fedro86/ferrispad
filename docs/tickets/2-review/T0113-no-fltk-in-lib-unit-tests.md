---
id: T0113
title: Guard: no library unit test builds FLTK objects
status: review          # todo | review | done
created: 2026-11-27
severity: minor
area: process
depends-on: []
issue: 218
---

## Goal

libtest runs unit tests on worker threads. On macOS, AppKit aborts the
**whole** test binary when FLTK touches the display off the main thread.
T0089 moved FerrisPad's FLTK checks into `main_thread_checks` modules,
which `tests/fltk_main_thread.rs` runs on the main thread. Still, #177
(T0087) added a unit test that built a `TextBuffer` (through
`TabManager::add_untitled`) and failed only the macOS gate. Linux and
Windows can't catch this class at runtime.

## In scope

- A source guard, `lib_unit_tests_build_no_fltk_objects`
  (`src/fltk_test_guard.rs`, test-only). It runs on every OS:
  - it scans the body of every `#[cfg(test)] mod … { … }` in `src/`,
    matched by braces, skipping nested `main_thread_checks` modules;
  - it fails on a narrow list of FLTK constructors (widgets, `Window`,
    `TextBuffer`), the document calls that create a buffer (`add_untitled`,
    `add_from_file`/`_buffer`, `Document::new_*`), `AppState::new`,
    `build_main_window`, the `DialogTheme` widget components and the
    `build_themed_*` builders;
  - the failure names `file:line: code` and points to `main_thread_checks`
    and `tests/fltk_main_thread.rs`.
- The pattern list comes from ferrispad-8e's audit, merged in so there is
  one guard: a plain `fltk::app::channel` or an empty `TabManager::new` is
  not flagged, since master's tab-bar tests use those and pass on macOS.

## Out of scope

- A helper **outside** `#[cfg(test)]` that builds FLTK objects and is
  called from a test (one indirection). Keep such helpers in the test
  module or in `main_thread_checks`.
- Integration tests in `tests/`: those build windows behind
  `display_available()`, which is skipped on macOS.

## How to test

```bash
nix develop -c cargo test --lib fltk_test_guard
```

- `guard_flags_t0087_js_tab`: T0087's `js_tab` helper, as it stood in
  `mod tests` on ticket/T0087 (54b1759), is flagged at
  `tabs.add_untitled()`.
- `guard_skips_main_thread_checks_and_plain_channels`: the same helper
  inside `main_thread_checks`, a channel-only test, and code outside a test
  module are not flagged.
- `lib_unit_tests_build_no_fltk_objects`: green on master.

## Acceptance criteria

- [ ] The guard flags T0087's pre-fix `js_tab`, and a planted
      `TextBuffer::default()` in a real test module, with the pointer to
      `main_thread_checks`.
- [ ] It passes on master, where every FLTK check is in
      `main_thread_checks`.
- [ ] Full gates green, including the macOS gate.

## Affected files

- `src/fltk_test_guard.rs` (new, test-only), `src/lib.rs`
  (`#[cfg(test)] mod fltk_test_guard;`).

## Notes

- Origin: #177's macOS failure; the orchestrator approved the guard on
  2026-11-27. ferrispad-8e's audit gave the pattern list and its
  calibration: 0 hits on master; real offenders on unlanded branches
  (T0091 `tabs.rs` / `session.rs`, T0093 `document.rs`, T0112 label tests).
- Brace matching does not special-case braces in string literals or
  comments. Test code with unbalanced braces inside literals could end a
  module early; none exists today.

## What was done (2026-11-27)

As in *In scope*.

### Red

- Fixture: `guard_flags_t0087_js_tab` flags
  `let id = tabs.add_untitled();`.
- Real tree: with a planted test module in `src/app/controllers/tabs.rs`
  that calls `fltk::text::TextBuffer::default()`, the guard fails:

  ```
  test fltk_test_guard::lib_unit_tests_build_no_fltk_objects ... FAILED
  unit tests build FLTK objects off the main thread (AppKit aborts the macOS
  test binary). Move these checks into a `main_thread_checks` module and call
  them from tests/fltk_main_thread.rs (T0089):
  …/src/app/controllers/tabs.rs:648: let _b = fltk::text::TextBuffer::default();
  ```

  (The planted module was then removed.)

### After

- On master 689b469 (all of rc.3's landed tickets), the guard and its two
  self-tests pass.
