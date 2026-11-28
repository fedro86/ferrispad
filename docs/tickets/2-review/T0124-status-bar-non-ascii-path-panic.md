---
id: T0124
title: A long file path with non-ASCII characters never crashes the status bar
status: review          # todo | review | done
created: 2026-11-28
severity: moderate      # a panic loses unsaved work
area: robustness
release: 0.9.11         # the planned release, X.Y.Z (e.g. 0.9.11); new-pr's page folder
depends-on: []
issue: 249
---

## Goal

The status bar shows the current file's path on the left and "Ln X, Col Y"
on the right. When the path is too long for the bar, it is shortened from
the left (`…/rest/of/path`). That code (`src/ui/status_bar.rs`) counts the
path in bytes and slices it at a byte offset: `&self.path_text[skip..]`.
If a multi-byte character (é, ü, ß, CJK, emoji…) straddles that offset, the
slice panics. FerrisPad then crashes and any unsaved work is lost. This
happens with an ordinary file, for example
`/home/user/Documents/Präsentationen/…/notes.txt` in a window narrower than
the path.

Counting bytes as characters also shortens non-ASCII paths more than needed
and pushes "Ln X, Col Y" off its place.

## In scope

- Extract the path shortening into a pure function, with no change in
  behaviour first, so it can be tested without FLTK.
- Red test first: a long path with non-ASCII characters around the cut
  point, at every available width, must not panic.
- Fix the whole class: measure and cut the path in characters, not bytes
  (`text_ops::floor_char_boundary` where a byte index is still needed), and
  use the same character count for the padding before "Ln X, Col Y".

## What was done

- `status_bar::fit_path_left(path, available)`: the path shortening,
  extracted from `StatusBar::update` with no change in behaviour first.
- Red: `long_non_ascii_path_never_panics` tries every width for a path
  with German and Japanese characters. On the extracted (byte-based) code it
  FAILED: `start byte index 90 is not a char boundary; it is inside 'モ'
  (bytes 88..91 of string)`.
- Fix: the function counts the path in characters and finds the cut with
  `char_indices`, so every slice starts on a character boundary. It still
  starts the shortened path at a `/` when there is one. The padding before
  "Ln X, Col Y" counts characters too.
- Guard: `shortened_path_starts_at_a_slash` (the `…/` form, a short path
  unchanged, too narrow gives nothing).

## Out of scope

- How the status bar draws special characters in file names (FLTK label
  symbols).
- The status bar's layout or styling.

## How to test

### Regression test

```bash
flatpak-spawn --host bash -lc "$PWD/scripts/gates.sh $PWD"
nix develop -c cargo test --lib status_bar   # just this module (pure, no window)
```

- Before the fix: `status_bar::tests::long_non_ascii_path_never_panics`
  FAILS with `byte index … is not a char boundary`.
- After the fix: it passes, with the shortened path still starting with
  "…/" at a `/` and fitting the width in characters.

### Manual repro

1. Create `/tmp/Präsentationen-ÄÖÜ-äöü-ß/Übersicht-Größenänderung/notes.txt`.
2. `nix develop -c cargo run -- /tmp/Präsentationen-ÄÖÜ-äöü-ß/Übersicht-Größenänderung/notes.txt`.
3. Make the window narrow, a little at a time.
4. Before: at some width FerrisPad panics (the terminal shows
   `byte index … is not a char boundary`). After: the path shortens from
   the left and FerrisPad keeps running.

## Acceptance criteria

- [ ] No path, at any window width, makes the status bar panic.
- [ ] A shortened path fits the space left of "Ln X, Col Y", counted in
      characters.
- [ ] The regression test failed before the fix.
- [ ] Gates green (`scripts/gates.sh`).

## Affected files

- `src/ui/status_bar.rs`: the path shortening and its tests.

## Notes

- Found by ferrispad-39 while reading the code (2026-11-28).
- Engineering standard: "No raw slicing of `&str` by byte offsets without a
  char-boundary guard".
