---
id: T0095
title: A restyle repaints only the restyled range, not the whole editor
status: done            # todo | review | done
created: 2026-11-26
severity: moderate
area: robustness
depends-on: [T0096]
issue: 204
---

## Goal

When syntax highlighting rewrites part of the active document's styles,
FerrisPad repaints the whole editor with `redraw()`. FLTK answers
`FL_DAMAGE_ALL` with two `recalc_display()` calls, and each one re-measures
the whole buffer. Two paths do this:
- **every keystroke's** re-highlight step;
- **every chunk** of the background (chunked) highlight.

On an 8 MB one-line file, typing a key costs about 2 s more with
highlighting on than off (6.3 s vs 3.8 s, T0087), although no syntect work
is left on that line. With 64 KiB chunks (T0087), the per-chunk repaint is
also about 25 % of the time it takes to highlight an 8 MB multi-line file.
A restyle must repaint only what it restyled.

## In scope

- One rule for showing a partial style write of the active document. The
  style table is re-bound only when it gained entries. Then only the
  restyled range is repainted (`redisplay_range`), never `redraw()`.
- That rule serves the keystroke step (`apply_incremental`), the
  background continuation steps (T0096, which already repaint a range)
  and every chunk of the chunked highlight (`continue_chunked_highlight`).
- Win back the chunk overhead T0087 accepted for 64 KiB chunks.

## What was done

- `HighlightController::show_restyle` is the one rule for every partial
  style write of the active document:
  - it re-binds the style table only when the table gained entries;
  - it repaints only the range it restyled (`redisplay_range`).
- `apply_incremental` (a keystroke step and a T0096 continuation step)
  and every chunk step of `continue_chunked_highlight` go through it. The
  `full_repaint` flag T0096 kept for the keystroke step is gone.
- Each chunk used to re-bind the style table and `redraw()` the whole
  editor. Now it re-binds only when the table changed and repaints only
  its own range.
- No change to the chunk size or to whole-buffer style writes.

## Out of scope

- Whole-buffer style writes (a full synchronous highlight, re-highlight
  all, a theme or font change, plugin line annotations). They happen once
  per action and keep their full repaint.
- FLTK's own cost of an edit on a very long line (the floor with
  highlighting off, T0087 cause B).
- Chunk size: it stays 64 KiB.

## How to test

### Regression test

```bash
nix develop -c cargo test --test restyle_damage
```

It builds the real main window and `AppState` (needs a display; skipped
without one unless `FERRISPAD_REQUIRE_UI_TESTS=1`, as CI sets).
- Keystroke: a typed `"` opens a string in a highlighted Rust document.
  After the re-highlight step, the editor has range damage
  (`FL_DAMAGE_SCROLL`) and no `FL_DAMAGE_ALL`.
- Chunks: a > 1 MiB document highlighted in chunks; no chunk step between
  the first and the last leaves `FL_DAMAGE_ALL` on the editor.

Red run on `ticket/T0096` (23190d2), before the fix:

```text
a keystroke's re-highlight repainted the whole editor: Damage(Expose | All)
a keystroke's re-highlight repainted no range: Damage(Expose | All)
26 of 27 chunk steps repainted the whole editor
```

After the fix it passes. Full gates: 677 passed (UI tests included, under
Xvfb), clippy `-D warnings` and fmt clean.

After 0.9.10 (2026-11-27) master was merged (e76e63c), with T0096's final
head first. The diff against master is this ticket's own three files.
Gates: 758 passed (UI tests required, under Xvfb); clippy `-D warnings` and
fmt are clean.

### Measurements (Xvfb, release builds)

"Before" is `ticket/T0096` (23190d2), "after" this branch, "256 KiB" the
T0087 binary with 256 KiB chunks. The fixtures are T0087's (`min8.js`: 8 MB
on one line; `multi8.js`: the same 8 MB in 66 667 lines). Highlighting on,
wrap on.

**FLTK recalcs per key** (`gdbkey.sh`, gdb
`break Fl_Text_Display::recalc_display`, one `x` typed in `min8.js`):
4 before, **2 after**. T0087 measured 2 with highlighting off.

**User-space instructions** (`t95perf.sh`: `perf stat -e instructions:u`
attached to the process, until idle). The machine is shared and wall times
swung by 5x under a load of 30 to 40, while instruction counts repeat
within 1 %. Two runs each:

| File | | open | one key |
|---|---|---|---|
| `min8.js` | before | 163.1 G / 162.6 G | 100.8 G / 100.5 G |
| `min8.js` | after | 125.6 G / 124.7 G (−23 %) | 61.9 G / 62.0 G (**−39 %**) |
| `multi8.js` | before (64 KiB) | 108.3 G / 108.2 G | 1.30 G / 1.25 G |
| `multi8.js` | after (64 KiB) | 59.8 G / 59.9 G (**−45 %**) | 1.26 G / 1.28 G |
| `multi8.js` | 256 KiB (T0087) | 72.4 G / 72.6 G | 1.29 G / 1.31 G |

The chunked open at 64 KiB now costs 17 % less than at 256 KiB before, so
T0087's chunk overhead is won back with room to spare.

**Wall time**, one interleaved pass at load 4 to 6 (`t0095.sh`, seconds):

| File | | open | one key |
|---|---|---|---|
| `min8.js` | before / after | 22.4 / 23.1 | **8.9 / 5.8** |
| `multi8.js` | before / after / 256 KiB | **10.8 / 6.8** / 6.0 | 0.12 / 0.12 / 0.12 |

Open times on `min8.js` are within the noise at this load; the instruction
counts above are the reliable measure.

**Colours unchanged** (`pix95.sh`, `compare -metric AE`, before vs after):
**0 differing pixels** on each of five screenshots:
- a normal Rust file (`highlight.rs`) after open;
- the same file after typing `/* x` on line 7 (the rest of the screen
  turns comment);
- after closing it with ` */`;
- `multi8.js` at the top, right after open (chunking in progress);
- `multi8.js` at the end, after the chunked pass.

Each screenshot was also inspected.

### Manual (the user)

Open a large source file (several MB) with highlighting on, type in the
middle and at the end, scroll while the "highlighting…" banner is shown:
colours appear as before, with no stale colour left after a pause.

## Acceptance criteria

- [x] A keystroke's re-highlight and a chunk step do not repaint the whole
      editor (no `FL_DAMAGE_ALL`).
- [x] The 8 MB one-line file: one key does 2 `recalc_display()` calls with
      highlighting on, as with it off.
- [x] The chunked highlight of the 8 MB multi-line file at 64 KiB is no
      slower than at 256 KiB before (−17 % instructions).
- [x] Colours unchanged: pixel-identical screenshots on a normal file and
      on the chunked file.
- [x] Regression test added; it failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [x] User verification (2026-11-27).

## User verification

The user approved the PR on 2026-11-27 ("ok 206, 240, 215"), relayed by
the orchestrator (ferrispad-03), after reading the presentation
`docs/pr/0.9.11/PR-206-restyle-without-full-repaint.html`. The reviewer approved
6271c11, plus the page-only nit in ba5c604 (the wall-time caveat). Before
the move, master was merged again (T0111, #213; 81c9ea6), with no conflict and no
change to this ticket's files.

## Affected files

- `src/app/controllers/highlight.rs` — new `show_restyle`, used by
  `apply_incremental` (its `full_repaint` parameter is gone) and
  `continue_chunked_highlight`.
- `tests/restyle_damage.rs` — new.

## Notes

- Follow-up from T0087's *Known limitation*. ID reserved by the
  orchestrator (ferrispad-5a); stacked on T0096, which moved the
  keystroke redraw into `apply_incremental`.
- T0087 evidence. FLTK 1.5.24 `Fl_Text_Display::draw()` runs
  `recalc_display()` twice under `FL_DAMAGE_ALL`. With gdb on one typed
  character in the 8 MB one-line file, highlighting on makes 4 calls (2
  draws) and highlighting off makes 2. The extra draw comes from the
  re-highlight's `redraw()`.
- T0087 chunk data (8 MB multi-line, release): 6.8–7.5 s at 256 KiB
  chunks vs 8.7–9.3 s at 64 KiB. Each chunk re-bound the style table
  (`FL_DAMAGE_EXPOSE`) and repainted everything (`FL_DAMAGE_ALL`).
- FLTK: `highlight_data()` only sets `FL_DAMAGE_EXPOSE`;
  `redisplay_range()` sets `FL_DAMAGE_SCROLL` and draws only that range.
  That is FLTK's own recipe for a style-buffer update (`test/editor.cxx`,
  `style_update`). Every style table entry uses the same font, so a
  restyle never changes glyph widths or wrap points.
- Considered and left out: skipping the style write when the new style
  bytes equal the current ones. On the 8 MB line the comparison would copy
  the same 8 MB out of the style buffer that the write moves. After this
  change the remaining repaint covers only the visible part of the range,
  so the skip would save little and add a branch.
- Seen in the screenshots, not part of this ticket: the line-number gutter
  cuts 5-digit numbers (`6661` for 66661) the same way before and after.
  That is T0107's bug, fixed on its own branch.
