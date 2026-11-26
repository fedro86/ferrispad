---
id: T0096
title: Bound the synchronous re-highlight of an edit; continue a non-converging one in chunks
status: review          # todo | review | done
created: 2026-11-26
severity: moderate
area: robustness
depends-on: [T0087]
issue: 193
---

## Goal

Typing an unclosed `/*` (or a backtick, or a quote) near the top of a large
normal source file stalls FerrisPad on every keystroke, for a time
proportional to the rest of the file (about 0.54 µs/byte, ~5 s per 10 MB).
The incremental re-highlight runs synchronously until the parse state
converges with the previous pass, and an unclosed comment never converges
before the end of the file. T0087 bounded the work per *line*; this ticket
bounds the work per *keystroke*.

## Cause

`highlight_incremental` (`services/syntax/highlighter.rs`) re-parses from
the checkpoint before the edit until its state equals a stored checkpoint
(convergence) or the text ends. There is no byte budget, so a
non-converging edit always parses to the end of the file, inside the
`DoRehighlight` message handler.

## In scope

One core, "highlight from a point until convergence, the end of the text,
or a byte budget", used in two ways:
- **Per keystroke** (synchronous): budget `CHUNK_BYTES` (64 KiB, the same
  bound T0087 set for one chunk step, about 35 ms on normal code). Most
  edits converge long before that. Otherwise the run *pauses* and records
  its resume point (line, byte offset, parse and highlight state).
- **Continuation** (asynchronous): the paused run continues through the
  existing `ContinueHighlight` message, one `CHUNK_BYTES` step per message,
  with the same convergence check. Checkpoints are updated in place, so a
  comment closed again stops the continuation at the next matching
  checkpoint instead of re-colouring to the end of the file.
- A new edit on the same document drops the pending continuation, and the
  next re-highlight starts at the earlier of the edit and the resume point,
  so fast typing never piles up work.
- At most one `ContinueHighlight` chain exists, ever: a `continue_pending`
  flag set on send and cleared when the message is handled. It replaces
  the per-site `defer_send` calls, which also closes the general "two
  chains" class.

## Out of scope

- The repaint after each step (T0095).
- Changing `CHUNK_BYTES` or the T0087 plan thresholds.

## How to test

### Regression tests

```bash
nix develop -c cargo test incremental_budget
```

- `services::syntax`: a highlighted ~1 MB JavaScript text, then `/*`
  inserted at line 1:
  - the synchronous re-highlight processes ≤ `CHUNK_BYTES` + one line and
    returns a resume point (before the fix: the whole rest of the text);
  - continuing in steps from that point gives exactly the styles of a fresh
    full highlight;
  - closing the comment again (`/* */`) converges within the first step,
    with no continuation.
- `controllers::highlight`: the continue slot is taken once; a second
  request while one is pending sends nothing; after the handler runs, a
  request sends again.

### Manual (Xvfb by Claude, then the user)

1. Generate a ~10 MB normal JavaScript file (120-column lines), open it with
   default settings, and wait for highlighting to finish.
2. Go to the top, type `/*`, and measure the per-key cost and the time until
   CPU is idle (colours settled). Before: about 5 s per key.
3. Type `*/` to close it: the colours return quickly.
4. A small file: unchanged.

Red runs, before each fix. Each later piece was red-run against its
bypassed form.

```text
incremental_budget_pauses_a_non_converging_edit ... FAILED
  the keystroke re-highlighted 143672 bytes synchronously
incremental_budget_reclosed_comment_converges_early ... FAILED
  stopped early: 143674 bytes
incremental_budget_one_continue_chain ... FAILED      (claim always true)
  already queued: no second chain
current_line_rewrite_elsewhere_keeps_twin_current ... FAILED   (always stale)
  far away: still current
```

All green after the fix. Gates: 670 passed, clippy `-D warnings` and fmt
clean.

### Xvfb results (release; "before" = `ticket/T0087` with 64 KiB chunks, the same incremental code without a budget)

A 10 MB normal JavaScript file (111 236 lines, generated), default
settings. Ctrl+Home, then one key at a time, measuring CPU and wall time
until idle after each key:

| Key | before | after |
|---|---|---|
| `/` | 0.07 s | 0.09 s |
| `*` (opens the comment) | 0.12 s | 0.11 s |
| `a` | 0.07 s | 0.08 s |
| `*` | 0.07 s | 0.06 s |
| `/` (closes it: the rest of the file is re-tokenized) | **5.1 s, synchronous** | 5.5 s, **in the background** |

- Inside a comment syntect does almost nothing per line, so *opening* the
  comment is cheap either way. *Closing* it re-tokenizes the whole rest of
  the file, about 5 s for 10 MB, in one synchronous step before the fix.
- **Latency of a key typed 1 s after the closing `/`**, measured as the
  time until the first text line on screen changes, 2 runs each:
  - before: **4.5 s / 4.8 s** (the UI is frozen);
  - after: **0.16 s / 0.16 s** (screenshot polling included).
- After settling, the end of the file is coloured as code again (screenshot,
  release and debug builds). No panic in the debug build's log.

## What was done

- `highlighter.rs`: one loop, `highlight_lines_from`, runs until
  convergence, the end of the text, or a byte budget; then it returns a
  `ResumePoint` (line, byte, total line count, parser state).
  `highlight_incremental` uses it from the checkpoint before the edit;
  `highlight_resume` continues a pause with the same convergence check and
  in-place checkpoint updates.
- `syntax/mod.rs`: `INCREMENTAL_BUDGET = CHUNK_BYTES` (64 KiB), the same
  bound as one chunk step, so one keystroke costs at most one step (≈35 ms
  on normal code, ≈0.23 s on dense minified lines).
- `controllers/highlight.rs`:
  - A pause is kept in one `PausedHighlight` slot together with the text it
    was computed on. Any edit drops the slot, so a continuation step never
    copies or rescans the buffer.
  - The slot is continued one step per `ContinueHighlight`, before the
    chunked queue.
  - A pause for another document turns the previous one into a full chunked
    pass, so its remainder is never lost.
  - A new edit of the same document folds the pause into the next
    re-highlight (`min(edit, resume)`).
  - Full re-highlights and disabling highlighting drop the slot.
  - **One chain:** `continue_pending` is set by `claim_continue()` and
    cleared on entry to the handler. Every `ContinueHighlight` send goes
    through `request_continue()`, including `queue_chunked`, the chunked
    path and the queued starts.
- **Deliberate deviation from the first scope (accepted by the
  orchestrator).** Found while measuring, and fixed here because the new
  per-step path made it matter. T0095 keeps the keystroke path. The first version cost 3.9 s to open the comment and 10 s to
  close it. Each continuation step caused a full editor repaint, and in
  wrap mode FLTK then re-counts the lines of the whole buffer (stacks:
  `recalc_display` → `count_lines` / `estimate_lines`). There were two
  sources:
  - The step's own `editor.redraw()`. Continuation steps now call
    `redisplay_range(start, end)` (`FL_DAMAGE_SCROLL`, no recalc); the
    keystroke's step still repaints fully, as before.
  - T0081's current-line twin. `Document::replace_styles` marked the twin
    stale on every style write, so the main loop re-twinned the cursor's
    line and called `redraw()`. `replace_styles` now uses
    `CurrentLineTwin::mark_stale_in(start, end)`, which marks it stale only
    when the rewritten range touches the twinned line. A same-length
    rewrite elsewhere cannot remove twin chars. Test
    `current_line_rewrite_elsewhere_keeps_twin_current`.
- Noticed, not changed: the line-number gutter cuts 6-digit numbers on a
  111 236-line file ("111233" shows as "1233"). The gutter width is
  computed from the digit count, but the widest digits do not fit. This is
  pre-existing; a backlog candidate.

## Acceptance criteria

- [x] A keystroke never re-highlights more than `CHUNK_BYTES` (+ one line)
      synchronously.
- [x] The continued result equals a full highlight; a re-closed comment
      stops early.
- [x] At most one `ContinueHighlight` chain; typing quickly does not pile
      up work.
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; Xvfb measurements before/after.
- [ ] Pending user verification: the manual recipe on a real desktop.

## Affected files

- `src/app/services/syntax/highlighter.rs` — the budgeted core, resume point
- `src/app/services/syntax/mod.rs` — facade for incremental + continuation
- `src/app/controllers/highlight.rs` — `PausedHighlight` slot,
  `continue_pending` / `request_continue`, `apply_incremental` (range
  repaint for continuation steps), wiring
- `src/app/services/current_line.rs` — `mark_stale_in`
- `src/app/domain/document.rs` — `replace_styles` uses it

## Notes

- Found by ferrispad-a3 in the T0087 review; assigned by the orchestrator
  (ferrispad-5a). An rc.3 candidate.
