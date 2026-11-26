---
id: T0087
title: Do not stall on files with very long lines (bound syntax-highlighting work in bytes)
status: todo            # todo | review | done
created: 2026-11-26
severity: moderate
area: robustness
depends-on: []
issue: 170
---

## Goal

Opening an 8 MB single-line minified JavaScript file keeps FerrisPad at 100 %
of one core for more than 20 s (43 s measured, default settings) before the
editor responds. Afterwards every keystroke on that line stalls again for
seconds. Once the work is done, idle CPU goes back to 0. This is a stall on
open and on edit, not an idle-CPU leak. Found during the T0081 Xvfb campaign;
it is older than T0081.

## Cause (profiled)

How it was measured:
- Release build on Xvfb, fresh config dir, settings
  `highlighting_enabled` / `word_wrap_enabled` set per run, nothing else
  running on the machine.
- "Open" is the wall time from launch until the process has used ≤ 1 CPU tick
  per 0.2 s for 1 s, read from `utime + stime` in `/proc/<pid>/stat`.
- "Click", "Right" and "Key" are the same measurement, taken after a click in
  the editor, one `Right` key press, and one typed character.
- Hot spots come from main-thread stacks sampled with `eu-stack`.
- The fixture is generated (see *How to test*); nothing large is committed.

There are two independent causes.

**A — FerrisPad's highlighter does unbounded synchronous work on one line**
(fixed here).
- The choice between highlighting synchronously and in chunks was
  `line_count <= LARGE_FILE_THRESHOLD` (5000 lines). A one-line file of any
  size was highlighted synchronously by `highlight_full`.
- The chunked path counted lines too, so one line could never be split.
- syntect costs 2.25 µs/byte on one long line, against 0.54 µs/byte for the
  same bytes split into 120-column lines. That is 18 s for the 8 MB line in a
  standalone benchmark (`highlight_full`, release).
- Every keystroke re-parsed the whole line from the last checkpoint: 2.4 s
  per key at 1 MB.
- The thousands of style runs that syntect produces make FLTK's measuring
  and drawing of the line several times slower. At 1 MB with wrap off: open
  28 s with highlighting against 6 s without, a cursor move 7.5 s against
  2 s.

**B — FLTK's `Fl_Text_Display` re-measures the whole visible logical line on
every recalc** (not fixed: known limitation, see below).

## What was done

**Simplify step (no behaviour change, existing tests green).**
- One per-line step, `highlighter::highlight_line`, used by all three paths
  (full, incremental, chunked). Before, each path had its own copy of the
  syntect parse + style loop.
- One decision helper, `controllers::highlight::highlight_plan(text)`,
  returns `Synchronous` or `Chunked`. It replaces the four copies of
  `line_count <= LARGE_FILE_THRESHOLD` in `detect_and_highlight`,
  `rehighlight_document`, `start_next_queued_highlight` and
  `rehighlight_all_documents`.

**The rule, in those two places.**
- `LONG_LINE_BYTES` = 16 KiB (`services/syntax/highlighter.rs`). A line
  longer than that, line ending included (`is_long_line`), is not handed to
  syntect.
  - It gets the plain style `'A'`, the theme foreground, as one run.
  - The parse and highlight state carries over unchanged to the next line.
  - Because it lives in `highlight_line`, it applies identically to full,
    incremental (every keystroke) and chunked highlighting.
- Highlight work is budgeted in **bytes handed to syntect**. Lines skipped
  by the rule above cost syntect nothing and do not count.
  - `highlight_plan` is `Synchronous` only for ≤ 5000 lines **and**
    ≤ `SYNC_HIGHLIGHT_MAX_BYTES` (1 MiB).
  - A chunk ends after `CHUNK_SIZE` (2000) lines **or** `CHUNK_BYTES`
    (256 KiB), whichever comes first. A chunk's `done` is now "the text is
    consumed" instead of "fewer than 2000 lines were processed".
  - Counting skipped lines would send the 8 MB one-line file down the
    chunked path. That adds nothing for syntect, but it shows the
    "Highlighting large file…" banner, and showing and hiding it relays out
    the editor twice. Each relayout is one full FLTK recalc of the 8 MB line
    (about 2.3 s, cause B). Measured: idle after 14.5 s with skipped bytes
    counted, against the hl-off floor of 8.7 s.
- No new thread, timer, setting or plugin API. Chunks still advance through
  the existing `ContinueHighlight` message.
- The T0081 current-line highlight already skips lines over 64 KiB
  (`current_line::MAX_LINE_BYTES`, test `current_line_skips_long_lines`). No
  sampled stack in any run contained FerrisPad code, T0081's included: all
  were inside syntect (before the fix) or FLTK.

**Visible effect (accepted, pinned in a test).** The skipped line is not
parsed, so a construct it would have opened or closed is not seen. Example:
`/* open` on one line, then a long line containing `*/`. The code after the
long line is still colored as comment
(`long_line_skipped_carries_parse_state_over`). VS Code makes the same trade
(tokenization stops for lines over 20 000 characters, and the state carries
over).

## Measurements before / after

Release, Xvfb, columns as defined above; "—" means not measured because the
open had not finished within 200 s.

<<TABLES>>

Normal file (`src/app/controllers/highlight.rs`, 789 lines, Rust, debug
build): <<NORMAL>>

## Known limitation — cause B (FLTK, not fixed)

After this ticket, FLTK's own work remains, and it does not depend on
highlighting:
- `Fl_Text_Display::show_insert_position()` (every cursor move) and
  `buffer_modified_cb` (every edit) schedule a `recalc_display()` for the
  next draw.
- **Wrap on:** `count_lines` → `wrapped_line_counter` measures every
  character of the visible logical lines ± 3 with `string_width` → `fl_font`
  + `fl_width` through pango, about 0.25 µs/byte. Normal files stay cheap
  because their visible logical lines are short. One 8 MB logical line costs
  about 2.3 s per recalc, i.e. per cursor move.
- **Wrap off:** `longest_vline` → `measure_vline` → `handle_vline` measures
  the whole visible logical line in one `fl_width` call, which shapes it with
  pango/harfbuzz (about 2 µs/byte), and every draw shapes it again. An 8 MB
  line costs tens of seconds per recalc or draw, with RSS around 420 MB.
- Every extra draw or recalc that the application triggers multiplies this
  cost. That is why "hl on" still measures somewhat slower than "hl off" with
  wrap off: restyling after an edit repaints the line once more.

Fixing B needs FLTK changes, e.g. caching per-character widths for the text
display's font, or measuring only the visible window of a long line; or a
different display widget. Follow-ups are left to the user. A possible
separate ticket is an info banner on open ("very long lines — editing will be
slow"); it is not built here.

## Out of scope

- Cause B (see above).
- Changing the user's word-wrap setting automatically for files with long
  lines.
- The large-file (≥ 50 MB) streaming path, which already skips highlighting.
- `HighlightController::clear_annotations` still runs a synchronous full
  highlight, as before. It is now bounded per line by the rule above, but not
  routed through `highlight_plan`. It only runs when a plugin clears its
  annotations.

## How to test

### Regression tests

```bash
nix develop -c cargo test --lib long_line
```

The six T0087 tests (the filter also matches three older long-line tests in
other modules, which stay green):

- `services::syntax::tests::long_line_is_one_plain_run_in_every_path`
  - The long line is in the middle, at the very start, and at the very end
    without a final newline.
  - It is one plain run in full, incremental and chunked highlighting, and
    the three agree byte for byte.
  - Every other line is still highlighted (more than one style).
- `…::long_line_skipped_carries_parse_state_over` — the comment case above.
- `…::long_line_edits_keep_it_plain_and_normal_lines_rehighlight`
  - A keystroke on the long line keeps it plain, i.e. it is not
    re-tokenized.
  - An edit on a normal line of the same file, applied incrementally, gives
    exactly the styles of a fresh full highlight.
- `…::long_line_growing_past_limit_turns_plain_and_back` — a line pasted
  past 16 KiB turns plain; shrunk back under, it is tokenized again and
  matches the original styles.
- `…::long_line_chunks_are_bounded_in_bytes`
  - 60 lines of 10 KiB (600 KiB) take at least 3 chunks, and the first one
    holds ≤ 256 KiB plus one line.
  - A skipped 600 KiB line does not use up the budget.
- `controllers::highlight::tests::long_line_highlight_plan_bounds_lines_and_bytes`
  - 1025 lines of 1 KiB are `Chunked`; 1024 are `Synchronous`.
  - Over 5000 lines is `Chunked`.
  - One 8 MB line plus short lines is `Synchronous`.

Red run on the code before the fix, with the simplify step and the constants
already in place:

```text
test app::controllers::highlight::tests::long_line_highlight_plan_bounds_lines_and_bytes ... FAILED
test app::services::syntax::tests::long_line_chunks_are_bounded_in_bytes ... FAILED
test app::services::syntax::tests::long_line_skipped_carries_parse_state_over ... FAILED
test app::services::syntax::tests::long_line_is_one_plain_run_in_every_path ... FAILED
test app::services::syntax::tests::long_line_growing_past_limit_turns_plain_and_back ... FAILED
test app::services::syntax::tests::long_line_edits_keep_it_plain_and_normal_lines_rehighlight ... FAILED

long_line_highlight_plan_bounds_lines_and_bytes: assertion `left == right` failed
  left: Synchronous   right: Chunked          (1025 × 1 KiB lines; old rule counts lines only)
long_line_chunks_are_bounded_in_bytes: 600 KiB do not fit one chunk
long_line_skipped_carries_parse_state_over: line 2 keeps the comment style: "JJJIKIIILII"
long_line_is_one_plain_run_in_every_path: line 1 has 6 styles
long_line_growing_past_limit_turns_plain_and_back: assertion failed: line_styles(&big, &styles, 0).bytes().all(|b| b == b'A')
long_line_edits_keep_it_plain_and_normal_lines_rehighlight: assertion failed: line_styles(&typed, &styles, 1).bytes().all(|b| b == b'A')
test result: FAILED. 3 passed; 6 failed
```

The plan test's first red run used one 1 MiB + 1 line of `a` as the over-bound
case. When skipped lines stopped counting (see *What was done*), that case
became `Synchronous` by design. The test now uses 1025 lines of 1 KiB, and it
was re-run red against the line-count-only helper:
`left: Synchronous, right: Chunked` at that assertion.

After the fix all six pass, and the full suite is green.

### Manual repro (Xvfb or desktop)

1. Generate the fixture, a single line of minified-looking JavaScript of
   8 000 000 bytes:
   ```bash
   python3 - <<'EOF'
   import random
   r = random.Random(87); ids = "abcdefhilnoprstu"
   def i(): return r.choice(ids) + (r.choice(ids) if r.random() < .3 else "")
   parts = [lambda: f"function {i()}({i()},{i()}){{return {i()}+{i()}*{r.randint(0,999)}}}",
            lambda: f"var {i()}=\"{'x' * r.randint(3, 20)}\";",
            lambda: f"{i()}=({i()},{i()})=>{i()}?{i()}.map({i()}=>{i()}[{r.randint(0,9)}]):null;"]
   s = ""
   while len(s) < 8_000_000: s += r.choice(parts)()
   open("/tmp/min8.js", "w").write(s[:8_000_000] + "\n")
   EOF
   ```
   (A 1 MB variant: replace both `8_000_000` with `1_000_000`.)
2. `nix develop -c cargo run --release -- /tmp/min8.js`, default settings
   (highlighting on, word wrap on).
3. Time from launch until `top` shows FerrisPad idle. Expect the "after" row
   of the table (before: 43 s). The line is shown in the plain text colour,
   with no "Highlighting large file…" banner.
4. Type a character on the line: the delay is FLTK's (cause B), the same as
   with highlighting off.
5. Open a normal source file (e.g. `src/main.rs`) and type in it: colors and
   behaviour are exactly as before.

## Acceptance criteria

- [x] A line longer than `LONG_LINE_BYTES` is never handed to syntect, on
      open (full or chunked) or on edit (incremental).
- [x] The synchronous/chunked decision bounds lines and syntect bytes, in one
      helper used by every call site.
- [x] Normal files highlight exactly as before: existing highlight tests are
      green, and the screenshot diff is identical.
- [x] Xvfb, release: with highlighting on, the 8 MB fixture and default
      settings are idle within the FLTK floor measured with highlighting off.
- [x] Regression tests added and green; they failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` all pass with zero warnings.
- [ ] Pending user verification: the manual repro on a real desktop (Wayland
      / GPU), where FLTK's per-recalc cost may differ from Xvfb.

## Affected files

- `src/app/services/syntax/highlighter.rs` — `LONG_LINE_BYTES`,
  `is_long_line`, the shared `highlight_line` step (full and incremental).
- `src/app/services/syntax/mod.rs` — the chunked path uses `highlight_line`,
  has the `CHUNK_BYTES` budget, and computes `done` from the consumed text.
  T0087 tests.
- `src/app/controllers/highlight.rs` — `HighlightPlan` / `highlight_plan`
  replace four line-count checks. Plan test.

## Notes

- Origin: T0081 Xvfb campaign (ferrispad-29); the orchestrator assigned it as
  T0087.
- Draft of an upstream FLTK issue for cause B. **Not posted**; the user
  decides.

  > **Title:** Fl_Text_Display: cursor moves and edits re-measure the whole
  > visible logical line (seconds per key on multi-MB single-line files)
  >
  > With a buffer holding one very long logical line (e.g. an 8 MB minified
  > .js), every cursor move and every edit is slow, in proportion to the
  > line length. `show_insert_position()` and `buffer_modified_cb()` call
  > `display_needs_recalc()`, and `recalc_display()` then measures the full
  > visible logical lines:
  > - With continuous wrap, `count_lines()` → `wrapped_line_counter()`
  >   calls `measure_proportional_character()` → `string_width()` →
  >   `fl_font()` + `fl_width()` for every character: about 2.3 s per
  >   recalc for 8 MB on the Cairo/pango driver.
  > - Without wrap, `longest_vline()` → `measure_vline()` →
  >   `handle_vline(GET_WIDTH)` passes the whole line to one `fl_width()`
  >   call (pango shapes all 8 MB), and `draw()` shapes it again.
  >
  > Caching per-character widths (at least for a fixed-pitch text font), or
  > measuring only up to the visible right edge, would bound the cost.
  >
  > **Repro:** FLTK 1.5 (git), Linux X11, Cairo/pango. Use the `editor` test
  > program; load a file containing one 8 MB line without newlines; press
  > Right a few times, with and without word wrap. Every key press takes
  > seconds, and CPU is at 100 % in `wrapped_line_counter` / `fl_width`.
