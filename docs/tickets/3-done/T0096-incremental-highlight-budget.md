---
id: T0096
title: Bound the synchronous re-highlight of an edit; continue a non-converging one in chunks
status: done            # todo | review | done
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

## Review fixes (ferrispad-a3, 2 blocking)

**1. A second key before the continuation left stale colours for good.**
- Sequence: type `/*`, so the step pauses at 64 KiB, then type one more key
  before any continuation.
- The folded pass starts from line 0 and "converged" against checkpoints
  the first pass had already rewritten in place before its pause point. So
  everything after the pause kept its pre-`/*` colours.
- Fix: a **convergence floor**.
  - `ResumePoint` carries `floor_line`, and `chain_break_line()` =
    max(pause line, inherited floor).
  - When `schedule_rehighlight` folds a pause, it records
    `pending_floor = (doc, chain_break_line, its line count)`.
  - `rehighlight_document` moves the floor down by any lines added since:
    `line + max(0, new_total − old_total)`. A floor too far down costs only
    work; one too early is the bug.
  - It passes the floor to `highlight_incremental`, where
    `first_comparable = max(start_cp + 1, floor.div_ceil(128))`. A pause of
    that pass carries the floor into its continuation (`highlight_resume`
    uses the same max).
  - A re-closed comment still stops early, at or after the floor, where the
    old checkpoints match.
- Test `incremental_budget_keep_typing_after_a_pause` (a3's probe). Red
  with the floor bypassed: `the second pass stopped before the old pause
  point`. Green: after continuing, the styles equal a full highlight.
- Xvfb: on the 10 MB file, type `/` `*` and immediately `abc`; after it
  settles, the end of the file is coloured as comment.

**2. `continue_pending` could stick, and with it all background
highlighting.**
- `main.rs` routes `ContinueHighlight` in the group it skips when no tab is
  open, and the flag was cleared only in the handler. Close a large file
  mid-chunking: the queued message is dropped, the flag stays set, and no
  chunked or continued highlight runs again in that process.
- That is the class T0093 found for `DirtyChanged`. It is closed here for
  the whole group: `AppState::note_dequeued(&msg)` runs in `main.rs` for
  every message taken off the queue, *before* dispatch, and clears each
  pending flag it owns.
- Audit of every message behind a `count() > 0` guard in `main.rs`:

  | Message | Guarded group | State |
  |---|---|---|
  | `EditUndo`, `EditRedo`, `EditCut`, `EditCopy`, `EditPaste`, `SelectAll`, `ShowFind`, `ShowReplace`, `ShowGoToLine` | edit | carries no state |
  | `BufferModified` | highlighting | carries no state |
  | `DoRehighlight` | highlighting | `rehighlight_timer_active`: cleared on dequeue (was: only in its handler, so stuck the same way; pre-existing) |
  | `ContinueHighlight` | highlighting | `continue_pending`: cleared on dequeue |
  | `DoTextChangeHook` | highlighting | `AppState::text_change_timer_active`: cleared on dequeue (pre-existing, same class) |

  `DirtyChanged` (T0093) has its own route and `document::note_dequeued`.
  When T0093 and this ticket meet on `master`, `AppState::note_dequeued`
  should call it as well, so there is one hook.
- Test `incremental_budget_flags_cleared_on_dequeue`: flag set, message
  dequeued without its handler → the next `request_continue` sends again;
  `DoRehighlight` likewise; other messages leave it alone. Red with the
  clearing bypassed: `the next continuation is sent again`.
- Xvfb: open the 10 MB file, close the tab 1.5 s in (chunked pass still
  running) → start page; Ctrl+O the same file again; it is highlighted to
  the end.

Gates after the fixes: 672 passed (twice), clippy `-D warnings` and fmt
clean.

**Review nit (non-blocking, applied): a displaced floor is not lost.**
- `pending_floor` holds one document, like `pending_rehighlight`. A fold for
  document B while A's floor was still pending would have dropped A's floor,
  so A's folded pass could converge early and keep a stale tail.
- The fold is now `fold_pause(id, pos)`, a pure method returning the
  displaced document. `schedule_rehighlight` hands that document to
  `queue_chunked`, for a full chunked pass, as `set_incremental_resume`
  already does for a displaced pause.
- Test `incremental_budget_displaced_floor_is_not_lost`. Red with the
  displacement ignored: `A's floor goes to a chunked pass`. Gates: 673
  passed.

## Acceptance criteria

- [x] A keystroke never re-highlights more than `CHUNK_BYTES` (+ one line)
      synchronously.
- [x] The continued result equals a full highlight; a re-closed comment
      stops early.
- [x] At most one `ContinueHighlight` chain; typing quickly does not pile
      up work.
- [x] Regression tests added; they failed before the fix.
- [x] Gates pass with zero warnings; Xvfb measurements before/after.
- [x] User verification: the manual recipe on a real desktop.

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

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration
build `verify/rc3` (checklist `VERIFICA-RC3.md`).
