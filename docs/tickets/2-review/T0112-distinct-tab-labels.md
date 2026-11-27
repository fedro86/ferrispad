---
id: T0112
title: Two open documents never share a tab label
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0091]
issue: 214
---

## Goal

Two tabs can show the same label, so the user cannot tell them apart in
the tab bar or the window title:
- **A file named "Untitled" next to an untitled document** (T0091 backlog,
  item 3). Both read "Untitled" until the next open or close. After that
  the untitled one reads "Untitled ()", with an empty suffix.
- **Save As to the name of another open file.** With `/y/main.rs` open,
  saving another tab as `/x/main.rs` leaves both reading "main.rs" until
  the next open or close.

The disambiguated label was a cached field, recomputed only by `TabManager`
on open and close. Any other change of a path or name left it stale:
`add_untitled`, Save As, and the tabs-off open, tail and chunk views. A
document without a path also took part in the folder suffixes, with an
empty one.

## In scope

- Tab labels are computed from the current paths and names whenever they
  are shown (`tabs::tab_labels`), never cached. The cached
  `disambiguated_name` and `disambiguate_display_names()` are removed, so no
  path or name change can leave a label stale, whoever makes it.
- A document without a path (untitled) keeps its name, which T0091 makes
  unique. Only files take folder suffixes, chosen to be unique among the
  files of the group. A file sharing its name with an untitled document
  gets at least one folder.
- The tab bar and the window title read the labels from there.

## Out of scope

- How a label looks: the "name (folders)" form and the minimal-folders rule
  (VS Code style) are unchanged. A group made only of files gets exactly the
  labels it got before; guard tests pin that.
- Tail and chunk views keep their own display names ("x (tail)").

## How to test

### Regression tests

```bash
nix develop -c cargo test --lib -- untitled_label save_as_same_name file_labels_unchanged
```

- `untitled_label_distinct_from_file_named_untitled`: the file
  `/home/u/notes/Untitled` opened before or after a new tab. Expected labels:
  "Untitled (notes)" and "Untitled". After closing the file, "Untitled".
- `untitled_label_with_two_files_of_its_name`: two untitled documents plus
  `/a/Untitled 2` and `/b/Untitled 2`. Expected: "Untitled", "Untitled 2",
  "Untitled 2 (a)", "Untitled 2 (b)".
- `save_as_same_name_labels_distinct`: `/y/main.rs` is open, and another tab
  gets the path `/x/main.rs` the way Save As sets it. Expected:
  "main.rs (y)" and "main.rs (x)".
- `file_labels_unchanged` (guard; passes before and after): a lone file,
  "main.rs (a)" / "main.rs (b)", "mod.rs (x/src)" / "mod.rs (y/src)", an
  untitled tab, and plain "main.rs" once the other one is closed.

Red run on `ticket/T0091` (a16c5dc):

```text
save_as_same_name_labels_distinct                ... FAILED  left: ("main.rs", "main.rs")    right: ("main.rs (y)", "main.rs (x)")
untitled_label_distinct_from_file_named_untitled ... FAILED  left: ("Untitled", "Untitled")  right: ("Untitled (notes)", "Untitled")
untitled_label_with_two_files_of_its_name        ... FAILED  left: "Untitled 2 ()"           right: "Untitled 2"
file_labels_unchanged                            ... ok      (guard)
```

After the fix all four pass, as do T0091's `untitled_*` tests. Gates: 666
passed, clippy `-D warnings` and fmt clean.

The red run above was taken on the first form of these tests, which built a
`TabManager` (FLTK `TextBuffer`s) in `#[cfg(test)]`. Since T0113 that is not
allowed: on macOS the first `Fl::lock()` off the main thread aborts. They are
now split (see *After the master merge* below); the cases and the expected
labels are the same. Both halves of the split were run red again; see
*Fresh red runs* below.

### After the master merge (2026-11-27)

T0091 landed on master (#181), then T0093, T0099, T0107 and T0113 did too.
The branch merged T0091's final tip (c919558), then `origin/master`:
- **Tests split for the macOS gate (T0113).** The rule is a pure function,
  `labels_for(&[(name, path)])`, behind `tab_labels`. The four tests above
  now call it on (name, path) pairs, with no FLTK object. The `TabManager`
  wiring (a new tab, a Save As done in place, closes) is
  `tabs::main_thread_checks::tab_label_follows_paths_and_closes`, run by
  `tests/fltk_main_thread.rs` on the main thread.
- **Window title (T0107).** Master now builds the title with the pure
  `window_title(active, session)`. `update_window_title` feeds it
  `TabManager::tab_label(id)` instead of the removed `Document::tab_label`.
- **`Document` (T0093, T0099).** Master's new field `restored_temp_file`
  is kept; only `disambiguated_name` and `Document::tab_label` go.

```bash
nix develop -c cargo test --lib -- untitled_label save_as_same_name file_labels_unchanged
nix develop -c cargo test --test fltk_main_thread
```

Both pass (4 unit tests; 9 main-thread checks, including
`tab_label_follows_paths_and_closes`). Gates: 757 passed, clippy
`-D warnings` and fmt clean.

The merge of T0091's tip (c919558) had also dropped master's two
`GroupColor` unit tests from `tabs.rs` (`test_group_color_to_rgb`,
`test_group_color_to_rgb_dark`). The reviewer (ferrispad-90) caught it; they
are restored verbatim. `tabs.rs` now differs from master only by T0112.

### Fresh red runs (review, 2026-11-27)

Asked by the reviewer (ferrispad-90) for the split form of the tests. Both
probes were throwaway and are not committed.

**The wiring, on master's code.** `tab_label_follows_paths_and_closes`
was added to a worktree of master (c3672fe), with the label read through
master's cached `Document::tab_label()`. The probe collects every wrong
step instead of stopping at the first one:
`nix develop -c cargo test --test fltk_main_thread`:

```text
new tab, file: got "Untitled", want "Untitled (notes)"
save as in place, open: got "main.rs", want "main.rs (y)"
save as in place, saved: got "main.rs", want "main.rs (x)"
test tab_label_follows_paths_and_closes ... FAILED
test result: FAILED. 8 passed; 1 failed
```

This is the bug's own reason: the cache is stale after a new tab and after
Save As. The steps after the closes pass on master, because `remove`
recomputes the cache.

**The rule, with master's disambiguation.** `labels_for` was temporarily
given master's rule: untitled documents grouped with files (empty path),
and the suffix applied even when empty.
`nix develop -c cargo test --lib -- untitled_label save_as_same_name file_labels_unchanged`:

```text
untitled_label_with_two_files_of_its_name        ... FAILED
  left: ["Untitled", "Untitled 2 ()", "Untitled 2 (a)", "Untitled 2 (b)"]
 right: ["Untitled", "Untitled 2", "Untitled 2 (a)", "Untitled 2 (b)"]
untitled_label_distinct_from_file_named_untitled ... FAILED: file opened first, then a new tab
  left: ["Untitled (notes)", "Untitled ()"]
 right: ["Untitled (notes)", "Untitled"]
save_as_same_name_labels_distinct                ... ok
file_labels_unchanged                            ... ok
```

`save_as_same_name_labels_distinct` passes under master's rule, as
expected. Once it ran, the old rule was right; the Save As defect is the
stale cache, and the wiring probe above shows it.

### Xvfb (debug builds, tabs on; "before" = `ticket/T0091` tip)

`uitest-T0087/t0112.sh`; the tab bar is shown in screenshots.

| Scenario | before | after |
|---|---|---|
| open `notes/Untitled`, then Ctrl+T | "Untitled" \| "Untitled" | "Untitled (notes)" \| "Untitled" |
| `y/main.rs` open, Ctrl+T, type, Save As `x/main.rs` (real save via the fake zenity) | "main.rs" \| "main.rs"; title "main.rs" | "main.rs (y)" \| "main.rs (x)"; title "main.rs (x)" |

The labels fit their tabs; nothing is clipped.

### Manual (the user)

1. Open a file named `Untitled` (any folder), then File → New: the tabs read
   "Untitled (folder)" and "Untitled".
2. With a `main.rs` open, Save As another tab to `main.rs` in another folder:
   both tabs show their folder at once.

## Acceptance criteria

- [x] A file named like an untitled document has a distinct label as soon as
      both are open; no "()" label.
- [x] Save As to an open file's name disambiguates both tabs at once.
- [x] Labels of files only (no untitled document in the group) are
      unchanged (guard test).
- [x] Labels cannot go stale: nothing caches them.
- [x] Regression tests added; they failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [ ] Pending user verification.

## Affected files

- `src/app/controllers/tabs.rs`:
  - new `tab_labels`, `labels_for` and `folder_suffixes`, plus
    `TabManager::tab_label`;
  - `disambiguate_display_names` and its three calls are removed;
  - tests.
- `src/app/domain/document.rs`: the `disambiguated_name` field and
  `Document::tab_label` are removed.
- `src/ui/tab_bar.rs`, `src/app/state.rs` (window title): read
  `tab_labels`.
- `tests/fltk_main_thread.rs`: runs `tab_label_follows_paths_and_closes`.

## Notes

- From T0091's "Other oddities" (item 3, backlog). ID reserved by the
  orchestrator (ferrispad-5a), target 0.9.11, stacked on T0091. The
  orchestrator added Save As to the scope and asked for a rule that holds by
  construction. Computing labels when they are shown does that without
  wrapping every write to a path or name.
- Cost: one pass over the open documents per tab-bar rebuild or title
  update, both of which only happen on user actions.
- Disambiguation had no tests before; `file_labels_unchanged` now pins
  today's labels.
