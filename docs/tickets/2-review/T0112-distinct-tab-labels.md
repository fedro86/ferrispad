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
  - new `tab_labels` and `folder_suffixes`, plus `TabManager::tab_label`;
  - `disambiguate_display_names` and its three calls are removed;
  - tests.
- `src/app/domain/document.rs`: the `disambiguated_name` field and
  `Document::tab_label` are removed.
- `src/ui/tab_bar.rs`, `src/app/state.rs` (window title): read
  `tab_labels`.

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
