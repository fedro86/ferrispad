---
id: T0087
title: Do not stall on files with very long lines (bound syntax-highlighting work in bytes)
status: done            # todo | review | done
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
- One `queue_chunked(id, sender)` replaces the three copies of "push on the
  queue, and start `ContinueHighlight` if idle". It also never queues a
  document twice; two of the three copies did not check that.

**The rule, in those two places.**
- `LONG_LINE_BYTES` = 16 KiB (`services/syntax/highlighter.rs`). A line
  longer than that, line ending included (`is_long_line`), is not handed to
  syntect.
  - It gets the plain style `'A'`, the theme foreground, as one run.
  - The parse and highlight state carries over unchanged to the next line.
  - Because it lives in `highlight_line`, it applies identically to full,
    incremental (every keystroke) and chunked highlighting.
  - The boundary is inclusive. A line of exactly 16 384 bytes, line ending
    included, is still tokenized; 16 385 bytes are not
    (`long_line_boundary_is_inclusive`).
- Highlight work is budgeted in **bytes handed to syntect**. Lines skipped
  by the rule above cost syntect nothing and do not count.
  - `highlight_plan` is `Synchronous` only for ≤ 5000 lines **and**
    ≤ `SYNC_HIGHLIGHT_MAX_BYTES` (1 MiB).
  - A chunk ends after `CHUNK_SIZE` (2000) lines **or** `CHUNK_BYTES`
    (64 KiB), whichever comes first. A chunk's `done` is now "the text is
    consumed" instead of "fewer than 2000 lines were processed".
  - Computing the plan is one pass over the text:
    `str::split_inclusive('\n')` finds each newline with a memchr-style
    search and yields borrowed slices. There is no per-line allocation, and
    the pass stops as soon as either bound is crossed. Before, the check was
    `text.lines().count()`, also one pass.
- **Worst-case cost of the bounds, measured.** Dense minified JavaScript
  just under the limit (64 lines × 16 000 bytes = 1 000 KiB, cut from the
  fixture), `highlight_full` in a release standalone benchmark:
  - 1 MiB synchronous: **3.7 s**;
  - a 256 KiB chunk: 0.93 s, i.e. about 3.66 µs/byte;
  - a second run on the loaded machine took twice as long.

  | Bound | Value | Worst case (dense, 3.66 µs/B) | Normal code (0.54 µs/B) |
  |---|---|---|---|
  | Synchronous highlight | 1 MiB | ≈ 3.7 s | ≈ 0.55 s |
  | One chunk step | 64 KiB (+ ≤ 16 KiB line) | ≈ 0.23 s (≤ 0.3 s) | ≈ 0.035 s |

  - **Synchronous bound kept at 1 MiB** (orchestrator decision). It decides
    which files show the "Highlighting large file…" banner; lowering it would
    flash the banner on ordinary 5000-line files. The 3.7 s worst case (dense
    lines just under 16 KiB) is not a regression: that file was highlighted
    synchronously before this ticket too. It is a known bound to revisit
    with T0095/T0096.
  - **Chunk budget lowered from 256 KiB to 64 KiB** (orchestrator decision,
    during review). It has no visible effect: no banner change. It cuts the
    worst per-chunk freeze while chunked highlighting runs from ≈ 0.93 s to
    ≈ 0.23 s.
    - Cost: about 4× more `ContinueHighlight` rounds, each rebinding the
      style table and repainting the editor.
    - Measured on the 8 MB multi-line file (66 667 lines of 120 columns), hl
      on, wrap on, release, interleaved: idle after 6.8 / 7.5 s with 256 KiB
      against 8.7 / 9.3 s with 64 KiB, about +1.8 s (+25 %) of total
      highlight time. The machine was heavily loaded (load 8–14); a first
      pair taken at load 17–22 is discarded.
    - The UI stays responsive between chunks; keystroke costs are unchanged
      (0.08–0.09 s).
    - The 1 MB one-line file, on the synchronous path, is unchanged (1.0 s
      both).
- **Deliberate change from the approved scope** (agreed with the
  orchestrator during the work). The scope as first approved counted all
  bytes. That would send the 8 MB one-line file down the chunked path, which
  gains nothing for syntect because the line is skipped anyway. It does show
  the "Highlighting large file…" banner, though, and showing and hiding it
  relays out the editor twice. Each relayout is one full FLTK recalc of the
  8 MB line (about 2.3 s, cause B). Measured on the 8 MB file, hl on, wrap
  on: idle after 14.5 s with all bytes counted, against the hl-off floor of
  about 8.7 s.
- **`clear_annotations` follows the same plan** (folded in at the
  orchestrator's request during review). It used to run one synchronous full
  highlight of the active document whenever a plugin cleared its
  annotations, which would stall a large multi-line file.
  - It now returns an action, in the controller's usual style:
    `ClearAnnotations::{Nothing, Restyled, Chunked(id)}`.
  - The dispatch applies it: `Restyled` rebinds the editor
    (`bind_active_styles`); `Chunked(id)` calls `queue_chunked`.
  - A chunked run already under way for that document is restarted in place
    from the top (`ClearAnnotations::Restarted`), because it may already
    have passed the annotated regions. It goes through `start_chunked` with
    the fresh text, and the `ContinueHighlight` already pending drives it.
    A first version cancelled the run and queued it again; that started a
    second message chain driving the same run, i.e. two chunks per
    event-loop round (review nit, ferrispad-a3).
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

Release builds, Xvfb. Before is the code at `origin/master` (7ea980d); after
is this branch. Values are wall seconds, median (min–max) over 3
interleaved before/after pairs. The 8 MB wrap-off rows are single pairs,
because each run takes minutes; "> 200" / "> 60" means the open or key hit
the timeout. The fixtures are the generated ones from *How to test*.

**Load caveat.** The machine is shared with other build sessions, and the
1-minute load average ranged from 1 to 9 across the 1 MB runs. Pairs were
interleaved so that both sides saw the same load. The hl-off rows run the
same code before and after, so their spread is the noise level. The 8 MB
wrap-on rows ran at load ≈ 1 and agree within 0.3 s across repetitions. One
"after" 1 MB hl-on/wrap-on run lost its key events (a stray window on the
display) and is left out of the click/Right/key columns.

| File | hl | wrap | | open | click | Right | key |
|---|---|---|---|---|---|---|---|
| 1 MB | on | on | before | 6.9 (4.7–15.9) | 0.4 (0.3–1.0) | 0.5 (0.3–1.2) | 8.8 (2.4–12.4) |
| 1 MB | on | on | after | 1.9 (1.0–1.9) | 0.5 (0.5–0.5) | 0.8 (0.7–0.9) | 2.1 (0.7–3.4) |
| 1 MB | on | off | before | 40.5 (36.7–80.5) | 9.7 (7.4–15.7) | 8.5 (7.8–8.8) | 17.9 (17.1–19.2) |
| 1 MB | on | off | after | 9.1 (9.1–10.4) | 2.0 (2.0–3.0) | 2.0 (1.9–2.9) | 4.3 (4.1–6.4) |
| 1 MB | off | on | before | 0.8 (0.8–1.2) | 0.3 (0.3–0.4) | 0.3 (0.3–0.8) | 0.7 (0.5–0.7) |
| 1 MB | off | on | after | 0.8 (0.8–1.0) | 0.3 (0.3–0.4) | 0.3 (0.3–0.4) | 0.8 (0.7–1.0) |
| 1 MB | off | off | before | 6.2 (6.2–9.4) | 2.0 (1.9–3.0) | 2.1 (2.0–2.5) | 2.1 (2.0–2.1) |
| 1 MB | off | off | after | 7.0 (6.2–7.5) | 2.1 (2.0–8.3) | 2.2 (1.9–9.1) | 2.3 (2.0–6.2) |
| 8 MB | on | on | before | **43.2** (43.2–43.4) | 2.3 (2.1–2.3) | 2.2 (2.1–2.3) | **22.9** (22.7–23.0) |
| 8 MB | on | on | after | **10.3** (10.3–10.4) | 2.0 (2.0–2.2) | 2.1 (2.0–2.2) | **6.3** (6.2–6.3) |
| 8 MB | on | off | before | > 200 | > 60 | > 60 | > 60 |
| 8 MB | on | off | after | 80.2 | 22.5 | 20.9 | 41.6 |
| 8 MB | off | on | before | 8.1 (8.1–8.3) | 2.3 (2.1–2.3) | 2.3 (2.1–2.3) | 3.8 (3.7–4.0) |
| 8 MB | off | on | after | 8.1 (7.8–8.7) | 2.2 (2.1–2.3) | 2.1 (2.1–2.2) | 3.8 (3.6–3.9) |
| 8 MB | off | off | before | 58.4 | 19.1 | 19.0 | 18.9 |
| 8 MB | off | off | after | 69.4 | 23.9 | 20.1 | 20.5 |

Reading it:
- With highlighting on (the default), the stall drops 4–5× everywhere:
  - 8 MB, wrap on: open 43 → 10 s, key 23 → 6 s;
  - 1 MB, wrap off: open 40 → 9 s, key 18 → 4 s;
  - 8 MB, wrap off: > 200 → 80 s.
- With highlighting off nothing changes, as expected. That is the FLTK
  floor of cause B.
- Highlighting on still costs about 2 s more than off on the 8 MB line. No
  syntect work is left; the cause is one extra full FLTK repaint (see
  *Known limitation*).

Normal file, hl on, wrap on, release: `src/app/controllers/highlight.rs` as
on `master` (773 lines of Rust). Opened, a line typed
(`let t0087 = "typed"; // check`) and re-highlighted. Screenshots before and
after: `compare -metric AE` = **0 differing pixels**, both after open and
after typing.

Debug-build UI smoke (fltk debug assertions), Xvfb:
- 1 MB file: open, click, End, type, Home.
- 8 MB file: open, click, type.
- No panic and no assertion in the log. The long line shows in the plain
  text colour, and the typed characters land at the cursor.

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
- FLTK's own `draw()` runs `recalc_display()` **twice** when the widget has
  `FL_DAMAGE_ALL`: once because `display_needs_recalc_` is set, and once in
  the `FL_DAMAGE_ALL` branch. `display_needs_recalc()` itself calls
  `redraw()`, i.e. `FL_DAMAGE_ALL`. So one keystroke is one draw with two
  full measurements of the line.
- Every extra full repaint the application triggers multiplies this cost.
  50 ms after a keystroke, `rehighlight_document` rewrites the style range
  and calls `editor.redraw()`. That is a second draw, with two more
  recalcs. gdb on one typed character in the 8 MB file, hl on: four
  `recalc_display()` calls, all from `Fl::flush` → `draw()`. This is the
  ~2.5 s gap between "hl on" and "hl off" in the key column, and the ~2 s gap
  at open.
- Possible follow-up, a separate ticket: restyle without a full repaint.
  Skip the style write and the redraw when the new style chars equal the
  current ones, which is always the case for a skipped long line; otherwise
  damage only the restyled range (`redisplay_range`) instead of
  `redraw()`. That would bring "hl on" down to the "hl off" floor on long
  lines. It is not done here: it changes the repaint path for every file
  and needs its own review.

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
- `highlight_incremental` has no total-bytes bound. An edit whose parse
  state never converges (e.g. typing an unclosed `/*` near the top of a
  large file) still re-parses to the end of the file synchronously. This
  ticket only bounds the per-line cost there. Follow-up: T0096.
- Restyling without a full repaint (Known limitation): follow-up T0095.

## How to test

### Regression tests

```bash
nix develop -c cargo test --lib long_line
```

The eight T0087 tests (the filter also matches three older long-line tests
in other modules, which stay green):

- `controllers::highlight::tests::long_line_clear_annotations_follows_the_plan`
  - 6000 short lines (over the plan's bound): `clear_annotations` returns
    `Chunked(id)` and leaves the style buffer untouched (no synchronous
    pass).
  - The same document with its chunked run already in flight:
    `Restarted`, the run restarts from byte 0, and nothing is queued (one
    message chain).
  - A small document: `Restyled`, with exactly the styles of a full
    highlight.
  - Red before the fold-in: `left: Restyled, right: Chunked(DocumentId(1))`.
  - The test must not reach `defer_send`. `fltk::app::add_timeout3` asserts
    fltk-rs's "UI thread" and pins it to the calling test thread, after which
    `ui::dialogs::tests::popup_is_top_level`, on another thread, fails. A
    first version that queued from inside the controller did exactly that.
    Returning the action keeps the test free of the FLTK runtime.

- `services::syntax::tests::long_line_boundary_is_inclusive`
  - A 16 384-byte line is tokenized; a 16 385-byte line is plain.
  - Added after the fix. It was run red with the rule disabled: `panicked …
    one byte over: plain`.

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
    holds ≤ `CHUNK_BYTES` (64 KiB) plus one line.
  - 17 lines of exactly 16 KiB (tokenized, counted in full): the first chunk
    stops at ≤ `CHUNK_BYTES` plus one line.
  - A skipped 600 KiB line does not use up the budget.
- `controllers::highlight::tests::long_line_highlight_plan_bounds_lines_and_bytes`
  - 1025 lines of 1 KiB are `Chunked`; 1024 are `Synchronous`.
  - 64 lines of exactly `LONG_LINE_BYTES` (= 1 MiB, all tokenized) are
    `Synchronous`; 65 are `Chunked`.
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
      green, and the screenshot diff shows 0 differing pixels.
- [x] Xvfb, release, highlighting on: the stall shrinks 4–5× (8 MB default:
      open 43 → 10 s, key 23 → 6 s). What remains is FLTK work only (no
      syntect frame in any sample); the 2 s above the hl-off floor is one
      extra full repaint (Known limitation, follow-up).
- [x] Regression tests added and green; they failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` all pass with zero warnings.
- [x] User verification: the manual repro on a real desktop (Wayland
      / GPU), where FLTK's per-recalc cost may differ from Xvfb.

## Affected files

- `src/app/services/syntax/highlighter.rs` — `LONG_LINE_BYTES`,
  `is_long_line`, the shared `highlight_line` step (full and incremental).
- `src/app/services/syntax/mod.rs` — the chunked path uses `highlight_line`,
  has the `CHUNK_BYTES` budget, and computes `done` from the consumed text.
  T0087 tests.
- `src/app/controllers/highlight.rs` — `HighlightPlan` / `highlight_plan`
  replace four line-count checks; `queue_chunked` replaces three queueing
  copies; `clear_annotations` returns a `ClearAnnotations` action;
  `bind_active_styles`. Plan and clear-annotations tests.
- `src/dispatch.rs` — `AnnotationsClear` applies the `ClearAnnotations`
  action.

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

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration
build `verify/rc3` (checklist `VERIFICA-RC3.md`).
