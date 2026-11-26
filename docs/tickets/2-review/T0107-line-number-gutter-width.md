---
id: T0107
title: Size the line-number gutter for the widest line number, and keep it up to date
status: review          # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 196
---

## Goal

The line-number gutter cuts numbers that do not fit: on a 111 236-line file,
"111233" shows as "1233". The user treats any clipping as a blocking layout
defect.

## Cause (observed on `master` 4cd7cf5, Xvfb)

- **The width is not updated when the line count changes.**
  `ViewController::update_linenumber_width` runs only on tab switch,
  buffer rebind and settings changes.
  - Tabs off: opening a file loads it into the existing document, so the
    gutter keeps its startup width of 40 px. A debug print showed only three
    calls, all at startup, with "Untitled" and 0 lines. A `settings.json`
    without `"tabs_enabled"` also means tabs off on `master` today.
  - Tabs on: typing or pasting across a digit boundary (99 999 → 100 000
    lines) keeps the old width until the next tab switch.
- **The width rule ignores the font.** It is `digits × 8 + 16`, but FLTK
  draws the numbers in `linenumber_font()` at `linenumber_size()`
  (Helvetica 14 by default), inside the gutter minus 3 px on each side
  (`Fl_Text_Display::draw_line_numbers`). A wider digit or a larger
  line-number size cuts the number.
- The diff/split view (`ui/split_panel.rs`) uses a fixed 40 px.

## In scope

- One width rule. `digits(largest line number) × width of the widest digit
  in the line-number font + 2 × 3 px (FLTK inset) + a gap`, at least 40 px.
  The widest digit is measured with FLTK.
- `AppState::on_buffer_modified` keeps `cached_line_count` and refreshes the
  gutter **only when the digit count changes**, so there is no
  per-keystroke cost. The tab-switch, rebind and settings paths use the same
  rule.
- The split/diff view uses the rule for its buffers.

## Out of scope

- Making the line-number font follow the editor font or size (unchanged:
  Helvetica 14). The rule measures whatever they are.
- The read-only large-file viewer, which has no gutter (width 0).

## How to test

### Regression test (red first)

```bash
xvfb-run -a nix develop -c cargo test --test linenumber_gutter
```

`tests/linenumber_gutter.rs` is a UI test binary (needs a display; skipped
without one unless `FERRISPAD_REQUIRE_UI_TESTS=1`). It builds the main window
and `AppState` with tabs off and private config and data dirs, then:
- loads 100 000 and then 1 000 000 lines into the active document, the way
  the tabs-off open does, and calls `on_buffer_modified`; expects the gutter
  to be ≥ the measured width of the widest number + 6 px. Before the fix:
  40 px;
- for line-number sizes 8 to 36 and a 1 000 000-line document, the gutter
  fits the widest number. Before the fix: fixed 8 px per digit.

Unit tests (`ui::line_numbers`): the digit count at the boundaries (9 999 /
10 000 / 99 999 / 100 000 lines) and the width formula.

### Manual (Xvfb by Claude, then the user)

Open files of 9 999, 100 000 and 1 000 000 lines; Ctrl+End; the last number
is fully visible (screenshots), with tabs on and off. Type to cross 99 999 →
100 000: the gutter grows.

Red run before the fix (the `on_buffer_modified` move was already in place,
with no gutter refresh):

```text
test line_number_gutter_fits_the_widest_number ... FAILED
9999 lines: gutter 40 px, the number 10000 needs 46 px
```

After the fix it passes, including line-number sizes 8–36 at 1 000 000
lines. The unit tests in `ui::line_numbers` pass. Gates: 668 passed (twice,
with a display), clippy `-D warnings` and fmt clean.

### Xvfb results (release; "master" = 7ea980d)

Files of 9 999 / 100 000 / 1 000 000 lines; Ctrl+End; screenshot of the
gutter:
- **master** (tabs off): every number is cut to 4 digits ("0000", "0001").
- **fixed, tabs off and tabs on**: "10000", "100001" and "1000001" are
  fully visible, with a gap before the text and nothing cut at the left
  edge.
- **Crossing a digit boundary while typing**, 99 998 lines: Ctrl+End, type
  `x` + Enter → line 100000 appears and the gutter widens to fit it;
  Backspace → back to 5 digits.

## What was done

- `ui::line_numbers` (new):
  - `digits(line_count)` gives the digits of `line_count + 1`, because FLTK
    also numbers the empty line after a final newline;
  - `width_for(digits, digit_w)` = number + 2 × 3 px FLTK inset + 6 px gap,
    at least 40;
  - `widest_digit(font, size)` measures `0`–`9` with FLTK and restores the
    current drawing font;
  - `gutter_width(...)` combines them.
- `ViewController::update_linenumber_width` uses the rule with the editor's
  `linenumber_font()` / `linenumber_size()`. It records the digits the
  gutter is sized for and sets the width only when it changes, because
  setting it makes FLTK recalculate the display.
  `ViewController::line_count_changed` resizes only when the active
  document's digit count differs from that. It compares with what the
  gutter shows, not with the previous count, so every path that changes
  `cached_line_count` (including reload, which uses `str::lines()`) is
  covered.
- `AppState::on_buffer_modified(id)` takes the `BufferModified`
  bookkeeping that `dispatch.rs` used to do inline (cached tree, line count)
  and calls `line_count_changed` for the active document. That makes it
  testable from the library.
- The split/diff view sizes each pane's gutter with the same rule when it
  loads its side. The startup width uses `MIN_WIDTH` in both places.
- The read-only large-file viewer has no gutter (width 0): nothing to do.

## Acceptance criteria

- [x] No line number is cut, at any line count or line-number size.
- [x] The gutter follows the line count when it crosses a digit boundary,
      and on tab switch, with no per-keystroke work otherwise (the digit
      comparison only).
- [x] The split/diff view fits its numbers too (same rule; not driven in
      Xvfb: a diff view needs an MCP or plugin request).
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; Xvfb screenshots at 9 999 / 100 000 /
      1 000 000 lines.
- [ ] Pending user verification.

## Affected files

- `src/ui/line_numbers.rs` (new) — digits, the width rule, the measurement
- `src/app/controllers/view.rs` — `update_linenumber_width` uses it
- `src/app/state.rs` — `on_buffer_modified`
- `src/dispatch.rs` — calls it
- `src/ui/split_panel.rs` — the same rule
- `tests/linenumber_gutter.rs` — new UI test binary

## Notes

- Noticed during the T0096 Xvfb pass; assigned by the orchestrator as T0107
  (rc.3 candidate).
