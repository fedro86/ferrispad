---
id: T0081
title: Highlight the current line (on by default, toggleable, theme-colored)
status: done            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0079, T0080]  # T0080: ordering only (linear stack, single verification build)
issue: 156              # GitHub issue number
---

## Goal

Code editors (VS Code `editor.lineHighlightBackground`, Sublime/TextMate
`lineHighlight`, JetBrains "caret row", Notepad++ "current line background")
highlight the line containing the cursor by default. FerrisPad has no such
highlight, and FLTK's `Fl_Text_Display` offers none natively. Add it: **on by
default**, toggleable, with its color taken from the active theme.

## In scope

- **Setting.** `highlight_current_line: bool` in `AppSettings`, defaulting to
  `true`, also when the field is missing from an older `settings.json`.
  - Toggle it from a "View/Toggle Current Line Highlight" menu item
    (`MenuFlag::Toggle`, like "Toggle Line Numbers") and from a checkbox in
    Settings.
  - Turning it off removes the highlight at once.
- **Color.** Use `HighlightController::editor_colors().current_line` from
  T0079: the theme's `line_highlight` for custom themes, a derived tint of
  the background for built-ins (built-ins ignore `theme.settings`, like the
  selection). Do not read syntect here; that would duplicate T0079's rule.
- **Rendering via the style buffer.** FLTK supports a background per style
  entry (`TextAttr::BgColorExt` fills to the right edge; the diff view in
  `src/ui/split_panel.rs:150` already uses `TextAttr::BgColor`). Give each
  plain/syntax style entry a "current-line twin" with the same foreground and
  the line color as `BgColorExt`. The cursor's line then uses twin chars.
  Marker entries with their own background are **not** twinned.
- **One rule for keeping it right.** At most one twinned line per document.
  - It is re-applied when the cursor's line changes, and whenever the
    document's style buffer is rewritten (every `style_buffer.set_text` in
    `src/app/controllers/highlight.rs` and `src/app/controllers/file.rs:257`
    goes through one helper that marks the twin stale).
  - The check runs in the main loop's existing per-iteration update
    (`src/main.rs:740`, next to the status bar), so there is no timer.
- **Spike first.** Before the full implementation, confirm that `BgColorExt`
  in a style table entry renders as a full-width band on Linux (X11 and
  Wayland), Windows and macOS, wrapped lines included. Record the result
  here. If it fails on a platform, stop and re-plan.

  **Spike result (2026-11-25).** Program: `examples/spike_current_line.rs`,
  kept uncommitted in the T0081 worktree. It captures the window with
  `draw::capture_window` and probes pixels row by row.
  - **Linux Wayland (GNOME) and X11 (`FLTK_BACKEND=x11`, XWayland):**
    identical results. A non-empty line whose chars use a `BgColorExt` entry
    gets a band to the right edge, on **every display row** of a word-wrapped
    line. An **empty** line gets **no** band, even when its `'\n'` uses the
    `BgColorExt` style.
  - Cause: in FLTK 1.4, `Fl_Text_Display::position_style()` fills past the
    end of a row with the style of the row's **last visible char**, and only
    when `lineLen > 0`. The `'\n'` is never consulted. An empty row always
    gets the plain background, so no style-buffer trick can band it.
  - Windows / macOS: **pending user verification** (same C++ code path).
  - Decision (user, via the orchestrator): **option A**. Accept no band on
    empty lines; a follow-up may add an overlay for them (see Notes).

## Out of scope

- Editors in the split / diff-review panel and the terminal.
- "Active line number" coloring in the gutter (VS Code
  `editorLineNumber.activeForeground`). This is a possible follow-up.
- A highlight mode that colors only the gutter.

## Edge cases (each one is covered by a test or a manual step below)

1. **Style-table budget.** `StyleMap` caps entries at 26 (`'A'`..`'Z'`,
   `src/app/services/syntax/style_map.rs:169`) and falls back to `'Z'` when
   full. Twins live at `index + 26`, i.e. chars `'['`..`'t'`.
   - Every style char must stay **ASCII (< 0x80)**. The style buffer is a
     UTF-8 `TextBuffer`, and a multi-byte char would desync its length from
     the text buffer.
   - So the total stays ≤ 63 entries. Assert it.
2. **Annotations win.** On the current line, chars using a marker style
   (error/warning/diff, `TextAttr::BgColor`) keep their marker background.
3. **Selection wins.** FLTK draws selected text with the selection color, so
   a selection on the current line stays visible. This is checked manually,
   since FLTK does it.
4. **Edits move lines.** Typing, multi-line paste, deleting lines above,
   undo/redo, plugin or MCP edits: the previously twinned line must never be
   left behind as a second highlighted line ("orphan").
   - Track the twinned range from FLTK's modify callback (pos /
     inserted / deleted), or untwin by range before re-applying. Never scan
     the whole buffer.
5. **No O(n) per event.** Detecting a line change uses
   `buffer.line_start(insert_position)` compared to the stored start, not
   counting lines from 0. That would be O(file size) on every event in a big
   file.
6. **Very long lines.** On a line over 64 KiB (minified JS, logs) the
   highlight is skipped, so moving the cursor never rewrites megabytes of
   styles.
7. **Empty lines and the last line.** A line with text gets the full-width
   band. An **empty line gets none**, and the same goes for an empty last
   line: FLTK's `position_style()` fills past the row end only for
   `lineLen > 0`, with the style of the last visible char (see the spike
   result). Accepted. Nothing is twinned on an empty line.
   - Related: when a line ends in a marker char (`BgColor`, not `Ext`), the
     band stops at the end of the text on that line, because the fill uses
     the last char's style. Accepted.
8. **Word wrap.** All display rows of the logical line are highlighted (same
   as VS Code).
9. **Tabs.** The twin state is per document. Switching tabs never leaves the
   old document with a stale twin when it is shown again.
10. **No style buffer / special views.** Documents shown in tail/chunk or
    memory-mapped read-only mode (over 1.9 GiB), if they have no style buffer
    bound, make the feature a silent no-op.
11. **Color sanity.** The derived or theme color must be visible but weaker
    than the selection, and must keep the text readable. "Readable" is
    relative: the band may cost the text at most 20% of its contrast on the
    plain background, and never has to beat 4.5:1
    (`text/band >= min(4.5, 0.8 * text/bg)`). See Notes.
12. **Unfocused selection on the current line.** For twin chars, FLTK's
    unfocused-selection blend (`clear_rect`:
    `fl_color_average(bgbasecolor, selection_color(), 0.4)`) uses the
    **line color** as `bgbasecolor`, not the editor background. That blend
    must also be registered as a readable-text background in `ui::contrast`
    (T0078), or selected text on the cursor line, such as a Find match while
    the Find dialog has focus, falls back to FLTK's level-10 rule. Covered by
    a test.

## How to test

### Unit tests

Put the twin/untwin/tracking logic in a pure helper over a `&mut [u8]` style
string plus line boundaries, so it can be tested without FLTK:

```bash
nix develop -c cargo test current_line
nix develop -c cargo test --test settings_persistence current_line
```

- `current_line_twin_and_untwin_roundtrip` — twin a line, then untwin it:
  the style bytes are identical to the original.
- `current_line_markers_not_twinned` — a line with marker chars (`'B'`..
  `'G'` and custom `BgColor` entries): only the plain/syntax chars change.
- `current_line_style_table_stays_ascii` — a `StyleMap` filled to its cap
  plus twins: every style char < 0x80, total ≤ 63 entries.
- `current_line_no_orphans_after_edits` — scripted sequence: twin line 10,
  insert 3 lines above, delete 1 line, paste a multi-line block, undo
  (simulated as range ops). After each step, exactly one line holds twin
  chars, and it is the cursor's.
- `current_line_restyle_marks_stale` — after a simulated
  `style_buffer.set_text`, the next update re-applies the twin on the same
  line.
- `current_line_skips_long_lines` — a 100 KiB line is not twinned; the
  neighbouring short line is.
- `current_line_empty_line_not_twinned` — pins the accepted behaviour: on an
  empty line (and an empty last line) nothing is twinned and nothing
  panics. A previously twinned line is still untwinned when the cursor
  moves onto it.
- `current_line_unfocused_selection_blend_registered` — with a current-line
  color registered, the blend `fl_color_average(line, selection, 0.4)`
  gets the readable-text rule (a Find match on the cursor line, unfocused).
- `current_line_color_sane_on_all_themes` — for each of the 7 built-ins,
  on T0079's `editor_colors().current_line`:
  `1.05 <= contrast(line, bg) < contrast(selection, bg)`, and
  `contrast(effective_text, line) >= min(4.5, 0.8 * contrast(effective_text, bg))`.
  The failure message names the theme and all four ratios.
- `test_current_line_default_on` (settings_persistence) — an old
  `settings.json` without the field loads with
  `highlight_current_line == true`; an explicit `false` roundtrips.

All pass; the full `nix develop -c cargo test` stays green.

### Manual check (record results here; Linux: state X11 or Wayland)

0. Spike result per platform (see In scope).
1. Fresh config, `nix develop -c cargo run`, open a source file. The cursor
   line has a subtle full-width band; arrow keys and mouse clicks move it.
2. Select text on that line. The selection is clearly visible on top.
3. Type on the line, paste 5 lines above, delete lines, then undo/redo.
   There is only ever one highlighted line.
4. Word wrap on, with a long wrapped line: every row of it is highlighted.
   Empty line: no band (accepted, see Notes).
5. Open a large file (over 5000 lines, chunked highlighting) and jump around
   while highlighting continues. There are no orphan bands. Open a minified
   file with one huge line: no lag when entering or leaving it.
6. Switch tabs back and forth: one band per document, on its cursor line.
7. View → Toggle Current Line Highlight: the band disappears and comes back.
   Restart: the choice persists.
8. Light, dark, system default, and every syntax theme: the band is visible
   but subtle, and text stays readable.
9. Idle check: with the editor idle, CPU stays at 0% (`top`).

## Acceptance criteria

- [x] The current line is highlighted by default and can be toggled off
      persistently on Linux X11 and Wayland (persistence verified).
- [x] The same on Windows/macOS (user, 2026-11-26, on 0.9.10-rc.2: Windows 11 and macOS).
- [x] The color comes from the theme (`line_highlight`) or a derived tint,
      and passes the color-sanity test on all built-ins (relative text
      criterion, edge case 11).
- [x] No orphan highlights after any edit; no per-event O(file) work; long
      lines skipped.
- [x] Style chars stay ASCII; annotations and selection keep priority.
      (ASCII and markers: unit-tested. Selection over the band: Xvfb/X11
      screenshot.)
- [x] All tests listed above exist and pass; the spike result is recorded.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass; idle CPU stays at 0% (0 CPU ticks over
      5 s idle, see step 9).

## What was actually done

- `src/app/services/current_line.rs` (new, pure): `twin_bytes` /
  `untwin_bytes` (twin = primary + 26, `'['..='t'`), `ByteRange`,
  `shift_range` (follows an edit conservatively: covering too much is
  harmless, too little would orphan), and `CurrentLineTwin`, the per-document
  state (`twinned` range, `stale`, `last_cursor`) with `plan()` / `applied()`.
  It also has `line_to_twin` (empty line or line over 64 KiB → nothing).
- `StyleMap::table_with_twins(line_bg)`: always 52 entries, the 26
  primaries (padded with copies of `'A'`) then their twins. A twin of a
  `TextAttr::None` entry has the same entry plus `BgColorExt` and `line_bg`;
  a marker's twin slot is an unchanged copy, used only for index alignment.
  `StyleMap::twinnable()` gives the mask of real plain/syntax entries.
  `SyntaxHighlighter::style_table()` returns the twinned table, so every
  existing `set_highlight_data_ext` call gets the twins with no change.
- Color: `HighlightController::set_theme` / `new` pass
  `editor_colors().current_line` (T0079) to the highlighter. Theme changes
  already re-bind the table through `bind_active_buffer()`.
- `Document`: `current_line: Rc<Cell<CurrentLineTwin>>` (a `Cell`, so no
  borrow can fail in a callback). The FLTK modify callback, which already
  mirrors edits into the style buffer, now also calls `on_modify` (shift +
  stale). Whole-buffer and partial style writes go through
  `Document::set_style_text` / `replace_styles`, which mark the twin stale:
  every `set_text` / `replace` in `controllers/highlight.rs`, and
  `controllers/file.rs` (the `:257` reset and the reload default style).
  Annotation writes call `mark_style_stale`.
- `AppState::update_current_line_highlight()`, called on every main-loop
  iteration next to the status bar (no timer):
  - it returns in O(1) when the cursor did not move and nothing was edited
    or restyled (`is_current`);
  - otherwise it untwins the tracked range and twins the cursor's line,
    reading and writing only those bytes (`text_range_no_leak`, a new
    non-leaking `text_range` in `infrastructure/buffer.rs`) and redrawing
    only if something changed;
  - it is a no-op when the style buffer length differs from the text
    (no style buffer bound, edge case 10).
  The cursor's line is found in a **bounded byte window** (review round 1,
  below), not with FLTK's `line_start` / `findchar_forward`.
- Setting `highlight_current_line` (default `true`, also for old files), the
  menu item "View/Toggle Current Line Highlight" (`MenuFlag::Toggle`;
  `Message::ToggleCurrentLineHighlight` → `AppState::toggle_current_line_highlight`,
  which flips, **saves** and marks every document stale), and the "Highlight
  Current Line" checkbox in Settings → View Options. `apply_settings` syncs
  the menu check and marks all documents stale.
- `ui::contrast`: `EditorSelectionBgs::with_current_line(line)` adds FLTK's
  unfocused-selection blend on the band,
  `fl_color_average(line, selection, 0.4)`, to the readable-text
  backgrounds. `register_editor_selection(bg, selection, current_line)`
  stores it (a third atomic). This covers edge case 12.
- Tests (`cargo test current_line`, 13): the 8 in `services::current_line`
  (roundtrip, markers, ASCII chars, no orphans, restyle marks stale, long
  lines, empty line, idle no-op, shift cases), the 2 in `style_map` (ASCII
  table budget, padding), `ui::contrast::current_line_unfocused_selection_blend_registered`,
  `ui::theme::current_line_color_sane_on_all_themes`, and
  `settings_persistence::test_current_line_default_on`.

### Review round 1 (ferrispad-a3): bounded line lookup

- **Finding (blocking):** `line_start(cursor)` and
  `find_char_forward(cursor, '\n')` are unbounded in FLTK
  (`findchar_backward`, and a per-char loop with a UTF-8 decode per step,
  `Fl_Text_Buffer.cxx:1804-1845`). `MAX_LINE_BYTES` only limited the
  rewrite, after both scans had already walked the whole line. On a 50 MB
  one-line file every cursor move or keystroke walked up to 50 MB, so the
  "no per-event O(file) work" criterion did not hold.
- **Fix:** `services::current_line::cursor_line(len, cursor, read)` reads
  **one** window of at most `MAX_LINE_BYTES + 1` bytes on each side of the
  cursor, clamped to the buffer. The pure `line_bounds_window` looks for
  `'\n'` in the **raw bytes** (`'\n'` never occurs inside a UTF-8
  multi-byte sequence, so offsets are exact even when the window cuts a
  character). A side with no `'\n'` that does not reach the buffer edge
  means the line is too long, so there is no band and the previous line is
  untwinned. A short read means an embedded NUL truncated FLTK's C string;
  that also gives no band (fail safe).
- New `infrastructure::buffer::text_range_bytes_no_leak`: raw bytes through
  `Fl_Text_Buffer_text_range` plus `free()`. fltk-rs's `text_range` leaks
  the copy and decodes lossily. `text_range_no_leak` (for the ASCII style
  buffer) now wraps it.
- Tests: `current_line_lookup_is_bounded_on_huge_lines` (bytes read ≤
  2·(MAX+1) on a 4 MiB line, cursor at start/middle/end, and a short line
  between two huge ones), plus `line_bounds_window_finds_line`,
  `line_bounds_window_too_long_returns_none`,
  `line_bounds_window_at_buffer_edges` and
  `line_bounds_window_truncated_returns_none`.
- Red run, against a version of `cursor_line` that reads the whole line,
  which is what FLTK's scans walk:
  ```
  test app::services::current_line::tests::current_line_lookup_is_bounded_on_huge_lines ... FAILED
  panicked at src/app/services/current_line.rs:468:13:
  cursor 0: read 4194304 bytes
  ```

### Verification recipe

```bash
nix develop -c cargo test current_line
nix develop -c cargo test --test settings_persistence current_line
```

Manual, Linux, isolated config:

```bash
mkdir -p /tmp/fp-cfg && XDG_CONFIG_HOME=/tmp/fp-cfg nix develop -c cargo run -- src/main.rs
FLTK_BACKEND=x11 XDG_CONFIG_HOME=/tmp/fp-cfg nix develop -c cargo run -- src/main.rs   # X11
```

### Manual results

- **Linux X11 (XWayland under GNOME), scripted with `xdotool` and captured
  with `xwd`**, opening `src/app/services/current_line.rs` (word wrap on,
  Ocean Light):
  - at startup, line 1 (two display rows) has the band on both rows, to the
    right edge (step 1, step 4 for wrapped lines);
  - a click on line 7 moves the band there, again on both wrapped rows, and
    line 1 is plain (step 1);
  - after Return ×2, typing, and Up (edits that move lines below), only the
    cursor line can carry a band, and no orphan is left. On the new empty
    line there is no band (accepted, step 4).
- **Xvfb/X11 (driven by Claude)**, debug build of the stacked tip
  (ticket/T0077 at 92b19f0), Xvfb 1280×800 + openbox, Ocean Light unless
  noted. Every check measures the band from the screenshot: a strip near
  the right edge of the text area, counting rows of one uniform non-background
  color (`uitest-T0081/bands.py`). Screenshots and logs are in
  `/home/fedro/Code/personal/ferrispad-wt/uitest-T0081/run*/` (scripts
  `uitest-T0081/t0081*.sh`).
  - Step 1: one band at open; arrow keys move it down; a click moves it, and
    there is still exactly one band — **pass**.
  - Step 2: Shift+End on the band line; the blue selection is clearly drawn
    over the band and the text is readable (`run/04-selection-on-band.png`)
    — **pass** (visual).
  - Step 3: typing on the line; copying 5 lines and pasting them; deleting
    2 lines; undo ×3; redo ×2; Return ×2 at line 1, undone again. After
    every edit there is exactly one band, with no orphan — **pass**.
  - Step 4: the long word-wrapped line has the band on all 3 display rows
    (`run/11-wrapped.png`); the empty line has no band, as accepted —
    **pass**.
  - Step 5: on a 6000-line `.rs` file (chunked highlighting), Ctrl+End,
    Page Down and Ctrl+Home while highlighting runs leave one band, and
    still one after it settles — **pass**. On an 8 MB one-line
    `minified.js` there is no band (over 64 KiB) — **pass**. Cost of 40
    cursor moves, feature on vs off: 363 vs 362 CPU ticks, no measurable
    difference — **pass**. Pre-existing and independent of this ticket:
    opening that file keeps the debug build busy for about 20 s (syntax off)
    or longer (syntax on), the same on a master-based build (T0082
    worktree).
  - Step 6: tabs on, `code.rs` + `big.rs`. One band on the active tab, and
    one after each tab switch back and forth — **pass**.
  - Step 7: View → Toggle Current Line Highlight removes the band and saves
    `false`; toggling again brings it back; after a restart with it off
    there is no band — **pass**.
  - Step 8: all 7 built-ins × theme mode Light/Dark (14 launches): exactly
    one visible band each, matching the measured table (e.g. Ocean Dark
    (57,62,72) on (43,48,59), InspiredGitHub (242,242,242) on white) —
    **pass**.
  - Step 9: idle for 5 s with a file open: 0 CPU ticks (`top`: 0.0 %) —
    **pass**.
  - No panic or debug assert in the app log.
- **Pending user verification:** native Wayland, Windows and macOS (also
  the spike, i.e. `BgColorExt` bands there).

## Affected files

- `src/app/services/syntax/style_map.rs` — twin entries, ASCII budget.
- `src/app/services/current_line.rs` (new) — pure twin/untwin/tracking
  helper.
- `src/app/controllers/highlight.rs` (`:174`, `:397`, `:446`, `:477`, `:624`),
  `src/app/controllers/file.rs:257` — route style writes through one helper
  that marks the twin stale.
- `src/main.rs:740` — per-iteration update hook.
- `src/app/domain/settings.rs` — `highlight_current_line` (default `true`).
- `src/ui/menu.rs:471-478`, `src/app/domain/messages.rs`, `src/dispatch.rs` —
  the toggle message and menu item.
- `src/ui/dialogs/settings_dialog.rs` — checkbox.
- `src/app/infrastructure/buffer.rs` — `text_range_bytes_no_leak` (raw
  bytes, frees FLTK's malloc'd copy) and `text_range_no_leak` on top of it.
- `src/ui/theme.rs`, `src/ui/contrast.rs` — use `editor_colors().current_line`
  from T0079; register the unfocused-selection blend on the line color.

## Notes

- Origin: GitHub issue #150 ("line colors"); see T0079 for the full split.
  Open a tracking issue when implementation starts.
- Stacked on T0079 (which contains T0078): T0079 already resolves the
  current-line color (`editor_colors().current_line`), and the color-sanity
  test compares against T0078's selection color. T0080 makes it editable;
  this ticket does not need it.
- **Color-sanity criterion amended** (orchestrator decision). The original
  absolute bar, `text/band >= 4.5`, could not be met for themes whose own
  `text/bg` is below 4.5 (SolarizedLight 4.13). That spec error was the
  orchestrator's. A relative bar,
  `text/band >= min(4.5, 0.8 * text/bg)`, stays theme-agnostic: no change
  to T0079 and no Solarized special case. Measured on T0079's colors:

  | Theme | band/bg | sel/bg | text/band | text/bg |
  |---|---|---|---|---|
  | Base16OceanDark | 1.232 | 3.87 | 6.19 | 7.63 |
  | Base16OceanLight | 1.124 | 3.98 | 5.47 | 6.15 |
  | Base16EightiesDark | 1.229 | 4.04 | 7.27 | 8.94 |
  | Base16MochaDark | 1.234 | 3.68 | 6.18 | 7.63 |
  | SolarizedDark | 1.210 | 4.40 | 3.92 | 4.75 |
  | SolarizedLight | 1.123 | 4.17 | 3.68 | 4.13 |
  | InspiredGitHub | 1.119 | 4.50 | 11.45 | 12.82 |

- Known limitation (pre-existing class, review note): text under a marker
  background (diff/lint `BgColor`) is drawn on that marker color. An
  unfocused selection over it therefore uses
  `fl_color_average(marker_bg, selection, 0.4)`, which is not registered
  with `ui::contrast`, so FLTK's CIELAB level-10 rule applies there, as
  before T0078/T0081.
- Follow-up candidate: an overlay for the empty current line (option B from
  the spike: a custom draw after FLTK's draw that fills the empty row's text
  area and redraws the cursor). No ticket opened yet.
- The highlighter never reads the style buffer back (it only calls
  `set_text`), so twin chars cannot confuse its checkpoints. Keep it that way.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland), build `ab9ed45`
(T0077–T0083 together): every checklist step passed, including W5 (band on
the cursor line, View toggle). Windows 11 and macOS are optional and still
to run on the 0.9.10-rc.1 release candidate.
