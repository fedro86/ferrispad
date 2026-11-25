---
id: T0081
title: Highlight the current line (on by default, toggleable, theme-colored)
status: todo            # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: [T0078]     # other ticket IDs that must land first
issue: ~                # GitHub issue number
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
- **Color.** Use the active syntect theme's `theme.settings.line_highlight`
  when present. Otherwise derive a subtle tint of the background, chosen by
  background luminance. For custom themes (T0079) the same field is
  user-editable (T0080).
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
7. **Empty lines and the last line.** An empty line is highlighted by
   twinning its `'\n'` (BgColorExt fills the row). An empty last line with no
   trailing `'\n'` has no char to style, so it is not highlighted. This is
   accepted and documented.
8. **Word wrap.** All display rows of the logical line are highlighted (same
   as VS Code).
9. **Tabs.** The twin state is per document. Switching tabs never leaves the
   old document with a stale twin when it is shown again.
10. **No style buffer / special views.** Documents shown in tail/chunk or
    memory-mapped read-only mode (over 1.9 GiB), if they have no style buffer
    bound, make the feature a silent no-op.
11. **Color sanity.** The derived or theme color must be visible but weaker
    than the selection, and must keep the text readable.

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
- `current_line_empty_line_twins_newline` — an empty line's `'\n'` is
  twinned.
- `current_line_color_sane_on_all_themes` — for each of the 7 built-ins:
  `1.05 <= contrast(line, bg) < contrast(selection, bg)`, and
  `contrast(effective_text, line) >= 4.5`.
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
   Empty line: highlighted.
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

- [ ] The current line is highlighted by default, on all supported
      platforms, and can be toggled off persistently.
- [ ] The color comes from the theme (`line_highlight`) or a derived tint,
      and passes the color-sanity test on all built-ins.
- [ ] No orphan highlights after any edit; no per-event O(file) work; long
      lines skipped.
- [ ] Style chars stay ASCII; annotations and selection keep priority.
- [ ] All tests listed above exist and pass; the spike result is recorded.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass; idle CPU stays at 0%.

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
- `src/ui/theme.rs` — resolve the line color (shares T0079's
  `effective_editor_colors` once that lands; before that, read the syntect
  `Theme` directly).

## Notes

- Origin: GitHub issue #150 ("line colors"); see T0079 for the full split.
  Open a tracking issue when implementation starts.
- Depends only on T0078, because the color-sanity test compares against the
  selection color. T0079 / T0080 make the color editable, but this ticket
  does not need them.
- The highlighter never reads the style buffer back (it only calls
  `set_text`), so twin chars cannot confuse its checkpoints. Keep it that way.
